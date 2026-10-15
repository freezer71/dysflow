//! Écran de configuration plein écran.
//!
//! Tout ce qui se règle est ici : les modèles installables, la voix, la
//! lecture et le moteur. Les valeurs
//! s'appliquent immédiatement — vitesse et volume au son, sur-le-champ ; les
//! autres réglages de voix en régénérant la synthèse — et `w` les écrit dans
//! `config.toml` pour les sessions suivantes.

use crate::config::{Config, Effect};
use crate::models;

/// Rubriques de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Models,
    Voice,
    Reading,
    Engine,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Models,
        Section::Voice,
        Section::Reading,
        Section::Engine,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Section::Models => "Modèles",
            Section::Voice => "Voix",
            Section::Reading => "Lecture",
            Section::Engine => "Moteur",
        }
    }

    /// Ce que la rubrique contient, en un coup d'œil.
    ///
    /// La colonne de gauche ne servait qu'à nommer cinq rubriques ; y porter
    /// l'état courant évite d'entrer dans chacune pour savoir ce qu'elle dit.
    pub fn summary(&self, config: &Config) -> String {
        match self {
            Section::Models => {
                let total = models::available().len();
                if total == 0 {
                    return "aucun".to_string();
                }
                let posés = models::available()
                    .iter()
                    .filter(|p| models::is_installed(p))
                    .count();
                format!("{posés} installé{}", if posés > 1 { "s" } else { "" })
            }
            Section::Voice => {
                let voix = Field::Voice.value(config, 1.0);
                let voix = voix.split(' ').next().unwrap_or(&voix).to_string();
                format!("{voix} · {:.2}×", config.voice.speed)
            }
            Section::Reading => {
                // Le résumé tient en une colonne étroite : la largeur s'abrège.
                let colonne = match config.reading.line_width {
                    0 => "plein écran".to_string(),
                    n => format!("{n} col."),
                };
                format!("{colonne} · {}", config.reading.code_blocks)
            }
            Section::Engine => {
                let famille = config.engine.family.trim();
                if famille.is_empty() {
                    config.engine.engine.clone()
                } else {
                    famille.to_string()
                }
            }
        }
    }

    /// Champs de la rubrique. Vide pour « Modèles », qui affiche une liste.
    pub fn fields(&self) -> &'static [Field] {
        match self {
            Section::Models => &[],
            Section::Voice => &[
                Field::Speed,
                Field::Pitch,
                Field::Volume,
                Field::Language,
                Field::Voice,
            ],
            Section::Reading => &[
                Field::LineWidth,
                Field::LineSpacing,
                Field::ReadingRuler,
                Field::CodeBlocks,
                Field::Autoplay,
                Field::SeekStep,
            ],
            Section::Engine => &[
                Field::EnginePref,
                Field::Threads,
                Field::Sessions,
                Field::Prefetch,
                Field::ModelPath,
                Field::Family,
            ],
        }
    }
}

/// Un réglage individuel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Speed,
    Pitch,
    Volume,
    Language,
    Voice,
    CodeBlocks,
    Autoplay,
    SeekStep,
    LineWidth,
    LineSpacing,
    ReadingRuler,
    EnginePref,
    Threads,
    Sessions,
    Prefetch,
    ModelPath,
    Family,
}

