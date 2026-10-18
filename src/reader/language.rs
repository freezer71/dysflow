//! La langue de chaque document : détectée à l'ouverture, ou choisie à la
//! main et retenue sous la même identité que la position de lecture.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::text::language::{self, Lang, MIXED_THRESHOLD};

use super::Reader;

/// La langue choisie pour un document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageChoice {
    pub document: String,
    pub language: String,
}

/// Les choix de langue, dans `languages.json` à côté des positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageChoices {
    #[serde(default)]
    pub items: Vec<LanguageChoice>,
}

impl LanguageChoices {
    pub fn path() -> Option<PathBuf> {
        crate::paths::data_dir().map(|d| d.join("languages.json"))
    }

    /// Relit les choix. Un fichier illisible est mis de côté
    /// (`paths::load_json`) et l'avertissement va au journal.
    pub fn load() -> Self {
        Self::path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("répertoire de données introuvable")?;
        self.save_to(&path)
    }

    pub(crate) fn load_from(path: &Path) -> Self {
        let (choices, notice) = crate::paths::load_json(path, "choix de langue");
        if let Some(notice) = notice {
            eprintln!("sillage : {notice}");
        }
        choices
    }

    pub(crate) fn save_to(&self, path: &Path) -> Result<()> {
        crate::paths::save_json(path, self)
    }

    pub fn get(&self, document: &str) -> Option<&str> {
        self.items.iter().find(|c| c.document == document).map(|c| c.language.as_str())
    }

    /// Pose (ou, avec `None`, oublie) le choix d'un document.
    pub fn set(&mut self, document: &str, language: Option<&str>) {
        self.items.retain(|c| c.document != document);
        if let Some(language) = language {
            self.items.push(LanguageChoice { document: document.to_string(), language: language.to_string() });
        }
    }

    /// Fait passer un choix retenu sous l'ancien nom d'un fichier à son
    /// identité courante. Vrai si quelque chose a bougé.
    pub fn adopt(&mut self, former: &str, current: &str) -> bool {
        if former == current || self.get(former).is_none() {
            return false;
        }
        if self.get(current).is_some() {
            self.set(former, None);
            return true;
        }
        for item in &mut self.items {
            if item.document == former {
                item.document = current.to_string();
            }
        }
        true
    }
}

/// La langue du document ouvert, telle que l'onglet Voix la montre.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentLanguage {
    /// Langue détectée (`fr`, `en`), ou rien de fiable.
    pub detected: Option<String>,
    /// Langue choisie à la main pour ce document.
    pub chosen: Option<String>,
    /// Langue lue : le choix, sinon la configuration, sinon la détection.
    pub effective: String,
    /// Part des lettres dans l'autre langue.
    pub minority_share: f32,
}

impl DocumentLanguage {
    /// « Document mêlé » : plus de 30 % des lettres dans l'autre langue.
    pub fn mixed(&self) -> bool {
        self.minority_share > MIXED_THRESHOLD
    }
}

impl Reader {
    /// Les choix, relus du disque à la première demande (jamais pour un
    /// lecteur hors disque).
    pub(super) fn choices(&mut self) -> &mut LanguageChoices {
        let persist = self.persist;
        let path = self.choices_path.clone();
        self.choices.get_or_insert_with(|| match (persist, path) {
            (true, Some(path)) => LanguageChoices::load_from(&path),
            _ => LanguageChoices::default(),
        })
    }

    pub(super) fn save_choices(&mut self) -> Result<()> {
        let Some(path) = self.choices_path.clone() else { bail!("répertoire de données introuvable") };
        self.choices().save_to(&path)
    }

    /// La langue d'un texte qu'on ouvre.
    pub(super) fn language_of(&mut self, markdown: &str, source: &str) -> DocumentLanguage {
        let detection = language::detect_markdown(markdown);
        let chosen = self.choices().get(source).map(str::to_string);
        DocumentLanguage {
            detected: detection.map(|d| d.language.code().to_string()),
            effective: language::resolve(&self.config.voice.language, chosen.as_deref(), detection),
            chosen,
            minority_share: detection.map_or(0.0, |d| d.minority_share),
        }
    }

    pub fn document_language(&self) -> Option<&DocumentLanguage> {
        self.text.as_ref().and(self.language.as_ref())
    }

    /// La langue lue : celle du document gardé, sinon la langue par défaut.
    pub fn current_language(&self) -> String {
        self.document_language()
            .map(|l| l.effective.clone())
            .unwrap_or_else(|| self.current_config().default_language())
    }

