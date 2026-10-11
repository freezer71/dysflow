//! Le lecteur qui dure : une voix gardée d'un texte à l'autre.
//!
//! `App` lit un document. `Reader` en ouvre un par texte sans recharger la
//! voix, et la relâche quand rien n'a été lu depuis un moment. Il ne quitte
//! jamais le fil qui l'a créé : la sortie audio n'est pas transférable d'un
//! fil à l'autre.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::audio::Player;
use crate::config::Config;
use crate::doc::DocumentBuilder;
use crate::tts::{Synthesizer, TtsEngine};

pub mod command;
pub mod data;
pub mod handle;
pub mod language;
pub mod preview;
pub mod settings;
pub mod view;

pub type VoiceFactory = Box<dyn Fn(&Config) -> Arc<dyn TtsEngine>>;
pub type PlayerFactory = Box<dyn Fn() -> Player>;

/// Le texte en cours, gardé pour le rouvrir après une mise au repos.
#[derive(Clone)]
struct Text {
    markdown: String,
    source: String,
    /// Un fichier : sa position se retient d'une ouverture à l'autre.
    identity: bool,
}

pub struct Reader {
    config: Config,
    voices: VoiceFactory,
    players: PlayerFactory,
    engine: Option<Arc<dyn TtsEngine>>,
    session: Option<App>,
    text: Option<Text>,
    /// Phrase où reprendre un texte mis au repos.
    resting_at: Option<usize>,
    revision: u64,
    last_activity: Instant,
    /// Téléchargement de voix en cours, sondé à chaque tour.
    download: Option<crate::models::Download>,
    /// Le document au repos, reconstruit une fois à la mise au repos : la
    /// vue le relit à chaque instantané sans le ré-analyser.
    resting_doc: Option<crate::doc::Document>,
    /// Faux pour le lecteur muet de l'app de test : rien n'est écrit ni
    /// effacé sur le disque de l'utilisateur.
    persist: bool,
    /// Aperçu de voix sans texte ouvert.
    lone_preview: Option<preview::LonePreview>,
    /// Dernier échec de cet aperçu.
    preview_error: Option<String>,
    /// Choix de langue faits à la main, par document.
    /// Relus au premier besoin, et seulement si le lecteur écrit sur disque.
    choices: Option<language::LanguageChoices>,
    /// Où les choix s'écrivent (le dossier de données, sauf test).
    choices_path: Option<std::path::PathBuf>,
    /// Langue du document gardé.
    language: Option<language::DocumentLanguage>,
}

impl Reader {
    pub fn new(config: Config, voices: VoiceFactory, players: PlayerFactory, now: Instant) -> Self {
        Self {
            config,
            voices,
            players,
            engine: None,
            session: None,
            text: None,
            resting_at: None,
            revision: 0,
            last_activity: now,
            download: None,
            resting_doc: None,
            persist: true,
            lone_preview: None,
            preview_error: None,
            choices: None,
            choices_path: language::LanguageChoices::path(),
            language: None,
        }
    }

    pub fn session(&self) -> Option<&App> {
        self.session.as_ref()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Hors disque : les réglages s'appliquent sans s'écrire, et rien ne
    /// s'efface (modèles, cache, signets).
    pub fn set_persistent(&mut self, on: bool) {
        self.persist = on;
        if !on {
            // Hors disque : ni lecture ni écriture des choix de langue.
            self.choices = Some(language::LanguageChoices::default());
        }
    }

    /// Change à chaque nouveau document — pas à chaque réveil de la voix.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn voice_loaded(&self) -> bool {
        self.engine.is_some()
    }

    pub fn resting_document(&self) -> Option<&crate::doc::Document> {
        self.resting_doc.as_ref()
    }

    pub fn is_resting(&self) -> bool {
        self.resting_at.is_some()
    }

    pub fn engine(&self) -> Option<&Arc<dyn TtsEngine>> {
        self.engine.as_ref()
    }

    pub fn text_markdown(&self) -> Option<&str> {
        self.text.as_ref().map(|t| t.markdown.as_str())
    }

    /// Charge la voix sans attendre de texte : le raccourci la demande
    /// pendant que la sélection se lit.
    pub fn prepare_voice(&mut self, now: Instant) {
        self.last_activity = now;
        self.ensure_engine();
    }