const CODE_POLICIES: &[&str] = &["announce", "skip", "read"];
const ENGINES: &[&str] = &["auto", "system"];

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Field::Speed => "Vitesse",
            Field::Pitch => "Hauteur",
            Field::Volume => "Volume",
            Field::Language => "Langue",
            Field::Voice => "Voix",
            Field::CodeBlocks => "Blocs de code",
            Field::Autoplay => "Lecture au démarrage",
            Field::SeekStep => "Pas d'avance rapide",
            Field::LineWidth => "Largeur de colonne",
            Field::LineSpacing => "Interlignage",
            Field::ReadingRuler => "Règle de lecture",
            Field::EnginePref => "Moteur",
            Field::Threads => "Threads",
            Field::Sessions => "Sessions simultanées",
            Field::Prefetch => "Phrases en avance",
            Field::ModelPath => "Chemin du modèle",
            Field::Family => "Famille",
        }
    }

    /// Explication affichée sous le champ sélectionné, adaptée à la famille.
    ///
    /// Certaines familles n'acceptent pas de référence externe ; annoncer un
    /// clonage possible là où il ne l'est pas induirait en erreur.
    /// Explication affichée sous le champ sélectionné, adaptée à la famille.
    ///
    /// Toutes les familles n'exposent pas de voix nommées ; annoncer un choix
    /// là où il n'y en a pas induirait en erreur.
    pub fn hint_for(&self, config: &Config) -> String {
        // Décrire ce qu'un réglage ferait, alors qu'il est marqué inerte,
        // laisse croire à une contradiction. Mieux vaut dire quoi faire.
        if self.is_ignored(config) {
            return format!(
                "{} ne consulte pas ce réglage : il reste sans effet. Changez de modèle dans \
                 « Modèles » si vous y tenez.",
                config.engine.family
            );
        }
        if *self == Field::Voice {
            let voices = models::voices_for_family(&config.engine.family);
            return if voices.is_empty() {
                "Chemin d'un WAV à cloner, ou préréglage propre au modèle.".to_string()
            } else {
                format!(
                    "{} voix intégrées, au choix avec ← →, t pour écouter la voix retenue.",
                    voices.len()
                )
            };
        }
        self.hint().to_string()
    }

    /// Explication affichée sous le champ sélectionné.
    pub fn hint(&self) -> &'static str {
        match self {
            Field::Speed => {
                "Immédiate, sans resynthèse : appliquée au son, la voix accélère sans \
                 monter dans les aigus. t pour l'entendre."
            }
            Field::Pitch => "Décalage de hauteur de la voix du système, de -1 à +1.",
            Field::Volume => "Volume de sortie, sans effet sur la synthèse.",
            Field::Language => "Langue des documents : « auto » suit chacun d'eux.",
            Field::Voice => "Voix du modèle.",
            Field::CodeBlocks => "announce : annonce sans lire · skip : ignore · read : récite.",
            Field::Autoplay => "Démarrer la lecture dès l'ouverture.",
            Field::SeekStep => "Nombre de phrases sautées par Maj+flèche.",
            Field::LineWidth => {
                "Une ligne trop large fait perdre le début de la suivante."
            }
            Field::LineSpacing => "Lignes vides entre deux lignes de texte.",
            Field::ReadingRuler => "Souligne d'un fond la ligne en cours de lecture.",
            Field::EnginePref => "auto lit avec le modèle installé (Kokoro), sinon la voix du système.",
            Field::Threads => "Threads alloués au modèle.",
            Field::Sessions => {
                "Synthèses menées en parallèle ; « auto » en ouvre ce que le modèle demande \
                 (une pour Kokoro)."
            }
            Field::Prefetch => "Profondeur d'avance de la synthèse.",
            Field::ModelPath => "Renseigné automatiquement à l'installation d'un modèle.",
            Field::Family => "Famille du modèle chargé (kokoro).",
        }
    }

    /// Vrai si le champ s'édite au clavier plutôt qu'aux flèches.
    ///
    /// La voix fait exception : quand la famille chargée expose des voix
    /// nommées, on les fait défiler au lieu d'attendre une saisie exacte.
    pub fn is_text(&self, config: &Config) -> bool {
        match self {
            Field::Voice => models::voices_for_family(&config.engine.family).is_empty(),
            Field::ModelPath | Field::Family => true,
            _ => false,
        }
    }

    /// Nom de l'option de synthèse correspondante, s'il y en a une.
    ///
    /// Sert à signaler les réglages qu'une famille n'honore pas. Volume et
    /// vitesse n'en ont pas : ils agissent sur le son, jamais sur le modèle,
    /// et valent donc pour toutes les voix.
    pub fn option_key(&self) -> Option<&'static str> {
        match self {
            Field::Pitch => Some("pitch_shift"),
            Field::Language => Some("language"),
            Field::Voice => Some("voice"),
            _ => None,
        }
    }

    /// Vrai si le réglage restera sans effet avec la famille chargée.
    ///
    /// La voix demande une nuance : chez les familles qui conçoivent la voix,
    /// le champ n'agit que s'il désigne un WAV à cloner. Un identifiant de
    /// préréglage hérité d'un autre modèle y serait reçu puis oublié — c'est
    /// exactement le genre de réglage muet qu'on cherche à signaler.
    pub fn is_ignored(&self, config: &Config) -> bool {
        let family = config.engine.family.as_str();
        self.option_key()
            .is_some_and(|key| !models::honours(family, key))
    }

    /// Valeur affichée.
    pub fn value(&self, config: &Config, volume: f32) -> String {
        let text = |s: &str| {
            if s.is_empty() {
                "(défaut)".to_string()
            } else {
                s.to_string()
            }
        };
        match self {
            Field::Speed => format!("{:.2}×", config.voice.speed),
            Field::Pitch => format!("{:+.2}", config.voice.pitch),
            Field::Volume => format!("{:.0} %", volume * 100.0),
            Field::Language => text(&config.voice.language),
            Field::Voice => {
                let voices = models::voices_for_family(&config.engine.family);
                let shown = voice_in_use(config);
                match voices.iter().find(|v| v.id == shown) {
                    Some(voice) => voice.label.to_string(),
                    None if config.voice.voice.is_empty() && !voices.is_empty() => {
                        format!("{} (défaut)", voices[0].label)
                    }
                    // Un enregistrement fourni : son nom suffit, le chemin
                    // complet déborderait de la colonne.
                    None if std::path::Path::new(&crate::config::expand_tilde(
                        config.voice.voice.trim(),
                    ))
                    .is_file() =>
                    {
                        std::path::Path::new(config.voice.voice.trim())
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| config.voice.voice.clone())
                    }
                    None => text(&config.voice.voice),
                }
            }
            Field::CodeBlocks => config.reading.code_blocks.clone(),
            Field::Autoplay => if config.reading.autoplay { "oui" } else { "non" }.to_string(),
            Field::SeekStep => format!("{} phrases", config.reading.seek_step),
            Field::LineWidth => match config.reading.line_width {
                0 => "plein écran".to_string(),
                n => format!("{n} caractères"),
            },
            Field::LineSpacing => match config.reading.line_spacing {
                0 => "aucun".to_string(),
                1 => "une ligne".to_string(),
                n => format!("{n} lignes"),
            },
            Field::ReadingRuler => {
                if config.reading.reading_ruler { "oui" } else { "non" }.to_string()
            }
            Field::EnginePref => config.engine.engine.clone(),
            Field::Threads => config.engine.threads.to_string(),
            Field::Sessions => match config.engine.sessions {
                0 => format!("auto · {}", config.sessions()),
                n => n.to_string(),
            },
            Field::Prefetch => config.engine.prefetch.to_string(),
            Field::ModelPath => text(&shorten_path(&config.engine.model_path)),
            Field::Family => text(&config.engine.family),
        }
    }

    /// Texte brut pour l'édition au clavier.
    pub fn raw_text(&self, config: &Config) -> String {
        match self {
            Field::Voice => config.voice.voice.clone(),
            Field::ModelPath => config.engine.model_path.clone(),
            Field::Family => config.engine.family.clone(),
            _ => String::new(),
        }
    }

    pub fn set_text(&self, config: &mut Config, value: String) {
        match self {
            Field::Voice => config.voice.voice = value,
            Field::ModelPath => config.engine.model_path = value,
            Field::Family => config.engine.family = value,
            _ => {}
        }
    }

    /// Recopie la valeur de ce réglage de `from` dans `to`, et rien d'autre :
    /// l'enregistrement automatique n'écrit que ce que l'écran vient de
    /// changer, jamais une option de la ligne de commande (`--speed`…).
    pub fn copy_value(&self, from: &Config, to: &mut Config) {
        match self {
            Field::Speed => to.voice.speed = from.voice.speed,
            Field::Pitch => to.voice.pitch = from.voice.pitch,
            Field::Volume => to.voice.volume = from.voice.volume,
            Field::Language => to.voice.language = from.voice.language.clone(),
            Field::Voice => {
                if models::voices_follow_language(&from.engine.family) {
                    // Kokoro : le champ écrit la ligne de la langue par défaut.
                    let language = from.default_language();
                    let code = crate::text::language::Lang::parse(&language)
                        .map_or_else(|| language.clone(), |l| l.code().to_string());
                    if let Some(voice) = from.voice.per_language.get(&code) {
                        to.voice.per_language.insert(code, voice.clone());
                    }
                } else {
                    to.voice.voice = from.voice.voice.clone();
                }
            }
            Field::CodeBlocks => to.reading.code_blocks = from.reading.code_blocks.clone(),
            Field::Autoplay => to.reading.autoplay = from.reading.autoplay,
            Field::SeekStep => to.reading.seek_step = from.reading.seek_step,
            Field::LineWidth => to.reading.line_width = from.reading.line_width,
            Field::LineSpacing => to.reading.line_spacing = from.reading.line_spacing,
            Field::ReadingRuler => to.reading.reading_ruler = from.reading.reading_ruler,
            Field::EnginePref => to.engine.engine = from.engine.engine.clone(),
            Field::Threads => to.engine.threads = from.engine.threads,
            Field::Sessions => to.engine.sessions = from.engine.sessions,
            Field::Prefetch => to.engine.prefetch = from.engine.prefetch,
            Field::ModelPath => to.engine.model_path = from.engine.model_path.clone(),
            Field::Family => {
                to.engine.family = from.engine.family.clone();
                to.align_voice_with_family();
            }
        }
    }

    /// Applique un pas de réglage. Renvoie ce qui doit être répercuté.
    pub fn adjust(&self, config: &mut Config, delta: i32) -> Effect {
        let step = delta as f32;
        match self {
            Field::Speed => {
                config.voice.speed = (config.voice.speed + 0.05 * step).clamp(0.5, 3.0);
                Effect::Speed
            }
            Field::Pitch => {
                config.voice.pitch = (config.voice.pitch + 0.05 * step).clamp(-1.0, 1.0);
                Effect::Resynthesize
            }
            Field::Volume => Effect::Volume(0.05 * step),
            Field::Language => {
                // « auto » d'abord : chaque document prend alors sa langue.
                // La normalisation en dépend : effet à la prochaine ouverture.
                let mut languages = vec![crate::text::language::AUTO];
                languages.extend(models::languages_for_family(&config.engine.family));
                cycle_string(&mut config.voice.language, &languages, delta);
                Effect::Reparse
            }
            Field::CodeBlocks => {
                cycle_string(&mut config.reading.code_blocks, CODE_POLICIES, delta);
                Effect::Reparse
            }
            Field::Autoplay => {
                config.reading.autoplay = !config.reading.autoplay;
                Effect::None
            }
            Field::SeekStep => {
                config.reading.seek_step =
                    config.reading.seek_step.saturating_add_signed(delta).clamp(1, 50);
                Effect::None
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
                config.reading.line_spacing =
                    config.reading.line_spacing.saturating_add_signed(delta).min(3);
                Effect::None
            }
            Field::ReadingRuler => {
                config.reading.reading_ruler = !config.reading.reading_ruler;
                Effect::None
            }
            Field::EnginePref => {
                cycle_string(&mut config.engine.engine, ENGINES, delta);
                Effect::ReloadEngine
            }
            Field::Threads => {
                config.engine.threads = config.engine.threads.saturating_add_signed(delta).clamp(1, 64);
                Effect::ReloadEngine
            }
            Field::Sessions => {
                // Zéro est « auto » : sous un, on s'en remet à la famille.
                config.engine.sessions =
                    config.engine.sessions.saturating_add_signed(delta).clamp(0, 16);
                Effect::ReloadEngine
            }
            Field::Prefetch => {
                config.engine.prefetch =
                    config.engine.prefetch.saturating_add_signed(delta).clamp(2, 64);
                Effect::ReloadEngine
            }
            Field::Voice => {
                let voices = models::voices_for_family(&config.engine.family);
                if voices.is_empty() {
                    // Champ libre : il s'édite avec Entrée.
                    return Effect::None;
                }
                // Seul Kokoro nomme ses voix, une par langue. Le champ défile
                // parmi celles de la langue par défaut et écrit sa ligne de
                // `[voice.per_language]`, que seule `tts_params_in` lit.
                let language = config.default_language();
                let code = crate::text::language::Lang::parse(&language)
                    .map_or_else(|| language.clone(), |l| l.code().to_string());
                let own: Vec<&models::VoiceOption> =
                    voices.iter().filter(|v| v.language.is_empty() || v.language == code).collect();
                if own.is_empty() {
                    return Effect::None;
                }
                let shown = voice_in_use(config);
                let current = own.iter().position(|v| v.id == shown).unwrap_or(0) as i32;
                let next = (current + delta).rem_euclid(own.len() as i32) as usize;
                // Une seule voix dans la langue (le français) : rien ne change,
                // rien à écrire ni à resynthétiser.
                if own[next].id == shown {
                    return Effect::None;
                }
                config.voice.per_language.insert(code, own[next].id.to_string());
                Effect::Resynthesize
            }
            // Les autres champs textuels s'éditent avec Entrée, pas aux flèches.
            Field::ModelPath | Field::Family => Effect::None,
        }
    }
}

