//! Synthèse vocale : contrat commun, moteurs, et ordonnanceur parallèle.

pub mod lazy;
pub mod params;
pub mod fallback;
pub mod kokoro;
pub mod pool;
pub mod preview;
pub mod silent;
pub mod system;
pub mod system_voices;
pub mod text_compute;


pub use pool::{SynthesisStats, Synthesizer};

pub use crate::audio::PcmBuffer;
pub use crate::doc::WordSpan;

/// Une phrase synthétisée, avec le temps de chacun de ses mots affichés
/// quand le moteur sait le dire.
#[derive(Debug, Clone)]
pub struct Synthesis {
    pub pcm: PcmBuffer,
    pub words: Option<Vec<WordSpan>>,
}

use anyhow::Result;
use std::sync::Arc;

/// Paramètres d'une requête de synthèse.
///
/// D'un côté le conditionnement de style (`StyleCondition`), de l'autre les
/// options libres du modèle.
#[derive(Debug, Clone, PartialEq)]
pub struct TtsParams {
    pub voice: String,
    pub language: String,
    // Pas de débit ici : la synthèse se fait toujours à 1×, et la vitesse
    // s'applique au son pendant la lecture (`audio::tempo`, par Sonic, sans
    // changer la hauteur). Donnée au modèle, elle comprimait la durée prévue
    // de la phrase et lui faisait avaler mots et ponctuation.
    /// Hauteur de la voix du système (-1 à 1) ; Kokoro ne la lit pas.
    pub pitch_shift: f32,
}

impl Default for TtsParams {
    fn default() -> Self {
        Self {
            voice: String::new(),
            language: "fr".to_string(),
            pitch_shift: 0.0,
        }
    }
}

/// Où en est un moteur, vu de l'interface.
#[derive(Debug, Clone, PartialEq)]
pub enum Readiness {
    Ready,
    Loading,
    /// Le moteur voulu a échoué ; la voix de repli parle à sa place.
    Fallback(String),
}

/// Un moteur TTS : détient le modèle, et ouvre des sessions de synthèse.
///
/// Les poids sont partagés par le moteur ; chaque session porte son propre
/// état. C'est ce qui autorise plusieurs synthèses en parallèle sur un seul
/// chargement du modèle.
pub trait TtsEngine: Send + Sync {
    fn name(&self) -> String;

    /// Ouvre une session, appelée une fois par thread de synthèse.
    fn open_session(&self) -> Result<Box<dyn TtsSession>>;

    /// Nombre de sessions simultanées raisonnable pour ce moteur.
    fn max_sessions(&self) -> usize;

    /// État du moteur, pour l'annoncer. Un moteur ordinaire est prêt.
    fn readiness(&self) -> Readiness {
        Readiness::Ready
    }

    /// Vrai quand le moteur gagne à synthétiser la phrase attendue en deux
    /// morceaux (tête jusqu'à la première virgule, puis la suite).
    fn splits_first_clause(&self) -> bool {
        false
    }
}

/// Le test d'abandon d'une synthèse : vrai quand son résultat n'a plus
/// d'usage (le curseur a quitté la phrase, ou la génération a changé).
///
/// Le pool le fournit ; le moteur le consulte là où il peut s'arrêter
/// (Kokoro MLX : entre les durées et le décodeur ; les autres : avant de
/// commencer). Chaque consultation prend un instant le verrou du pool : à
/// appeler quelques fois par synthèse, pas dans une boucle.
#[derive(Clone)]
pub struct Cancel(Option<Arc<dyn Fn() -> bool + Send + Sync>>);

impl Cancel {
    /// Jamais abandonnée : aperçus, préparation, essais.
    pub fn never() -> Self {
        Self(None)
    }

    pub fn when(test: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(test)))
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.as_ref().is_some_and(|test| test())
    }

    /// Faux pour `never` : inutile de surveiller l'abandon.
    pub fn may_cancel(&self) -> bool {
        self.0.is_some()
    }

    /// `Err(Cancelled)` si la synthèse est devenue inutile.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(Cancelled.into());
        }
        Ok(())
    }
}

impl std::fmt::Debug for Cancel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "Cancel(test)" } else { "Cancel(jamais)" })
    }
}

