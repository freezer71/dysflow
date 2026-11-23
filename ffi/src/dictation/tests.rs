//! La session de dictée du cœur, menée par un faux reconnaisseur injecté
//! dans le processus : aucun modèle, aucun micro, aucun fichier de
//! l'utilisateur (les dossiers sont ceux des essais, R21).

use super::*;
use std::sync::atomic::AtomicUsize;
use std::sync::OnceLock;
use std::time::Instant;

/// Le modèle gardé est global : les essais qui y touchent passent l'un
/// après l'autre.
static SERIE: Mutex<()> = Mutex::new(());

fn serie() -> MutexGuard<'static, ()> {
    lock(&SERIE)
}

const RATE: usize = 16_000;

/// De la « parole » pour le détecteur d'énergie : une sinusoïde dont le
/// niveau varie d'une syllabe à l'autre (un niveau constant serait un
/// bruit).
fn voice(seconds: f64) -> Vec<f32> {
    (0..(seconds * RATE as f64) as usize)
        .map(|i| {
            let syllable = (i / 512) as f32 * std::f32::consts::TAU / 8.0;
            let tone = (i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin();
            0.1 * (1.0 + 0.3 * syllable.sin()) * tone
        })
        .collect()
}

fn silence(seconds: f64) -> Vec<f32> {
    vec![0.0; (seconds * RATE as f64) as usize]
}

/// Un faux Parakeet : « bonjour » pour tout énoncé, lu sous le verrou du
/// GPU comme le vrai (le fil MLX le tient pendant le calcul).
#[derive(Default)]
struct Fake {
    loads: AtomicUsize,
    releases: AtomicUsize,
    /// Le chargement échoue.
    broken: bool,
    /// Le chargement attend ce signal.
    load_gate: Option<Mutex<mpsc::Receiver<()>>>,
}

impl Recognizer for Fake {
    fn load(&self, _: &Path) -> anyhow::Result<()> {
        if let Some(gate) = &self.load_gate {
            let _ = lock(gate).recv();
        }
        anyhow::ensure!(!self.broken, "poids tronqués");
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn warm_up(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn transcribe(&self, samples: &[f32], _: &str, _: &Cancel) -> anyhow::Result<String> {
        Ok(dysflow::compute::gpu::run(|| {
            if samples.len() < 1_600 {
                String::new()
            } else {
                "bonjour".to_string()
            }
        }))
    }
    fn release(&self) {
        self.releases.fetch_add(1, Ordering::SeqCst);
    }
    fn memory(&self) -> Option<u64> {
        Some(800 << 20)
    }
}

/// Une nouvelle reçue, avec ce qui l'entourait.
#[derive(Debug, Clone)]
struct Received {
    update: DictationUpdate,
    thread: String,
    under_gpu_lock: bool,
}

/// Note tout ; peut rappeler la session (`cancel_on`).
#[derive(Default)]
struct Notes {
    seen: Mutex<Vec<Received>>,
    session: OnceLock<Arc<DictationSession>>,
    /// À la première nouvelle qui vérifie ce test, la session est annulée
    /// depuis `on_update` même.
    cancel_on: Option<fn(&DictationUpdate) -> bool>,
    /// À la première nouvelle qui vérifie ce test, `finish` est appelé
    /// depuis `on_update` même ; son résultat part ici.
    finish_on: Option<fn(&DictationUpdate) -> bool>,
    finished: Mutex<Option<mpsc::Sender<Result<String, String>>>>,
}

impl DictationListener for Notes {
    fn on_update(&self, update: DictationUpdate) {
        let thread = std::thread::current().name().unwrap_or("").to_string();
        let cancel = self.cancel_on.is_some_and(|f| f(&update));
        let finish = self.finish_on.is_some_and(|f| f(&update));
        lock(&self.seen).push(Received {
            update,
            thread,
            under_gpu_lock: dysflow::compute::gpu::held_by_this_thread(),
        });
        if cancel {
            if let Some(session) = self.session.get() {
                session.cancel();
                // Le cœur aussi se rappelle d'ici sans blocage.
                core_dictation_set_keep_voice_minutes(10);
            }
        }
        if finish {
            if let (Some(session), Some(answer)) = (self.session.get(), lock(&self.finished).take())
            {
                let _ = answer.send(session.finish().map_err(|e| e.to_string()));
            }
        }
    }
}

/// `body` sur un autre fil, 5 s au plus : un blocage fait échouer l'essai
/// au lieu de le faire pendre.
fn within<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(body());
    });
    answer
        .recv_timeout(Duration::from_secs(5))
        .expect("bloqué plus de 5 s")
}

