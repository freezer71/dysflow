//! Les essais de Kokoro MLX. Ceux marqués « poids Kokoro » lancent le vrai
//! modèle : `DYSFLOW_MODELS` désigne le dossier des modèles (celui qui
//! contient `kokoro/`), et ils échouent sans lui.
//!
//! `. scripts/mlx-sources.sh && DYSFLOW_MODELS=… cargo test --release -p dysflow --features mlx --lib tts::kokoro::mlx -- --ignored`

use super::*;
use crate::tts::{self, Cancelled};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;

/// Les essais qui chargent posent et retirent le besoin `"kokoro"` dans les
/// plafonds, globaux au processus : un à la fois.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------- la fixture du Swift ----------

#[derive(serde::Deserialize)]
struct Reference {
    ids: Vec<i64>,
    style: Vec<f32>,
    durations: Vec<u32>,
    audio_i16_base64: String,
}

fn reference() -> Reference {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("macos/Tests/Fixtures/kokoro-parite.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0);
    for six in text.bytes().filter_map(value) {
        buffer = (buffer << 6) | six as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

/// L'audio de la fixture, comme le test Swift le lit : PCM 16 bits / 32767.
fn expected_audio(reference: &Reference) -> Vec<f32> {
    base64(&reference.audio_i16_base64)
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32767.0)
        .collect()
}

fn mean_error(samples: &[f32], expected: &[f32]) -> f32 {
    let n = samples.len().min(expected.len());
    samples[..n].iter().zip(&expected[..n]).map(|(a, b)| (a - b).abs()).sum::<f32>() / n as f32
}

/// Le dossier du vrai modèle ; manquant, l'essai échoue (jamais à vide).
fn model_dir() -> PathBuf {
    let models = std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS : le dossier des modèles");
    let dir = PathBuf::from(models).join("kokoro");
    assert!(dir.join("config.json").is_file(), "pas de Kokoro dans {}", dir.display());
    dir
}

fn loaded() -> MlxBackend {
    let backend = MlxBackend::new();
    backend.load(&model_dir()).unwrap();
    backend
}

/// Un test d'abandon qui compte ses consultations et dit vrai à partir de
/// la `from`-ième.
fn counting_cancel(from: usize) -> (Cancel, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    (Cancel::when(move || counter.fetch_add(1, Ordering::SeqCst) + 1 >= from), asked)
}

fn zero_style() -> Vec<f32> {
    vec![0.0; 256]
}

// ---------- parité (KokoroParityTests) ----------

#[test]
#[ignore = "poids Kokoro"]
fn parity() {
    let _serial = serial();
    let reference = reference();
    let expected = expected_audio(&reference);
    let backend = loaded();
    let held = backend.held().unwrap();
    let (ids, style) = (reference.ids.clone(), reference.style.clone());
    // Graine 7, comme `MLXRandom.seed(7)` du test Swift.
    let out = held
        .with(move |kokoro| {
            kokoro.rng = RandomState::with_seed(7)?;
            kokoro.forward(&ids, &style, &Cancel::never())
        })
        .unwrap();
    let error = mean_error(&out.samples, &expected);
    eprintln!(
        "parité : {} durées, {} échantillons (fixture {}), écart moyen {error:.5}",
        out.durations.len(),
        out.samples.len(),
        expected.len()
    );
    assert_eq!(out.durations, reference.durations);
    assert_eq!(out.samples.len(), expected.len());
    assert!(error < 0.01, "écart moyen {error}");

    // La lecture passe un test d'abandon : mêmes sorties avec les
    // évaluations de ses points d'arrêt.
    let (ids, style) = (reference.ids.clone(), reference.style.clone());
    let stoppable = held
        .with(move |kokoro| {
            kokoro.rng = RandomState::with_seed(7)?;
            kokoro.forward(&ids, &style, &Cancel::when(|| false))
        })
        .unwrap();
    let error = mean_error(&stoppable.samples, &expected);
    eprintln!("parité, avec points d'arrêt : écart moyen {error:.5}");
    assert_eq!(stoppable.durations, reference.durations);
    assert_eq!(stoppable.samples.len(), expected.len());
    assert!(error < 0.01, "écart moyen {error}");
    backend.release();
}

/// Le passage de la porte du lot 3 (`mlx_rust_kokoro_check`) : graine fixée,
/// il rend la fixture ; `inside` est appelé sur le fil MLX, verrou du GPU
/// tenu, et ce qu'il rend n'est lâché qu'à la fin du calcul, verrou encore
/// tenu.
#[test]
#[ignore = "poids Kokoro"]
fn a_seeded_pass_gives_the_fixture_and_brackets_the_compute_under_the_lock() {
    struct Leave(mpsc::Sender<(&'static str, bool)>);
    impl Drop for Leave {
        fn drop(&mut self) {
            let _ = self.0.send(("leave", crate::compute::gpu::held_by_this_thread()));
        }
    }

    let _serial = serial();
    let reference = reference();
    let expected = expected_audio(&reference);
    let backend = loaded();
    let (said, heard) = mpsc::channel();
    for _ in 0..2 {
        let said = said.clone();
        let out = backend
            .infer_seeded(&reference.ids, &reference.style, 7, move || {
                let on_worker = std::thread::current().name() == Some("dysflow-mlx");
                let _ = said.send(("enter", on_worker && crate::compute::gpu::held_by_this_thread()));
                Leave(said)
            })
            .unwrap();
        assert_eq!(heard.try_recv(), Ok(("enter", true)), "sur le fil MLX, verrou tenu");
        assert_eq!(heard.try_recv(), Ok(("leave", true)), "lâché dans le calcul, verrou tenu");
        assert_eq!(out.durations, reference.durations);
        assert_eq!(out.samples.len(), expected.len());
        let error = mean_error(&out.samples, &expected);
        assert!(error < 0.01, "écart moyen {error}");
    }
    backend.release();
}

/// Sans modèle chargé, le passage à graine fixe est une erreur, et `inside`
/// n'est pas appelé. Sans MLX ni poids.
#[test]
fn a_seeded_pass_without_weights_is_an_error() {
    let backend = MlxBackend::new();
    let called = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&called);
    let err = backend.infer_seeded(&[0], &zero_style(), 7, move || counter.fetch_add(1, Ordering::SeqCst));
    assert!(err.is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

// ---------- abandon (KokoroMLXComputeTests) ----------

#[test]
#[ignore = "poids Kokoro"]
fn a_cancelled_synthesis_is_cancelled_not_failed() {
    let _serial = serial();
    let backend = loaded();
    let ids = reference().ids;
    let err = backend.infer(&ids, &zero_style(), &Cancel::when(|| true)).unwrap_err();
    assert!(err.is::<tts::Cancelled>(), "{err:#}");
    backend.release();
}

/// Déjà inutile avant le passage : MLX ne calcule rien.
#[test]
#[ignore = "poids Kokoro"]
fn an_abandon_before_the_pass_skips_mlx() {
    let _serial = serial();
    let backend = loaded();
    let err = backend.infer(&[0, 43, 0], &zero_style(), &Cancel::when(|| true)).unwrap_err();
    assert!(err.is::<Cancelled>(), "{err:#}");
    assert_eq!(backend.passes(), 0);
    backend.release();
}

/// Inutile une fois les durées calculées : le décodeur ne tourne pas (aucun
/// de ses points n'est consulté), et l'abandon revient comme tel. Le passage
/// compte : il a eu lieu.
#[test]
#[ignore = "poids Kokoro"]
fn an_abandon_after_the_durations_skips_the_decoder() {
    let _serial = serial();
    let backend = loaded();
    let (cancel, asked) = counting_cancel(2);
    let err = backend.infer(&[0, 43, 0], &zero_style(), &cancel).unwrap_err();
    assert!(err.is::<Cancelled>(), "{err:#}");
    assert_eq!(asked.load(Ordering::SeqCst), 2, "avant le passage, puis après les durées");
    assert_eq!(backend.passes(), 1, "le passage a eu lieu");

    // Une synthèse utile va au bout.
    let raw = backend.infer(&[0, 43, 0], &zero_style(), &Cancel::never()).unwrap();
    assert_eq!(raw.durations.len(), 3);
    crate::tts::kokoro::check(&raw, 3).unwrap();
    backend.release();
}

/// Avant le passage, durées, `encode` : utile ; au premier bloc de `decode` :
/// plus. Toujours utile : les 9 points sont consultés (avant le passage, les
/// durées, `encode`, 4 blocs de `decode`, 2 étages du générateur).
#[test]
#[ignore = "poids Kokoro"]
fn a_stop_requested_in_the_decoder_stops_it() {
    let _serial = serial();
    let backend = loaded();
    let (cancel, asked) = counting_cancel(4);
    let err = backend.infer(&[0, 43, 0], &zero_style(), &cancel).unwrap_err();
    assert!(err.is::<Cancelled>(), "{err:#}");
    assert_eq!(asked.load(Ordering::SeqCst), 4);

    let (never, asked) = counting_cancel(usize::MAX);
    let raw = backend.infer(&[0, 43, 0], &zero_style(), &never).unwrap();
    assert_eq!(asked.load(Ordering::SeqCst), 1 + 1 + 1 + 4 + 2);
    crate::tts::kokoro::check(&raw, 3).unwrap();
    backend.release();
}

/// Une synthèse inutile pendant une brève libération des poids : un
/// abandon, pas un échec. Sans MLX ni poids.
#[test]
fn an_abandon_without_weights_is_still_an_abandon() {
    let backend = MlxBackend::new();
    let err = backend.infer(&[0], &[], &Cancel::when(|| true)).unwrap_err();
    assert!(err.is::<Cancelled>(), "{err:#}");
    let err = backend.infer(&[0], &[], &Cancel::never()).unwrap_err();
    assert!(!err.is::<Cancelled>(), "rien de chargé : un échec");
}

// ---------- préparation ----------

#[test]
fn warm_up_ids_match_the_swift_ones() {
    let ids = warm_up_ids();
    assert_eq!((ids.len(), ids[0], ids[ids.len() - 1]), (38, 0, 0));
    // `testTheWarmUpSpeaksARealisticSentenceWithoutStopPoints` : espaces et virgule.
    assert!(ids.contains(&16) && ids.contains(&3), "{ids:?}");
    // « la » : l = 54, a = 43.
    assert_eq!(&ids[1..4], &[54, 43, 16]);
}

/// Une préparation venue après une synthèse ne fait rien ; deux préparations
/// d'affilée n'en font qu'une. Le drapeau est celui du processus : dans un
/// processus à part, où rien n'a encore calculé.
#[test]
#[ignore = "poids Kokoro"]
fn the_warm_up_is_skipped_once_a_synthesis_compiled_the_kernels() {
    const ALONE: &str = "DYSFLOW_TEST_ALONE";
    if std::env::var_os(ALONE).is_none() {
        let name = "tts::kokoro::mlx::tests::the_warm_up_is_skipped_once_a_synthesis_compiled_the_kernels";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name, "--exact", "--ignored"])
            .env(ALONE, "1")
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}{stderr}");
        return;
    }
    assert!(!KERNELS_COMPILED.load(Ordering::SeqCst), "processus neuf");
    let backend = loaded();
    backend.warm_up().unwrap();
    backend.warm_up().unwrap();
    assert_eq!(backend.passes(), 1, "une préparation, la seconde est inutile");

    // Un passage abandonné en route ne compte pas comme complet…
    let other = loaded();
    assert_eq!(other.passes(), 0);
    let (cancel, _) = counting_cancel(3);
    assert!(other.infer(&[0, 43, 0], &zero_style(), &cancel).unwrap_err().is::<Cancelled>());
    // … mais le drapeau est déjà levé par la préparation du premier : le
    // second, qui n'a fait aucun passage complet, n'en refait pas.
    other.warm_up().unwrap();
    assert_eq!(other.passes(), 1);
    other.infer(&[0, 43, 0], &zero_style(), &Cancel::never()).unwrap();
    other.warm_up().unwrap();
    assert_eq!(other.passes(), 2, "aucun passage de plus après une synthèse");
    backend.release();
    other.release();
}

/// Une préparation demandée pendant une synthèse, rangée derrière elle dans
/// la file du fil MLX : elle voit le passage complet qui l'a précédée et ne
/// refait rien. Déterministe : la synthèse est tenue dans son travail (son
/// test d'abandon attend) jusqu'à ce que la préparation soit dans la file.
#[test]
#[ignore = "poids Kokoro"]
fn a_warm_up_queued_behind_a_synthesis_does_nothing() {
    const ALONE: &str = "DYSFLOW_TEST_ALONE";
    if std::env::var_os(ALONE).is_none() {
        let name = "tts::kokoro::mlx::tests::a_warm_up_queued_behind_a_synthesis_does_nothing";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name, "--exact", "--ignored"])
            .env(ALONE, "1")
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}{stderr}");
        return;
    }
    let backend = Arc::new(loaded());
    let (entered, has_entered) = std::sync::mpsc::channel::<()>();
    let (go, may_go) = std::sync::mpsc::channel::<()>();
    let hook = Mutex::new(Some((entered, may_go)));
    // Première consultation (avant le passage, sous le verrou) : tenir.
    let cancel = Cancel::when(move || {
        if let Some((entered, may_go)) = hook.lock().unwrap().take() {
            entered.send(()).unwrap();
            may_go.recv().unwrap();
        }
        false
    });
    let reading = {
        let backend = Arc::clone(&backend);
        std::thread::spawn(move || backend.infer(&[0, 43, 0], &zero_style(), &cancel).map(|raw| raw.durations.len()))
    };
    has_entered.recv().unwrap();
    assert!(!KERNELS_COMPILED.load(Ordering::SeqCst), "processus neuf");
    let warming = {
        let backend = Arc::clone(&backend);
        std::thread::spawn(move || backend.warm_up())
    };
    // La préparation est dans la file, derrière la synthèse tenue.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while crate::compute::mlx::queued() == 0 {
        assert!(std::time::Instant::now() < deadline, "la préparation n'est jamais arrivée dans la file");
        std::thread::yield_now();
    }
    go.send(()).unwrap();
    assert_eq!(reading.join().unwrap().unwrap(), 3);
    warming.join().unwrap().unwrap();
    assert_eq!(backend.passes(), 1, "la préparation n'a pas refait de passage");
    backend.release();
}

