//! Le lecteur sur son propre fil.
//!
//! Les commandes arrivent par un canal et sont appliquées aussitôt, puis un
//! instantané est publié. Entre deux, le fil fait tourner la lecture toutes
//! les 16 ms tant que quelque chose avance seul, veille à la seconde quand il
//! n'y a qu'un délai à surveiller, et dort sinon. Le document n'est republié
//! qu'à chaque nouvelle révision.

use crate::sync::LockExt;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::command::Command;
use super::view::{self, DocView, Snapshot};
use super::Reader;
use crate::tts::Readiness;

const TICK: Duration = Duration::from_millis(16);
/// Voix chargée mais rien qui avance : le fil se contente de veiller, pour
/// que la mise au repos de la voix et le sondage restent assurés.
const TICK_IDLE: Duration = Duration::from_secs(1);

/// Ce qui règle la cadence du fil, lu sur le lecteur.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Activity {
    /// Une lecture avance (lancée, pas arrivée au bout).
    reading: bool,
    /// La voix se charge : son état change sans commande.
    loading: bool,
    /// Un téléchargement est en cours.
    downloading: bool,
    /// Un téléchargement fini, échoué ou annulé attend d'être sondé.
    download_settled: bool,
    /// Une voix est chargée : son délai de garde court.
    engine: bool,
    /// L'aperçu de la bibliothèque tient un modèle (sans moteur de lecture,
    /// parfois) : son délai de garde court aussi.
    library_model: bool,
    /// Un aperçu se prépare ou sonne : il faut le sonder.
    previewing: bool,
}

impl Activity {
    fn of(reader: &Reader) -> Self {
        let reading = reader
            .session()
            .is_some_and(|app| app.player.is_playing() && !app.is_finished());
        let loading = !reader.is_resting()
            && reader.engine().is_some_and(|e| matches!(e.readiness(), Readiness::Loading));
        let (downloading, download_settled) = match reader.download.as_ref() {
            Some(d) if d.is_finished() => (false, true),
            Some(_) => (true, false),
            None => (false, false),
        };
        Self {
            reading,
            loading,
            downloading,
            download_settled,
            engine: reader.engine().is_some(),
            library_model: reader.library_preview.is_some(),
            previewing: reader.previewing(),
        }
    }
}

/// Cadence du fil : vive seulement quand quelque chose avance sans commande
/// (lecture, chargement, téléchargement) ; lente quand il n'y a qu'un délai
/// à surveiller ; `None` quand rien ne peut changer avant la prochaine
/// requête — le fil dort alors jusqu'à elle.
fn cadence(a: Activity) -> Option<Duration> {
    if a.reading || a.loading || a.downloading || a.previewing {
        Some(TICK)
    } else if a.engine || a.library_model || a.download_settled {
        Some(TICK_IDLE)
    } else {
        None
    }
}

fn tick_interval(reader: &Reader) -> Option<Duration> {
    cadence(Activity::of(reader))
}

pub type ReplySender = Sender<Result<(), String>>;
pub type ReplyReceiver = Receiver<Result<(), String>>;

/// Canal de réponse d'un `OpenFile`, pour qui n'a pas `crossbeam` sous la main.
pub fn reply_channel() -> (ReplySender, ReplyReceiver) {
    crossbeam_channel::bounded(1)
}

pub enum Request {
    OpenText { markdown: String, source: String },
    OpenFile { path: PathBuf, reply: Sender<Result<(), String>> },
    PrepareVoice,
    Rest,
    Command(Command),
    Ask(Box<dyn FnOnce(&mut Reader) + Send>),
    Shutdown,
}

#[derive(Default)]
struct Published {
    snapshot: Snapshot,
    document: Arc<DocView>,
}

pub struct ReaderHandle {
    tx: Sender<Request>,
    published: Arc<Mutex<Published>>,
    thread: Option<JoinHandle<()>>,
}