/// La voix réellement parlée par le champ « Voix » : pour Kokoro, celle de la
/// langue par défaut dans `[voice.per_language]` (ou son défaut) ; pour les
/// autres familles, `voice.voice`.
fn voice_in_use(config: &Config) -> String {
    if models::voices_follow_language(&config.engine.family) {
        return config.tts_params().voice;
    }
    config.voice.voice.clone()
}

/// Raccourcit un chemin pour qu'il tienne dans une colonne.
///
/// Le chemin complet d'un modèle dépasse la centaine de caractères et se
/// faisait couper au milieu d'un mot ; ne restent que le foyer abrégé et les
/// deux derniers segments, qui suffisent à reconnaître le fichier.
fn shorten_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    let court = match &home {
        Some(h) if path.starts_with(h.as_str()) => format!("~{}", &path[h.len()..]),
        _ => path.to_string(),
    };
    if court.chars().count() <= 34 {
        return court;
    }
    let segments: Vec<&str> = court.split('/').filter(|s| !s.is_empty()).collect();
    match segments.len() {
        0 => court,
        1 => segments[0].to_string(),
        n => format!("…/{}/{}", segments[n - 2], segments[n - 1]),
    }
}

/// Fait défiler une valeur parmi une liste de choix.
fn cycle_string(current: &mut String, choices: &[&str], delta: i32) {
    let index = choices.iter().position(|c| *c == current).unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(choices.len() as i32) as usize;
    *current = choices[next].to_string();
}