/// Un passage abandonné en route ne lève pas le drapeau : dans un processus
/// neuf, la préparation qui suit calcule.
#[test]
#[ignore = "poids Kokoro"]
fn an_abandoned_pass_leaves_the_warm_up_to_do() {
    const ALONE: &str = "DYSFLOW_TEST_ALONE";
    if std::env::var_os(ALONE).is_none() {
        let name = "tts::kokoro::mlx::tests::an_abandoned_pass_leaves_the_warm_up_to_do";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name, "--exact", "--ignored"])
            .env(ALONE, "1")
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}{stderr}");
        return;
    }
    let backend = loaded();
    let (cancel, _) = counting_cancel(3);
    assert!(backend.infer(&[0, 43, 0], &zero_style(), &cancel).unwrap_err().is::<Cancelled>());
    assert!(!KERNELS_COMPILED.load(Ordering::SeqCst));
    backend.warm_up().unwrap();
    assert_eq!(backend.passes(), 2);
    assert!(KERNELS_COMPILED.load(Ordering::SeqCst));
    backend.release();
}

// ---------- budget ----------

/// `testKokoroHoldsItsNeedInTheBudgetWhileLoaded` : inscrit au chargement,
/// retiré à la libération ; ensuite, plus rien à calculer.
#[test]
#[ignore = "poids Kokoro"]
fn kokoro_holds_its_need_in_the_budget_while_loaded() {
    let _serial = serial();
    let backend = loaded();
    assert_eq!(budget::of(BUDGET_KEY), Some(NEED));
    backend.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert!(backend.infer(&[0, 43, 0], &zero_style(), &Cancel::never()).is_err());
    backend.release();
    assert_eq!(budget::of(BUDGET_KEY), None, "une libération de trop ne fait rien");
}

