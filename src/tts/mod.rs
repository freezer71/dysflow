//! Synthèse vocale : contrat commun, moteurs, et ordonnanceur parallèle.

pub mod lazy;
pub mod fallback;
pub mod kokoro;
pub mod pool;
pub mod preview;
pub mod silent;
pub mod system;

#[cfg(feature = "audio-cpp")]
pub mod audio_cpp;

pub use pool::{SynthesisStats, Synthesizer};

pub use crate::doc::WordSpan;

/// Une phrase synthétisée, avec le temps de chacun de ses mots affichés
/// quand le moteur sait le dire.
#[derive(Debug, Clone)]
pub struct Synthesis {
    pub pcm: PcmBuffer,
    pub words: Option<Vec<WordSpan>>,
}

use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Paramètres d'une requête de synthèse.
///
/// Le découpage suit celui d'audio.cpp : d'un côté le conditionnement de style
/// (`StyleCondition`), de l'autre les options libres du modèle. Tout ce que le
/// moteur accepte est donc atteignable depuis la configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct TtsParams {
    pub voice: String,
    pub language: String,
    // Pas de débit ici : la synthèse se fait toujours à 1×, et la vitesse
    // s'applique au son pendant la lecture (`audio::tempo`, par Sonic, sans
    // changer la hauteur). Donnée au modèle, elle comprimait la durée prévue
    // de la phrase et lui faisait avaler mots et ponctuation.
    pub pitch_shift: f32,
    pub energy: f32,
    pub emotion: String,
    /// Consigne de style en langage naturel, pour les modèles qui la prennent.
    ///
    /// Aucune famille du catalogue ne la consulte : elle sert aux familles
    /// qu'on compilerait soi-même, et l'écran la marque inerte en attendant.
    pub instruction: String,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
    /// Options transmises telles quelles au modèle.
    pub extra: BTreeMap<String, String>,
}

impl Default for TtsParams {
    fn default() -> Self {
        Self {
            voice: String::new(),
            language: "fr".to_string(),
            pitch_shift: 0.0,
            energy: 1.0,
            emotion: String::new(),
            instruction: String::new(),
            temperature: None,
            top_p: None,
            top_k: None,
            guidance_scale: None,
            repetition_penalty: None,
            inference_steps: None,
            seed: None,
            extra: BTreeMap::new(),
        }
    }
}

impl TtsParams {
    /// Options nommées à passer au modèle, filtrées pour la famille visée.
    ///
    /// Le filtrage n'est pas cosmétique : certaines familles valident leur
    /// requête et refusent tout net une option qu'elles ne connaissent pas —
    /// MagpieTTS répond « unknown MagpieTTS request option: speaking_rate » et
    /// ne produit rien. Une famille non répertoriée reçoit tout, faute de
    /// savoir ce qu'elle accepte.
    #[cfg_attr(not(feature = "audio-cpp"), allow(dead_code))]
    pub fn model_options(&self, family: &str) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = self
            .extra
            .iter()
            .filter(|(key, _)| crate::models::honours(family, key))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut set = |key: &str, value: String| {
            if !crate::models::honours(family, key) {
                return;
            }
            out.entry(key.to_string()).or_insert(value);
        };
        if let Some(v) = self.temperature {
            set("temperature", v.to_string());
        }
        if let Some(v) = self.top_p {
            set("top_p", v.to_string());
        }
        if let Some(v) = self.top_k {
            set("top_k", v.to_string());
        }
        if let Some(v) = self.guidance_scale {
            set("guidance_scale", v.to_string());
        }
        if let Some(v) = self.repetition_penalty {
            set("repetition_penalty", v.to_string());
        }
        if let Some(v) = self.inference_steps {
            set("num_inference_steps", v.to_string());
        }
        if let Some(v) = self.seed {
            set("seed", v.to_string());
        }
        if !self.instruction.is_empty() {
            set("instruction", self.instruction.clone());
        }
        out
    }
}

/// Audio synthétisé, prêt pour la sortie.
#[derive(Debug, Clone)]
pub struct PcmBuffer {
    pub samples: Arc<[f32]>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Durée réelle du signal, mesurée sur les échantillons.
    ///
    /// C'est elle qui pilote le curseur de lecture, et non une estimation en
    /// mots par seconde.
    pub duration: Duration,
}

impl PcmBuffer {
    /// Le même son, privé de ses `offset` premières secondes.
    ///
    /// La coupe tombe sur une trame entière : couper entre deux canaux
    /// inverserait gauche et droite pour toute la suite.
    pub fn skip(&self, offset: Duration) -> PcmBuffer {
        let start = self.sample_index(offset);
        PcmBuffer::new(self.samples[start..].to_vec(), self.sample_rate, self.channels)
    }