/// Page affichée par l'écran de configuration.
///
/// Deux colonnes côte à côte obligeaient à savoir laquelle recevait les
/// flèches, et à en changer par une touche qui ne se devine pas. Une page à la
/// fois supprime la question : on choisit une rubrique, elle s'ouvre en plein
/// écran, `Échap` revient au menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// La liste des rubriques, seule à l'écran.
    Menu,
    /// Une rubrique ouverte, en pleine largeur.
    Detail,
}

/// État de l'écran de configuration.
pub struct ConfigScreen {
    pub section: usize,
    pub row: usize,
    pub page: Page,
    /// Afficher aussi les réglages que le modèle chargé ignore.
    ///
    /// Masqués par défaut : sur Supertonic, cinq des sept réglages
    /// d'échantillonnage sont inertes, et les afficher noyait les deux qui
    /// agissent.
    pub show_all: bool,
    /// Tampon d'édition d'un champ textuel.
    pub editing: Option<String>,
    pub download: Option<models::Download>,
    /// Dernier message d'installation ou d'erreur.
    pub notice: String,
    /// Suppression demandée une première fois, en attente de la seconde.
    pending_delete: Option<(&'static models::ModelPackage, std::time::Instant)>,
}

/// Délai pendant lequel une seconde pression confirme la suppression.
const DELETE_CONFIRM_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigScreen {
    pub fn new() -> Self {
        Self {
            section: 0,
            row: 0,
            page: Page::Menu,
            show_all: false,
            editing: None,
            download: None,
            notice: String::new(),
            pending_delete: None,
        }
    }

    /// Vrai si cette pression confirme une suppression demandée juste avant.
    ///
    /// Un modèle pèse jusqu'à 1,6 Go : la touche qui retire un signet ne doit
    /// pas le supprimer d'un seul geste. La première pression arme, la
    /// seconde — sur le même paquet, dans les secondes qui suivent — agit.
    pub fn confirm_delete(&mut self, package: &'static models::ModelPackage) -> bool {
        self.confirm_delete_at(package, std::time::Instant::now())
    }

    fn confirm_delete_at(
        &mut self,
        package: &'static models::ModelPackage,
        now: std::time::Instant,
    ) -> bool {
        let confirmed = self
            .pending_delete
            .is_some_and(|(p, t)| std::ptr::eq(p, package) && now.duration_since(t) <= DELETE_CONFIRM_WINDOW);
        self.pending_delete = if confirmed { None } else { Some((package, now)) };
        confirmed
    }

    pub fn current_section(&self) -> Section {
        Section::ALL[self.section.min(Section::ALL.len() - 1)]
    }

    /// Champs réellement affichés, une fois écartés ceux qui n'agissent pas.
    pub fn visible_fields(&self, config: &Config) -> Vec<Field> {
        self.current_section()
            .fields()
            .iter()
            .copied()
            .filter(|field| self.show_all || !field.is_ignored(config))
            .collect()
    }

    /// Nombre de réglages retirés de la vue parce qu'ils resteraient inertes.
    pub fn hidden_count(&self, config: &Config) -> usize {
        if self.show_all {
            return 0;
        }
        self.current_section()
            .fields()
            .iter()
            .filter(|field| field.is_ignored(config))
            .count()
    }

    /// Nombre de lignes de la rubrique courante.
    pub fn row_count(&self, config: &Config) -> usize {
        match self.current_section() {
            Section::Models => models::available().len(),
            _ => self.visible_fields(config).len(),
        }
    }

    pub fn current_field(&self, config: &Config) -> Option<Field> {
        self.visible_fields(config).get(self.row).copied()
    }

    pub fn current_package(&self) -> Option<&'static models::ModelPackage> {
        if self.current_section() != Section::Models {
            return None;
        }
        models::available().get(self.row).copied()
    }

