//! Configuration persistante du lecteur.
//!
//! Le fichier est optionnel : tous les champs ont un défaut et `#[serde(default)]`
//! permet d'ouvrir une configuration écrite par une version antérieure sans la
//! casser.

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
    pub reading: ReadingConfig,
    pub app: AppConfig,
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
    /// La sortie audio, par son nom ; vide : celle du système, qui suit
    /// macOS. Choisie, le son n'en sort jamais : retirée, la lecture se met
    /// en pause au lieu de passer aux haut-parleurs.
    pub output_device: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { keep_voice_minutes: 10, voice_migration: 0, output_device: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig {
    /// `auto` ou `system` (`audio_cpp`, ancienne valeur, vaut `auto`).
    pub engine: String,
    /// Chemin du modèle (répertoire ou fichier GGUF).
    pub model_path: String,
    /// Famille du modèle : `kokoro`.
    pub family: String,
    /// Fils de calcul de Kokoro au terminal (ONNX Runtime, borné à 4).
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
            threads: cores.clamp(1, 8),
            // Ce que la famille demande : une session pour Kokoro. Voir
            // `models::sessions_for_family`.
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
    /// Décalage de hauteur de la voix du système (-1.0 à 1.0).
    pub pitch: f32,
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
            volume: 1.0,
            per_language: BTreeMap::new(),
        }
    }
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
        self.voice.volume = finite(self.voice.volume, defaults.voice.volume).clamp(0.0, 2.0);
        self.engine.threads = self.engine.threads.clamp(1, 64);
        self.engine.sessions = self.engine.sessions.min(16);
        self.engine.prefetch = self.engine.prefetch.clamp(2, 64);
        // audio.cpp n'est plus (14 oct. 2026) : son nom devient « auto », ce
        // qu'il valait déjà, et l'écran des réglages ne le montre plus.
        if matches!(self.engine.engine.to_ascii_lowercase().as_str(), "audio_cpp" | "audio-cpp" | "audiocpp") {
            self.engine.engine = "auto".to_string();
        }
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
            // Hors Kokoro, aucune famille ne nomme plus ses voix : la voix
            // configurée part telle quelle, sans filtre. Une config Magpie
            // écrite à la main avec un préréglage étranger (« F1 ») retombe
            // sur l'erreur « stoll: no conversion » du modèle : accepté, ce
            // modèle est hors catalogue. Un chemin de WAV sous `~` est
            // développé : le moteur ne le ferait pas.
            expand_tilde(self.voice.voice.trim())
        };
        TtsParams {
            voice,
            language: language.to_string(),
            pitch_shift: self.voice.pitch.clamp(-1.0, 1.0),
        }
    }

    /// Fait suivre la voix quand la famille change.
    ///
    /// Seul Kokoro nomme ses voix, une par langue : `voice.voice` reste celle
    /// de l'ancien modèle, pour qui voudra y revenir, et seules les langues
    /// sans voix reçoivent la leur. Une voix retirée du catalogue n'est pas
    /// réécrite : `tts_params_in` la remplace par la voix recommandée de sa
    /// langue (règle des valeurs non listées). Les autres familles ne nomment
    /// aucune voix : rien à aligner. Renvoie vrai si quelque chose a changé.
    pub fn align_voice_with_family(&mut self) -> bool {
        let family = self.engine.family.clone();
        if !crate::models::voices_follow_language(&family) {
            return false;
        }
        let mut changed = false;
        for language in ["fr", "en"] {
            if !self.voice.per_language.contains_key(language) {
                let voice = crate::models::default_voice_for_language(&family, language);
                self.voice.per_language.insert(language.to_string(), voice.to_string());
                changed = true;
            }
        }
        changed
    }

    pub fn engine_preference(&self) -> EnginePreference {
        EnginePreference::parse(&self.engine.engine).unwrap_or(EnginePreference::Auto)
    }

    pub fn engine_settings(&self) -> EngineSettings {
        EngineSettings {
            model_path: expand_tilde(&self.engine.model_path),
            family: self.engine.family.clone(),
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
    fn the_former_audio_cpp_engine_reads_as_auto() {
        let parsed: Config = toml::from_str("[engine]\nengine = \"audio_cpp\"\n").unwrap();
        assert_eq!(parsed.normalised().engine.engine, "auto");
        let system: Config = toml::from_str("[engine]\nengine = \"system\"\n").unwrap();
        assert_eq!(system.normalised().engine.engine, "system");
    }

    #[test]
    fn sessions_follow_the_family_when_left_to_auto() {
        // Kokoro calcule en série : une session.
        let mut config = Config::default();
        assert_eq!(config.engine.sessions, 0, "le défaut est « auto »");
        config.engine.family = "kokoro".to_string();
        assert_eq!(config.sessions(), 1);
        // Un nombre posé à la main l'emporte, quelle que soit la famille.
        config.engine.sessions = 3;
        assert_eq!(config.sessions(), 3);
        assert_eq!(config.engine_settings().sessions, 3);
    }

    #[test]
    fn the_pitch_reaches_the_tts_params() {
        let mut config = Config::default();
        config.voice.pitch = 0.3;
        assert_eq!(config.tts_params().pitch_shift, 0.3);
    }

    /// Hors Kokoro, plus aucune famille ne nomme ses voix : la voix
    /// configurée part telle quelle, sans filtre (l'erreur « stoll » d'une
    /// config Magpie écrite à la main avec « F1 » est acceptée, hors
    /// catalogue). Un `~` est développé.
    #[test]
    fn outside_kokoro_the_configured_voice_goes_unfiltered() {
        let mut config = Config::default();
        config.engine.family = "magpie_tts".into();
        config.voice.voice = " F1 ".into();
        assert_eq!(config.tts_params().voice, "F1");
        config.voice.voice = "~/voix.wav".into();
        let home = dirs::home_dir().map(|h| h.join("voix.wav").to_string_lossy().into_owned());
        assert_eq!(Some(config.tts_params().voice), home.or(Some("~/voix.wav".into())));
    }

    #[test]
    fn a_stale_voice_never_reaches_the_engine() {
        // Le préréglage d'un ancien modèle reste dans `voice.voice` : Kokoro
        // prend la voix de sa langue, jamais celui-là.
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.voice = "F1".into();
        assert_eq!(config.tts_params().voice, "p-fenrir60");
        // Une famille hors catalogue ne nomme aucune voix : rien à aligner.
        config.engine.family = "supertonic".into();
        assert!(!config.align_voice_with_family());
        assert_eq!(config.voice.voice, "F1");
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
        config.voice.per_language.insert("en".into(), "am_fenrir".into());
        config.voice.per_language.insert("fr".into(), "F1".into());
        assert_eq!(config.tts_params_in("en").voice, "am_fenrir");
        // Inconnue : la voix par défaut de la langue, sans réécrire la valeur.
        assert_eq!(config.tts_params_in("fr").voice, "p-fenrir60");
        assert_eq!(config.voice.per_language["fr"], "F1");
    }

    /// Les voix Kokoro retirées du catalogue le 14 oct. 2026 (`ff_siwis`,
    /// `bf_emma`…) parlent avec la voix recommandée de leur langue. Le
    /// fichier n'est pas réécrit, et le passage à Kokoro ne les remplace pas.
    #[test]
    fn a_retired_kokoro_voice_speaks_with_the_recommended_one() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.per_language.insert("fr".into(), "ff_siwis".into());
        config.voice.per_language.insert("en".into(), "bf_emma".into());
        assert!(!config.align_voice_with_family(), "rien à réécrire");
        assert_eq!(config.tts_params_in("fr").voice, "p-fenrir60");
        assert_eq!(config.tts_params_in("en").voice, "af_bella");
        assert_eq!(config.voice.per_language["en"], "bf_emma", "la valeur reste dans le fichier");
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