/// `testAFailedLoadHoldsNothing`.
#[test]
fn a_failed_load_holds_nothing() {
    let _serial = serial();
    let backend = MlxBackend::new();
    let absent = crate::paths::test_scratch("kokoro-mlx-absent").join("nulle-part");
    let err = backend.load(&absent).unwrap_err();
    assert!(format!("{err:#}").contains("nulle-part"), "{err:#}");
    assert!(backend.warm_up().is_err(), "rien de chargé");
    assert_eq!(backend.passes(), 0);
    assert_eq!(budget::of(BUDGET_KEY), None, "un chargement raté ne garde aucun besoin");
}

// ---------- poids invalides, sans poids réels ----------

/// Le `config.json` de Kokoro v1.0, sans son vocabulaire.
const CONFIG: &str = r#"{
    "hidden_dim": 512, "max_dur": 50, "n_layer": 3, "n_token": 178, "style_dim": 128,
    "text_encoder_kernel_size": 5,
    "plbert": {"hidden_size": 768, "num_attention_heads": 12, "intermediate_size": 2048,
               "max_position_embeddings": 512, "num_hidden_layers": 12, "dropout": 0.1},
    "istftnet": {"upsample_kernel_sizes": [20, 12], "upsample_rates": [10, 6],
                 "gen_istft_hop_size": 5, "gen_istft_n_fft": 20,
                 "resblock_dilation_sizes": [[1, 3, 5], [1, 3, 5], [1, 3, 5]],
                 "resblock_kernel_sizes": [3, 7, 11], "upsample_initial_channel": 512}
}"#;

