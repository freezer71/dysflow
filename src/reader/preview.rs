//! « Écouter » depuis l'app : la phrase d'exemple avec la voix et les
//! réglages en cours, texte ouvert ou non.
//!
//! Avec un texte, l'aperçu passe par sa session (elle a déjà sa voix et sa
//! sortie). Sans texte, le lecteur charge la voix au besoin et garde un
//! aperçu à lui, relâché dès qu'un texte s'ouvre ou que la voix se repose.

use std::sync::Arc;
use std::time::Instant;

use crate::audio::Player;
use crate::tts::preview::{self as sample, Previewer};

use super::Reader;

/// L'aperçu d'un lecteur sans texte : sa fabrique et sa sortie.
pub(super) struct LonePreview {
    previewer: Previewer,
    player: Player,
}

/// Ce que l'aperçu fait, pour l'interface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreviewView {
    /// La phrase d'exemple se synthétise.
    pub pending: bool,
    /// Elle sonne.
    pub playing: bool,
    /// Le dernier échec de synthèse.
    pub error: Option<String>,
    /// L'aperçu attend que sa voix se charge (bibliothèque des voix : un
    /// autre modèle que l'actif, quelques secondes la première fois).
    pub preparing: bool,
}

impl Reader {
    /// La phrase d'exemple dans `language` : chaque voix s'écoute dans la
    /// sienne. Un texte au repos se réveille ; sans texte, la voix se charge.
    /// Rend la raison d'un refus.
    pub fn preview_voice_in(&mut self, language: &str, now: Instant) -> Result<(), String> {
        self.last_activity = now;
        self.preview_error = None;
        self.wake(now);
        if let Some(app) = self.session.as_mut() {
            return app.request_preview_in(language);
        }
        let engine = self.ensure_engine();
        if self.lone_preview.is_none() {
            let player = (self.players)(&self.config.app.output_device);
            self.lone_preview = Some(LonePreview { previewer: Previewer::new(Arc::clone(&engine)), player });
        }
        let lone = self.lone_preview.as_mut().expect("posé juste au-dessus");
        lone.player.set_volume(self.config.voice.volume);
        let label = format!("{} · {:.2}×", self.config.voice.voice, self.config.voice.speed);
        sample::start(
            &mut lone.previewer,
            &mut lone.player,
            &self.config,
            language,
            label,
        )
    }

    /// Coupe l'aperçu, qu'il se prépare ou qu'il sonne (celui de la
    /// bibliothèque des voix compris).
    pub fn stop_preview(&mut self) {
        self.stop_library_preview();
        if let Some(app) = self.session.as_mut() {
            app.stop_preview();
        }
        if let Some(lone) = self.lone_preview.as_mut() {
            lone.previewer.cancel();
            lone.player.stop_preview();
        }
    }

    /// L'aperçu des réglages et celui de la bibliothèque des voix, ensemble ;
    /// la bibliothèque ouverte, l'échec dit est le sien.
    pub fn preview_view(&self) -> PreviewView {
        let base = self.base_preview_view();
        match self.library_preview_state() {
            Some((pending, playing, preparing, error)) => PreviewView {
                pending: base.pending || pending,
                playing: base.playing || playing,
                // Seul son échec : un vieil échec d'« Écouter » ne s'y montre pas.
                error,
                preparing,
            },
            None => base,
        }
    }

    fn base_preview_view(&self) -> PreviewView {
        if let Some(app) = self.session.as_ref() {
            return PreviewView {
                pending: app.preview_pending(),
                playing: app.preview_playing(),
                error: app.preview_error.clone(),
                preparing: false,
            };
        }
        match &self.lone_preview {
            Some(lone) => PreviewView {
                pending: lone.previewer.pending_for().is_some(),
                playing: lone.player.preview_playing(),
                error: self.preview_error.clone(),
                preparing: false,
            },
            None => PreviewView { error: self.preview_error.clone(), ..PreviewView::default() },
        }
    }

    /// Un aperçu se prépare ou sonne : le fil doit rester vif.
    pub fn previewing(&self) -> bool {
        let view = self.preview_view();
        view.pending || view.playing
    }

    /// Joue l'aperçu sans texte dès qu'il est prêt.
    pub(super) fn poll_lone_preview(&mut self) {
        let Some(lone) = self.lone_preview.as_mut() else { return };
        match lone.previewer.poll() {
            Some(Ok(audio)) => lone.player.play_preview(&audio),
            Some(Err(err)) => {
                self.preview_error = Some(crate::tr!(en = "preview failed: {err}", fr = "aperçu impossible : {err}"))
            }
            None => {}
        }
        // Le lecteur de l'aperçu ferme sa sortie audio, une fois l'aperçu
        // dit, comme celui de la lecture.
        let _ = lone.player.reconcile();
    }

