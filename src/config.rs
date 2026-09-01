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
    /// Options transmises telles quelles au modèle TTS.
    ///
    /// Échappatoire pour tout paramètre propre à une famille que le lecteur ne
    /// connaît pas nommément.
    pub extra: BTreeMap<String, String>,
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
            // Ce que la famille demande : une session suffit à Supertonic,
            // MagpieTTS en veut plusieurs. Voir `models::sessions_for_family`.
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
    pub language: String,
    /// Débit par défaut. Appliqué à la synthèse, jamais par ré-échantillonnage.
    pub speed: f32,
    /// Décalage de hauteur, en demi-tons normalisés (-1.0 à 1.0).
    pub pitch: f32,
    pub energy: f32,
    /// Intonation demandée au modèle, si la famille la prend en charge.
    pub emotion: String,
    /// Consigne de style en langage naturel.
    pub instruction: String,
    pub volume: f32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice: String::new(),
            language: "fr".to_string(),
            speed: 1.0,
            pitch: 0.0,
            energy: 1.0,
            emotion: String::new(),
            instruction: String::new(),
            volume: 1.0,
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
        dirs::config_dir().map(|d| d.join("rs-reader").join("config.toml"))
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
                Ok(config) => (config, None),
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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("création de {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("sérialisation de la configuration")?;
        std::fs::write(&path, text).with_context(|| format!("écriture de {}", path.display()))?;
        Ok(path)
    }

    /// Paramètres de synthèse dérivés de la configuration.
    pub fn tts_params(&self) -> TtsParams {
        TtsParams {
            // Une voix que la famille ne connaît pas ne doit jamais atteindre
            // le moteur : elle y provoque une erreur illisible.
            voice: crate::models::sanitised_voice(&self.engine.family, &self.voice.voice),
            language: self.voice.language.clone(),
            speaking_rate: self.voice.speed.clamp(0.25, 4.0),
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
    /// Renvoie vrai si la voix a dû être remplacée.
    pub fn align_voice_with_family(&mut self) -> bool {
        if crate::models::knows_voice(&self.engine.family, &self.voice.voice) {
            return false;
        }
        self.voice.voice = crate::models::default_voice(&self.engine.family).to_string();
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
    let Some(rest) = path.strip_prefix('~') else {
        return path.to_string();
    };
    let Some(home) = dirs::home_dir() else {
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
        assert_eq!(parsed.voice.language, "fr");
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
        // MagpieTTS est plus lent que la parole : il lui en faut plusieurs.
        config.engine.family = "magpie_tts".to_string();
        assert!(config.sessions() >= 2);
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

        // Supertonic, lui, lit bien le débit.
        let supertonic = params.model_options("supertonic");
        assert!(supertonic.contains_key("speaking_rate"));
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
    fn speed_is_clamped_to_a_usable_range() {
        let mut config = Config::default();
        config.voice.speed = 99.0;
        assert_eq!(config.tts_params().speaking_rate, 4.0);
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
}
