//! Synthèse vocale : contrat commun, moteurs, et ordonnanceur parallèle.

pub mod lazy;
pub mod params;
pub mod fallback;
pub mod kokoro;
pub mod pool;
pub mod preview;
pub mod silent;
pub mod system;


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
        f.write_str("synthèse abandonnée")
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
    build_engine_with(preference, settings, None)
}

/// `build_engine`, avec le calcul MLX de Kokoro quand l'app le fournit.
///
/// `mlx` est un calcul partagé (`SharedCompute`) : l'app l'enveloppe une
/// fois et passe le même `Arc` à chaque construction, pour qu'un moteur
/// relâché après le chargement du suivant ne vide pas ses poids.
pub fn build_engine_with(
    preference: EnginePreference,
    settings: &EngineSettings,
    mlx: Option<Arc<kokoro::SharedCompute>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    if preference == EnginePreference::System {
        return (Arc::new(system::SystemEngine::new()), None);
    }
    if settings.family == "kokoro" {
        return build_kokoro(settings, mlx);
    }
    // Kokoro est le seul modèle : une autre famille (Supertonic, MagpieTTS,
    // laissés par une ancienne configuration) ne se charge plus.
    let note = if settings.model_path.trim().is_empty() {
        None
    } else {
        Some(format!(
            "modèle « {} » non pris en charge — voix du système (installez Kokoro)",
            if settings.family.is_empty() { settings.model_path.as_str() } else { settings.family.as_str() }
        ))
    };
    (Arc::new(system::SystemEngine::new()), note)
}

/// Kokoro : le calcul selon le fichier du paquet (`.onnx` : ORT ; sinon
/// MLX, fourni par l'app), espeak-ng cherché au chargement.
fn build_kokoro(
    settings: &EngineSettings,
    mlx: Option<Arc<kokoro::SharedCompute>>,
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
    mlx: Option<Arc<kokoro::SharedCompute>>,
) -> Result<Arc<dyn kokoro::KokoroBackend>, String> {
    if settings.model_path.ends_with(".onnx") {
        // ONNX Runtime sur processeur, `min(engine.threads, 4)` fils. Un
        // calcul propre à chaque construction : l'envelopper dans un
        // `SharedCompute` neuf garde le même contrat que MLX.
        #[cfg(feature = "kokoro-onnx")]
        return Ok(Arc::new(kokoro::SharedCompute::new(Arc::new(kokoro::onnx::OrtBackend::new(settings.threads)))));
        #[cfg(not(feature = "kokoro-onnx"))]
        return Err("Kokoro int8 : binaire compilé sans `kokoro-onnx`".to_string());
    }
    match mlx {
        Some(backend) => Ok(backend as Arc<dyn kokoro::KokoroBackend>),
        None => Err("Calcul Metal indisponible — Kokoro MLX ne tourne que dans l'app, sur la puce graphique".to_string()),
    }
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

    fn kokoro_settings(model_path: &str) -> EngineSettings {
        EngineSettings {
            model_path: model_path.into(),
            family: "kokoro".into(),
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
        let settings = kokoro_settings("/nulle/part/kokoro-v1_0.safetensors");
        let (_, note) = build_engine_with(EnginePreference::Auto, &settings, None);
        assert!(note.unwrap().contains("Kokoro n'est pas installé"));
    }

    #[test]
    fn kokoro_mlx_needs_the_compute_of_the_app() {
        let path = kokoro_files("mlx", "kokoro-v1_0.safetensors");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), None);
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    #[cfg(not(feature = "kokoro-onnx"))]
    fn kokoro_int8_needs_the_onnx_feature() {
        let path = kokoro_files("onnx", "model_quantized.onnx");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), None);
        assert!(note.unwrap().contains("kokoro-onnx"));
    }

    #[test]
    #[cfg(feature = "kokoro-onnx")]
    fn kokoro_int8_loads_onnx_runtime_in_the_background() {
        // Aucune bibliothèque ORT sur la machine de test : le chargement
        // échouera en arrière-plan, mais rien n'est tranché d'avance.
        let path = kokoro_files("onnx-ort", "model_quantized.onnx");
        let (engine, note) =
            build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy()), None);
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
            Some(Arc::new(kokoro::SharedCompute::new(Arc::new(Idle)))),
        );
        assert_eq!(note, None, "rien à dire d'avance : le chargement part en arrière-plan");
        // Le nom dépend de l'issue du chargement (espeak-ng présent ou non
        // sur la machine de test) : seule la coupe, annoncée d'avance, est sûre.
        assert!(engine.splits_first_clause(), "annoncé avant la fin du chargement");
    }
}