/// L'erreur d'une synthèse abandonnée : ni un échec (rien ne compte contre
/// la voix, pas de repli), ni un résultat (rien en cache).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::tr!("synthesis cancelled", "synthèse abandonnée"))
    }
}

impl std::error::Error for Cancelled {}

/// Vrai si l'erreur dit un abandon, et non un échec.
pub fn is_cancelled(err: &anyhow::Error) -> bool {
    err.is::<Cancelled>()
}

/// Une session de synthèse, utilisée par un seul thread.
pub trait TtsSession: Send {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer>;

    /// Comme `synthesize`, avec le temps de chaque mot de `words` — les
    /// formes parlées des mots affichés, dans l'ordre. Sans minutage par
    /// défaut : le lecteur répartit alors la durée selon les lettres.
    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        let _ = words;
        Ok(Synthesis { pcm: self.synthesize(text, params)?, words: None })
    }

    /// Comme `synthesize_timed`, abandonnée (`Err(Cancelled)`) quand
    /// `cancel` le dit. Par défaut, vérifié avant de commencer seulement :
    /// un moteur qui sait s'arrêter en route (Kokoro) la redéfinit.
    fn synthesize_cancellable(
        &mut self,
        text: &str,
        words: &[String],
        params: &TtsParams,
        cancel: &Cancel,
    ) -> Result<Synthesis> {
        cancel.check()?;
        self.synthesize_timed(text, words, params)
    }
}

/// Construit le moteur demandé, sans jamais bloquer l'appelant.
///
/// Les conditions qui se vérifient instantanément — feature absente, aucun
/// modèle configuré, fichier manquant — sont tranchées tout de suite, et donnent
/// lieu à un repli annoncé. Seul le chargement réel, qui prend quelques
/// secondes, part en arrière-plan : l'interface s'ouvre sans l'attendre.
pub fn build_engine(
    preference: EnginePreference,
    settings: &EngineSettings,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    build_engine_with(preference, settings, &AppCompute::default())
}

/// Les calculs que l'app fournit : Kokoro MLX (phonèmes) et les modèles à
/// texte (Qwen3, Magpie). Le terminal n'en a aucun.
#[derive(Clone, Default)]
pub struct AppCompute {
    /// Enveloppé **une fois** : chaque moteur reçoit le même `Arc`.
    pub kokoro: Option<Arc<kokoro::SharedCompute>>,
    pub text: Option<Arc<dyn text_compute::TextCompute>>,
}

/// `build_engine`, avec les calculs MLX quand l'app les fournit.
///
/// `compute.kokoro` est un calcul partagé (`SharedCompute`) : l'app
/// l'enveloppe une fois et passe le même `Arc` à chaque construction, pour
/// qu'un moteur relâché après le chargement du suivant ne vide pas ses
/// poids. `compute.text` tient un modèle par dossier : la lecture et
/// l'aperçu de la bibliothèque ne se marchent pas dessus.
///
/// L'avis rendu dit aussi un `DYSFLOW_ACCELERATOR` ignoré (une fois par
/// processus) et le repli sur un autre paquet installé.
pub fn build_engine_with(
    preference: EnginePreference,
    settings: &EngineSettings,
    compute: &AppCompute,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let app_mlx = compute.kokoro.is_some() || compute.text.is_some();
    let (accelerators, warning) = crate::compute::ordered(app_mlx);
    let loadable = move |r: crate::models::Runtime| r.loadable(app_mlx);
    let machine = Machine {
        accelerators,
        loadable: &loadable,
        installed: &crate::models::is_installed,
        path: &crate::models::install_path,
    };
    let (engine, note) = build_engine_on(preference, settings, compute, &machine);
    let note = match (warning, note) {
        (Some(warning), Some(note)) => Some(format!("{warning} · {note}")),
        (warning, note) => warning.or(note),
    };
    (engine, note)
}

/// Ce que le choix d'un paquet lit de la machine ; les tests passent le leur.
struct Machine<'a> {
    /// Du meilleur au moins bon, accélérateur forcé en tête (`compute::ordered`).
    accelerators: Vec<crate::compute::Accelerator>,
    loadable: &'a dyn Fn(crate::models::Runtime) -> bool,
    installed: &'a dyn Fn(&crate::models::ModelPackage) -> bool,
    path: &'a dyn Fn(&crate::models::ModelPackage) -> Option<std::path::PathBuf>,
}

