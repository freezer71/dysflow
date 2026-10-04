//! Le lecteur sur son propre fil.
//!
//! Les commandes arrivent par un canal et sont appliquées aussitôt ; entre
//! deux, le fil fait tourner la lecture et publie un instantané toutes les
//! 16 ms. Le document n'est republié qu'à chaque nouvelle révision.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::command::Command;
use super::view::{self, DocView, Snapshot};
use super::Reader;

const TICK: Duration = Duration::from_millis(16);
/// Sans texte à lire ni téléchargement, le fil se contente de veiller : la
/// mise au repos de la voix et le sondage restent assurés.
const TICK_IDLE: Duration = Duration::from_secs(1);

/// Cadence du fil : vive tant qu'un texte est ouvert, lente sinon.
fn tick_interval(reader: &Reader) -> Duration {
    if reader.session().is_some() || reader.download().is_some() {
        TICK
    } else {
        TICK_IDLE
    }
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
        self.published.lock().unwrap().snapshot.clone()
    }

    pub fn document(&self) -> Arc<DocView> {
        Arc::clone(&self.published.lock().unwrap().document)
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
        match rx.recv_timeout(tick_interval(&reader)) {
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
        Request::Command(command) => reader.with_session(now, |app| command.apply(app)),
        Request::Ask(f) => f(reader),
        Request::Shutdown => return false,
    }
    true
}

fn publish(reader: &Reader, out: &Mutex<Published>, revision: &mut u64) {
    let snapshot = view::snapshot(reader);
    let document = (snapshot.revision != *revision).then(|| Arc::new(view::document(reader)));
    let mut out = out.lock().unwrap();
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

    fn silent_handle() -> ReaderHandle {
        ReaderHandle::spawn(|| {
            let mut config = Config::default();
            config.reading.autoplay = false;
            Reader::new(
                config,
                Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                Box::new(Player::silent),
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

    #[test]
    fn the_thread_ticks_fast_with_a_text_and_slowly_without() {
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut reader = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        assert_eq!(tick_interval(&reader), Duration::from_secs(1));
        reader.open_text("Un.", "t", Instant::now());
        assert_eq!(tick_interval(&reader), Duration::from_millis(16));
        reader.rest();
        assert_eq!(tick_interval(&reader), Duration::from_secs(1));
    }
}