const WEIGHTS: &str = "kokoro-v1_0.safetensors";

/// Un dossier de modèle synthétique : la configuration, et `weights` comme
/// fichier de poids.
fn synthetic(name: &str, weights: &[u8]) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("kokoro-mlx-{name}"));
    std::fs::write(dir.join("config.json"), CONFIG).unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    dir
}

/// Un safetensors de tenseurs F32 nuls.
fn safetensors(tensors: &[(&str, &[usize])]) -> Vec<u8> {
    let (mut header, mut offset) = (serde_json::Map::new(), 0usize);
    for (name, shape) in tensors {
        let bytes = shape.iter().product::<usize>() * 4;
        header.insert(
            name.to_string(),
            serde_json::json!({"dtype": "F32", "shape": shape, "data_offsets": [offset, offset + bytes]}),
        );
        offset += bytes;
    }
    let mut json = serde_json::Value::Object(header).to_string();
    while !json.len().is_multiple_of(8) {
        json.push(' ');
    }
    let mut file = (json.len() as u64).to_le_bytes().to_vec();
    file.extend_from_slice(json.as_bytes());
    file.resize(file.len() + offset, 0);
    file
}

fn load_error(dir: &Path) -> String {
    let _serial = serial();
    let backend = MlxBackend::new();
    let err = backend.load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None);
    format!("{err:#}")
}