    /// Choisit la langue du document gardé (`None` : automatique). Le texte
    /// est remis en forme à la même phrase : la normalisation en dépend.
    pub fn set_document_language(&mut self, language: Option<&str>, now: Instant) -> Result<()> {
        if let Some(code) = language {
            if Lang::parse(code).is_none() {
                bail!("langue inconnue : {code}");
            }
        }
        let Some(identity) = self.document_identity().map(str::to_string) else {
            bail!("aucun document ouvert");
        };
        let code = language.map(|c| Lang::parse(c).map_or(c, |l| l.code()));
        self.choices().set(&identity, code);
        // Le choix s'applique d'abord ; un disque qui refuse l'écriture ne
        // le défait pas, il se signale.
        self.reparse(now);
        if self.persist {
            if let Err(e) = self.save_choices() {
                if let Some(app) = self.session.as_mut() {
                    app.notify(&format!("choix de langue non enregistré : {e}"));
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::reader::settings::{SettingsEffect, SettingsPatch};
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    const EN: &str = "Monads let you chain computations that carry a context, without handling \
                      it by hand at every single step. The report has 37 pages.";
    const FR: &str = "Les monades permettent d'enchaîner des calculs qui portent un contexte, \
                      sans le manipuler à la main. Le rapport a 37 pages.";

    fn reader_with(config: Config) -> Reader {
        let mut config = config;
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    fn reader() -> Reader {
        reader_with(Config::default())
    }

    #[test]
    fn an_english_text_is_read_in_english_with_its_figures() {
        let mut r = reader();
        r.open_text(EN, "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.effective, "en");
        assert_eq!(lang.detected.as_deref(), Some("en"));
        let app = r.session().unwrap();
        assert_eq!(app.synth.params().language, "en");
        assert!(app.doc().sentences.iter().any(|s| s.text.contains("37")), "chiffres gardés");
    }

    #[test]
    fn a_short_text_takes_the_default_language_and_says_so() {
        let mut r = reader();
        r.open_text("Hello world.", "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.detected, None, "rien de fiable sous 40 lettres");
        assert_eq!(lang.effective, "fr");
        let snapshot = crate::reader::view::snapshot(&r);
        assert_eq!(snapshot.document_language.unwrap().detected, None);
    }

    #[test]
    fn a_configured_language_beats_the_detection() {
        let mut config = Config::default();
        config.voice.language = "en".into();
        let mut r = reader_with(config);
        r.open_text(FR, "sélection", Instant::now());
        assert_eq!(r.document_language().unwrap().effective, "en");
        assert_eq!(r.document_language().unwrap().detected.as_deref(), Some("fr"));
    }

    #[test]
    fn a_manual_choice_reparses_at_the_same_sentence_and_is_remembered() {
        let mut r = reader();
        r.open_text(&format!("{FR} {FR} {FR}"), "sélection", Instant::now());
        r.with_session(Instant::now(), |app| app.seek_to(2));
        r.set_document_language(Some("en"), Instant::now()).unwrap();
        assert_eq!(r.document_language().unwrap().effective, "en");
        assert_eq!(r.document_language().unwrap().chosen.as_deref(), Some("en"));
        assert_eq!(r.session().unwrap().current_sentence(), 2);
        // Le même texte, recapturé, retrouve son choix.
        r.open_text(&format!("{FR} {FR} {FR}"), "sélection", Instant::now());
        assert_eq!(r.document_language().unwrap().effective, "en");
        // Revenir à « automatique » oublie le choix.
        r.set_document_language(None, Instant::now()).unwrap();
        assert_eq!(r.document_language().unwrap().effective, "fr");
        assert!(r.set_document_language(Some("de"), Instant::now()).is_err());
    }

    #[test]
    fn a_choice_follows_a_file_from_its_former_name() {
        let mut store = LanguageChoices::default();
        store.set("notes.md", Some("en"));
        assert!(store.adopt("notes.md", "/docs/notes.md"));
        assert_eq!(store.get("/docs/notes.md"), Some("en"));
        assert_eq!(store.get("notes.md"), None);
        assert!(!store.adopt("absent.md", "/docs/absent.md"));
    }

    #[test]
    fn choices_round_trip_through_their_file() {
        let path = crate::paths::test_scratch("langues").join("langues.json");
        let mut store = LanguageChoices::default();
        store.set("/a.md", Some("en"));
        store.save_to(&path).unwrap();
        assert_eq!(LanguageChoices::load_from(&path).get("/a.md"), Some("en"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_reparse_keeps_the_same_prose_sentence_when_code_shifts_the_numbering() {
        let mut r = reader();
        let text = "Les monades permettent d'enchaîner des calculs qui portent un contexte.\n\n\
                    ```\nlet x = 1;\n```\n\n\
                    Le rapport complet est disponible. Il garde ses chiffres, comme 37 pages.\n\n\
                    La dernière phrase conclut le propos du jour.";
        r.open_text(text, "sélection", Instant::now());
        let last = r.session().unwrap().doc().sentences.len() - 1;
        r.with_session(Instant::now(), |app| app.seek_to(last));
        let before = r.session().unwrap().doc().sentences[last].text.clone();
        r.set_document_language(Some("en"), Instant::now()).unwrap();
        let app = r.session().unwrap();
        assert_eq!(app.synth.params().language, "en");
        // La normalisation change les chiffres, pas la phrase.
        assert_eq!(app.doc().sentences[app.current_sentence()].text[..8], before[..8]);
        // Une phrase au milieu, après le bloc de code.
        r.with_session(Instant::now(), |app| app.seek_to(3));
        let middle = r.session().unwrap().doc().sentences[3].text.clone();
        r.set_document_language(None, Instant::now()).unwrap();
        let app = r.session().unwrap();
        assert_eq!(app.doc().sentences[app.current_sentence()].text[..8], middle[..8]);
    }

    #[test]
    fn a_missing_or_corrupt_store_is_empty_and_set_aside() {
        let dir = crate::paths::test_scratch("langues-abimes");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(LanguageChoices::load_from(&dir.join("absent.json")).items.is_empty());
        let corrupt = dir.join("abime.json");
        std::fs::write(&corrupt, "{ pas du json").unwrap();
        assert!(LanguageChoices::load_from(&corrupt).items.is_empty());
        // Le fichier abîmé est gardé à part, jamais écrasé.
        let backup = dir.join("abime.json.bak");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{ pas du json");
        // Ouvrir un document sans rien changer n'écrit rien.
        let mut r = reader();
        r.choices_path = Some(corrupt.clone());
        r.open_text(FR, "sélection", Instant::now());
        assert!(!corrupt.exists());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{ pas du json");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_reader_off_disk_neither_reads_nor_writes_the_choices() {
        let dir = crate::paths::test_scratch("langues-hors");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("languages.json");
        let mut seeded = LanguageChoices::default();
        seeded.set("quelconque", Some("en"));
        seeded.save_to(&file).unwrap();
        let mut r = reader();
        r.choices_path = Some(file.clone());
        r.set_persistent(false);
        r.open_text(FR, "sélection", Instant::now());
        assert_eq!(r.document_language().unwrap().chosen, None, "le fichier n'est pas lu");
        r.set_document_language(Some("en"), Instant::now()).unwrap();
        let on_disk = LanguageChoices::load_from(&file);
        assert_eq!(on_disk.items.len(), 1, "le fichier n'est pas écrit");
        assert_eq!(on_disk.get("quelconque"), Some("en"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_save_keeps_the_applied_choice_and_says_so() {
        let dir = crate::paths::test_scratch("langues-echec");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("fichier");
        std::fs::write(&blocker, "x").unwrap();
        let mut r = reader();
        // Un « dossier » qui est un fichier : l'écriture ne peut pas aboutir.
        r.choices_path = Some(blocker.join("languages.json"));
        r.open_text(&format!("{FR} {FR}"), "sélection", Instant::now());
        r.set_document_language(Some("en"), Instant::now()).unwrap();
        assert_eq!(r.document_language().unwrap().effective, "en", "appliqué malgré l'échec");
        assert_eq!(r.session().unwrap().synth.params().language, "en");
        assert!(r.session().unwrap().status().contains("non enregistré"), "{}", r.session().unwrap().status());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_mixed_document_reports_its_share() {
        let mut r = reader();
        r.open_text(&format!("{FR}\n\n{FR}\n\n{EN}"), "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.effective, "fr");
        assert!(lang.mixed(), "{}", lang.minority_share);
    }

    #[test]
    fn the_language_and_the_voices_trigger_the_right_effect() {
        use SettingsEffect::*;
        let mut before = Config::default();
        before.engine.family = "kokoro".into();
        let mut after = before.clone();
        after.voice.language = "en".into();
        assert_eq!(SettingsEffect::between_in(&before, &after, "fr"), Reparse);
        let mut other = before.clone();
        other.voice.per_language.insert("en".into(), "am_fenrir".into());
        assert_eq!(SettingsEffect::between_in(&before, &other, "fr"), Display, "voix de l'autre langue : rien à resynthétiser");
        assert_eq!(SettingsEffect::between_in(&before, &other, "en"), Resynthesize);
    }

    #[test]
    fn a_voice_per_language_round_trips_through_a_patch() {
        let mut r = reader();
        let patch = SettingsPatch { voice_for_language: Some(("en".into(), "am_fenrir".into())), ..Default::default() };
        let applied = r.apply_patch(&patch, Instant::now());
        assert_eq!(applied.voices_by_language.get("en").map(String::as_str), Some("am_fenrir"));
        assert_eq!(r.settings().voices_by_language.len(), 1);
    }
}