    pub fn move_section(&mut self, delta: i32) {
        let count = Section::ALL.len() as i32;
        self.section = ((self.section as i32 + delta).rem_euclid(count)) as usize;
        self.row = 0;
        self.editing = None;
    }

    /// Ouvre une rubrique par son rang affiché.
    ///
    /// Un chiffre y mène directement : c'est le geste le plus court, et il ne
    /// demande pas d'avoir d'abord placé le curseur.
    pub fn goto_section(&mut self, index: usize) {
        if index >= Section::ALL.len() {
            return;
        }
        self.section = index;
        self.row = 0;
        self.editing = None;
        self.page = Page::Detail;
    }

    /// Ouvre la rubrique sous le curseur.
    pub fn open_section(&mut self) {
        self.row = 0;
        self.editing = None;
        self.page = Page::Detail;
    }

    /// Revient au menu. Faux si l'on y était déjà.
    pub fn back_to_menu(&mut self) -> bool {
        if self.page == Page::Detail {
            self.page = Page::Menu;
            self.editing = None;
            return true;
        }
        false
    }

    /// Montre ou masque les réglages sans effet sur le modèle chargé.
    pub fn toggle_show_all(&mut self, config: &Config) {
        self.show_all = !self.show_all;
        let count = self.row_count(config);
        self.row = self.row.min(count.saturating_sub(1));
    }

