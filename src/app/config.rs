//! La page des réglages du terminal : six réglages, sans rubriques, chacun
//! enregistré dès qu'il change.
//!
//! Le reste de `config.toml` (moteur, fils de calcul, blocs de code, langue,
//! hauteur…) ne s'affiche plus : il garde sa valeur, et se règle à la main
//! pour qui y tient.

use crate::config::{Config, Effect};
use crate::models;

/// Un réglage de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    VoiceFr,
    VoiceEn,
    Speed,
    LineWidth,
    LineSpacing,
    ReadingRuler,
}

/// Les réglages, dans l'ordre de la page.
pub const FIELDS: [Field; 6] = [
    Field::VoiceFr,
    Field::VoiceEn,
    Field::Speed,
    Field::LineWidth,
    Field::LineSpacing,
    Field::ReadingRuler,
];

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Field::VoiceFr => crate::tr!("French voice", "Voix française"),
            Field::VoiceEn => crate::tr!("English voice", "Voix anglaise"),
            Field::Speed => crate::tr!("Speed", "Vitesse"),
            Field::LineWidth => crate::tr!("Column width", "Largeur de colonne"),
            Field::LineSpacing => crate::tr!("Line spacing", "Interlignage"),
            Field::ReadingRuler => crate::tr!("Reading ruler", "Règle de lecture"),
        }
    }

    /// La langue d'une ligne de voix.
    pub fn language(&self) -> Option<&'static str> {
        match self {
            Field::VoiceFr => Some("fr"),
            Field::VoiceEn => Some("en"),
            _ => None,
        }
    }

    /// L'explication affichée sous la page, pour la ligne choisie.
    pub fn hint(&self, config: &Config) -> &'static str {
        match self {
            Field::VoiceFr | Field::VoiceEn if models::voices_follow_language(&config.engine.family) => {
                let language = self.language().expect("une ligne de voix a sa langue");
                let choices = models::voices_for_family(&config.engine.family)
                    .iter()
                    .filter(|v| v.language == language)
                    .count();
                if choices > 1 {
                    crate::tr!(
                        "← → to change voice, t to listen. Each document speaks with the voice of its language.",
                        "← → pour changer de voix, t pour l'écouter. Chaque document parle avec la voix de sa langue."
                    )
                } else {
                    crate::tr!(
                        "t to listen. Each document speaks with the voice of its language.",
                        "t pour l'écouter. Chaque document parle avec la voix de sa langue."
                    )
                }
            }
            Field::VoiceFr | Field::VoiceEn => crate::tr!(
                "The system voice reads until Kokoro is installed.",
                "La voix du système lit tant que Kokoro n'est pas installé."
            ),
            Field::Speed => crate::tr!(
                "Applied to the sound, without resynthesis: the voice speeds up without going higher. \
                 t to hear it.",
                "Appliquée au son, sans resynthèse : la voix accélère sans monter dans les aigus. \
                 t pour l'entendre."
            ),
            Field::LineWidth => crate::tr!(
                "A line too wide makes you lose the start of the next one.",
                "Une ligne trop large fait perdre le début de la suivante."
            ),
            Field::LineSpacing => crate::tr!("Blank lines between two lines of text.", "Lignes vides entre deux lignes de texte."),
            Field::ReadingRuler => crate::tr!(
                "Shades the line being read.",
                "Souligne d'un fond la ligne en cours de lecture."
            ),
        }
    }

    /// Valeur affichée.
    pub fn value(&self, config: &Config) -> String {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                if !models::voices_follow_language(&config.engine.family) {
                    return crate::tr!("system voice", "voix du système").to_string();
                }
                let language = self.language().expect("une ligne de voix a sa langue");
                let id = config.tts_params_in(language).voice;
                models::voices_for_family(&config.engine.family)
                    .iter()
                    .find(|v| v.id == id)
                    .map_or_else(|| id.clone(), |v| v.label.get().to_string())
            }
            Field::Speed => format!("{:.2}×", config.voice.speed),
            Field::LineWidth => match config.reading.line_width {
                0 => crate::tr!("full screen", "plein écran").to_string(),
                n => crate::tr!(en = "{n} characters", fr = "{n} caractères"),
            },
            Field::LineSpacing => match config.reading.line_spacing {
                0 => crate::tr!("none", "aucun").to_string(),
                1 => crate::tr!("one line", "une ligne").to_string(),
                n => crate::tr!(en = "{n} lines", fr = "{n} lignes"),
            },
            Field::ReadingRuler => if config.reading.reading_ruler {
                crate::tr!("on", "oui")
            } else {
                crate::tr!("off", "non")
            }
            .to_string(),
        }
    }

    /// Applique un pas de réglage. Renvoie ce qui doit être répercuté.
    pub fn adjust(&self, config: &mut Config, delta: i32) -> Effect {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                let language = self.language().expect("une ligne de voix a sa langue");
                let family = config.engine.family.clone();
                if !models::voices_follow_language(&family) {
                    return Effect::None;
                }
                let own: Vec<&models::VoiceOption> =
                    models::voices_for_family(&family).iter().filter(|v| v.language == language).collect();
                if own.is_empty() {
                    return Effect::None;
                }
                let current = config.tts_params_in(language).voice;
                let index = own.iter().position(|v| v.id == current).unwrap_or(0) as i32;
                let next = own[(index + delta).rem_euclid(own.len() as i32) as usize];
                // Une seule voix dans la langue (le français) : rien ne change,
                // rien à écrire ni à resynthétiser.
                if next.id == current {
                    return Effect::None;
                }
                config.voice.per_language.insert(language.to_string(), next.id.to_string());
                Effect::Resynthesize
            }
            Field::Speed => {
                config.voice.speed = (config.voice.speed + 0.05 * delta as f32).clamp(0.5, 3.0);
                Effect::Speed
            }
            // Le plein écran, noté zéro, prolonge la course vers le haut : il
            // est la largeur la plus grande, pas une valeur à part.
            Field::LineWidth => {
                const MIN: u32 = 40;
                const MAX: u32 = 120;
                const STEP: u32 = 4;
                config.reading.line_width = match (config.reading.line_width, delta) {
                    (0, d) if d < 0 => MAX,
                    (0, _) => 0,
                    (w, d) if d > 0 && w >= MAX => 0,
                    (w, d) => (w.saturating_add_signed(d * STEP as i32)).clamp(MIN, MAX),
                };
                Effect::None
            }
            Field::LineSpacing => {
                config.reading.line_spacing = config.reading.line_spacing.saturating_add_signed(delta).min(3);
                Effect::None
            }
            Field::ReadingRuler => {
                config.reading.reading_ruler = !config.reading.reading_ruler;
                Effect::None
            }
        }
    }

    /// Recopie la valeur de ce réglage de `from` dans `to`, et rien d'autre :
    /// l'enregistrement automatique n'écrit que ce que la page vient de
    /// changer, jamais une option de la ligne de commande (`--speed`…).
    pub fn copy_value(&self, from: &Config, to: &mut Config) {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                let language = self.language().expect("une ligne de voix a sa langue");
                if let Some(voice) = from.voice.per_language.get(language) {
                    to.voice.per_language.insert(language.to_string(), voice.clone());
                }
            }
            Field::Speed => to.voice.speed = from.voice.speed,
            Field::LineWidth => to.reading.line_width = from.reading.line_width,
            Field::LineSpacing => to.reading.line_spacing = from.reading.line_spacing,
            Field::ReadingRuler => to.reading.reading_ruler = from.reading.reading_ruler,
        }
    }
}