#[test]
fn truncated_weights_are_an_error() {
    // Un en-tête annoncé de 1 000 octets, et 8 octets seulement.
    let mut bytes = 1000u64.to_le_bytes().to_vec();
    bytes.extend_from_slice(b"{\"a\":{\"d");
    assert_eq!(bytes.len(), 16);
    let err = load_error(&synthetic("tronque", &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn truncated_tensor_data_is_an_error() {
    // Un en-tête juste, des données coupées.
    let mut bytes = safetensors(&[("bert.embeddings.word_embeddings.weight", &[178, 128])]);
    bytes.truncate(bytes.len() - 100);
    let err = load_error(&synthetic("donnees-coupees", &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let err = load_error(&synthetic("cle-absente", &safetensors(&[("autre.chose", &[4])])));
    assert!(err.contains("bert.embeddings.word_embeddings.weight"), "{err}");
}

#[test]
fn a_wrong_shape_is_an_error_that_names_the_key() {
    let bytes = safetensors(&[("bert.embeddings.word_embeddings.weight", &[178, 64])]);
    let err = load_error(&synthetic("mauvaise-forme", &bytes));
    assert!(err.contains("bert.embeddings.word_embeddings.weight"), "{err}");
    assert!(err.contains("[178, 64]") && err.contains("[178, 128]"), "{err}");
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn an_unreadable_config_is_an_error_that_names_it() {
    let dir = synthetic("config-illisible", &safetensors(&[]));
    std::fs::write(dir.join("config.json"), "{").unwrap();
    let err = load_error(&dir);
    assert!(err.contains("config.json"), "{err}");
}

#[test]
fn the_alignment_gives_each_token_its_frames_in_order() {
    // 2, 1, 3 trames : 6 colonnes.
    let matrix = alignment(&[2, 1, 3]);
    let rows: Vec<&[f32]> = matrix.chunks(6).collect();
    assert_eq!(rows, [&[1., 1., 0., 0., 0., 0.][..], &[0., 0., 1., 0., 0., 0.], &[0., 0., 0., 1., 1., 1.]]);
}
