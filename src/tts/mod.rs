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
pub fn build_engine_with(
    preference: EnginePreference,
    settings: &EngineSettings,
    compute: &AppCompute,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    if preference == EnginePreference::System {
        return (Arc::new(system::SystemEngine::new()), None);
    }
    if settings.family == "kokoro" {
        return build_kokoro(settings, compute.kokoro.clone());
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
    (Arc::new(system::SystemEngine::new()), note)
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
        return (
            fallback,
            Some(crate::tr!(
                en = "Metal compute unavailable — {} only runs in the app",
                fr = "Calcul Metal indisponible — {} ne tourne que dans l'app",
                package.display_name.get(),
            )),
        );
    };
    let main = std::path::Path::new(&settings.model_path);
    let dir = main.parent().filter(|_| main.is_file()).map(std::path::Path::to_path_buf);
    let Some(dir) = dir else {
        // L'app ne garde que le moteur : la raison doit y rester lisible.
        let note = crate::tr!(
            en = "{} is not installed — see the settings",
            fr = "{} n'est pas installé — voir les réglages",
            package.display_name.get(),
        );
        return (fallback::explained(fallback, note.clone()), Some(note));
    };
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn(package.display_name.get().to_string(), 1, fallback, move || {
        let engine = text_compute::TextComputeEngine::load(compute, package, &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    (Arc::new(engine), None)
}

/// Kokoro : le calcul selon le fichier du paquet (`.onnx` : ORT ; sinon
/// MLX, fourni par l'app), espeak-ng cherché au chargement.
fn build_kokoro(
    settings: &EngineSettings,
    mlx: Option<Arc<kokoro::SharedCompute>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let fallback = fallback::fallback_engine(settings);
    let Some(dir) = crate::models::kokoro_dir(&settings.model_path) else {
        return (
            fallback,
            Some(crate::tr!("Kokoro is not installed — see the Models tab", "Kokoro n'est pas installé — voir l'onglet Modèles").to_string()),
        );
    };
    let backend = match kokoro_backend(settings, &dir, mlx) {
        Ok(backend) => backend,
        Err(reason) => return (fallback, Some(reason)),
    };
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn("Kokoro".to_string(), 1, fallback, move || {
        let phonemizer = kokoro::phonemes::EspeakProcess::locate()?;
        let engine = kokoro::KokoroEngine::load(backend, Box::new(phonemizer), &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    (Arc::new(engine), None)
}

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
    match mlx {
        Some(backend) => Ok(backend as Arc<dyn kokoro::KokoroBackend>),
        None => Err(crate::tr!(
            "Metal compute unavailable — Kokoro MLX only runs in the app, on the GPU",
            "Calcul Metal indisponible — Kokoro MLX ne tourne que dans l'app, sur la puce graphique"
        )
        .to_string()),
    }
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
        let (_, note) = build_engine_with(EnginePreference::Auto, &settings, &AppCompute::default());
        assert!(note.unwrap().contains("Kokoro n'est pas installé"));
    }

    #[test]
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