    /// Relâche l'aperçu sans texte. Son arrêt attend la synthèse en cours :
    /// hors de ce fil, comme `App::reload_engine`.
    pub(super) fn drop_lone_preview(&mut self) {
        if let Some(LonePreview { previewer, player }) = self.lone_preview.take() {
            crate::sync::spawn_named("dysflow-liberation", move || drop(previewer));
            drop(player);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::LockExt;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::{TtsEngine, TtsSession};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    fn reader_with(engine: fn() -> Arc<dyn TtsEngine>) -> (Reader, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let r = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                engine()
            }),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        );
        (r, loads)
    }

    fn silent() -> Arc<dyn TtsEngine> {
        Arc::new(SilentEngine)
    }

    #[test]
    fn a_preview_without_text_loads_the_voice_and_explains_a_missing_output() {
        let (mut r, loads) = reader_with(silent);
        let err = r.preview_voice_in("fr", Instant::now()).unwrap_err();
        assert_eq!(err, sample::AUDIO_MISSING.get());
        assert_eq!(loads.load(Ordering::SeqCst), 1, "la voix se charge au besoin");
        assert!(r.voice_loaded());
        assert!(r.session().is_none(), "aucun texte n'est ouvert pour autant");
        assert!(!r.previewing());
    }

    #[test]
    fn a_preview_with_an_open_text_goes_through_the_session() {
        let (mut r, loads) = reader_with(silent);
        r.open_text("Un. Deux.", "a", Instant::now());
        let err = r.preview_voice_in("fr", Instant::now()).unwrap_err();
        assert_eq!(err, sample::AUDIO_MISSING.get());
        assert_eq!(loads.load(Ordering::SeqCst), 1, "la voix du texte sert");
        assert!(r.lone_preview.is_none(), "pas d'aperçu à part");
    }

    #[test]
    fn a_resting_text_wakes_for_a_preview() {
        let (mut r, loads) = reader_with(silent);
        r.open_text("Un. Deux.", "a", Instant::now());
        r.rest();
        let _ = r.preview_voice_in("fr", Instant::now());
        assert!(!r.is_resting());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn opening_a_text_drops_the_lone_preview() {
        let (mut r, _) = reader_with(silent);
        let _ = r.preview_voice_in("fr", Instant::now());
        assert!(r.lone_preview.is_some());
        r.open_text("Un.", "a", Instant::now());
        assert!(r.lone_preview.is_none());
    }

    #[test]
    fn stopping_without_a_preview_is_harmless() {
        let (mut r, _) = reader_with(silent);
        r.stop_preview();
        assert_eq!(r.preview_view(), PreviewView::default());
    }

    /// Un moteur dont la synthèse attend le feu vert du test.
    struct GatedEngine {
        gate: Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
    }
    struct GatedSession(Arc<Mutex<std::sync::mpsc::Receiver<()>>>);
    impl TtsEngine for GatedEngine {
        fn name(&self) -> String {
            "à feu vert".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn TtsSession>> {
            Ok(Box::new(GatedSession(Arc::clone(&self.gate))))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }
    impl TtsSession for GatedSession {
        fn synthesize(&mut self, text: &str, params: &crate::tts::TtsParams) -> anyhow::Result<crate::tts::PcmBuffer> {
            let _ = self.0.locked().recv();
            SilentEngine.open_session()?.synthesize(text, params)
        }
    }

    #[test]
    fn a_preview_in_progress_is_not_cut_by_the_voice_resting() {
        let (release, gate) = std::sync::mpsc::channel();
        let gate = Arc::new(Mutex::new(gate));
        let engine: Arc<dyn TtsEngine> = Arc::new(GatedEngine { gate });
        let mut config = Config::default();
        config.app.keep_voice_minutes = 0;
        let voice = Arc::clone(&engine);
        let mut r = Reader::new(config, Box::new(move |_| Arc::clone(&voice)), Box::new(|_: &str| Player::silent()), Instant::now());
        r.set_persistent(false);
        let t0 = Instant::now();
        r.ensure_engine();
        // Un aperçu à lui, qui se prépare : le délai de garde est nul.
        let mut previewer = Previewer::new(engine);
        previewer.request("Un.".into(), r.config.tts_params(), "essai".into()).unwrap();
        r.lone_preview = Some(LonePreview { previewer, player: Player::silent() });
        assert!(r.previewing());
        r.tick(t0 + Duration::from_secs(5));
        assert!(r.engine().is_some(), "la voix ne se repose pas sous un aperçu");
        release.send(()).unwrap();
        // L'aperçu fini et joué (sortie muette : rien ne sonne), la voix peut se reposer.
        for _ in 0..200 {
            r.tick(t0 + Duration::from_secs(10));
            if !r.previewing() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        r.tick(t0 + Duration::from_secs(10));
        assert!(r.engine().is_none(), "au repos une fois l'aperçu fini");
    }
}