impl Notes {
    fn updates(&self) -> Vec<DictationUpdate> {
        lock(&self.seen).iter().map(|r| r.update.clone()).collect()
    }
}

/// Un modèle gardé neuf, sous l'id `id`, autour de `fake`.
fn keep(id: &str, fake: Arc<Fake>) -> Arc<dyn KeptModel> {
    core_dictation_release();
    *lock(&KEPT) = None;
    let shared: Arc<dyn Recognizer> = fake;
    kept(id, || Ok(Arc::new(Holder::new(shared, None)))).unwrap()
}

fn dir() -> PathBuf {
    std::env::temp_dir().join("dysflow-dictee-faux")
}

fn start(model: Arc<dyn KeptModel>, notes: Arc<Notes>) -> Arc<DictationSession> {
    let session = Arc::new(DictationSession::begin(model, &dir(), "fr", notes.clone()).unwrap());
    let _ = notes.session.set(session.clone());
    session
}

/// Deux phrases séparées d'un silence : deux énoncés.
fn two_sentences() -> Vec<f32> {
    [voice(0.8), silence(1.0), voice(0.8), silence(0.2)].concat()
}

#[test]
fn a_dictation_reads_through_the_kept_model_and_tells_the_app_in_order() {
    let _serie = serie();
    let fake = Arc::new(Fake::default());
    let model = keep("faux-dictee", fake.clone());
    let notes = Arc::new(Notes::default());
    let session = start(model.clone(), notes.clone());
    for chunk in two_sentences().chunks(160) {
        session.push(chunk.to_vec());
    }
    assert_eq!(session.finish().unwrap(), "bonjour bonjour");

    // `finish` rend la main une fois tout remis, `Ended` compris.
    let seen = lock(&notes.seen).clone();
    let updates: Vec<_> = seen.iter().map(|r| r.update.clone()).collect();
    assert!(
        matches!(
            &updates[..3],
            [
                DictationUpdate::Loading,
                DictationUpdate::Loaded { .. },
                DictationUpdate::Ready
            ]
        ),
        "{updates:?}"
    );
    assert_eq!(
        updates.last(),
        Some(&DictationUpdate::Ended {
            text: "bonjour bonjour".into()
        })
    );
    let committed = updates
        .iter()
        .filter(|u| matches!(u, DictationUpdate::Committed { text, .. } if text == "bonjour"))
        .count();
    assert_eq!(committed, 2, "{updates:?}");
    assert!(updates.contains(&DictationUpdate::Partial {
        committed: "bonjour bonjour".into(),
        pending: String::new()
    }));
    assert!(updates.contains(&DictationUpdate::Memory { bytes: 800 << 20 }));
    // Le fil d'envoi, jamais celui de la dictée, jamais sous le verrou du GPU.
    for r in &seen {
        assert_eq!(r.thread, "dysflow-dictee-envoi", "{r:?}");
        assert!(!r.under_gpu_lock, "{r:?}");
    }
    assert_eq!(fake.loads.load(Ordering::SeqCst), 1);

    // La dictée suivante trouve le modèle chargé : prêt tout de suite.
    let notes = Arc::new(Notes::default());
    let session = start(model, notes.clone());
    session.push(two_sentences());
    assert_eq!(session.finish().unwrap(), "bonjour bonjour");
    assert_eq!(notes.updates().first(), Some(&DictationUpdate::Ready));
    assert_eq!(
        fake.loads.load(Ordering::SeqCst),
        1,
        "gardé d'une dictée à l'autre"
    );
    core_dictation_release();
    assert_eq!(fake.releases.load(Ordering::SeqCst), 1);
}

/// R41 : pendant le chargement, `push` rend la main tout de suite, et rien
/// de ce qui a été dit n'est perdu.
#[test]
fn push_never_waits_for_the_model_to_load() {
    let _serie = serie();
    let (open, gate) = mpsc::channel();
    let fake = Arc::new(Fake {
        load_gate: Some(Mutex::new(gate)),
        ..Default::default()
    });
    let notes = Arc::new(Notes::default());
    let session = start(keep("faux-chargement", fake.clone()), notes.clone());
    let mut slowest = Duration::ZERO;
    for chunk in two_sentences().chunks(160) {
        let t = Instant::now();
        session.push(chunk.to_vec());
        slowest = slowest.max(t.elapsed());
    }
    assert!(
        slowest < Duration::from_millis(50),
        "push a attendu {slowest:?}"
    );
    assert_eq!(
        notes.updates(),
        vec![DictationUpdate::Loading],
        "le modèle se charge encore"
    );
    open.send(()).unwrap();
    assert_eq!(session.finish().unwrap(), "bonjour bonjour");
    core_dictation_release();
}