impl ReaderHandle {
    pub fn spawn<F>(build: F) -> Self
    where
        F: FnOnce() -> Reader + Send + 'static,
    {
        let (tx, rx) = crossbeam_channel::unbounded();
        let published = Arc::new(Mutex::new(Published::default()));
        let out = Arc::clone(&published);
        let thread = thread::Builder::new()
            .name("sillon-lecteur".into())
            .spawn(move || run(build(), rx, out))
            .expect("fil du lecteur");
        Self { tx, published, thread: Some(thread) }
    }

    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.published.locked().snapshot.clone()
    }

    pub fn document(&self) -> Arc<DocView> {
        Arc::clone(&self.published.locked().document)
    }

    /// Exécute `f` sur le fil du lecteur et rend son résultat.
    ///
    /// BLOQUE l'appelant jusqu'à ce que le fil du lecteur ait répondu (au plus
    /// 5 s, puis `None`) : un travail long (ouvrir un fichier, reconnaître une
    /// image) ne doit donc pas être demandé depuis le fil de l'interface.
    pub fn ask<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Reader) -> T + Send + 'static,
    ) -> Option<T> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.send(Request::Ask(Box::new(move |reader| {
            let _ = reply.send(f(reader));
        })));
        answer.recv_timeout(Duration::from_secs(5)).ok()
    }
}

impl Drop for ReaderHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(mut reader: Reader, rx: Receiver<Request>, out: Arc<Mutex<Published>>) {
    let mut revision = u64::MAX;
    'outer: loop {
        let received = match tick_interval(&reader) {
            Some(interval) => rx.recv_timeout(interval),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(request) => {
                if !handle(&mut reader, request) {
                    break 'outer;
                }
                // Les touches répétées arrivent en rafale : tout appliquer
                // avant de publier.
                while let Ok(request) = rx.try_recv() {
                    if !handle(&mut reader, request) {
                        break 'outer;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        reader.tick(Instant::now());
        publish(&reader, &out, &mut revision);
    }
    reader.close();
}

/// Applique une requête ; `false` quand il faut s'arrêter.
fn handle(reader: &mut Reader, request: Request) -> bool {
    let now = Instant::now();
    match request {
        Request::OpenText { markdown, source } => reader.open_text(&markdown, &source, now),
        Request::OpenFile { path, reply } => {
            let _ = reply.send(reader.open_file(&path, now).map_err(|e| e.to_string()));
        }
        Request::PrepareVoice => reader.prepare_voice(now),
        Request::Rest => reader.rest(),
        Request::Command(command) => reader.apply(command, now),
        Request::Ask(f) => f(reader),
        Request::Shutdown => return false,
    }
    true
}

fn publish(reader: &Reader, out: &Mutex<Published>, revision: &mut u64) {
    let snapshot = view::snapshot(reader);
    let document = (snapshot.revision != *revision).then(|| Arc::new(view::document(reader)));
    let mut out = out.locked();
    if let Some(document) = document {
        *revision = snapshot.revision;
        out.document = document;
    }
    out.snapshot = snapshot;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::time::Duration;

    #[test]
    fn a_preview_keeps_the_reader_thread_awake() {
        // Sans texte ni lecture, le fil dormirait : l'aperçu prêt ne serait
        // jamais sondé, donc jamais joué.
        let none = Activity::default();
        assert_eq!(cadence(Activity { previewing: true, ..none }), Some(TICK));
        assert_eq!(cadence(Activity { previewing: true, engine: true, ..none }), Some(TICK));
    }

    fn silent_handle() -> ReaderHandle {
        ReaderHandle::spawn(|| {
            let mut config = Config::default();
            config.reading.autoplay = false;
            Reader::new(
                config,
                Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                Box::new(|_: &str| Player::silent()),
                Instant::now(),
            )
        })
    }

    fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(Instant::now() < deadline, "{what} : jamais vu");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_opened_text_is_published_with_its_document() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un. Deux.".into(), source: "t".into() });
        eventually("révision 1", || h.snapshot().revision == 1);
        assert_eq!(h.document().sentence_count, 2);
    }

    #[test]
    fn commands_reach_the_reader() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un. Deux. Trois.".into(), source: "t".into() });
        h.send(Request::Command(Command::Sentences(2)));
        eventually("phrase 2", || h.snapshot().sentence == 2);
    }

    #[test]
    fn ask_returns_a_value_from_the_reader_thread() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un.".into(), source: "t".into() });
        let revision = h.ask(|r| r.revision());
        assert_eq!(revision, Some(1));
    }