fn build_engine_on(
    preference: EnginePreference,
    settings: &EngineSettings,
    compute: &AppCompute,
    machine: &Machine,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    if preference == EnginePreference::System {
        return (Arc::new(system::SystemEngine::new()), None);
    }
    if settings.family == "kokoro" {
        let mlx = compute.kokoro.clone();
        return build_kokoro(settings, machine, |settings, dir| kokoro_backend(settings, dir, mlx.clone()));
    }
    let text_package = crate::models::CATALOG.iter().find(|p| {
        p.family == settings.family
            && matches!(p.runtime, crate::models::Runtime::Qwen3TtsMlx | crate::models::Runtime::MagpieMlx)
    });
    if let Some(package) = text_package {
        return build_text_engine(package, settings, compute.text.clone());
    }
    // Kokoro, Qwen3 et Magpie sont les seuls modèles : une autre famille
    // (Supertonic, l'ancien MagpieTTS d'audio.cpp, laissés par une ancienne
    // configuration) ne se charge plus.
    let note = if settings.model_path.trim().is_empty() {
        None
    } else {
        Some(crate::tr!(
            en = "model “{}” not supported — system voice (install Kokoro)",
            fr = "modèle « {} » non pris en charge — voix du système (installez Kokoro)",
            if settings.family.is_empty() { settings.model_path.as_str() } else { settings.family.as_str() },
        ))
    };
    match note {
        Some(note) => (fallback::explained(Arc::new(system::SystemEngine::new()), note.clone()), Some(note)),
        None => (Arc::new(system::SystemEngine::new()), None),
    }
}

/// La voix de repli, et la raison, gardée aussi dans le moteur : l'app ne
/// garde que lui (`Readiness::Fallback`), la raison ne doit pas s'y perdre.
fn noted(fallback: Arc<dyn TtsEngine>, note: String) -> (Arc<dyn TtsEngine>, Option<String>) {
    (fallback::explained(fallback, note.clone()), Some(note))
}