    pub fn move_row(&mut self, delta: i32, config: &Config) {
        let count = self.row_count(config);
        if count == 0 {
            self.row = 0;
            return;
        }
        let next = (self.row as i32 + delta).clamp(0, count as i32 - 1);
        self.row = next as usize;
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
        match outcome {
            models::Progress::Done => {
                self.notice = format!("{} installé", package.display_name);
                Some(package)
            }
            models::Progress::Failed(err) => {
                self.notice = format!("échec : {err}");
                None
            }
            models::Progress::Cancelled => {
                self.notice = "téléchargement interrompu (reprise possible)".to_string();
                None
            }
            models::Progress::Running { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn copying_the_kokoro_voice_copies_only_its_language_line() {
        let mut from = Config::default();
        from.engine.family = "kokoro".into();
        from.voice.language = "en".into();
        from.voice.voice = "voix-de-la-ligne-de-commande".into();
        Field::Voice.adjust(&mut from, 1);
        let chosen = from.voice.per_language["en"].clone();
        let mut to = Config::default();
        to.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        Field::Voice.copy_value(&from, &mut to);
        assert_eq!(to.voice.per_language["en"], chosen);
        assert_eq!(to.voice.per_language["fr"], "p-fenrir60", "l'autre langue reste");
        assert_eq!(to.voice.voice, "", "voice.voice n'est pas écrit");
    }

    #[test]
    fn languages_cycle_through_those_of_the_model() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.language = "fr".into();
        Field::Language.adjust(&mut config, 1);
        assert_eq!(config.voice.language, "en");
        Field::Language.adjust(&mut config, 1);
        assert_eq!(config.voice.language, "auto");
        Field::Language.adjust(&mut config, 1);
        assert_eq!(config.voice.language, "fr", "deux langues et « auto », un tour complet");
        // Famille hors catalogue : les sept langues du terminal.
        config.engine.family = "inconnue".into();
        for _ in 0..8 {
            Field::Language.adjust(&mut config, 1);
        }
        assert_eq!(config.voice.language, "fr", "sept langues et « auto »");
    }

    #[test]
    fn every_section_but_models_exposes_fields() {
        // Les deux formes de la rubrique « Voix » doivent tenir debout.
        for section in Section::ALL {
            if section == Section::Models {
                assert!(section.fields().is_empty());
            } else {
                assert!(!section.fields().is_empty(), "{section:?}");
            }
        }
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
    fn enum_fields_cycle_in_both_directions() {
        let mut config = Config::default();
        assert_eq!(config.reading.code_blocks, "announce");
        Field::CodeBlocks.adjust(&mut config, 1);
        assert_eq!(config.reading.code_blocks, "skip");
        Field::CodeBlocks.adjust(&mut config, -1);
        assert_eq!(config.reading.code_blocks, "announce");
        // Le défilement boucle au lieu de buter.
        Field::CodeBlocks.adjust(&mut config, -1);
        assert_eq!(config.reading.code_blocks, "read");
    }

    #[test]
    fn sessions_can_be_left_to_the_family() {
        let mut config = Config::default();
        config.engine.family = "kokoro".to_string();
        // « auto » dit ce qu'il vaut : un réglage muet ne se comprend pas.
        assert_eq!(Field::Sessions.value(&config, 1.0), "auto · 1");
        Field::Sessions.adjust(&mut config, 1);
        assert_eq!(Field::Sessions.value(&config, 1.0), "1");
        // Descendre sous un revient à « auto », et on ne va pas plus bas.
        Field::Sessions.adjust(&mut config, -1);
        assert_eq!(config.engine.sessions, 0);
        Field::Sessions.adjust(&mut config, -1);
        assert_eq!(config.engine.sessions, 0);
    }

    #[test]
    fn engine_changes_request_a_reload() {
        let mut config = Config::default();
        assert_eq!(Field::Threads.adjust(&mut config, 1), Effect::ReloadEngine);
        assert_eq!(Field::Sessions.adjust(&mut config, 1), Effect::ReloadEngine);
    }

    #[test]
    fn text_fields_round_trip() {
        let mut config = Config::default();
        Field::Voice.set_text(&mut config, "ma_voix.wav".into());
        assert_eq!(Field::Voice.raw_text(&config), "ma_voix.wav");
        assert_eq!(Field::Voice.value(&config, 1.0), "ma_voix.wav");
        // Vidé, le champ réaffiche le défaut plutôt qu'une chaîne vide.
        Field::Voice.set_text(&mut config, String::new());
        assert_eq!(Field::Voice.value(&config, 1.0), "(défaut)");
    }

    #[test]
    fn kokoro_voice_field_cycles_the_voices_of_the_default_language() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.voice = "M3".into();
        assert!(!Field::Voice.is_text(&config));
        // Français par défaut : une seule voix, les flèches ne changent rien
        // et ne relancent pas la synthèse.
        assert_eq!(Field::Voice.value(&config, 1.0), "p-fenrir60 · française, recommandée");
        assert_eq!(Field::Voice.adjust(&mut config, 1), Effect::None);
        assert_eq!(Field::Voice.adjust(&mut config, -1), Effect::None);
        assert!(!config.voice.per_language.contains_key("fr"), "rien d'écrit");
        assert_eq!(config.tts_params().voice, "p-fenrir60");
        assert_eq!(config.voice.voice, "M3", "la voix de l'ancien modèle reste");
        // Langue imposée en anglais : les trois voix anglaises, on boucle.
        config.voice.language = "en".into();
        assert_eq!(Field::Voice.adjust(&mut config, 1), Effect::Resynthesize);
        assert_eq!(config.voice.per_language["en"], "am_fenrir");
        assert_eq!(config.tts_params().voice, "am_fenrir");
        Field::Voice.adjust(&mut config, -2);
        assert_eq!(config.voice.per_language["en"], "af_heart");
    }

    #[test]
    fn language_cycle_starts_with_auto_and_reparses() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.language = "fr".into();
        assert_eq!(Field::Language.adjust(&mut config, -1), Effect::Reparse);
        assert_eq!(config.voice.language, "auto");
    }

    #[test]
    fn kokoro_voices_are_never_flagged() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.voice = "p-fenrir60".into();
        assert!(!Field::Voice.is_ignored(&config));
        assert!(!Field::Speed.is_ignored(&config));
        assert!(Field::Pitch.is_ignored(&config), "Kokoro ne lit pas la hauteur");
    }