    /// Indice du premier échantillon de la trame qui tombe à `offset`.
    pub fn sample_index(&self, offset: Duration) -> usize {
        let frames = (offset.as_secs_f64() * self.sample_rate as f64) as usize;
        (frames * self.channels as usize).min(self.samples.len())
    }

    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1);
        let sample_rate = sample_rate.max(1);
        let frames = samples.len() / channels as usize;
        let duration = Duration::from_secs_f64(frames as f64 / sample_rate as f64);
        Self {
            samples: samples.into(),
            sample_rate,
            channels,
            duration,
        }
    }


    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
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
/// La séparation moteur / session reflète audio.cpp, où les poids sont
/// partagés (`shared_ptr<const Assets>`) tandis que chaque session porte son
/// propre graphe. C'est ce qui autorise plusieurs synthèses en parallèle sur un
/// seul chargement du modèle.
pub trait TtsEngine: Send + Sync {
    fn name(&self) -> String;

    /// Ouvre une session, appelée une fois par thread de synthèse.
    fn open_session(&self) -> Result<Box<dyn TtsSession>>;

    /// Nombre de sessions simultanées raisonnable pour ce moteur.
    fn max_sessions(&self) -> usize;

    /// Vrai quand le moteur exige un WAV de référence pour parler.
    ///
    /// Certaines familles ne savent que cloner : sans référence elles refusent
    /// chaque phrase, une par une. Le savoir d'avance permet de le dire une
    /// fois, au bon endroit.
    fn requires_reference(&self) -> bool {
        false
    }

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
    build_engine_with(preference, settings, None)
}

/// `build_engine`, avec le calcul MLX de Kokoro quand l'app le fournit.
pub fn build_engine_with(
    preference: EnginePreference,
    settings: &EngineSettings,
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    if preference == EnginePreference::System {
        return (Arc::new(system::SystemEngine::new()), None);
    }
    if settings.family == "kokoro" {
        return build_kokoro(settings, mlx);
    }
    let fallback: Arc<dyn TtsEngine> = Arc::new(system::SystemEngine::new());
    if let Err(reason) = audio_cpp_precondition(settings) {
        let note = format!("audio.cpp indisponible ({reason}) — repli sur la voix système");
        return (fallback, Some(note));
    }

    let sessions = settings.sessions.max(1) as usize;
    let owned = settings.clone();
    let engine = lazy::LazyEngine::spawn("audio.cpp".to_string(), sessions, fallback, move || {
        try_audio_cpp(&owned)
    });
    (Arc::new(engine), None)
}

/// Kokoro : le calcul selon le fichier du paquet (`.onnx` : ORT ; sinon
/// MLX, fourni par l'app), espeak-ng cherché au chargement.
fn build_kokoro(
    settings: &EngineSettings,
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let fallback = fallback::fallback_engine(settings);
    let Some(dir) = crate::models::kokoro_dir(&settings.model_path) else {
        return (fallback, Some("Kokoro n'est pas installé — voir l'onglet Modèles".to_string()));
    };
    let backend = match kokoro_backend(settings, mlx) {
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
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> Result<Arc<dyn kokoro::KokoroBackend>, String> {
    if settings.model_path.ends_with(".onnx") {
        // Le calcul ONNX Runtime (`kokoro::onnx::OrtBackend`) arrive avec la
        // Tâche 10 ; d'ici là la feature, encore vide, ne charge rien.
        #[cfg(feature = "kokoro-onnx")]
        return Err("Kokoro int8 : calcul ONNX Runtime pas encore livré".to_string());
        #[cfg(not(feature = "kokoro-onnx"))]
        return Err("Kokoro int8 : binaire compilé sans `kokoro-onnx`".to_string());
    }
    match mlx {
        Some(backend) if settings.backend != "cpu" => Ok(backend),
        _ => Err("Calcul Metal indisponible — Kokoro MLX ne tourne que dans l'app, sur la puce graphique".to_string()),
    }
}

/// Vérifie ce qui peut l'être sans toucher au modèle.
#[cfg(feature = "audio-cpp")]
fn audio_cpp_precondition(settings: &EngineSettings) -> Result<(), String> {
    let path = settings.model_path.trim();
    if path.is_empty() {
        return Err("aucun modèle configuré — renseignez engine.model_path ou --model".into());
    }
    if !std::path::Path::new(path).exists() {
        return Err(format!("modèle introuvable : {path}"));
    }
    Ok(())
}

#[cfg(not(feature = "audio-cpp"))]
fn audio_cpp_precondition(_settings: &EngineSettings) -> Result<(), String> {
    Err("binaire compilé sans la feature `audio-cpp`".into())
}

#[cfg(feature = "audio-cpp")]
fn try_audio_cpp(settings: &EngineSettings) -> Result<Arc<dyn TtsEngine>> {
    let engine = audio_cpp::AudioCppEngine::load(settings)?;
    Ok(Arc::new(engine))
}

#[cfg(not(feature = "audio-cpp"))]
fn try_audio_cpp(_settings: &EngineSettings) -> Result<Arc<dyn TtsEngine>> {
    anyhow::bail!("binaire compilé sans la feature `audio-cpp`")
}

/// Moteur souhaité par l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePreference {
    Auto,
    AudioCpp,
    System,
}

impl EnginePreference {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(EnginePreference::Auto),
            "audio_cpp" | "audio-cpp" | "audiocpp" => Some(EnginePreference::AudioCpp),
            "system" | "systeme" | "système" => Some(EnginePreference::System),
            _ => None,
        }
    }
}