    /// Ouvre un texte capturé (sélection, presse-papiers). Son identité —
    /// sous laquelle ses signets se rangent — est sa source suivie d'une
    /// empreinte du texte : deux sélections ne partagent pas leurs signets,
    /// et le même texte, recapturé, retrouve les siens.
    pub fn open_text(&mut self, markdown: &str, source: &str, now: Instant) {
        let source = format!(
            "{source}:{:016x}",
            crate::source::cache::stable_hash(markdown.as_bytes())
        );
        self.open(Text { markdown: markdown.to_string(), source, identity: false }, now);
        self.revision += 1;
    }

    /// L'identité du document gardé, ouvert ou au repos.
    pub fn document_identity(&self) -> Option<&str> {
        match &self.session {
            Some(app) => Some(app.source.as_str()),
            None => self.text.as_ref().map(|t| t.source.as_str()),
        }
    }

    /// Les signets du document gardé. Au repos, il n'y a plus de session
    /// pour porter le magasin : on le relit.
    pub fn bookmarks(&self) -> Vec<crate::bookmark::Bookmark> {
        match &self.session {
            Some(app) => self.bookmarks_in(&app.bookmarks),
            None if self.text.is_some() => self.bookmarks_in(&crate::bookmark::BookmarkStore::load().0),
            None => Vec::new(),
        }
    }

    /// Les signets du document gardé, pris dans `store`.
    pub fn bookmarks_in(&self, store: &crate::bookmark::BookmarkStore) -> Vec<crate::bookmark::Bookmark> {
        let Some(identity) = self.document_identity() else { return Vec::new() };
        store.for_document(identity).cloned().collect()
    }

    pub fn open_file(&mut self, path: &Path, now: Instant) -> anyhow::Result<()> {
        let markdown = crate::source::load(path)?;
        let source = crate::launch::document_identity(path);
        // Le choix de langue suit le fichier, comme sa position : il doit
        // être là avant l'ouverture, qui en dépend.
        if let Some(name) = path.file_name() {
            if self.choices().adopt(&name.to_string_lossy(), &source) && self.persist {
                let _ = self.save_choices();
            }
        }
        self.open(Text { markdown, source, identity: true }, now);
        if let Some(app) = self.session.as_mut() {
            if let Some(name) = path.file_name() {
                app.adopt_former_identity(&name.to_string_lossy());
            }
            app.resume_where_left();
        }
        self.revision += 1;
        Ok(())
    }

    /// Applique une commande au document en cours, en réveillant la voix
    /// si elle était au repos.
    pub fn with_session(&mut self, now: Instant, f: impl FnOnce(&mut App)) {
        self.last_activity = now;
        self.wake(now);
        if let Some(app) = self.session.as_mut() {
            f(app);
        }
    }

    /// Applique une commande de l'interface. Une pause n'a rien à arrêter
    /// sans texte ou voix au repos : elle ne réveille pas la voix pour rien.
    pub fn apply(&mut self, command: command::Command, now: Instant) {
        let idle = self.is_resting() || self.session.is_none();
        if command == command::Command::Pause && idle {
            return;
        }
        self.with_session(now, |app| command.apply(app));
    }

    pub fn tick(&mut self, now: Instant) {
        self.poll_download(now);
        self.poll_lone_preview();
        let Some(app) = self.session.as_mut() else {
            // Voix préparée sans texte : le délai court quand même.
            if self.engine.is_some() && !self.previewing() && self.idle_expired(now) {
                self.rest();
            }
            return;
        };
        app.tick();
        // Le délai ne court qu'à l'arrêt : avec « relâchée dès l'arrêt »
        // (0 min), il serait sinon écoulé à chaque tour, lecture comprise.
        if app.player.is_playing() && !app.is_finished() {
            self.last_activity = now;
        } else if !self.previewing() && self.idle_expired(now) {
            self.rest();
        }
    }

    /// Relâche la voix, la synthèse et la sortie audio ; garde le texte et
    /// la phrase pour reprendre.
    pub fn rest(&mut self) {
        self.drop_lone_preview();
        let Some(mut app) = self.session.take() else {
            self.engine = None;
            return;
        };
        app.remember_position();
        app.flush_config();
        self.config = app.config.clone();
        self.resting_at = Some(app.current_sentence());
        drop(app);
        let language = self.current_language();
        self.resting_doc = self.text.as_ref().map(|t| {
            crate::doc::builder::parse_document_in(&t.markdown, self.config.code_policy(), &language)
        });
        self.engine = None;
    }

