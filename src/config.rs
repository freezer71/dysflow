//! Configuration persistante du lecteur.
//!
//! Le fichier est optionnel : tous les champs ont un défaut et `#[serde(default)]`
//! permet d'ouvrir une configuration écrite par une version antérieure sans la
//! casser. Les réglages de voix reprennent le vocabulaire d'audio.cpp, si bien
//! qu'ils atteignent réellement le modèle au lieu de rester décoratifs.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::doc::builder::CodePolicy;
use crate::tts::{EnginePreference, EngineSettings, TtsParams};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub engine: EngineConfig,
    pub voice: VoiceConfig,
    pub sampling: SamplingConfig,
    pub reading: ReadingConfig,
    pub app: AppConfig,
    /// Options transmises telles quelles au modèle TTS.
    ///
    /// Échappatoire pour tout paramètre propre à une famille que le lecteur ne
    /// connaît pas nommément.
    pub extra: BTreeMap<String, String>,
}

/// Réglages propres à l'app macOS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    /// Minutes pendant lesquelles la voix reste chargée après la dernière
    /// lecture. `-1` : toujours ; `0` : relâchée dès l'arrêt.
    pub keep_voice_minutes: i64,
    /// Migrations de voix déjà faites ou refusées (1 : le passage à Kokoro).
    pub voice_migration: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { keep_voice_minutes: 10, voice_migration: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig {
    /// `auto`, `audio_cpp` ou `system`.
    pub engine: String,
    /// Chemin du modèle (répertoire ou fichier GGUF).
    pub model_path: String,
    /// Famille audio.cpp, par exemple `supertonic` ou `pocket_tts`.
    pub family: String,
    /// Backend de calcul : `cpu`, `metal`, `cuda`, `vulkan`.
    pub compute: String,
    pub threads: u32,
    /// Sessions de synthèse simultanées ; `0` laisse la famille décider.
    pub sessions: u32,
    /// Nombre de phrases synthétisées en avance du curseur.
    pub prefetch: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        Self {
            engine: "auto".to_string(),
            model_path: String::new(),
            family: String::new(),
            compute: if cfg!(target_os = "macos") { "metal" } else { "cpu" }.to_string(),
            threads: cores.clamp(1, 8),
            // Ce que la famille demande : une session pour Supertonic comme
            // pour MagpieTTS. Voir `models::sessions_for_family`.
            sessions: 0,
            prefetch: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceConfig {
    /// Nom de voix, préréglage, ou chemin d'un WAV de référence.
    pub voice: String,
    /// `auto` : chaque document prend sa langue détectée. `fr`, `en`… : tous
    /// les documents parlent cette langue, sauf choix fait pour l'un d'eux.
    pub language: String,
    /// Vitesse de lecture. Appliquée au son par Sonic, sans changer la hauteur ;
    /// la synthèse, elle, se fait toujours à 1×.
    pub speed: f32,
    /// Décalage de hauteur, en demi-tons normalisés (-1.0 à 1.0).
    pub pitch: f32,
    pub energy: f32,
    /// Intonation demandée au modèle, si la famille la prend en charge.
    pub emotion: String,
    /// Consigne de style en langage naturel.
    pub instruction: String,
    pub volume: f32,
    /// Voix par langue de document, pour les familles qui en ont une par
    /// langue (Kokoro) : `fr = "p-fenrir60"`, `en = "af_bella"`.
    pub per_language: BTreeMap<String, String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice: String::new(),
            language: crate::text::language::AUTO.to_string(),
            speed: 1.0,
            pitch: 0.0,
            energy: 1.0,
            emotion: String::new(),
            instruction: String::new(),
            volume: 1.0,
            per_language: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SamplingConfig {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ReadingConfig {
    /// `announce`, `skip` ou `read`.
    pub code_blocks: String,
    /// Démarre la lecture dès l'ouverture.
    pub autoplay: bool,
    /// Pas des touches d'avance/recul rapide, en phrases.
    pub seek_step: u32,
    /// Largeur maximale de la colonne de texte, en caractères. 0 = plein écran.
    ///
    /// Plein écran par défaut : une colonne centrée laisse de larges marges
    /// vides sur un grand écran, et ce vide gêne plus qu'il n'aide. Réduire la
    /// largeur reste possible pour qui préfère une ligne courte.
    pub line_width: u32,
    /// Lignes vides insérées entre deux lignes de texte.
    pub line_spacing: u32,
    /// Souligne d'un fond la ligne où se trouve le mot prononcé.
    pub reading_ruler: bool,
}

impl Default for ReadingConfig {
    fn default() -> Self {
        Self {
            code_blocks: "announce".to_string(),
            autoplay: true,
            seek_step: 5,
            line_width: 0,
            line_spacing: 1,
            reading_ruler: true,
        }
    }
}

impl Config {
    /// Emplacement du fichier de configuration.
    pub fn path() -> Option<PathBuf> {
        crate::paths::config_dir().map(|d| d.join("config.toml"))
    }

    /// Charge la configuration, ou renvoie les défauts.
    ///
    /// Un fichier illisible ou invalide est signalé plutôt qu'ignoré : une
    /// coquille de syntaxe ne doit pas se traduire par des réglages
    /// silencieusement absents.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        if !path.exists() {
            return (Self::default(), None);
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => (config.normalised(), None),
                Err(err) => (
                    Self::default(),
                    Some(format!("configuration ignorée ({}) : {err}", path.display())),
                ),
            },
            Err(err) => (
                Self::default(),
                Some(format!("configuration illisible ({}) : {err}", path.display())),
            ),
        }
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path().context("répertoire de configuration introuvable")?;
        self.save_to(&path)?;
        Ok(path)
    }

    pub(crate) fn save_to(&self, path: &std::path::Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("sérialisation de la configuration")?;
        crate::paths::write_atomic(path, text.as_bytes())
            .with_context(|| format!("écriture de {}", path.display()))?;
        Ok(())
    }

    /// Ramène chaque réglage dans les bornes que l'écran lui donne.
    ///
    /// Les flèches bornent ce qu'elles touchent ; un fichier écrit à la main
    /// ne l'est pas. `line_width = 5` donnait un écran vide sans un mot,
    /// `sessions = 200` ouvrait deux cents sessions de 460 Mo, et un `nan`
    /// passait tel quel au moteur. Une valeur hors bornes est ramenée à la
    /// borne, un nombre sans valeur au défaut.
    pub fn normalised(mut self) -> Self {
        let defaults = Config::default();
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        self.voice.speed = finite(self.voice.speed, defaults.voice.speed).clamp(0.5, 3.0);
        self.voice.pitch = finite(self.voice.pitch, defaults.voice.pitch).clamp(-1.0, 1.0);
        self.voice.energy = finite(self.voice.energy, defaults.voice.energy).clamp(0.0, 2.0);
        self.voice.volume = finite(self.voice.volume, defaults.voice.volume).clamp(0.0, 2.0);
        self.engine.threads = self.engine.threads.clamp(1, 64);
        self.engine.sessions = self.engine.sessions.min(16);
        self.engine.prefetch = self.engine.prefetch.clamp(2, 64);
        // L'échantillonnage suit les bornes de l'écran ; un nombre sans
        // valeur rend la main au modèle plutôt que d'atteindre le moteur.
        let bounded = |value: Option<f32>, low: f32, high: f32| {
            value.filter(|v| v.is_finite()).map(|v| v.clamp(low, high))
        };
        self.sampling.temperature = bounded(self.sampling.temperature, 0.0, 2.0);
        self.sampling.top_p = bounded(self.sampling.top_p, 0.0, 1.0);
        self.sampling.guidance_scale = bounded(self.sampling.guidance_scale, 0.0, 20.0);
        self.sampling.repetition_penalty = bounded(self.sampling.repetition_penalty, 0.5, 2.0);
        self.sampling.top_k = self.sampling.top_k.map(|v| v.clamp(1, 200));
        self.sampling.inference_steps = self.sampling.inference_steps.map(|v| v.clamp(1, 200));
        self.reading.seek_step = self.reading.seek_step.clamp(1, 50);
        // Zéro reste « plein écran » ; une largeur posée se lit de 40 à 120.
        if self.reading.line_width != 0 {
            self.reading.line_width = self.reading.line_width.clamp(40, 120);
        }
        self.reading.line_spacing = self.reading.line_spacing.min(3);
        self
    }

    /// La langue des documents quand rien d'autre ne décide : celle de la
    /// configuration, le français pour `auto`.
    pub fn default_language(&self) -> String {
        crate::text::language::resolve(&self.voice.language, None, None)
    }

    /// Paramètres de synthèse dans la langue par défaut.
    pub fn tts_params(&self) -> TtsParams {
        self.tts_params_in(&self.default_language())
    }

    /// Paramètres de synthèse pour un document dans `language`.
    pub fn tts_params_in(&self, language: &str) -> TtsParams {
        let family = &self.engine.family;
        let voice = if crate::models::voices_follow_language(family) {
            let code = crate::text::language::Lang::parse(language).map_or(language, |l| l.code());
            let configured = self.voice.per_language.get(code).map(String::as_str);
            crate::models::voice_for_language(family, code, configured).to_string()
        } else {
            // Une voix que la famille ne connaît pas ne doit jamais atteindre
            // le moteur : elle y provoque une erreur illisible. Un chemin de
            // WAV sous `~` est développé : le moteur ne le ferait pas.
            expand_tilde(&crate::models::sanitised_voice(family, &self.voice.voice))
        };
        TtsParams {
            voice,
            language: language.to_string(),
            pitch_shift: self.voice.pitch.clamp(-1.0, 1.0),
            energy: self.voice.energy.clamp(0.0, 2.0),
            emotion: self.voice.emotion.clone(),
            instruction: self.voice.instruction.clone(),
            temperature: self.sampling.temperature,
            top_p: self.sampling.top_p,
            top_k: self.sampling.top_k,
            guidance_scale: self.sampling.guidance_scale,
            repetition_penalty: self.sampling.repetition_penalty,
            inference_steps: self.sampling.inference_steps,
            seed: self.sampling.seed,
            extra: self.extra.clone(),
        }
    }

    /// Fait suivre la voix quand la famille change.
    ///
    /// Les préréglages sont propres à chaque modèle : garder celui du
    /// précédent afficherait un nom que le nouveau ne saurait pas honorer.
    /// Kokoro choisit sa voix par langue : `voice.voice` reste alors celle de
    /// l'ancien modèle, pour qui voudra y revenir, et seules les langues sans
    /// voix reçoivent la leur. Renvoie vrai si quelque chose a changé.
    pub fn align_voice_with_family(&mut self) -> bool {
        let family = self.engine.family.clone();
        if crate::models::voices_follow_language(&family) {
            let mut changed = false;
            for language in ["fr", "en"] {
                if !self.voice.per_language.contains_key(language) {
                    let voice = crate::models::default_voice_for_language(&family, language);
                    self.voice.per_language.insert(language.to_string(), voice.to_string());
                    changed = true;
                }
            }
            return changed;
        }
        if crate::models::knows_voice(&family, &self.voice.voice) {
            return false;
        }
        self.voice.voice = crate::models::default_voice(&family).to_string();
        true
    }

    pub fn engine_preference(&self) -> EnginePreference {
        EnginePreference::parse(&self.engine.engine).unwrap_or(EnginePreference::Auto)
    }

    pub fn engine_settings(&self) -> EngineSettings {
        EngineSettings {
            model_path: expand_tilde(&self.engine.model_path),
            family: self.engine.family.clone(),
            backend: self.engine.compute.clone(),
            threads: self.engine.threads.max(1),
            sessions: self.sessions(),
        }
    }

    /// Sessions de synthèse réellement ouvertes.
    ///
    /// Un nombre posé à la main l'emporte ; `0` s'en remet à la famille.
    pub fn sessions(&self) -> u32 {
        match self.engine.sessions {
            0 => crate::models::sessions_for_family(&self.engine.family),
            n => n,
        }
    }

    pub fn code_policy(&self) -> CodePolicy {
        CodePolicy::parse(&self.reading.code_blocks).unwrap_or(CodePolicy::Announce)
    }
}

/// Développe un `~` initial : un chemin littéral `~/...` n'existe pas.
pub fn expand_tilde(path: &str) -> String {
    expand_tilde_with(path, dirs::home_dir().as_deref())
}

/// `expand_tilde` avec un dossier personnel explicite (les tests n'ont pas à
/// dépendre de celui de la machine).
pub fn expand_tilde_with(path: &str, home: Option<&std::path::Path>) -> String {
    let Some(rest) = path.strip_prefix('~') else {
        return path.to_string();
    };
    let Some(home) = home else {
        return path.to_string();
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home.to_string_lossy().into_owned()
    } else {
        home.join(rest).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn partial_config_keeps_defaults_for_missing_fields() {
        let parsed: Config = toml::from_str("[voice]\nspeed = 1.5\n").unwrap();
        assert_eq!(parsed.voice.speed, 1.5);
        assert_eq!(parsed.voice.language, "auto");
        assert_eq!(parsed.reading.code_blocks, "announce");
    }

    #[test]
    fn sessions_follow_the_family_when_left_to_auto() {
        // Supertonic synthétise à vingt fois le temps réel : une session
        // suffit, et chacune de plus coûte 460 Mo — deux fois le modèle.
        let mut config = Config::default();
        assert_eq!(config.engine.sessions, 0, "le défaut est « auto »");
        config.engine.family = "supertonic".to_string();
        assert_eq!(config.sessions(), 1);
        // MagpieTTS sature le GPU à lui seul : une session, sinon la phrase
        // attendue arrive après les autres.
        config.engine.family = "magpie_tts".to_string();
        assert_eq!(config.sessions(), 1);
        // Un nombre posé à la main l'emporte, quelle que soit la famille.
        config.engine.sessions = 3;
        config.engine.family = "supertonic".to_string();
        assert_eq!(config.sessions(), 3);
        assert_eq!(config.engine_settings().sessions, 3);
    }

    #[test]
    fn every_voice_setting_reaches_the_tts_params() {
        let mut config = Config::default();
        config.voice.emotion = "calme".into();
        config.voice.instruction = "ton posé".into();
        config.voice.pitch = 0.3;
        config.voice.energy = 1.2;
        config.sampling.temperature = Some(0.7);
        config.sampling.seed = Some(42);
        config.extra.insert("cfg_scale".into(), "3".into());

        let params = config.tts_params();
        assert_eq!(params.emotion, "calme");
        assert_eq!(params.instruction, "ton posé");
        assert_eq!(params.pitch_shift, 0.3);
        assert_eq!(params.energy, 1.2);

        // Famille non répertoriée : rien n'est filtré, tout passe.
        let options = params.model_options("inconnue");
        assert_eq!(options.get("temperature").map(String::as_str), Some("0.7"));
        assert_eq!(options.get("seed").map(String::as_str), Some("42"));
        assert_eq!(options.get("cfg_scale").map(String::as_str), Some("3"));
        assert_eq!(options.get("instruction").map(String::as_str), Some("ton posé"));

        // MagpieTTS valide sa requête et refuse toute option étrangère : il ne
        // doit recevoir que ce qu'il sait lire.
        let magpie = params.model_options("magpie_tts");
        assert_eq!(magpie.get("seed").map(String::as_str), Some("42"));
        assert_eq!(magpie.get("temperature").map(String::as_str), Some("0.7"));
        assert!(!magpie.contains_key("speaking_rate"), "{magpie:?}");
        assert!(!magpie.contains_key("instruction"), "{magpie:?}");
        assert!(!magpie.contains_key("cfg_scale"), "{magpie:?}");

        // Le débit n'est plus une option du modèle : la vitesse s'applique au son.
        let supertonic = params.model_options("supertonic");
        assert!(!supertonic.contains_key("speaking_rate"));
    }

    #[test]
    fn the_voice_follows_the_family_it_belongs_to() {
        let mut config = Config::default();
        config.engine.family = "supertonic".into();
        config.voice.voice = "F1".into();
        // Tant qu'on reste chez Supertonic, rien ne bouge.
        assert!(!config.align_voice_with_family());

        // En changeant de modèle, le préréglage n'a plus cours.
        config.engine.family = "magpie_tts".into();
        assert!(config.align_voice_with_family());
        assert_eq!(config.voice.voice, "Aria");
        // Et il ne se réaligne pas deux fois.
        assert!(!config.align_voice_with_family());
    }

    #[test]
    fn a_stale_voice_never_reaches_the_engine() {
        // Défense de dernier recours, si la configuration n'a pas été alignée.
        let mut config = Config::default();
        config.engine.family = "magpie_tts".into();
        config.voice.voice = "F1".into();
        assert_eq!(config.tts_params().voice, "");
    }

    #[test]
    fn the_voice_is_posed_by_the_session_not_by_the_parameters() {
        // Son orthographe dépend du paquet chargé, pas de la famille : c'est la
        // session qui la pose, une fois qu'elle sait laquelle passe.
        let mut config = Config::default();
        config.engine.family = "magpie_tts".into();
        config.voice.voice = "Sofia".into();
        let options = config.tts_params().model_options("magpie_tts");
        assert!(!options.contains_key("voice_id"));
        assert!(!options.contains_key("speaker"));
        assert_eq!(config.tts_params().voice, "Sofia");
        // Les deux orthographes connues restent proposées à la session.
        assert_eq!(
            crate::models::voice_option_keys("magpie_tts"),
            ["voice_id", "speaker"]
        );
        assert!(crate::models::voice_option_keys("supertonic").is_empty());
    }

    #[test]
    fn the_reading_speed_never_reaches_the_model() {
        // La vitesse s'applique au son, à la lecture : passée au modèle, elle
        // compressait la durée prévue de la phrase et lui faisait avaler des
        // mots. La synthèse reste à 1×, et le cache sert à toutes les vitesses.
        let mut slow = Config::default();
        slow.voice.speed = 1.0;
        let mut fast = slow.clone();
        fast.voice.speed = 2.5;
        assert_eq!(fast.tts_params(), slow.tts_params(), "la vitesse ne change pas la requête");
        for family in ["supertonic", "magpie_tts", "inconnue"] {
            let options = fast.tts_params().model_options(family);
            assert!(!options.contains_key("speaking_rate"), "{family} : {options:?}");
            assert!(!options.contains_key("speed"), "{family} : {options:?}");
        }
    }

    #[test]
    fn tilde_is_expanded() {
        let expanded = expand_tilde("~/modeles/tts.gguf");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("modeles/tts.gguf"));
        // Un chemin absolu reste intact.
        assert_eq!(expand_tilde("/opt/x"), "/opt/x");
    }

    #[test]
    fn invalid_config_is_reported_not_swallowed() {
        let err = toml::from_str::<Config>("[voice]\nspeed = \"vite\"\n");
        assert!(err.is_err());
    }

    #[test]
    fn legacy_window_settings_are_ignored_and_not_saved() {
        let parsed: Config = toml::from_str(
            "[voice]\nspeed = 1.5\n[gui]\nfont = \"atkinson\"\nfont_size = 22.0\n",
        ).unwrap();
        assert_eq!(parsed.voice.speed, 1.5);
        let saved = toml::to_string(&parsed).unwrap();
        assert!(!saved.contains("[gui]"));
        assert!(!saved.contains("font_size"));
        assert_eq!(toml::from_str::<Config>(&saved).unwrap(), parsed);
    }


    #[test]
    fn saving_leaves_no_temporary_file_next_to_the_config() {
        let dir = std::env::temp_dir().join(format!("sillon-cfg-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");
        Config::default().save_to(&path).expect("écriture");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["config.toml"]);
        std::fs::remove_dir_all(&dir).ok();
    }


    #[test]
    fn a_config_read_from_disk_is_brought_back_within_bounds() {
        // Les flèches bornent chaque réglage ; un fichier écrit à la main ne
        // l'est pas. `line_width = 5` donnait un écran vide sans un mot.
        let parsed: Config = toml::from_str(
            "[reading]\nline_width = 5\nline_spacing = 500\nseek_step = 0\n\
             [engine]\nsessions = 200\nprefetch = 0\nthreads = 0\n\
             [voice]\nspeed = nan\nvolume = 9.0\npitch = -4.0\n",
        )
        .unwrap();
        let c = parsed.normalised();
        assert_eq!(c.reading.line_width, 40);
        assert_eq!(c.reading.line_spacing, 3);
        assert_eq!(c.reading.seek_step, 1);
        assert_eq!(c.engine.sessions, 16);
        assert_eq!(c.engine.prefetch, 2);
        assert_eq!(c.engine.threads, 1);
        assert_eq!(c.voice.speed, 1.0, "NaN redevient le défaut");
        assert_eq!(c.voice.volume, 2.0);
        assert_eq!(c.voice.pitch, -1.0);
        // Zéro reste « plein écran », et les valeurs sages ne bougent pas.
        assert_eq!(Config::default().normalised(), Config::default());
    }

    #[test]
    fn a_reference_wav_under_the_home_is_expanded() {
        let mut config = Config::default();
        config.voice.voice = "~/voix.wav".into();
        let voice = config.tts_params().voice;
        assert!(!voice.starts_with('~'), "{voice}");
        assert!(voice.ends_with("voix.wav"), "{voice}");
    }

    #[test]
    fn sampling_values_are_brought_back_within_the_screen_bounds() {
        let mut config = Config::default();
        config.sampling.temperature = Some(f32::NAN);
        config.sampling.top_p = Some(4.0);
        config.sampling.top_k = Some(900);
        config.sampling.guidance_scale = Some(-3.0);
        config.sampling.repetition_penalty = Some(9.0);
        config.sampling.inference_steps = Some(0);
        let c = config.normalised();
        assert_eq!(c.sampling.temperature, None, "NaN rend la main au modèle");
        assert_eq!(c.sampling.top_p, Some(1.0));
        assert_eq!(c.sampling.top_k, Some(200));
        assert_eq!(c.sampling.guidance_scale, Some(0.0));
        assert_eq!(c.sampling.repetition_penalty, Some(2.0));
        assert_eq!(c.sampling.inference_steps, Some(1));
    }

    #[test]
    fn auto_is_the_default_and_speaks_french_until_a_document_says_otherwise() {
        let config = Config::default();
        assert_eq!(config.voice.language, "auto");
        assert_eq!(config.default_language(), "fr");
        assert_eq!(config.tts_params().language, "fr");
        assert_eq!(config.tts_params_in("en").language, "en");
    }

    #[test]
    fn kokoro_takes_its_voice_from_the_document_language() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.per_language.insert("en".into(), "af_sky".into());
        config.voice.per_language.insert("fr".into(), "F1".into());
        assert_eq!(config.tts_params_in("en").voice, "af_sky");
        // Inconnue : la voix par défaut de la langue, sans réécrire la valeur.
        assert_eq!(config.tts_params_in("fr").voice, "p-fenrir60");
        assert_eq!(config.voice.per_language["fr"], "F1");
    }

    #[test]
    fn voices_per_language_round_trip_through_toml() {
        let mut config = Config::default();
        config.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        config.voice.per_language.insert("en".into(), "af_bella".into());
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("[voice.per_language]"), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn switching_to_kokoro_keeps_the_former_voice_for_a_way_back() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.voice = "F2".into();
        assert!(config.align_voice_with_family());
        assert_eq!(config.voice.voice, "F2", "la voix de Supertonic reste pour y revenir");
        assert_eq!(config.voice.per_language["fr"], "p-fenrir60");
        assert_eq!(config.voice.per_language["en"], "af_bella");
        assert!(!config.align_voice_with_family(), "rien à refaire");
    }
}