/// Réglages de chargement du moteur natif.
///
/// Seule la liaison audio.cpp les consulte ; sans cette feature ils restent
/// renseignés mais inertes.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "audio-cpp"), allow(dead_code))]
pub struct EngineSettings {
    pub model_path: String,
    pub family: String,
    pub backend: String,
    pub threads: u32,
    pub sessions: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipping_drops_whole_frames_from_the_start() {
        // Stéréo, 10 trames par seconde : 0,3 s = 3 trames = 6 échantillons.
        let pcm = PcmBuffer::new((0..20).map(|i| i as f32).collect(), 10, 2);
        let rest = pcm.skip(Duration::from_millis(300));
        assert_eq!(rest.samples.len(), 14);
        assert_eq!(rest.samples[0], 6.0);
        assert_eq!(rest.channels, 2);
    }

    #[test]
    fn skipping_past_the_end_leaves_silence() {
        let pcm = PcmBuffer::new(vec![0.5; 10], 10, 1);
        assert!(pcm.skip(Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn a_model_rate_written_by_hand_still_reaches_the_model() {
        // `extra` est la porte de sortie : un débit propre au modèle, écrit à
        // la main, part tel quel — il s'ajoute à la vitesse de lecture.
        let mut extra = BTreeMap::new();
        extra.insert("speaking_rate".to_string(), "0.8".to_string());
        let params = TtsParams {
            extra,
            ..TtsParams::default()
        };
        assert_eq!(
            params.model_options("supertonic").get("speaking_rate").map(String::as_str),
            Some("0.8")
        );
    }

    fn kokoro_settings(model_path: &str, backend: &str) -> EngineSettings {
        EngineSettings {
            model_path: model_path.into(),
            family: "kokoro".into(),
            backend: backend.into(),
            threads: 4,
            sessions: 1,
        }
    }

    fn kokoro_files(name: &str, file: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-build-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        std::fs::write(dir.join(file), b"x").unwrap();
        dir.join(file)
    }

    #[test]
    fn kokoro_without_its_files_falls_back_and_says_why() {
        let settings = kokoro_settings("/nulle/part/kokoro-v1_0.safetensors", "metal");
        let (_, note) = build_engine_with(EnginePreference::Auto, &settings, None);
        assert!(note.unwrap().contains("Kokoro n'est pas installé"));
    }

    #[test]
    fn kokoro_mlx_needs_the_compute_of_the_app() {
        let path = kokoro_files("mlx", "kokoro-v1_0.safetensors");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "metal"), None);
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    #[cfg(not(feature = "kokoro-onnx"))]
    fn kokoro_int8_needs_the_onnx_feature() {
        let path = kokoro_files("onnx", "model_quantized.onnx");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "cpu"), None);
        assert!(note.unwrap().contains("kokoro-onnx"));
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
            fn infer(&self, _: &[i64], _: &[f32]) -> Result<kokoro::KokoroRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self) {}
        }
        let path = kokoro_files("charge", "kokoro-v1_0.safetensors");
        let (engine, note) =
            build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "metal"), Some(Arc::new(Idle)));
        assert_eq!(note, None, "rien à dire d'avance : le chargement part en arrière-plan");
        // Le nom dépend de l'issue du chargement (espeak-ng présent ou non
        // sur la machine de test) : seule la coupe, annoncée d'avance, est sûre.
        assert!(engine.splits_first_clause(), "annoncé avant la fin du chargement");
    }
}