#[test]
fn a_model_that_will_not_load_fails_before_ready() {
    let _serie = serie();
    let fake = Arc::new(Fake {
        broken: true,
        ..Default::default()
    });
    let notes = Arc::new(Notes::default());
    let session = start(keep("faux-casse", fake), notes.clone());
    session.push(two_sentences());
    let err = session.finish().unwrap_err().to_string();
    assert!(err.contains("poids tronqués"), "{err}");
    let updates = notes.updates();
    assert_eq!(updates.first(), Some(&DictationUpdate::Loading));
    assert!(!updates.contains(&DictationUpdate::Ready), "{updates:?}");
    assert!(
        matches!(updates.last(), Some(DictationUpdate::Failed { message }) if message.contains("poids tronqués")),
        "{updates:?}"
    );
}

/// L'app annule depuis `on_update` même : ni blocage, ni nouvelle après.
#[test]
fn the_listener_may_cancel_from_inside_an_update() {
    let _serie = serie();
    let fake = Arc::new(Fake::default());
    let notes = Arc::new(Notes {
        cancel_on: Some(|u| matches!(u, DictationUpdate::Ready)),
        ..Default::default()
    });
    let session = start(keep("faux-annule", fake), notes.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !notes.updates().contains(&DictationUpdate::Ready) {
        assert!(Instant::now() < deadline, "jamais prêt");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.push(two_sentences());
    let ending = session.clone();
    assert!(within(move || ending.finish()).is_err(), "annulée");
    let updates = notes.updates();
    assert_eq!(
        updates.last(),
        Some(&DictationUpdate::Ready),
        "rien après l'annulation : {updates:?}"
    );
    core_dictation_release();
}

/// `finish` depuis `on_update` ne peut pas attendre le fil qui l'appelle :
/// il rend une erreur claire, sans bloquer, et la dictée reste à finir
/// d'ailleurs.
#[test]
fn finish_from_inside_an_update_is_an_error_not_a_deadlock() {
    let _serie = serie();
    let fake = Arc::new(Fake::default());
    let (answer, finished) = mpsc::channel();
    let notes = Arc::new(Notes {
        finish_on: Some(|u| matches!(u, DictationUpdate::Ready)),
        finished: Mutex::new(Some(answer)),
        ..Default::default()
    });
    let session = start(keep("faux-fin-dedans", fake), notes.clone());
    let err = finished
        .recv_timeout(Duration::from_secs(5))
        .expect("finish a bloqué le fil d'envoi")
        .expect_err("finish depuis on_update");
    assert!(err.contains("on_update"), "{err}");
    session.push(two_sentences());
    let ending = session.clone();
    assert_eq!(within(move || ending.finish()).unwrap(), "bonjour bonjour");
    assert_eq!(
        notes.updates().last(),
        Some(&DictationUpdate::Ended {
            text: "bonjour bonjour".into()
        })
    );
    core_dictation_release();
}

/// Un autre modèle gardé libère d'abord le précédent : une seule part du
/// budget « dictee » à la fois.
#[test]
fn another_model_releases_the_one_kept() {
    let _serie = serie();
    let a = Arc::new(Fake::default());
    let first = keep("faux-a", a.clone());
    first.prepare(&dir(), &|| {}).unwrap();
    let second = kept("faux-b", || {
        Ok(Arc::new(Holder::new(Fake::default(), None)))
    })
    .unwrap();
    assert_eq!(a.releases.load(Ordering::SeqCst), 1);
    assert!(
        Arc::ptr_eq(&second, &kept("faux-b", || unreachable!()).unwrap()),
        "le même, gardé"
    );
    core_dictation_release();
}

/// `release_if_loaded` ne libère que le modèle de ce paquet, et le délai
/// suit `keep_voice_minutes`.
#[test]
fn the_kept_model_follows_the_release_calls_and_the_delay() {
    let _serie = serie();
    let fake = Arc::new(Fake::default());
    let model = keep("faux-delai", fake.clone());
    model.prepare(&dir(), &|| {}).unwrap();
    core_dictation_release_if_loaded("un-autre".into());
    assert_eq!(
        fake.releases.load(Ordering::SeqCst),
        0,
        "un autre paquet : rien"
    );
    core_dictation_set_keep_voice_minutes(-1);
    assert_eq!(*lock(&RELEASE_AFTER), None);
    core_dictation_set_keep_voice_minutes(0);
    assert_eq!(
        *lock(&RELEASE_AFTER),
        Some(dysflow::stt::holder::SHORTEST_DELAY)
    );
    core_dictation_set_keep_voice_minutes(10);
    assert_eq!(*lock(&RELEASE_AFTER), Some(DEFAULT_DELAY));
    let before = core_dictation_compute_seconds();
    model
        .transcribe(&voice(0.5), "fr", &Cancel::never())
        .unwrap();
    assert!(core_dictation_compute_seconds() >= before);
    core_dictation_release();
    assert_eq!(fake.releases.load(Ordering::SeqCst), 1);
}

/// Un paquet inconnu, ou pas installé (les dossiers des essais sont vides) :
/// refusé avant toute dictée, et l'app se replie.
#[test]
fn an_unknown_or_missing_model_is_refused() {
    let _serie = serie();
    for id in ["inconnu", "parakeet_tdt_v3"] {
        let notes = Arc::new(Notes::default());
        let err = DictationSession::start(id.into(), "fr".into(), notes.clone())
            .err()
            .expect("refusé")
            .to_string();
        assert!(err.contains(id) || err.contains("Parakeet"), "{err}");
        assert!(transcribe_once(id.into(), Vec::new(), "fr".into()).is_err());
        assert!(notes.updates().is_empty());
    }
}

/// Parakeet MLX ne se dicte par le cœur que dans un binaire qui a MLX ; le
/// cœur le dit à l'app (`core_dictation_supports`), qui ne bascule que
/// ces paquets.
#[test]
fn parakeet_dictates_through_the_core_only_with_mlx() {
    let mlx = cfg!(all(
        feature = "mlx",
        target_os = "macos",
        target_arch = "aarch64"
    ));
    for id in ["parakeet_tdt_v3", "parakeet_tdt_v2"] {
        let parakeet = dysflow::models::CATALOG
            .iter()
            .find(|p| p.id == id)
            .unwrap();
        let made = model_for(parakeet);
        assert_eq!(core_dictation_supports(id.into()), mlx, "{id}");
        if mlx {
            assert!(made.is_ok(), "{id}");
        } else {
            let err = made.err().expect("sans MLX").to_string();
            assert!(err.contains("Parakeet"), "{err}");
        }
    }
    // Phonon-2 (tâche 16), Whisper (tâche 17) et Canary (tâche 19), P1 à
    // P4 : le cœur sait les dicter ; l'app ne bascule pas encore
    // (`CoreDictation.ported` vide jusqu'à P6).
    for id in [
        "phonon_2",
        "whisper_tiny_4bit",
        "whisper_small_4bit",
        "whisper_large_v3_turbo_4bit",
        "canary_1b_v2_q8",
    ] {
        assert_eq!(core_dictation_supports(id.into()), mlx, "{id}");
    }
    assert!(
        !core_dictation_supports("nemotron_35_asr_streaming_8bit".into()),
        "pas porté"
    );
    assert!(!core_dictation_supports("inconnu".into()));
}

/// Measure lit la copie Rust de MLX par ici : avec la feature `mlx`, et
/// seulement quand elle tient un modèle (sinon la lire démarrerait MLX).
#[test]
fn the_rust_mlx_memory_is_known_only_with_mlx_and_a_model() {
    let _serie = serie();
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    {
        use dysflow::compute::mlx::budget;
        let key = "test:memoire-rust";
        if budget::total().memory == 0 {
            assert_eq!(core_mlx_memory(), None, "rien de chargé");
        }
        budget::set(
            key,
            budget::Need {
                memory: 8 << 30,
                cache: 256 << 20,
            },
        );
        let memory = core_mlx_memory();
        budget::remove(key);
        let memory = memory.expect("la copie Rust de MLX répond");
        assert!(memory.limit >= 8 << 30, "{memory:?}");
    }
    #[cfg(not(all(feature = "mlx", target_os = "macos", target_arch = "aarch64")))]
    assert_eq!(core_mlx_memory(), None);
}