    #[test]
    fn dropping_the_handle_stops_the_thread() {
        let h = silent_handle();
        drop(h); // ne doit pas bloquer
    }

    fn quiet_reader() -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    const FAST: Option<Duration> = Some(Duration::from_millis(16));
    const SLOW: Option<Duration> = Some(Duration::from_secs(1));

    #[test]
    fn the_cadence_is_fast_only_while_something_moves_on_its_own() {
        let none = Activity::default();
        assert_eq!(cadence(Activity { reading: true, engine: true, ..none }), FAST);
        assert_eq!(cadence(Activity { loading: true, engine: true, ..none }), FAST);
        assert_eq!(cadence(Activity { downloading: true, ..none }), FAST);
        // Un texte ouvert en pause, ou fini : seul le délai de la voix court.
        assert_eq!(cadence(Activity { engine: true, ..none }), SLOW);
        // Un téléchargement échoué ou annulé ne garde pas le rythme vif.
        assert_eq!(cadence(Activity { download_settled: true, ..none }), SLOW);
        // Ni voix ni téléchargement : le fil attend la prochaine requête.
        assert_eq!(cadence(none), None);
    }

    #[test]
    fn a_paused_text_ticks_slowly_and_a_resting_one_sleeps() {
        let mut reader = quiet_reader();
        assert_eq!(tick_interval(&reader), None);
        reader.prepare_voice(Instant::now());
        assert_eq!(tick_interval(&reader), SLOW, "voix préparée : son délai court");
        reader.open_text("Un. Deux.", "t", Instant::now());
        assert_eq!(tick_interval(&reader), SLOW, "texte en pause");
        reader.with_session(Instant::now(), |app| app.toggle_play());
        assert_eq!(tick_interval(&reader), FAST, "lecture en cours");
        reader.with_session(Instant::now(), |app| app.toggle_play());
        assert_eq!(tick_interval(&reader), SLOW);
        reader.rest();
        assert_eq!(tick_interval(&reader), None);
    }

    #[test]
    fn a_model_held_by_the_library_preview_keeps_the_slow_tick() {
        // Sans moteur de lecture, seul l'aperçu de la bibliothèque tient un
        // modèle : le fil doit veiller, sinon son délai de garde ne court
        // jamais et le modèle reste chargé indéfiniment.
        let none = Activity::default();
        assert_eq!(cadence(Activity { library_model: true, ..none }), SLOW);
        let mut reader = quiet_reader();
        super::super::library::settled_library_preview(&mut reader);
        assert!(!reader.previewing(), "aperçu fini : plus rien à sonder");
        assert_eq!(tick_interval(&reader), SLOW, "le modèle de l'aperçu a un délai de garde");
        reader.rest();
        assert_eq!(tick_interval(&reader), None, "relâché : le fil peut dormir");
    }

    #[test]
    fn the_idle_delay_rests_a_model_held_by_the_library_preview() {
        let mut reader = quiet_reader();
        reader.config.app.keep_voice_minutes = 0;
        super::super::library::settled_library_preview(&mut reader);
        reader.tick(Instant::now() + Duration::from_secs(1));
        assert!(reader.preview_model_loaded().is_none(), "le délai écoulé, le modèle de l'aperçu part");
    }

    #[test]
    fn a_sleeping_thread_still_answers_requests() {
        let h = ReaderHandle::spawn(quiet_reader);
        // Fil endormi sans délai : une requête le réveille.
        assert_eq!(h.ask(|r| r.revision()), Some(0));
        h.send(Request::OpenText { markdown: "Un.".into(), source: "t".into() });
        eventually("révision 1", || h.snapshot().revision == 1);
    }
}