    #[test]
    fn voice_stays_a_text_field_for_unknown_families() {
        let mut config = Config::default();
        config.engine.family = "inconnue".into();
        assert!(Field::Voice.is_text(&config));
        assert_eq!(Field::Voice.adjust(&mut config, 1), Effect::None);
    }

    #[test]
    fn arrows_never_alter_text_fields() {
        let mut config = Config::default();
        config.engine.family = "inconnue".into();
        config.voice.voice = "intacte".into();
        assert_eq!(Field::Voice.adjust(&mut config, 1), Effect::None);
        assert_eq!(config.voice.voice, "intacte");
    }

    #[test]
    fn navigation_stays_within_bounds() {
        let config = Config::default();
        let mut screen = ConfigScreen::new();
        screen.section = 1; // Voix
        screen.move_row(-5, &config);
        assert_eq!(screen.row, 0);
        screen.move_row(99, &config);
        assert_eq!(screen.row, screen.visible_fields(&config).len() - 1);
        // Les rubriques bouclent.
        screen.move_section(-1);
        assert_eq!(screen.current_section(), Section::Models);
    }

    #[test]
    fn ineffective_settings_are_hidden_until_asked_for() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        let mut screen = ConfigScreen::new();
        screen.section = 1; // Voix

        // Kokoro ne lit pas la hauteur (voix du système seule) : elle attend
        // qu'on la demande.
        assert!(!screen.visible_fields(&config).contains(&Field::Pitch));
        assert_eq!(screen.hidden_count(&config), 1);