    /// Fermeture de l'app : rien de ce qui a été lu ou réglé ne se perd.
    pub fn close(&mut self) {
        if let Some(app) = self.session.as_mut() {
            app.remember_position();
            app.flush_config();
        }
    }

    fn idle_expired(&self, now: Instant) -> bool {
        let minutes = self.config.app.keep_voice_minutes;
        minutes >= 0
            && now.saturating_duration_since(self.last_activity)
                >= Duration::from_secs(minutes as u64 * 60)
    }

    fn ensure_engine(&mut self) -> Arc<dyn TtsEngine> {
        if let Some(engine) = &self.engine {
            return Arc::clone(engine);
        }
        let engine = (self.voices)(&self.config);
        self.engine = Some(Arc::clone(&engine));
        engine
    }

    fn open(&mut self, text: Text, now: Instant) {
        self.drop_lone_preview();
        self.last_activity = now;
        self.resting_at = None;
        self.resting_doc = None;
        let engine = self.ensure_engine();
        let language = self.language_of(&text.markdown, &text.source);
        let (synth, player) = match self.session.take() {
            Some(mut old) => {
                old.remember_position();
                self.config = old.config.clone();
                old.into_parts()
            }
            None => (
                Synthesizer::new(
                    engine,
                    self.config.tts_params_in(&language.effective),
                    self.config.sessions() as usize,
                    self.config.engine.prefetch as usize,
                ),
                (self.players)(),
            ),
        };
        synth.reset();
        let mut builder = DocumentBuilder::with_language(self.config.code_policy(), &language.effective);
        self.language = Some(language);
        for block in crate::input::split_blocks(&text.markdown) {
            builder.push_block(&block);
        }
        let mut app = App::from_parts(
            builder,
            None,
            synth,
            self.config.clone(),
            text.source.clone(),
            Vec::new(),
            player,
        );
        // Le terminal liste les avertissements de l'ouverture (sortie audio
        // indisponible, signets illisibles) ; l'interface ne montre que le
        // statut : ils y passent.
        let notices = std::mem::take(&mut app.notices);
        if !notices.is_empty() {
            app.notify(&notices.join(" · "));
        }
        self.session = Some(app);
        self.text = Some(text);
    }