/// Un modèle à texte (Qwen3, Magpie) : le calcul de l'app, le dossier du
/// fichier principal ; le chargement part en arrière-plan.
fn build_text_engine(
    package: &'static crate::models::ModelPackage,
    settings: &EngineSettings,
    text: Option<Arc<dyn text_compute::TextCompute>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let fallback = fallback::fallback_engine(settings);
    let Some(compute) = text else {
        let note = crate::tr!(
            en = "Metal compute unavailable — {} only runs in the app",
            fr = "Calcul Metal indisponible — {} ne tourne que dans l'app",
            package.display_name.get(),
        );
        return noted(fallback, note);
    };
    let main = std::path::Path::new(&settings.model_path);
    let dir = main.parent().filter(|_| main.is_file()).map(std::path::Path::to_path_buf);
    let Some(dir) = dir else {
        let note = crate::tr!(
            en = "{} is not installed — see the settings",
            fr = "{} n'est pas installé — voir les réglages",
            package.display_name.get(),
        );
        return noted(fallback, note);
    };
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn(package.display_name.get().to_string(), 1, fallback, move || {
        let engine = text_compute::TextComputeEngine::load(compute, package, &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    (Arc::new(engine), None)
}

/// Kokoro : le paquet choisi s'il se charge ici (`models::resolve`), sinon
/// les autres installés de sa famille dans l'ordre des accélérateurs, chacun
/// essayé une fois, puis la voix du système. Rien ne se télécharge.
///
/// `backend_for` tranche ce qui se voit tout de suite (calcul absent,
/// feature absente) ; seul le chargement réel part en arrière-plan.
fn build_kokoro(
    settings: &EngineSettings,
    machine: &Machine,
    mut backend_for: impl FnMut(&EngineSettings, &std::path::Path) -> Result<Arc<dyn kokoro::KokoroBackend>, String>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    use crate::models::ModelPackage;
    let fallback = fallback::fallback_engine(settings);
    let configured = crate::models::package_of(&settings.family, &settings.model_path);
    // Le choix compte comme installé là où les réglages le désignent (un
    // `--model` hors du dossier des modèles).
    let here = |p: &ModelPackage| configured == Some(p) && crate::models::kokoro_dir(&settings.model_path).is_some();
    let installed = |p: &ModelPackage| here(p) || (machine.installed)(p);
    let loadable = |r| (machine.loadable)(r);
    let resolved = configured.and_then(|c| crate::models::resolve(c.id, &machine.accelerators, loadable, installed));
    let others = configured.map_or_else(Vec::new, |c| crate::models::fallbacks(c.family, &machine.accelerators, loadable, installed));
    // Rien qui se charge ici : le choix tel quel, pour dire pourquoi il échoue.
    let first = resolved.or(configured);
    let settings_for = |p: Option<&'static ModelPackage>| match p {
        Some(p) if Some(p) != configured => {
            let path = (machine.path)(p)?;
            Some(EngineSettings { model_path: path.to_string_lossy().into_owned(), ..settings.clone() })
        }
        _ => Some(settings.clone()),
    };
    let mut attempt = |settings: &EngineSettings| -> Result<(Arc<dyn kokoro::KokoroBackend>, std::path::PathBuf), String> {
        let Some(dir) = crate::models::kokoro_dir(&settings.model_path) else {
            return Err(crate::tr!("Kokoro is not installed — see the Models tab", "Kokoro n'est pas installé — voir l'onglet Modèles").to_string());
        };
        Ok((backend_for(settings, &dir)?, dir))
    };
    let name = |p: Option<&'static ModelPackage>| p.map_or("Kokoro", |p| p.display_name.get());
    let mut failed: Option<(Option<&'static ModelPackage>, String)> = None;
    let order = std::iter::once(first).chain(others.into_iter().map(Some));
    let mut tried: Vec<Option<&'static ModelPackage>> = Vec::new();
    for package in order {
        if tried.contains(&package) {
            continue;
        }
        tried.push(package);
        let Some(settings) = settings_for(package) else { continue };
        match attempt(&settings) {
            Ok((backend, dir)) => {
                let note = failed.map(|(wanted, reason)| {
                    crate::tr!(
                        en = "{} unavailable ({}) — {} reads instead",
                        fr = "{} indisponible ({}) — {} lit à la place",
                        name(wanted),
                        reason,
                        name(package),
                    )
                });
                return (lazy_kokoro(backend, dir, fallback), note);
            }
            Err(reason) => {
                failed.get_or_insert((package, reason));
            }
        }
    }
    let reason = failed.map_or_else(
        || crate::tr!("Kokoro is not installed — see the Models tab", "Kokoro n'est pas installé — voir l'onglet Modèles").to_string(),
        |(_, reason)| reason,
    );
    noted(fallback, reason)
}

/// Kokoro chargé en arrière-plan ; `fallback` parle en attendant, ou s'il
/// échoue.
fn lazy_kokoro(
    backend: Arc<dyn kokoro::KokoroBackend>,
    dir: std::path::PathBuf,
    fallback: Arc<dyn TtsEngine>,
) -> Arc<dyn TtsEngine> {
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn("Kokoro".to_string(), 1, fallback, move || {
        let phonemizer = kokoro::phonemes::EspeakProcess::locate()?;
        let engine = kokoro::KokoroEngine::load(backend, Box::new(phonemizer), &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    Arc::new(engine)
}

/// Le calcul selon le fichier du paquet : `.onnx`, ONNX Runtime ; sinon MLX.
/// Le calcul de l'app gagne quand elle en fournit un (règle R3, jusqu'à la
/// bascule de l'app) ; le Kokoro MLX en Rust ne sert que sans lui.
fn kokoro_backend(
    settings: &EngineSettings,
    dir: &std::path::Path,
    mlx: Option<Arc<kokoro::SharedCompute>>,
) -> Result<Arc<dyn kokoro::KokoroBackend>, String> {
    if settings.model_path.ends_with(".onnx") {
        // ONNX Runtime sur processeur, `min(engine.threads, 4)` fils.
        #[cfg(feature = "kokoro-onnx")]
        return Ok(shared_ort(dir, settings.threads));
        #[cfg(not(feature = "kokoro-onnx"))]
        {
            let _ = dir;
            return Err(crate::tr!(
                "Kokoro int8: binary built without `kokoro-onnx`",
                "Kokoro int8 : binaire compilé sans `kokoro-onnx`"
            )
            .to_string());
        }
    }
    match mlx.map(|app| app as Arc<dyn kokoro::KokoroBackend>) {
        Some(app) => Ok(app),
        None => rust_mlx().ok_or_else(|| {
            crate::tr!(
                "Metal compute unavailable — Kokoro MLX only runs in the app, on the GPU",
                "Calcul Metal indisponible — Kokoro MLX ne tourne que dans l'app, sur la puce graphique"
            )
            .to_string()
        }),
    }
}

/// Le Kokoro MLX en Rust, un pour le processus, enveloppé une fois : les
/// moteurs qui se succèdent partagent ses poids (`SharedCompute`). Vide tant
/// qu'aucun moteur ne le tient.
#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
fn rust_mlx() -> Option<Arc<dyn kokoro::KokoroBackend>> {
    static SHARED: std::sync::OnceLock<Arc<kokoro::SharedCompute>> = std::sync::OnceLock::new();
    let shared = SHARED.get_or_init(|| Arc::new(kokoro::SharedCompute::new(Arc::new(kokoro::mlx::MlxBackend::new()))));
    Some(Arc::clone(shared) as Arc<dyn kokoro::KokoroBackend>)
}

#[cfg(not(all(feature = "mlx", target_os = "macos", target_arch = "aarch64")))]
fn rust_mlx() -> Option<Arc<dyn kokoro::KokoroBackend>> {
    None
}

/// Le calcul ORT d'un dossier et d'un nombre de fils, partagé entre les
/// moteurs qui vivent en même temps : au rechargement, l'ancien moteur part
/// sur un autre fil pendant que le nouveau charge, et deux calculs à part
/// tiendraient deux fois le modèle en mémoire. Seule une référence faible est
/// gardée : le dernier moteur parti, le calcul part avec lui.
#[cfg(feature = "kokoro-onnx")]
fn shared_ort(dir: &std::path::Path, threads: u32) -> Arc<kokoro::SharedCompute> {
    use crate::sync::LockExt;
    use std::sync::{Mutex, Weak};
    type Slot = Option<((std::path::PathBuf, u32), Weak<kokoro::SharedCompute>)>;
    static SHARED: Mutex<Slot> = Mutex::new(None);
    let mut slot = SHARED.locked();
    if let Some((key, weak)) = slot.as_ref() {
        if key.0 == dir && key.1 == threads {
            if let Some(shared) = weak.upgrade() {
                return shared;
            }
        }
    }
    let shared = Arc::new(kokoro::SharedCompute::new(Arc::new(kokoro::onnx::OrtBackend::new(threads))));
    *slot = Some(((dir.to_path_buf(), threads), Arc::downgrade(&shared)));
    shared
}

/// Moteur souhaité par l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePreference {
    Auto,
    System,
}

impl EnginePreference {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(EnginePreference::Auto),
            // L'ancien `audio_cpp` est ramené à `auto` par `Config::normalised`.
            "system" | "systeme" | "système" => Some(EnginePreference::System),
            _ => None,
        }
    }
}

/// Réglages de chargement du moteur (Kokoro).
#[derive(Debug, Clone)]
pub struct EngineSettings {
    pub model_path: String,
    pub family: String,
    pub threads: u32,
    pub sessions: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(feature = "kokoro-onnx")]
    #[test]
    fn engines_alive_together_share_one_ort_compute() {
        let dir = std::path::Path::new("/nulle/part/kokoro-partage");
        let first = shared_ort(dir, 3);
        assert!(Arc::ptr_eq(&first, &shared_ort(dir, 3)), "deux calculs pour un même modèle");
        assert!(!Arc::ptr_eq(&first, &shared_ort(dir, 2)), "d'autres fils : un autre calcul");
        let weak = Arc::downgrade(&first);
        drop(first);
        assert!(weak.upgrade().is_none(), "le calcul survit à ses moteurs");
    }

    fn kokoro_settings(model_path: &str) -> EngineSettings {
        EngineSettings {
            model_path: model_path.into(),
            family: "kokoro".into(),
            threads: 4,
            sessions: 1,
        }
    }

    fn kokoro_files(name: &str, file: &str) -> std::path::PathBuf {
        let dir = crate::paths::test_scratch(&format!("build-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        std::fs::write(dir.join(file), b"x").unwrap();
        dir.join(file)
    }

    #[test]
    fn kokoro_without_its_files_falls_back_and_says_why() {
        let settings = kokoro_settings("/nulle/part/kokoro-v1_0.safetensors");
        let (engine, note) = build_engine_with(EnginePreference::Auto, &settings, &AppCompute::default());
        let note = note.unwrap();
        assert!(note.contains("Kokoro n'est pas installé"));
        // L'app ne garde que le moteur : il dit lui-même le repli.
        assert_eq!(engine.readiness(), Readiness::Fallback(note));
        assert!(engine.name().contains("(repli)"), "{}", engine.name());
    }

    #[test]
    #[cfg(not(all(feature = "mlx", target_os = "macos", target_arch = "aarch64")))]
    fn kokoro_mlx_needs_the_compute_of_the_app() {
        let path = kokoro_files("mlx", "kokoro-v1_0.safetensors");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), &AppCompute::default());
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    #[cfg(not(feature = "kokoro-onnx"))]
    fn kokoro_int8_needs_the_onnx_feature() {
        let path = kokoro_files("onnx", "model_quantized.onnx");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), &AppCompute::default());
        assert!(note.unwrap().contains("kokoro-onnx"));
    }

    #[test]
    #[cfg(feature = "kokoro-onnx")]
    fn kokoro_int8_loads_onnx_runtime_in_the_background() {
        // Aucune bibliothèque ORT sur la machine de test : le chargement
        // échouera en arrière-plan, mais rien n'est tranché d'avance.
        let path = kokoro_files("onnx-ort", "model_quantized.onnx");
        let (engine, note) =
            build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), &AppCompute::default());
        assert_eq!(note, None, "le calcul ORT part en arrière-plan");
        assert!(engine.splits_first_clause());
    }

    #[test]
    fn kokoro_with_its_compute_loads_in_the_background() {
        struct Idle;
        impl kokoro::KokoroBackend for Idle {
            fn label(&self) -> &'static str {
                "MLX"
            }
            fn load(&self, _: &std::path::Path) -> Result<()> {
                std::thread::sleep(Duration::from_millis(200));
                Ok(())
            }
            fn warm_up(&self) -> Result<()> {
                Ok(())
            }
            fn infer(&self, _: &[i64], _: &[f32], _: &Cancel) -> Result<kokoro::KokoroRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self) {}
        }
        let path = kokoro_files("charge", "kokoro-v1_0.safetensors");
        let (engine, note) =
            build_engine_with(
            EnginePreference::Auto,
            &kokoro_settings(&path.to_string_lossy()),
            &AppCompute { kokoro: Some(Arc::new(kokoro::SharedCompute::new(Arc::new(Idle)))), text: None },
        );
        assert_eq!(note, None, "rien à dire d'avance : le chargement part en arrière-plan");
        // Le nom dépend de l'issue du chargement (espeak-ng présent ou non
        // sur la machine de test) : seule la coupe, annoncée d'avance, est sûre.
        assert!(engine.splits_first_clause(), "annoncé avant la fin du chargement");
    }

    /// Un calcul Kokoro qui ne sert pas : seul compte qu'il soit choisi.
    struct Idle(&'static str);
    impl kokoro::KokoroBackend for Idle {
        fn label(&self) -> &'static str {
            self.0
        }
        fn load(&self, _: &std::path::Path) -> Result<()> {
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn infer(&self, _: &[i64], _: &[f32], _: &Cancel) -> Result<kokoro::KokoroRaw> {
            anyhow::bail!("non utilisé")
        }
        fn release(&self) {}
    }

    fn same_backend(a: &Arc<dyn kokoro::KokoroBackend>, b: &Arc<dyn kokoro::KokoroBackend>) -> bool {
        std::ptr::eq(Arc::as_ptr(a) as *const (), Arc::as_ptr(b) as *const ())
    }

    /// R3 : tant que l'app n'a pas basculé (tâche 6), le calcul qu'elle
    /// fournit sert, même dans un binaire qui calcule Kokoro MLX en Rust.
    #[test]
    fn the_compute_of_the_app_wins_over_rust_mlx() {
        let path = kokoro_files("r3", "kokoro-v1_0.safetensors");
        let settings = kokoro_settings(&path.to_string_lossy());
        let app: Arc<dyn kokoro::KokoroBackend> = Arc::new(Idle("Swift"));
        let shared = Arc::new(kokoro::SharedCompute::new(Arc::clone(&app)));
        let chosen = kokoro_backend(&settings, path.parent().unwrap(), Some(Arc::clone(&shared))).unwrap();
        assert!(same_backend(&chosen, &(shared as Arc<dyn kokoro::KokoroBackend>)), "le calcul de l'app, pas celui de Rust");
        assert_eq!(chosen.label(), "Swift");
    }

    #[test]
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    fn without_the_app_rust_computes_kokoro_mlx() {
        let path = kokoro_files("mlx-rust", "kokoro-v1_0.safetensors");
        let settings = kokoro_settings(&path.to_string_lossy());
        let first = kokoro_backend(&settings, path.parent().unwrap(), None).expect("MLX en Rust");
        assert_eq!(first.label(), "MLX");
        let again = kokoro_backend(&settings, path.parent().unwrap(), None).unwrap();
        assert!(same_backend(&first, &again), "un calcul pour le processus : les poids ne sont pas chargés deux fois");
        let (_, note) = build_engine_with(EnginePreference::Auto, &settings, &AppCompute::default());
        assert_eq!(note, None, "le chargement part en arrière-plan");
    }

    /// Les deux Kokoro installés sous `root`, comme `models::install_path`
    /// les range.
    fn two_kokoros(name: &str) -> std::path::PathBuf {
        let root = crate::paths::test_scratch(&format!("repli-{name}"));
        let dir = root.join("kokoro");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        for file in ["kokoro-v1_0.safetensors", "model_quantized.onnx"] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        root
    }

    fn mac_machine<'a>(
        loadable: &'a dyn Fn(crate::models::Runtime) -> bool,
        installed: &'a dyn Fn(&crate::models::ModelPackage) -> bool,
        path: &'a dyn Fn(&crate::models::ModelPackage) -> Option<std::path::PathBuf>,
    ) -> Machine<'a> {
        Machine {
            accelerators: vec![crate::compute::Accelerator::Mlx, crate::compute::Accelerator::OrtCpu],
            loadable,
            installed,
            path,
        }
    }

    #[test]
    fn a_failed_mlx_load_falls_back_to_installed_int8() {
        let root = two_kokoros("int8");
        let path = |p: &crate::models::ModelPackage| Some(root.join(p.family).join(p.file_name()));
        let machine = mac_machine(&|_| true, &|_| true, &path);
        let settings = kokoro_settings(&root.join("kokoro/kokoro-v1_0.safetensors").to_string_lossy());
        let mut tried = Vec::new();
        let (engine, note) = build_kokoro(&settings, &machine, |settings, _| {
            tried.push(settings.model_path.clone());
            if settings.model_path.ends_with(".onnx") {
                Ok(Arc::new(Idle("ORT")) as Arc<dyn kokoro::KokoroBackend>)
            } else {
                Err("MLX en panne".to_string())
            }
        });
        let note = note.expect("le repli se dit");
        assert!(note.contains("int8") && note.contains("MLX en panne"), "{note}");
        assert_eq!(tried.len(), 2, "{tried:?}");
        assert!(tried[1].ends_with("model_quantized.onnx"), "le moteur int8 : {tried:?}");
        assert!(engine.splits_first_clause(), "Kokoro int8 se charge, pas la voix du système");
        assert!(!matches!(engine.readiness(), Readiness::Fallback(_)));
    }

    #[test]
    fn the_failed_package_is_not_retried() {
        let root = two_kokoros("une-fois");
        let path = |p: &crate::models::ModelPackage| Some(root.join(p.family).join(p.file_name()));
        let machine = mac_machine(&|_| true, &|_| true, &path);
        let settings = kokoro_settings(&root.join("kokoro/kokoro-v1_0.safetensors").to_string_lossy());
        let mut tried = Vec::new();
        let (engine, note) = build_kokoro(&settings, &machine, |settings, _| {
            tried.push(settings.model_path.clone());
            Err(format!("échec {}", tried.len()))
        });
        let names: Vec<_> = tried.iter().map(|p| p.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(names, vec!["kokoro-v1_0.safetensors", "model_quantized.onnx"], "un seul essai par paquet");
        let note = note.unwrap();
        assert!(note.contains("échec 1"), "la raison du choix : {note}");
        assert_eq!(engine.readiness(), Readiness::Fallback(note), "puis la voix du système");
    }

    #[test]
    fn nothing_installed_to_fall_back_on_keeps_the_reason() {
        let root = two_kokoros("seul");
        let path = |p: &crate::models::ModelPackage| Some(root.join(p.family).join(p.file_name()));
        // Seul le MLX choisi est là : rien d'autre à essayer.
        let machine = mac_machine(&|_| true, &|p| p.id == "kokoro_82m_mlx", &path);
        let settings = kokoro_settings(&root.join("kokoro/kokoro-v1_0.safetensors").to_string_lossy());
        let mut tried = 0;
        let (engine, note) = build_kokoro(&settings, &machine, |_, _| {
            tried += 1;
            Err("MLX en panne".to_string())
        });
        assert_eq!(tried, 1);
        assert_eq!(note.as_deref(), Some("MLX en panne"));
        assert!(matches!(engine.readiness(), Readiness::Fallback(_)));
    }

    fn text_settings(model_path: &str, family: &str) -> EngineSettings {
        EngineSettings { model_path: model_path.into(), family: family.into(), threads: 4, sessions: 1 }
    }

    fn text_files(name: &str, file: &str) -> std::path::PathBuf {
        let dir = crate::paths::test_scratch(&format!("texte-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), b"x").unwrap();
        dir.join(file)
    }

    #[test]
    fn a_text_model_without_the_app_compute_falls_back_and_says_why() {
        let path = text_files("sans-calcul", "config.json");
        let (_, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings(&path.to_string_lossy(), "magpie_tts_357m_8bit"),
            &AppCompute::default(),
        );
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    fn a_text_model_without_its_files_falls_back_and_says_why() {
        struct Never;
        impl text_compute::TextCompute for Never {
            fn load(&self, _: &std::path::Path, _: &str) -> Result<()> {
                anyhow::bail!("non utilisé")
            }
            fn warm_up(&self, _: &std::path::Path) -> Result<()> {
                Ok(())
            }
            fn synthesize(
                &self,
                _: &std::path::Path,
                _: &str,
                _: &str,
                _: &str,
                _: &Cancel,
            ) -> Result<text_compute::SpeechRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self, _: &std::path::Path) {}
        }
        let compute = AppCompute { kokoro: None, text: Some(Arc::new(Never)) };
        let (engine, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings("/nulle/part/model.safetensors", "qwen3_tts_06b_base_8bit"),
            &compute,
        );
        let note = note.unwrap();
        assert!(note.contains("n'est pas installé"));
        // L'app jette la note : le moteur la porte lui-même.
        assert_eq!(engine.readiness(), Readiness::Fallback(note));
        assert!(engine.name().ends_with("(repli)"), "{}", engine.name());
        assert_eq!(engine.max_sessions(), system::SystemEngine::new().max_sessions(), "la voix du système parle");
    }

    /// L'ancien Magpie d'audio.cpp (famille `magpie_tts`, un `.gguf`) n'est
    /// pas le nouveau : il n'est toujours pas pris en charge.
    #[test]
    fn an_old_magpie_tts_config_is_not_the_new_magpie() {
        let (_, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings("/ailleurs/magpie.gguf", "magpie_tts"),
            &AppCompute::default(),
        );
        assert!(note.unwrap().contains("non pris en charge"));
    }

    #[test]
    fn a_text_model_loads_in_the_background_and_splits_its_first_clause() {
        struct Slow;
        impl text_compute::TextCompute for Slow {
            fn load(&self, _: &std::path::Path, _: &str) -> Result<()> {
                std::thread::sleep(Duration::from_millis(200));
                Ok(())
            }
            fn warm_up(&self, _: &std::path::Path) -> Result<()> {
                Ok(())
            }
            fn synthesize(
                &self,
                _: &std::path::Path,
                _: &str,
                _: &str,
                _: &str,
                _: &Cancel,
            ) -> Result<text_compute::SpeechRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self, _: &std::path::Path) {}
        }
        let path = text_files("charge", "model.safetensors");
        let compute = AppCompute { kokoro: None, text: Some(Arc::new(Slow)) };
        let (engine, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings(&path.to_string_lossy(), "qwen3_tts_06b_base_8bit"),
            &compute,
        );
        assert_eq!(note, None, "le chargement part en arrière-plan");
        assert!(engine.splits_first_clause());
    }
}