        screen.toggle_show_all(&config);
        assert!(screen.visible_fields(&config).contains(&Field::Pitch));
        assert_eq!(screen.hidden_count(&config), 0);
    }

    #[test]
    fn showing_everything_never_leaves_the_cursor_outside() {
        let config = Config::default();
        let mut screen = ConfigScreen::new();
        screen.section = 1;
        screen.toggle_show_all(&config);
        screen.move_row(99, &config);
        let dernier = screen.row;
        // Revenir à la vue filtrée doit ramener le curseur dans les clous.
        screen.toggle_show_all(&config);
        assert!(screen.row < screen.visible_fields(&config).len(), "{dernier}");
    }

    #[test]
    fn the_screen_opens_on_the_menu_and_comes_back_to_it() {
        let mut screen = ConfigScreen::new();
        assert_eq!(screen.page, Page::Menu);

        screen.section = 1;
        screen.open_section();
        assert_eq!(screen.page, Page::Detail);
        assert_eq!(screen.row, 0, "une rubrique s'ouvre en haut");

        // Échap recule d'un cran, puis n'a plus rien à reculer : c'est alors à
        // l'appelant de fermer l'écran.
        assert!(screen.back_to_menu());
        assert_eq!(screen.page, Page::Menu);
        assert!(!screen.back_to_menu());
    }

    #[test]
    fn a_digit_opens_its_section_straight_away() {
        let mut screen = ConfigScreen::new();
        screen.goto_section(2);
        assert_eq!(screen.page, Page::Detail);
        assert_eq!(screen.current_section(), Section::Reading);

        // Hors bornes : rien ne bouge, et surtout on ne quitte pas la page.
        screen.goto_section(99);
        assert_eq!(screen.current_section(), Section::Reading);
        assert_eq!(screen.page, Page::Detail);
    }

    #[test]
    fn an_open_section_forgets_a_pending_edit() {
        // Sans quoi le tampon d'un champ suivrait dans la rubrique suivante.
        let mut screen = ConfigScreen::new();
        screen.editing = Some("à moitié tapé".into());
        screen.goto_section(1);
        assert!(screen.editing.is_none());

        screen.editing = Some("à moitié tapé".into());
        screen.back_to_menu();
        assert!(screen.editing.is_none());
    }

    #[test]
    fn every_section_says_what_it_holds() {
        let config = Config::default();
        for section in Section::ALL {
            let résumé = section.summary(&config);
            assert!(!résumé.is_empty(), "{section:?}");
            assert!(résumé.chars().count() <= 22, "{section:?} : {résumé}");
        }
    }

    #[test]
    fn long_paths_keep_their_tail() {
        assert_eq!(shorten_path(""), "");
        assert_eq!(shorten_path("court.gguf"), "court.gguf");
        // Les deux derniers segments suffisent à reconnaître le fichier.
        let long = "/un/chemin/vraiment/tres/long/qui/depasse/la/colonne/modele.gguf";
        assert_eq!(shorten_path(long), "…/colonne/modele.gguf");
    }

    #[test]
    fn every_field_has_a_label_and_a_hint() {
        for family in ["supertonic", "magpie_tts", "inconnue"] {
            let mut config = Config::default();
            config.engine.family = family.into();
            for section in Section::ALL {
                for field in section.fields() {
                    assert!(!field.label().is_empty(), "{field:?}");
                    assert!(field.hint().len() > 10, "{field:?}");
                    assert!(!field.hint_for(&config).is_empty(), "{field:?}");
                }
            }
        }
    }


    #[test]
    fn the_reading_column_walks_between_forty_and_full_screen() {
        let mut config = Config::default();
        // Par défaut, le texte occupe l'écran : c'est le réglage qu'on quitte
        // volontairement, pas celui qu'on subit.
        assert_eq!(config.reading.line_width, 0);
        assert_eq!(Field::LineWidth.value(&config, 1.0), "plein écran");

        // Descendre depuis le plein écran donne la colonne la plus large.
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 120);
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 116, "la colonne recule par quatre");

        config.reading.line_width = 40;
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 40, "quarante caractères est le plancher");

        // Au-delà de la borne haute vient le plein écran, noté zéro.
        config.reading.line_width = 120;
        Field::LineWidth.adjust(&mut config, 1);
        assert_eq!(config.reading.line_width, 0);
        assert_eq!(Field::LineWidth.value(&config, 1.0), "plein écran");
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 120);
    }

    #[test]
    fn line_spacing_stays_between_none_and_three() {
        // Les bornes de `Config::normalised`, partout : trois au plus.
        let mut config = Config::default();
        assert_eq!(config.reading.line_spacing, 1);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.line_spacing, 3);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, -1);
        }
        assert_eq!(config.reading.line_spacing, 0);
        assert_eq!(Field::LineSpacing.value(&config, 1.0), "aucun");
    }

    #[test]
    fn the_seek_step_stops_at_fifty() {
        let mut config = Config::default();
        config.reading.seek_step = 48;
        for _ in 0..5 {
            Field::SeekStep.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.seek_step, 50);
    }

    #[test]
    fn the_reading_ruler_toggles() {
        let mut config = Config::default();
        assert!(config.reading.reading_ruler);
        Field::ReadingRuler.adjust(&mut config, 1);
        assert!(!config.reading.reading_ruler);
        assert_eq!(Field::ReadingRuler.value(&config, 1.0), "non");
    }

    #[test]
    fn the_reading_section_shows_the_column_before_the_code_policy() {
        let config = Config::default();
        assert_eq!(Section::Reading.summary(&config), "plein écran · announce");
    }


    #[test]
    fn deleting_a_model_asks_to_press_again() {
        // Un modèle pèse jusqu'à 1,6 Go : la touche qui retire un signet ne
        // doit pas le supprimer d'un seul geste.
        let mut screen = ConfigScreen::new();
        let first = &models::CATALOG[0];
        let second = &models::CATALOG[1];
        let t0 = std::time::Instant::now();
        let s = std::time::Duration::from_secs;
        assert!(!screen.confirm_delete_at(first, t0));
        assert!(screen.confirm_delete_at(first, t0 + s(2)));
        // Confirmé : la demande est consommée.
        assert!(!screen.confirm_delete_at(first, t0 + s(3)));
        // Trop tard, ou un autre paquet : on redemande.
        assert!(!screen.confirm_delete_at(first, t0 + s(20)));
        assert!(!screen.confirm_delete_at(second, t0 + s(21)));
    }
}