    /// Rouvre le texte mis au repos, à sa phrase, lecture arrêtée : la
    /// commande qui a réveillé la voix décide de la suite.
    fn wake(&mut self, now: Instant) {
        let Some(at) = self.resting_at else { return };
        let Some(text) = self.text.clone() else { return };
        let identity = text.identity;
        self.open(text, now);
        if let Some(app) = self.session.as_mut() {
            app.player.pause();
            if identity {
                app.resume_where_left();
            }
            app.seek_to(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn reader(keep: i64) -> (Reader, Arc<AtomicUsize>, Instant) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.app.keep_voice_minutes = keep;
        let t0 = Instant::now();
        let reader = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            t0,
        );
        (reader, loads, t0)
    }

    fn written(reader: &Reader) -> String {
        reader.session().unwrap().doc().sentences[0].written()
    }

    const MIN: Duration = Duration::from_secs(60);

    #[test]
    fn two_texts_share_one_voice() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Premier texte.", "a", t0);
        r.open_text("Second texte.", "b", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(r.revision(), 2);
        assert_eq!(written(&r), "Second texte.");
    }

    #[test]
    fn the_second_text_replaces_the_first() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.open_text("Autre.", "b", t0);
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.player.queue_len(), 0);
    }

    #[test]
    fn the_voice_rests_after_the_delay_and_comes_back_in_place() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.tick(t0 + 9 * MIN);
        assert!(r.voice_loaded());
        r.tick(t0 + 11 * MIN);
        assert!(!r.voice_loaded());
        assert!(r.is_resting());

        // Un déplacement réveille la voix, sans lancer la lecture.
        r.with_session(t0 + 12 * MIN, |app| app.seek_sentences(-1));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(r.revision(), 1, "même document : pas de nouvelle révision");
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 1);
        assert!(!app.player.is_playing());
    }

    #[test]
    fn a_prepared_voice_without_text_is_released_after_the_delay() {
        let (mut r, _, t0) = reader(10);
        r.prepare_voice(t0);
        r.tick(t0 + 9 * MIN);
        assert!(r.voice_loaded());
        r.tick(t0 + 11 * MIN);
        assert!(!r.voice_loaded());
        assert!(!r.is_resting(), "pas de texte à reprendre");
    }

    #[test]
    fn a_prepared_voice_kept_forever_stays_loaded() {
        let (mut r, _, t0) = reader(-1);
        r.prepare_voice(t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
    }

    #[test]
    fn keeping_forever_never_rests() {
        let (mut r, _, t0) = reader(-1);
        r.open_text("Un.", "a", t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
    }

    #[test]
    fn released_at_stop_never_rests_while_reading() {
        let (mut r, _, t0) = reader(0);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.toggle_play());
        assert!(r.session().unwrap().player.is_playing());
        r.tick(t0 + MIN);
        assert!(r.voice_loaded(), "la lecture en cours garde sa voix");
        assert!(!r.is_resting());
    }

    #[test]
    fn released_at_stop_rests_once_paused() {
        let (mut r, _, t0) = reader(0);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.toggle_play());
        r.with_session(t0, |app| app.toggle_play());
        assert!(!r.session().unwrap().player.is_playing());
        r.tick(t0);
        assert!(!r.voice_loaded());
        assert!(r.is_resting());
    }

    #[test]
    fn pausing_a_resting_text_does_not_wake_the_voice() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply(command::Command::Pause, t0 + MIN);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "Échap ne recharge pas la voix");
        assert!(r.is_resting());
        assert!(!r.voice_loaded());
    }

    #[test]
    fn pausing_without_a_text_does_nothing() {
        let (mut r, loads, t0) = reader(10);
        r.apply(command::Command::Pause, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        assert!(r.session().is_none());
    }

    #[test]
    fn other_commands_still_wake_a_resting_text() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply(command::Command::Sentences(1), t0);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(r.session().unwrap().current_sentence(), 1);
    }

    fn bookmark(document: &str, sentence_id: usize) -> crate::bookmark::Bookmark {
        crate::bookmark::Bookmark {
            document: document.to_string(),
            sentence_id,
            block_id: 0,
            section_title: String::new(),
            snippet: String::new(),
            created_at: 0,
        }
    }

    #[test]
    fn each_captured_text_has_its_own_identity() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Premier texte.", "sélection", t0);
        let first = r.document_identity().unwrap().to_string();
        r.open_text("Second texte.", "sélection", t0);
        let second = r.document_identity().unwrap().to_string();
        assert_ne!(first, second, "deux sélections ne partagent pas leurs signets");
        assert!(first.starts_with("sélection:"));
        r.open_text("Premier texte.", "sélection", t0);
        assert_eq!(r.document_identity().unwrap(), first, "le même texte se retrouve");
        assert_eq!(r.session().unwrap().source, first);
    }

    #[test]
    fn a_resting_text_reads_its_own_bookmarks() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux.", "sélection", t0);
        let identity = r.document_identity().unwrap().to_string();
        r.rest();
        assert_eq!(r.document_identity(), Some(identity.as_str()));
        let store = crate::bookmark::BookmarkStore {
            items: vec![bookmark(&identity, 1), bookmark("sélection:autre", 0)],
        };
        let found = r.bookmarks_in(&store);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].sentence_id, 1);
    }

    #[test]
    fn notices_of_the_opening_reach_the_status() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        let app = r.session().unwrap();
        // La sortie muette des essais se dit indisponible : l'interface le
        // montre au lieu de le garder dans une liste qu'elle ne lit pas.
        assert!(app.status().contains("sortie audio indisponible"), "{}", app.status());
        assert!(app.notices.is_empty(), "dites une seule fois");
    }

    #[test]
    fn speed_carries_over_to_the_next_text() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        r.with_session(t0, |app| app.set_speed(1.5));
        r.open_text("Deux.", "b", t0);
        assert_eq!(r.session().unwrap().config.voice.speed, 1.5);
    }

    #[test]
    fn preparing_the_voice_loads_it_once() {
        let (mut r, loads, t0) = reader(10);
        r.prepare_voice(t0);
        r.prepare_voice(t0);
        r.open_text("Un.", "a", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