/// État de la page des réglages.
pub struct ConfigScreen {
    /// Ligne choisie, rang dans `FIELDS`.
    pub row: usize,
    /// Téléchargement de la voix recommandée, lancé seul au premier usage.
    pub download: Option<models::Download>,
    /// Pourquoi la voix recommandée n'a pas pu s'installer : dit jusqu'au
    /// prochain lancement, qui réessaie.
    pub install_error: Option<String>,
    /// Dernier message de la page (aperçu refusé…).
    pub notice: String,
}

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigScreen {
    pub fn new() -> Self {
        Self { row: 0, download: None, install_error: None, notice: String::new() }
    }

    pub fn current_field(&self) -> Field {
        FIELDS[self.row.min(FIELDS.len() - 1)]
    }

    /// Place le curseur sur `field`.
    pub fn select(&mut self, field: Field) {
        self.row = FIELDS.iter().position(|f| *f == field).unwrap_or(0);
    }

    pub fn move_row(&mut self, delta: i32) {
        self.row = (self.row as i32 + delta).clamp(0, FIELDS.len() as i32 - 1) as usize;
    }

    /// Consulte l'avancement du téléchargement et range ce qui est terminé.
    ///
    /// Renvoie le paquet fraîchement installé, pour que l'appelant le
    /// sélectionne.
    pub fn poll_download(&mut self) -> Option<&'static models::ModelPackage> {
        let download = self.download.as_ref()?;
        if !download.is_finished() {
            return None;
        }
        let package = download.package;
        let outcome = download.progress();
        self.download = None;
        let reason = match outcome {
            models::Progress::Done => {
                self.install_error = None;
                return Some(package);
            }
            models::Progress::Running { .. } => return None,
            models::Progress::Failed(err) => err,
            models::Progress::Cancelled => crate::tr!("download interrupted", "téléchargement interrompu").to_string(),
        };
        self.install_error = Some(crate::tr!(
            en = "Voice {} not installed: {reason} — will retry at next launch",
            fr = "Voix {} non installée : {reason} — nouvel essai au prochain lancement",
            package.display_name.get(),
        ));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_holds_the_six_settings_of_the_spec() {
        let labels: Vec<&str> = FIELDS.iter().map(|f| f.label()).collect();
        assert_eq!(
            labels,
            ["Voix française", "Voix anglaise", "Vitesse", "Largeur de colonne", "Interlignage", "Règle de lecture"]
        );
    }

    #[test]
    fn the_cursor_stays_on_the_page() {
        let mut screen = ConfigScreen::new();
        screen.move_row(-5);
        assert_eq!(screen.current_field(), Field::VoiceFr);
        screen.move_row(99);
        assert_eq!(screen.current_field(), Field::ReadingRuler);
        screen.select(Field::Speed);
        assert_eq!(screen.current_field(), Field::Speed);
    }

    fn kokoro() -> Config {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config
    }

    #[test]
    fn the_english_voice_cycles_through_the_english_voices() {
        let mut config = kokoro();
        assert_eq!(Field::VoiceEn.value(&config), "af_bella · américaine, femme, recommandée");
        assert_eq!(Field::VoiceEn.adjust(&mut config, 1), Effect::Resynthesize);
        assert_eq!(config.voice.per_language["en"], "af_heart");
        Field::VoiceEn.adjust(&mut config, -2);
        assert_eq!(config.voice.per_language["en"], "bm_lewis", "la liste boucle");
        assert!(!config.voice.per_language.contains_key("fr"), "l'autre langue ne bouge pas");
    }

    #[test]
    fn the_change_hint_is_for_languages_with_a_choice() {
        let config = kokoro();
        assert!(Field::VoiceEn.hint(&config).contains("← →"));
        assert!(Field::VoiceFr.hint(&config).contains("← →"), "deux voix françaises");
        assert!(Field::VoiceFr.hint(&config).contains("t pour l'écouter"));
    }

    #[test]
    fn the_french_voice_cycles_between_its_two_voices() {
        let mut config = kokoro();
        assert_eq!(Field::VoiceFr.value(&config), "p-fenrir60 · française, recommandée");
        assert_eq!(Field::VoiceFr.adjust(&mut config, 1), Effect::Resynthesize);
        assert_eq!(config.voice.per_language["fr"], "ff_siwis");
        Field::VoiceFr.adjust(&mut config, 1);
        assert_eq!(config.voice.per_language["fr"], "p-fenrir60", "la liste boucle");
    }

    #[test]
    fn without_kokoro_the_voice_lines_name_the_system_voice() {
        let mut config = Config::default();
        assert_eq!(Field::VoiceFr.value(&config), "voix du système");
        assert_eq!(Field::VoiceEn.adjust(&mut config, 1), Effect::None);
        assert!(config.voice.per_language.is_empty());
    }

    #[test]
    fn speed_is_bounded_and_applies_at_once() {
        let mut config = Config::default();
        for _ in 0..200 {
            assert_eq!(Field::Speed.adjust(&mut config, 1), Effect::Speed);
        }
        assert_eq!(config.voice.speed, 3.0);
        for _ in 0..200 {
            Field::Speed.adjust(&mut config, -1);
        }
        assert_eq!(config.voice.speed, 0.5);
    }

    #[test]
    fn the_reading_column_walks_between_forty_and_full_screen() {
        let mut config = Config::default();
        assert_eq!(config.reading.line_width, 0);
        assert_eq!(Field::LineWidth.value(&config), "plein écran");
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 120);
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 116, "la colonne recule par quatre");
        config.reading.line_width = 40;
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 40, "quarante caractères est le plancher");
        config.reading.line_width = 120;
        Field::LineWidth.adjust(&mut config, 1);
        assert_eq!(config.reading.line_width, 0);
    }

    #[test]
    fn line_spacing_stays_between_none_and_three() {
        let mut config = Config::default();
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.line_spacing, 3);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, -1);
        }
        assert_eq!(config.reading.line_spacing, 0);
        assert_eq!(Field::LineSpacing.value(&config), "aucun");
    }

    #[test]
    fn the_reading_ruler_toggles() {
        let mut config = Config::default();
        assert!(config.reading.reading_ruler);
        Field::ReadingRuler.adjust(&mut config, 1);
        assert!(!config.reading.reading_ruler);
        assert_eq!(Field::ReadingRuler.value(&config), "non");
    }

    #[test]
    fn every_setting_has_a_label_and_a_hint() {
        for config in [Config::default(), kokoro()] {
            for field in FIELDS {
                assert!(!field.label().is_empty(), "{field:?}");
                assert!(field.hint(&config).len() > 10, "{field:?}");
                assert!(!field.value(&config).is_empty(), "{field:?}");
            }
        }
    }

    #[test]
    fn copying_a_field_copies_nothing_else() {
        let mut from = Config::default();
        from.voice.speed = 2.5; // une option --speed, par exemple
        from.reading.line_spacing = 3;
        let mut to = Config::default();
        Field::LineSpacing.copy_value(&from, &mut to);
        assert_eq!(to.reading.line_spacing, 3);
        assert_eq!(to.voice.speed, Config::default().voice.speed, "la vitesse ne suit pas");
    }

    #[test]
    fn copying_a_voice_copies_only_its_language_line() {
        let mut from = kokoro();
        from.voice.voice = "voix-de-la-ligne-de-commande".into();
        Field::VoiceEn.adjust(&mut from, 1);
        let mut to = Config::default();
        to.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        Field::VoiceEn.copy_value(&from, &mut to);
        assert_eq!(to.voice.per_language["en"], "af_heart");
        assert_eq!(to.voice.per_language["fr"], "p-fenrir60", "l'autre langue reste");
        assert_eq!(to.voice.voice, "", "voice.voice n'est pas écrit");
    }
}
