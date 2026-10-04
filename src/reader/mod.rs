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
        }
    }

    pub fn session(&self) -> Option<&App> {
        self.session.as_ref()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Change à chaque nouveau document — pas à chaque réveil de la voix.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn voice_loaded(&self) -> bool {
        self.engine.is_some()
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

    pub fn open_text(&mut self, markdown: &str, source: &str, now: Instant) {
        self.open(
            Text { markdown: markdown.to_string(), source: source.to_string(), identity: false },
            now,
        );
        self.revision += 1;
    }

    pub fn open_file(&mut self, path: &Path, now: Instant) -> anyhow::Result<()> {
        let markdown = crate::source::load(path)?;
        let source = crate::launch::document_identity(path);
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

    pub fn tick(&mut self, now: Instant) {
        let Some(app) = self.session.as_mut() else { return };
        app.tick();
        if app.player.is_playing() && !app.is_finished() {
            self.last_activity = now;
        }
        if self.idle_expired(now) {
            self.rest();
        }
    }

    /// Relâche la voix, la synthèse et la sortie audio ; garde le texte et
    /// la phrase pour reprendre.
    pub fn rest(&mut self) {
        let Some(mut app) = self.session.take() else {
            self.engine = None;
            return;
        };
        app.remember_position();
        app.flush_config();
        self.config = app.config.clone();
        self.resting_at = Some(app.current_sentence());
        drop(app);
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
        self.last_activity = now;
        self.resting_at = None;
        let engine = self.ensure_engine();
        let (synth, player) = match self.session.take() {
            Some(mut old) => {
                old.remember_position();
                self.config = old.config.clone();
                old.into_parts()
            }
            None => (
                Synthesizer::new(
                    engine,
                    self.config.tts_params(),
                    self.config.sessions() as usize,
                    self.config.engine.prefetch as usize,
                ),
                (self.players)(),
            ),
        };
        synth.reset();
        let mut builder = DocumentBuilder::new(self.config.code_policy());
        for block in crate::input::split_blocks(&text.markdown) {
            builder.push_block(&block);
        }
        let app = App::from_parts(
            builder,
            None,
            synth,
            self.config.clone(),
            text.source.clone(),
            Vec::new(),
            player,
        );
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
    fn keeping_forever_never_rests() {
        let (mut r, _, t0) = reader(-1);
        r.open_text("Un.", "a", t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
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
