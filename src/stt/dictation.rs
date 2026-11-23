//! La dictée : l'audio arrive par morceaux, le texte part en événements.
//!
//! Port de `MLXDictationEngine.swift` et `StreamingDictationEngine.swift`,
//! sans micro ni interface. Un fil par dictée : `push` y dépose l'audio sans
//! jamais attendre, le fil charge le modèle (l'audio attend, rien n'est
//! perdu), puis lit.
//!
//! - `Engine::Segmented` : la découpe aux silences (`Segmenter`). Chaque
//!   énoncé clos est lu une dernière fois et son texte devient définitif
//!   (`Committed`) ; dans l'énoncé ouvert, une relecture provisoire au rythme
//!   de `Rolling` : la première tout de suite, puis toutes les
//!   `max(0,4 s, 3 × la dernière lecture)`, sur 1 600 échantillons au moins.
//!   Le coût d'une relecture est borné par la longueur d'un énoncé, pas par
//!   celle de la dictée. Sur une machine lente, les énoncés clos passent
//!   avant toute relecture provisoire : le texte arrive phrase par phrase.
//! - `Engine::Streaming` : tout ce qui est arrivé part au flux, par pas de
//!   80 ms ; chaque échantillon n'est lu qu'une fois.

use crate::stt::detector::SpeechDetector;
use crate::stt::recognizer::{Hold, Recognizer, RecognizerStream, StreamingRecognizer};
use crate::stt::rolling::{Clock, Rolling, SystemClock, INITIAL};
use crate::stt::segmenter::{Segment, Segmenter};
use crate::tts::Cancel;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// Moins d'audio que cela (0,1 s) : rien à lire.
pub const MIN_SAMPLES: usize = 1_600;
/// Le pas du flux : un jeton de Voxtral, une trame de Nemotron.
pub const STREAM_STEP: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Juste avant un vrai chargement du modèle.
    Loading,
    /// Le chargement fini, et sa durée.
    Loaded(Duration),
    /// Le modèle est prêt (chargé à l'instant, ou déjà là) et, en flux, le
    /// flux ouvert : une seule fois, avant tout texte ou toute mémoire ;
    /// jamais si le chargement échoue. Un `Failed` qui le suit n'arrête pas
    /// la dictée.
    Ready,
    /// Le texte à montrer : `committed`, définitif, puis `pending`, la
    /// relecture de l'énoncé en cours (ou, en flux, le texte reçu jusqu'ici).
    Partial { committed: String, pending: String },
    /// Le texte définitif d'un énoncé (vide s'il n'y a rien entendu), et le
    /// temps écoulé entre l'arrivée de l'audio qui l'a clos et ce texte.
    Committed { text: String, delay: Duration },
    /// La mémoire du modèle, s'il la connaît, après une lecture.
    Memory(u64),
    /// La fin : tout le texte, les énoncés joints par une espace.
    Ended { text: String },
    /// Un échec, dit à l'utilisateur. Seul un chargement manqué arrête la
    /// dictée ; après un énoncé illisible, la dictée continue et garde ce qui
    /// a été validé.
    Failed(String),
}

pub enum Engine {
    Segmented(Arc<dyn Recognizer>, Box<dyn SpeechDetector>),
    Streaming(Arc<dyn StreamingRecognizer>),
}

/// Une dictée en cours. `sink` est appelé sur le fil de la dictée, jamais
/// après `cancel` ; il ne doit pas rappeler la dictée.
pub struct Dictation {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<Result<String>>>>,
}

struct Shared {
    inbox: Mutex<Inbox>,
    wake: Condvar,
    cancelled: Arc<AtomicBool>,
    sink: Sink,
    clock: Arc<dyn Clock>,
    /// L'audio que le fil a pris mais pas encore lu (`Work::queued`), publié
    /// après chaque pas : de quoi borner `push_bounded`.
    queued: AtomicUsize,
    /// Le fil a vidé l'arrivée ou lu un énoncé : `push_bounded` regarde de
    /// nouveau.
    drained: Condvar,
}

/// L'audio déposé par `push`, avec son heure d'arrivée, et la fin demandée.
#[derive(Default)]
struct Inbox {
    chunks: Vec<(Vec<f32>, Duration)>,
    finish_at: Option<Duration>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cancelled() -> anyhow::Error {
    anyhow!(crate::tr!("dictation cancelled", "dictée annulée"))
}

impl Dictation {
    pub fn start(
        engine: Engine,
        model_dir: &Path,
        language: &str,
        sink: impl Fn(Event) + Send + 'static,
    ) -> Self {
        Self::start_with_clock(
            engine,
            model_dir,
            language,
            sink,
            Arc::new(SystemClock::new()),
        )
    }

    pub(crate) fn start_with_clock(
        engine: Engine,
        model_dir: &Path,
        language: &str,
        sink: impl Fn(Event) + Send + 'static,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let shared = Arc::new(Shared {
            inbox: Mutex::new(Inbox::default()),
            wake: Condvar::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            sink: Sink::new(sink),
            clock,
            queued: AtomicUsize::new(0),
            drained: Condvar::new(),
        });
        let model_dir = model_dir.to_path_buf();
        let language = language.to_string();
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("dysflow-dictee".into())
            .spawn(move || run(engine, &model_dir, &language, &worker_shared));
        let worker = match worker {
            Ok(handle) => Some(handle),
            Err(err) => {
                shared.sink.emit(Event::Failed(crate::tr!(
                    en = "the dictation won't start: {err}",
                    fr = "la dictée ne démarre pas : {err}"
                )));
                None
            }
        };
        Self {
            shared,
            worker: Mutex::new(worker),
        }
    }

    /// Dépose de l'audio (16 kHz mono) ; ne bloque jamais : ni le modèle, ni
    /// une lecture en cours. Après `finish` ou `cancel`, ignoré.
    pub fn push(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let at = self.shared.clock.now();
        let mut inbox = lock(&self.shared.inbox);
        if inbox.finish_at.is_some() || self.shared.cancelled.load(Ordering::SeqCst) {
            return;
        }
        inbox.chunks.push((samples.to_vec(), at));
        drop(inbox);
        self.shared.wake.notify_one();
    }

    /// L'audio qui attend le modèle, en échantillons : déposé par `push` et
    /// pas encore pris, ou pris et pas encore lu (énoncés clos en attente).
    /// L'énoncé encore ouvert n'en fait pas partie : il se referme avec
    /// l'audio qui vient.
    pub fn backlog(&self) -> usize {
        let inbox: usize = lock(&self.shared.inbox)
            .chunks
            .iter()
            .map(|(c, _)| c.len())
            .sum();
        inbox + self.shared.queued.load(Ordering::SeqCst)
    }

    /// `push` pour une source qui peut attendre, un fichier : bloque tant que
    /// plus de `limit` échantillons attendent le modèle (`backlog`), puis
    /// dépose `samples`. La mémoire reste bornée quelle que soit la longueur
    /// de la source : `limit` plus ce morceau, plus les énoncés que le fil
    /// vient de clore (chacun d'au plus 12 s). Jamais pour une capture en
    /// direct, qui ne doit jamais attendre (R41) : `push`, qui ne regarde ni
    /// le fil ni l'attente.
    ///
    /// Rend `true` si `samples` est déposé, `false` si la dictée est arrêtée
    /// et que rien ne sera plus lu : annulée, fin déjà demandée, `finish` déjà
    /// pris, ou fil terminé (le modèle ne s'est pas chargé). Rien n'est alors
    /// déposé : l'appelant cesse de lire sa source et appelle `finish`, qui
    /// rend l'erreur du chargement ou l'annulation. Sans ce signal, l'audio
    /// s'entasserait dans l'arrivée pour un fil qui ne le prendra jamais.
    #[must_use = "false : la dictée est arrêtée, cesser de lire la source"]
    pub fn push_bounded(&self, samples: &[f32], limit: usize) -> bool {
        loop {
            let stopped = self.shared.cancelled.load(Ordering::SeqCst)
                || lock(&self.shared.inbox).finish_at.is_some()
                || lock(&self.worker).as_ref().is_none_or(|w| w.is_finished());
            if stopped {
                return false;
            }
            if self.backlog() <= limit {
                break;
            }
            // Le fil réveille ; le délai couvre une notification manquée.
            let inbox = lock(&self.shared.inbox);
            drop(
                self.shared
                    .drained
                    .wait_timeout(inbox, Duration::from_millis(20)),
            );
        }
        self.push(samples);
        true
    }

    /// Fin de la parole : lit tout ce qui a été dit (le chargement fini, s'il
    /// est en cours), puis rend le texte entier.
    ///
    /// **Bloque** jusqu'à la dernière lecture, des secondes sur une machine
    /// lente : jamais sur le fil d'une interface. `Err` si le modèle ne s'est
    /// pas chargé, si la dictée a été annulée, ou si elle est déjà finie.
    pub fn finish(&self) -> Result<String> {
        {
            let mut inbox = lock(&self.shared.inbox);
            if inbox.finish_at.is_none() {
                inbox.finish_at = Some(self.shared.clock.now());
            }
        }
        self.shared.wake.notify_one();
        let Some(worker) = lock(&self.worker).take() else {
            anyhow::bail!(crate::tr!(
                "the dictation is already over",
                "la dictée est déjà finie"
            ));
        };
        worker.join().unwrap_or_else(|_| {
            Err(anyhow!(crate::tr!(
                "the dictation stopped unexpectedly",
                "la dictée s'est arrêtée d'elle-même"
            )))
        })
    }

    /// Plus aucun événement après le retour : une lecture déjà partie va au
    /// bout, sans effet. N'attend que la fin d'un appel de `sink` en cours.
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::SeqCst);
        self.shared.sink.close();
        let _inbox = lock(&self.shared.inbox);
        self.shared.wake.notify_one();
    }
}

impl Drop for Dictation {
    /// Une dictée laissée tomber sans `finish` est annulée ; son fil finit
    /// seul, sans être attendu.
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Shared {
    /// Prend tout l'audio déposé, et la fin si elle est demandée : sous le
    /// même verrou, l'audio pris avec la fin est tout l'audio.
    fn take(&self) -> (Vec<(Vec<f32>, Duration)>, Option<Duration>) {
        let mut inbox = lock(&self.inbox);
        (std::mem::take(&mut inbox.chunks), inbox.finish_at)
    }

    /// Attend de l'audio, la fin, l'annulation, ou l'échéance `due`.
    fn wait(&self, due: Option<Duration>) {
        let inbox = lock(&self.inbox);
        if !inbox.chunks.is_empty()
            || inbox.finish_at.is_some()
            || self.cancelled.load(Ordering::SeqCst)
        {
            return;
        }
        match due {
            None => drop(self.wake.wait(inbox)),
            Some(due) => {
                let pause = self.clock.pause(due.saturating_sub(self.clock.now()));
                drop(self.wake.wait_timeout(inbox, pause));
            }
        }
    }
}

type Listener = Box<dyn Fn(Event) + Send>;

/// Les événements partent par ici ; `close` le vide : plus rien n'en sort.
struct Sink(Mutex<Option<Listener>>);

impl Sink {
    fn new(sink: impl Fn(Event) + Send + 'static) -> Self {
        Self(Mutex::new(Some(Box::new(sink))))
    }

    fn emit(&self, event: Event) {
        if let Some(sink) = lock(&self.0).as_ref() {
            sink(event);
        }
    }

    fn close(&self) {
        lock(&self.0).take();
    }
}

/// Le fil d'une dictée.
fn run(engine: Engine, model_dir: &Path, language: &str, shared: &Shared) -> Result<String> {
    let cancel = {
        let flag = shared.cancelled.clone();
        Cancel::when(move || flag.load(Ordering::SeqCst))
    };
    let sink = &shared.sink;
    let announce = || sink.emit(Event::Loading);
    // La garde est prise avant le chargement et lâchée à la fin du fil : le
    // modèle n'est jamais libéré au milieu d'une dictée.
    let (mut work, _hold): (Box<dyn Work>, Hold) = match engine {
        Engine::Segmented(recognizer, detector) => {
            let hold = recognizer.hold();
            loaded(recognizer.prepare(model_dir, &announce), sink)?;
            (
                Box::new(Segmented::new(recognizer, detector, language)),
                hold,
            )
        }
        Engine::Streaming(recognizer) => {
            let hold = recognizer.hold();
            loaded(recognizer.prepare(model_dir, &announce), sink)?;
            let stream = recognizer
                .open_stream(language)
                .map_err(|err| failed_to_load(err, sink))?;
            (Box::new(Streaming::new(recognizer, stream)), hold)
        }
    };
    // Prêt : avant toute lecture, jamais après un chargement manqué.
    sink.emit(Event::Ready);
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let (chunks, finish_at) = shared.take();
        for (chunk, at) in &chunks {
            work.accept(chunk, *at, sink);
        }
        publish(&*work, shared);
        if let Some(at) = finish_at {
            let text = work.end(at, &*shared.clock, &cancel, sink);
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            sink.emit(Event::Ended { text: text.clone() });
            return Ok(text);
        }
        let step = work.step(&*shared.clock, &cancel, sink);
        publish(&*work, shared);
        match step {
            Step::Worked => {}
            Step::Idle(due) => shared.wait(due),
        }
    }
}

/// Dit à `push_bounded` ce qui attend encore le modèle.
fn publish(work: &dyn Work, shared: &Shared) {
    shared.queued.store(work.queued(), Ordering::SeqCst);
    shared.drained.notify_all();
}

fn loaded(prepared: Result<Option<Duration>>, sink: &Sink) -> Result<()> {
    match prepared {
        Ok(Some(took)) => sink.emit(Event::Loaded(took)),
        Ok(None) => {}
        Err(err) => return Err(failed_to_load(err, sink)),
    }
    Ok(())
}

fn failed_to_load(err: anyhow::Error, sink: &Sink) -> anyhow::Error {
    let message = crate::tr!(
        en = "the model won't load: {err:#}",
        fr = "le modèle ne se charge pas : {err:#}"
    );
    sink.emit(Event::Failed(message.clone()));
    anyhow!(message)
}

#[derive(Debug, PartialEq)]
enum Step {
    /// Un appel au modèle : regarder de nouveau l'audio arrivé.
    Worked,
    /// Rien à faire avant de l'audio neuf, ou avant l'heure donnée.
    Idle(Option<Duration>),
}

/// Le travail d'une dictée, sans fil ni attente : les tests le mènent pas à
/// pas sur une horloge simulée.
trait Work: Send {
    /// L'audio arrivé à l'heure `at`.
    fn accept(&mut self, chunk: &[f32], at: Duration, sink: &Sink);
    /// Un appel au modèle au plus.
    fn step(&mut self, clock: &dyn Clock, cancel: &Cancel, sink: &Sink) -> Step;
    /// La fin, demandée à l'heure `at` : lit tout ce qui reste, rend le texte.
    fn end(&mut self, at: Duration, clock: &dyn Clock, cancel: &Cancel, sink: &Sink) -> String;
    /// L'audio pris et pas encore lu, en échantillons (`Dictation::backlog`).
    fn queued(&self) -> usize;
    #[cfg(test)]
    fn finals_waiting(&self) -> usize {
        0
    }
    #[cfg(test)]
    fn backlog(&self) -> usize {
        0
    }
}

/// La découpe aux silences.
struct Segmented {
    recognizer: Arc<dyn Recognizer>,
    language: String,
    segmenter: Segmenter,
    /// Les énoncés clos, pas encore lus, avec l'heure de l'audio qui les a
    /// clos.
    finals: VecDeque<(Segment, Duration)>,
    rolling: Rolling,
    committed: String,
    /// Longueur de l'énoncé ouvert à sa dernière relecture : sans audio neuf,
    /// pas de relecture.
    read_len: usize,
    /// Le détecteur a échoué et ne s'est pas repris : l'erreur est déjà dite.
    detector_failing: bool,
}

impl Segmented {
    fn new(
        recognizer: Arc<dyn Recognizer>,
        detector: Box<dyn SpeechDetector>,
        language: &str,
    ) -> Self {
        Self {
            recognizer,
            language: language.to_string(),
            segmenter: Segmenter::new(detector),
            finals: VecDeque::new(),
            rolling: Rolling::new(INITIAL),
            committed: String::new(),
            read_len: 0,
            detector_failing: false,
        }
    }

    fn queue(&mut self, segments: impl IntoIterator<Item = Segment>, at: Duration) {
        for segment in segments {
            self.finals.push_back((segment, at));
            self.read_len = 0;
        }
    }

    /// Une erreur du détecteur est dite une fois, jusqu'à ce qu'il se
    /// reprenne. L'audio en attente est rejugé à l'appel suivant.
    fn detector_failed(&mut self, err: anyhow::Error, sink: &Sink) {
        if !self.detector_failing {
            sink.emit(Event::Failed(crate::tr!(
                en = "the speech detector failed: {err:#}",
                fr = "le détecteur de parole a échoué : {err:#}"
            )));
        }
        self.detector_failing = true;
    }

    /// La dernière lecture d'un énoncé clos. Un échec est dit et l'énoncé
    /// perdu ; ce qui a été validé reste.
    fn read_final(
        &mut self,
        segment: &Segment,
        at: Duration,
        clock: &dyn Clock,
        cancel: &Cancel,
        sink: &Sink,
    ) {
        if segment.samples.len() < MIN_SAMPLES {
            return;
        }
        match self
            .recognizer
            .transcribe(&segment.samples, &self.language, cancel)
        {
            Ok(text) => {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    if !self.committed.is_empty() {
                        self.committed.push(' ');
                    }
                    self.committed.push_str(&text);
                }
                let delay = clock.now().saturating_sub(at);
                sink.emit(Event::Committed { text, delay });
                sink.emit(Event::Partial {
                    committed: self.committed.clone(),
                    pending: String::new(),
                });
            }
            Err(_) if cancel.is_cancelled() => return,
            Err(err) => sink.emit(Event::Failed(crate::tr!(
                en = "a sentence could not be read: {err:#}",
                fr = "une phrase n'a pas pu être lue : {err:#}"
            ))),
        }
        report_memory(self.recognizer.memory(), sink);
    }
}

fn report_memory(memory: Option<u64>, sink: &Sink) {
    if let Some(bytes) = memory {
        sink.emit(Event::Memory(bytes));
    }
}

impl Work for Segmented {
    fn accept(&mut self, chunk: &[f32], at: Duration, sink: &Sink) {
        match self.segmenter.push(chunk) {
            Ok(segments) => {
                self.detector_failing = false;
                self.queue(segments, at);
            }
            Err(err) => self.detector_failed(err, sink),
        }
    }

    fn step(&mut self, clock: &dyn Clock, cancel: &Cancel, sink: &Sink) -> Step {
        // Les textes définitifs d'abord : sur une machine lente, aucune
        // relecture provisoire ne les fait attendre.
        if let Some((segment, at)) = self.finals.pop_front() {
            self.read_final(&segment, at, clock, cancel, sink);
            return Step::Worked;
        }
        let Some(open) = self.segmenter.open() else {
            return Step::Idle(None);
        };
        if open.len() < MIN_SAMPLES || open.len() <= self.read_len {
            return Step::Idle(None);
        }
        let now = clock.now();
        if !self.rolling.should_read(now, false) {
            return Step::Idle(self.rolling.due());
        }
        self.read_len = open.len();
        // Un échec d'une relecture provisoire ne dit rien : la suivante, ou
        // la lecture définitive, la remplace.
        if let Ok(text) = self.recognizer.transcribe(open, &self.language, cancel) {
            self.rolling.adapt(clock.now().saturating_sub(now));
            sink.emit(Event::Partial {
                committed: self.committed.clone(),
                pending: text.trim().to_string(),
            });
        }
        report_memory(self.recognizer.memory(), sink);
        Step::Worked
    }

    /// Après une erreur du détecteur, `Segmenter::finish` rendrait l'erreur
    /// gardée sans clore l'énoncé ouvert, et ne rejuge pas les trames qui
    /// attendent. D'où deux `push(&[])` : le premier rend l'erreur gardée,
    /// le second rejuge ces trames ; puis `finish`, repris une fois s'il rend
    /// encore une erreur gardée (le second ne juge rien : il clôt). Rien de
    /// ce qui a été dit dans un énoncé ouvert n'est perdu.
    fn end(&mut self, at: Duration, clock: &dyn Clock, cancel: &Cancel, sink: &Sink) -> String {
        for _ in 0..2 {
            match self.segmenter.push(&[]) {
                Ok(segments) => self.queue(segments, at),
                Err(err) => self.detector_failed(err, sink),
            }
        }
        let last = match self.segmenter.finish() {
            Ok(last) => last,
            Err(err) => {
                self.detector_failed(err, sink);
                self.segmenter.finish().unwrap_or_else(|err| {
                    self.detector_failed(err, sink);
                    None
                })
            }
        };
        self.queue(last, at);
        while let Some((segment, at)) = self.finals.pop_front() {
            if cancel.is_cancelled() {
                break;
            }
            self.read_final(&segment, at, clock, cancel, sink);
        }
        self.committed.clone()
    }

    fn queued(&self) -> usize {
        self.finals.iter().map(|(s, _)| s.samples.len()).sum()
    }

    #[cfg(test)]
    fn finals_waiting(&self) -> usize {
        self.finals.len()
    }

    #[cfg(test)]
    fn backlog(&self) -> usize {
        self.segmenter.buffered()
            + self
                .finals
                .iter()
                .map(|(s, _)| s.samples.len())
                .sum::<usize>()
    }
}

/// Le flux : tout ce qui est arrivé part au modèle, par pas de 80 ms.
struct Streaming {
    recognizer: Arc<dyn StreamingRecognizer>,
    stream: Box<dyn RecognizerStream>,
    waiting: Vec<f32>,
    next_feed: Duration,
    /// Les morceaux de texte rendus par le flux, bout à bout.
    text: String,
    failing: bool,
}

impl Streaming {
    fn new(recognizer: Arc<dyn StreamingRecognizer>, stream: Box<dyn RecognizerStream>) -> Self {
        Self {
            recognizer,
            stream,
            waiting: Vec::new(),
            next_feed: Duration::ZERO,
            text: String::new(),
            failing: false,
        }
    }

    fn feed(&mut self, sink: &Sink) {
        let samples = std::mem::take(&mut self.waiting);
        match self.stream.feed(&samples) {
            Ok(delta) => {
                self.failing = false;
                if !delta.is_empty() {
                    self.text.push_str(&delta);
                    sink.emit(Event::Partial {
                        committed: String::new(),
                        pending: self.text.trim().to_string(),
                    });
                }
            }
            Err(err) => {
                if !self.failing {
                    sink.emit(Event::Failed(crate::tr!(
                        en = "the speech could not be read: {err:#}",
                        fr = "la parole n'a pas pu être lue : {err:#}"
                    )));
                }
                self.failing = true;
            }
        }
        report_memory(self.recognizer.memory(), sink);
    }
}

impl Work for Streaming {
    fn queued(&self) -> usize {
        self.waiting.len()
    }

    fn accept(&mut self, chunk: &[f32], _at: Duration, _sink: &Sink) {
        self.waiting.extend_from_slice(chunk);
    }

    fn step(&mut self, clock: &dyn Clock, _cancel: &Cancel, sink: &Sink) -> Step {
        if self.waiting.is_empty() {
            return Step::Idle(None);
        }
        let now = clock.now();
        if now < self.next_feed {
            return Step::Idle(Some(self.next_feed));
        }
        self.next_feed = now + STREAM_STEP;
        self.feed(sink);
        Step::Worked
    }

    /// Le reste de l'audio part, puis le flux se ferme et rend le texte
    /// entier ; s'il échoue, le texte reçu jusque-là reste.
    fn end(&mut self, _at: Duration, _clock: &dyn Clock, cancel: &Cancel, sink: &Sink) -> String {
        if !self.waiting.is_empty() && !cancel.is_cancelled() {
            self.feed(sink);
        }
        match self.stream.finish() {
            Ok(text) => text.trim().to_string(),
            Err(err) => {
                sink.emit(Event::Failed(crate::tr!(
                    en = "the end of the speech could not be read: {err:#}",
                    fr = "la fin de la parole n'a pas pu être lue : {err:#}"
                )));
                self.text.trim().to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::detector::FRAME;
    use crate::stt::holder::{idle_delay, Holder, SHORTEST_DELAY};
    use crate::stt::rolling::ManualClock;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Condvar;

    const RATE: usize = 16_000;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn samples(seconds: f64) -> usize {
        (seconds * RATE as f64).round() as usize
    }

    /// Détecteur scripté : une trame est de la parole si elle n'est pas nulle.
    struct Scripted;
    impl SpeechDetector for Scripted {
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
            Ok(frame.iter().any(|x| *x != 0.0))
        }
        fn reset(&mut self) {}
    }

    /// Comme `Scripted`, mais l'appel numéro `from` et les suivants, jusqu'à
    /// `until` exclu, échouent.
    struct Flaky {
        calls: usize,
        from: usize,
        until: usize,
    }
    impl SpeechDetector for Flaky {
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
            self.calls += 1;
            if (self.from..self.until).contains(&self.calls) {
                anyhow::bail!("détecteur en panne");
            }
            Ok(frame.iter().any(|x| *x != 0.0))
        }
        fn reset(&mut self) {}
    }

    /// Une phrase : le marqueur `k` (1 à 99) tenu `seconds` ; le faux modèle
    /// y lit « wk ».
    fn sentence(k: u32, seconds: f64) -> Vec<f32> {
        vec![k as f32 * 0.01; samples(seconds)]
    }

    fn silence(seconds: f64) -> Vec<f32> {
        vec![0.0; samples(seconds)]
    }

    /// Les marqueurs entendus, dans l'ordre, sans répétition de suite.
    fn markers(audio: &[f32]) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for &x in audio {
            if x != 0.0 {
                let k = (x * 100.0).round() as u32;
                if out.last() != Some(&k) {
                    out.push(k);
                }
            }
        }
        out
    }

    fn words(audio: &[f32]) -> String {
        markers(audio)
            .iter()
            .map(|k| format!("w{k}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Un loquet : `wait` attend `open`, 10 s au plus.
    #[derive(Default)]
    struct Latch {
        open: Mutex<bool>,
        changed: Condvar,
    }
    impl Latch {
        fn open(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
        fn wait(&self) {
            let open = self.open.lock().unwrap();
            let _ = self
                .changed
                .wait_timeout_while(open, Duration::from_secs(10), |open| !*open)
                .unwrap();
        }
    }

    /// Le journal d'une dictée : les lectures du faux modèle et les
    /// événements, dans l'ordre où ils arrivent.
    #[derive(Debug, Clone, PartialEq)]
    enum Entry {
        Read(Vec<u32>),
        Said(Event),
    }
    type Log = Arc<Mutex<Vec<Entry>>>;

    fn said(log: &Log) -> Vec<Event> {
        log.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Entry::Said(event) => Some(event.clone()),
                Entry::Read(_) => None,
            })
            .collect()
    }

    fn reads(log: &Log) -> Vec<Vec<u32>> {
        log.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Entry::Read(m) => Some(m.clone()),
                Entry::Said(_) => None,
            })
            .collect()
    }

    fn committed(log: &Log) -> Vec<(String, Duration)> {
        said(log)
            .into_iter()
            .filter_map(|e| match e {
                Event::Committed { text, delay } => Some((text, delay)),
                _ => None,
            })
            .collect()
    }

    fn failures(log: &Log) -> usize {
        said(log)
            .iter()
            .filter(|e| matches!(e, Event::Failed(_)))
            .count()
    }

    /// Le faux modèle par lot : il lit les marqueurs. Une lecture coûte
    /// `fixed + per_second × durée de l'audio` sur l'horloge simulée.
    #[derive(Default)]
    struct Marks {
        clock: Option<Arc<ManualClock>>,
        fixed: Duration,
        per_second: f64,
        /// Une lecture qui contient ce marqueur échoue.
        fail_on: Option<u32>,
        /// La première lecture qui contient ce marqueur ouvre `entered`
        /// puis attend `resume`.
        block_on: Option<u32>,
        blocked: AtomicBool,
        entered: Latch,
        resume: Latch,
        /// Le chargement attend ce loquet.
        load_gate: Option<Arc<Latch>>,
        /// Le chargement échoue.
        broken: bool,
        memory: Option<u64>,
        releases: Arc<AtomicUsize>,
        log: Log,
    }

    impl Recognizer for Marks {
        fn load(&self, _model_dir: &Path) -> Result<()> {
            if let Some(gate) = &self.load_gate {
                gate.wait();
            }
            if self.broken {
                anyhow::bail!("poids tronqués");
            }
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn transcribe(&self, audio: &[f32], _language: &str, _cancel: &Cancel) -> Result<String> {
            let found = markers(audio);
            self.log.lock().unwrap().push(Entry::Read(found.clone()));
            if let Some(k) = self.block_on {
                if found.contains(&k) && !self.blocked.swap(true, Ordering::SeqCst) {
                    self.entered.open();
                    self.resume.wait();
                }
            }
            if let Some(clock) = &self.clock {
                let audio_seconds = audio.len() as f64 / RATE as f64;
                clock
                    .advance(self.fixed + Duration::from_secs_f64(self.per_second * audio_seconds));
            }
            if let Some(k) = self.fail_on {
                if found.contains(&k) {
                    anyhow::bail!("marqueur {k} illisible");
                }
            }
            Ok(words(audio))
        }
        fn release(&self) {
            self.releases.fetch_add(1, Ordering::SeqCst);
        }
        fn memory(&self) -> Option<u64> {
            self.memory
        }
    }

    fn sink_into(log: &Log) -> Sink {
        let log = log.clone();
        Sink::new(move |e| log.lock().unwrap().push(Entry::Said(e)))
    }

    struct Run {
        text: String,
        /// Le plus d'énoncés clos en attente de leur lecture.
        max_waiting: usize,
        /// Le plus d'échantillons gardés : arrivés et pas encore acceptés,
        /// dans la découpe, et dans les énoncés clos en attente.
        max_backlog: usize,
    }

    /// La dictée en temps réel simulé, sur un seul fil : l'audio arrive par
    /// morceaux de `chunk` échantillons, chacun à l'heure où son dernier
    /// échantillon est dit ; le temps de calcul du faux modèle avance
    /// l'horloge, et l'audio arrivé pendant une lecture attend la suivante.
    /// `check(avant, après)` reçoit le nombre d'énoncés clos en attente avant
    /// et après chaque pas de travail.
    fn simulate(
        work: &mut dyn Work,
        clock: &ManualClock,
        audio: &[f32],
        chunk: usize,
        sink: &Sink,
        mut check: impl FnMut(usize, usize),
    ) -> Run {
        let chunks: Vec<&[f32]> = audio.chunks(chunk).collect();
        let arrival = |i: usize| Duration::from_secs_f64(((i + 1) * chunk) as f64 / RATE as f64);
        let arrived = |now: Duration| (0..chunks.len()).take_while(|&i| arrival(i) <= now).count();
        let cancel = Cancel::never();
        let mut next = 0;
        let mut run = Run {
            text: String::new(),
            max_waiting: 0,
            max_backlog: 0,
        };
        loop {
            while next < chunks.len() && arrival(next) <= clock.now() {
                work.accept(chunks[next], arrival(next), sink);
                next += 1;
            }
            let before = work.finals_waiting();
            let step = work.step(clock, &cancel, sink);
            check(before, work.finals_waiting());
            run.max_waiting = run.max_waiting.max(before);
            let inbox: usize = chunks[next..arrived(clock.now()).max(next)]
                .iter()
                .map(|c| c.len())
                .sum();
            run.max_backlog = run.max_backlog.max(work.backlog() + inbox);
            let Step::Idle(due) = step else {
                continue;
            };
            let next_at = (next < chunks.len()).then(|| arrival(next));
            let Some(at) = due.into_iter().chain(next_at).min() else {
                break;
            };
            if at <= clock.now() {
                clock.advance(ms(1));
            } else {
                clock.advance_to(at);
            }
        }
        run.text = work.end(clock.now(), clock, &cancel, sink);
        run
    }

    /// Un modèle simulé : 20 ms, plus 0,05 s par seconde d'audio.
    fn quick(clock: &Arc<ManualClock>, log: &Log) -> Marks {
        Marks {
            clock: Some(clock.clone()),
            fixed: ms(20),
            per_second: 0.05,
            log: log.clone(),
            ..Default::default()
        }
    }

    fn segmented(marks: Marks) -> Segmented {
        Segmented::new(Arc::new(marks), Box::new(Scripted), "fr")
    }

    #[test]
    fn the_partial_rereads_only_the_open_segment() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let mut work = segmented(quick(&clock, &log));
        let audio = [
            silence(0.5),
            sentence(1, 3.0),
            silence(1.0),
            sentence(2, 3.0),
            silence(1.0),
        ]
        .concat();
        let run = simulate(
            &mut work,
            &clock,
            &audio,
            1_600,
            &sink_into(&log),
            |_, _| {},
        );
        assert_eq!(run.text, "w1 w2");
        let reads = reads(&log);
        assert!(
            reads.iter().filter(|m| m.as_slice() == [2]).count() >= 3,
            "la seconde phrase est relue en route : {reads:?}"
        );
        assert!(
            reads.iter().all(|m| !(m.contains(&1) && m.contains(&2))),
            "jamais la première relue avec la seconde : {reads:?}"
        );
        assert!(said(&log).contains(&Event::Partial {
            committed: "w1".into(),
            pending: "w2".into()
        }));
    }

    #[test]
    fn a_committed_segment_is_never_read_again() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let mut work = segmented(quick(&clock, &log));
        let audio = [
            sentence(1, 2.0),
            silence(1.0),
            sentence(2, 2.0),
            silence(1.0),
            sentence(3, 2.0),
            silence(1.0),
        ]
        .concat();
        let run = simulate(
            &mut work,
            &clock,
            &audio,
            1_600,
            &sink_into(&log),
            |_, _| {},
        );
        assert_eq!(run.text, "w1 w2 w3");
        let entries = log.lock().unwrap().clone();
        for k in 1..=3u32 {
            let text = format!("w{k}");
            let commits: Vec<usize> = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, Entry::Said(Event::Committed { text: t, .. }) if *t == text))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(commits.len(), 1, "{text} validé une fois");
            assert!(
                entries[commits[0]..]
                    .iter()
                    .all(|e| !matches!(e, Entry::Read(m) if m.contains(&k))),
                "{text} relu après sa validation"
            );
        }
    }

    /// 10 minutes simulées : 120 phrases de 4 s séparées d'une seconde, un
    /// modèle à 0,1 s par seconde d'audio.
    #[test]
    fn the_final_delay_does_not_grow_with_duration() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let mut work = segmented(Marks {
            per_second: 0.1,
            ..quick(&clock, &log)
        });
        let audio: Vec<f32> = (0..120u32)
            .flat_map(|i| [sentence(1 + i % 99, 4.0), silence(1.0)].concat())
            .collect();
        let run = simulate(
            &mut work,
            &clock,
            &audio,
            1_600,
            &sink_into(&log),
            |_, _| {},
        );
        assert_eq!(run.text.split(' ').count(), 120);
        let delays: Vec<Duration> = committed(&log).into_iter().map(|(_, d)| d).collect();
        assert_eq!(delays.len(), 120);
        let (first, last) = (&delays[..40], &delays[80..]);
        let max = |d: &[Duration]| d.iter().copied().max().unwrap();
        let mean = |d: &[Duration]| d.iter().sum::<Duration>() / d.len() as u32;
        assert!(
            max(last) <= max(first) + ms(100),
            "pire délai : {:?} au début, {:?} à la fin",
            max(first),
            max(last)
        );
        assert!(
            mean(last) <= mean(first) + ms(100),
            "délai moyen : {:?} au début, {:?} à la fin",
            mean(first),
            mean(last)
        );
        assert!(
            max(&delays) <= Duration::from_secs(1),
            "pire délai {:?}",
            max(&delays)
        );
    }

    /// Machine lente : chaque lecture d'une phrase dure plus que la phrase.
    #[test]
    fn pending_finals_go_before_provisional_reads() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let mut work = segmented(Marks {
            fixed: ms(300),
            per_second: 1.0,
            ..quick(&clock, &log)
        });
        let audio: Vec<f32> = (1..=10u32)
            .flat_map(|k| [sentence(k, 1.0), silence(0.6)].concat())
            .collect();
        let run = simulate(
            &mut work,
            &clock,
            &audio,
            1_600,
            &sink_into(&log),
            |before, after| {
                if before > 0 {
                    assert_eq!(
                        after,
                        before - 1,
                        "une relecture provisoire avec un texte définitif en attente"
                    );
                }
            },
        );
        assert_eq!(run.text, "w1 w2 w3 w4 w5 w6 w7 w8 w9 w10");
        assert!(
            run.max_waiting >= 2,
            "la machine n'a pas pris de retard : {}",
            run.max_waiting
        );
        assert!(
            said(&log)
                .iter()
                .any(|e| matches!(e, Event::Partial { pending, .. } if !pending.is_empty())),
            "des relectures provisoires quand rien n'attend"
        );
    }

    /// Avec ou sans mémoire connue du modèle.
    #[test]
    fn memory_is_reported_only_when_the_model_knows_it() {
        for memory in [None, Some(42)] {
            let (clock, log) = (ManualClock::new(), Log::default());
            let mut work = segmented(Marks {
                memory,
                ..quick(&clock, &log)
            });
            let audio = [sentence(1, 2.0), silence(1.0)].concat();
            simulate(
                &mut work,
                &clock,
                &audio,
                1_600,
                &sink_into(&log),
                |_, _| {},
            );
            let reported: Vec<Event> = said(&log)
                .into_iter()
                .filter(|e| matches!(e, Event::Memory(_)))
                .collect();
            match memory {
                None => assert!(reported.is_empty()),
                Some(m) => {
                    assert!(reported.len() >= 2, "une par lecture : {reported:?}");
                    assert!(reported.iter().all(|e| *e == Event::Memory(m)));
                }
            }
        }
    }

    /// Une erreur du détecteur juste avant la fin, dans le même `push` que
    /// la fermeture de la première phrase (l'erreur est gardée) : rien de ce
    /// qui a été dit n'est perdu tant que le détecteur peut le juger.
    #[test]
    fn a_detector_error_before_finish_loses_no_speech() {
        let one = sentence(1, 2.0);
        let gap = silence(1.0);
        let audio = [one.clone(), gap.clone(), sentence(2, 1.0)].concat();
        // Les appels au détecteur comptent de 1 : la trame `i` est l'appel
        // `i + 1`. La trame où la seconde phrase commence, et une au milieu.
        let opening = (one.len() + gap.len()) / FRAME + 1;
        let inside = opening + 10;
        let cases = [
            // Le détecteur se reprend : les trames en attente sont rejugées,
            // la seconde phrase s'ouvre et se lit.
            (opening, opening + 1, "w1 w2"),
            (inside, inside + 1, "w1 w2"),
            // Il ne se reprend pas : la phrase ouverte est close avec tout
            // ce qui attend ; une phrase qu'il n'a jamais vue s'ouvrir ne
            // peut pas l'être.
            (inside, usize::MAX, "w1 w2"),
            (opening, usize::MAX, "w1"),
        ];
        for (from, until, expected) in cases {
            let (clock, log) = (ManualClock::new(), Log::default());
            let detector = Flaky {
                calls: 0,
                from,
                until,
            };
            let mut work = Segmented::new(Arc::new(quick(&clock, &log)), Box::new(detector), "fr");
            let sink = sink_into(&log);
            work.accept(&audio, clock.now(), &sink);
            assert_eq!(
                work.finals_waiting(),
                1,
                "la première phrase close avant l'erreur"
            );
            let text = work.end(clock.now(), &*clock, &Cancel::never(), &sink);
            assert_eq!(text, expected, "appels {from} à {until}");
            assert_eq!(failures(&log), 1, "l'erreur dite une fois");
        }
    }

    /// Le faux flux : il garde tout ce qu'on lui donne, et l'heure de chaque
    /// pas.
    #[derive(Default, Clone)]
    struct Fed {
        samples: Arc<Mutex<Vec<f32>>>,
        steps: Arc<Mutex<Vec<Duration>>>,
    }

    struct Stream {
        fed: Fed,
        clock: Arc<ManualClock>,
    }

    impl RecognizerStream for Stream {
        fn feed(&mut self, audio: &[f32]) -> Result<String> {
            self.fed.samples.lock().unwrap().extend_from_slice(audio);
            self.fed.steps.lock().unwrap().push(self.clock.now());
            Ok(format!("{} ", audio.len()))
        }
        fn finish(&mut self) -> Result<String> {
            Ok(format!(
                "{} échantillons",
                self.fed.samples.lock().unwrap().len()
            ))
        }
    }

    struct Streamer {
        fed: Fed,
        clock: Arc<ManualClock>,
    }

    impl StreamingRecognizer for Streamer {
        fn load(&self, _model_dir: &Path) -> Result<()> {
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn open_stream(&self, _language: &str) -> Result<Box<dyn RecognizerStream>> {
            Ok(Box::new(Stream {
                fed: self.fed.clone(),
                clock: self.clock.clone(),
            }))
        }
        fn release(&self) {}
    }

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 + 1.0) * 1e-5).collect()
    }

    #[test]
    fn streaming_reads_each_sample_once() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let fed = Fed::default();
        let streamer = Streamer {
            fed: fed.clone(),
            clock: clock.clone(),
        };
        let dictation = start(Engine::Streaming(Arc::new(streamer)), &clock, &log);
        let audio = ramp(40_000);
        let mut rest = audio.as_slice();
        for size in [1, 100, 1_599, 5_000, 37, 2_048].into_iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (now, later) = rest.split_at(size.min(rest.len()));
            dictation.push(now);
            rest = later;
            clock.advance(ms(40));
        }
        assert_eq!(dictation.finish().unwrap(), "40000 échantillons");
        assert_eq!(
            *fed.samples.lock().unwrap(),
            audio,
            "chaque échantillon lu une fois, dans l'ordre"
        );
        assert_eq!(
            said(&log).last(),
            Some(&Event::Ended {
                text: "40000 échantillons".into()
            })
        );
    }

    #[test]
    fn streaming_feeds_in_80_ms_steps() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let fed = Fed::default();
        let streamer = Arc::new(Streamer {
            fed: fed.clone(),
            clock: clock.clone(),
        });
        let stream = streamer.open_stream("fr").unwrap();
        let mut work = Streaming::new(streamer, stream);
        let audio = ramp(samples(3.0));
        let run = simulate(&mut work, &clock, &audio, 160, &sink_into(&log), |_, _| {});
        assert_eq!(*fed.samples.lock().unwrap(), audio);
        assert_eq!(run.text, format!("{} échantillons", audio.len()));
        let steps = fed.steps.lock().unwrap().clone();
        assert!(steps.len() >= 30, "{} pas", steps.len());
        assert!(
            steps.windows(2).all(|w| w[1] - w[0] == STREAM_STEP),
            "pas : {steps:?}"
        );
        assert!(said(&log)
            .iter()
            .any(|e| matches!(e, Event::Partial { pending, .. } if !pending.is_empty())));
    }

    // Les tests qui suivent passent par le fil de la dictée. Le faux modèle
    // n'avance pas l'horloge : leurs assertions ne dépendent pas de l'ordre
    // dans lequel les fils se suivent.

    fn start(engine: Engine, clock: &Arc<ManualClock>, log: &Log) -> Dictation {
        let log = log.clone();
        Dictation::start_with_clock(
            engine,
            Path::new("/modeles/essai"),
            "fr",
            move |e| log.lock().unwrap().push(Entry::Said(e)),
            clock.clone(),
        )
    }

    /// Attend (en temps réel, 5 s au plus) que `done` soit vrai.
    fn wait_until(done: impl Fn() -> bool) -> bool {
        let start = std::time::Instant::now();
        while !done() {
            if start.elapsed() > Duration::from_secs(5) {
                return false;
            }
            std::thread::sleep(ms(1));
        }
        true
    }

    fn has_committed(log: &Log, text: &str) -> bool {
        committed(log).iter().any(|(t, _)| t == text)
    }

    fn engine(recognizer: Arc<dyn Recognizer>) -> Engine {
        Engine::Segmented(recognizer, Box::new(Scripted))
    }

    #[test]
    fn finish_during_loading_reads_everything_said() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let gate = Arc::new(Latch::default());
        let marks = Marks {
            load_gate: Some(gate.clone()),
            ..Default::default()
        };
        let dictation = start(engine(Arc::new(marks)), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0), sentence(2, 1.0)].concat());
        let opener = std::thread::spawn(move || {
            std::thread::sleep(ms(5));
            gate.open();
        });
        assert_eq!(dictation.finish().unwrap(), "w1 w2");
        opener.join().unwrap();
        let events = said(&log);
        assert_eq!(events.first(), Some(&Event::Loading));
        assert!(
            matches!(events.get(1), Some(Event::Loaded(_))),
            "{events:?}"
        );
        assert_eq!(
            events.last(),
            Some(&Event::Ended {
                text: "w1 w2".into()
            })
        );
    }

    /// L'ordre de `Ready` : après `Loading` et `Loaded`, une seule fois,
    /// avant tout texte, mémoire ou fin.
    fn assert_ready_once_before_any_reading(events: &[Event]) {
        let ready: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| **e == Event::Ready)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(ready.len(), 1, "{events:?}");
        let first_reading = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    Event::Partial { .. }
                        | Event::Committed { .. }
                        | Event::Memory(_)
                        | Event::Ended { .. }
                        | Event::Failed(_)
                )
            })
            .expect("une lecture");
        assert!(ready[0] < first_reading, "{events:?}");
        assert!(
            events[..ready[0]]
                .iter()
                .all(|e| matches!(e, Event::Loading | Event::Loaded(_))),
            "{events:?}"
        );
    }

    #[test]
    fn ready_comes_once_after_a_load_and_before_any_reading() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let marks = Marks {
            memory: Some(7),
            log: log.clone(),
            ..Default::default()
        };
        let dictation = start(engine(Arc::new(marks)), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0), sentence(2, 1.0)].concat());
        dictation.finish().unwrap();
        let events = said(&log);
        assert!(
            matches!(
                &events[..3],
                [Event::Loading, Event::Loaded(_), Event::Ready]
            ),
            "{events:?}"
        );
        assert_ready_once_before_any_reading(&events);
    }

    /// Le modèle déjà là : rien ne se charge, `Ready` vient le premier.
    #[test]
    fn ready_comes_first_when_the_model_is_already_loaded() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let holder = Arc::new(Holder::new(
            Marks {
                log: log.clone(),
                ..Default::default()
            },
            None,
        ));
        holder.prepare(Path::new("/modeles/essai"), &|| {}).unwrap();
        let dictation = start(engine(holder), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0)].concat());
        dictation.finish().unwrap();
        let events = said(&log);
        assert_eq!(events.first(), Some(&Event::Ready), "{events:?}");
        assert_ready_once_before_any_reading(&events);
    }

    /// Le flux : `Ready` une fois le flux ouvert, avant son texte ; un flux
    /// qui ne s'ouvre pas n'est jamais prêt.
    #[test]
    fn ready_waits_for_the_stream_and_never_follows_a_failure() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let streamer = Streamer {
            fed: Fed::default(),
            clock: clock.clone(),
        };
        let dictation = start(Engine::Streaming(Arc::new(streamer)), &clock, &log);
        dictation.push(&ramp(4_000));
        dictation.finish().unwrap();
        assert_ready_once_before_any_reading(&said(&log));

        struct Closed;
        impl StreamingRecognizer for Closed {
            fn load(&self, _: &Path) -> Result<()> {
                Ok(())
            }
            fn warm_up(&self) -> Result<()> {
                Ok(())
            }
            fn open_stream(&self, _: &str) -> Result<Box<dyn RecognizerStream>> {
                anyhow::bail!("flux fermé")
            }
            fn release(&self) {}
        }
        let log = Log::default();
        let dictation = start(Engine::Streaming(Arc::new(Closed)), &clock, &log);
        dictation.push(&ramp(4_000));
        assert!(dictation.finish().is_err());
        let events = said(&log);
        assert!(!events.contains(&Event::Ready), "{events:?}");
        assert!(
            matches!(events.last(), Some(Event::Failed(_))),
            "{events:?}"
        );
    }

    #[test]
    fn a_model_that_will_not_load_fails_the_dictation() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let marks = Marks {
            broken: true,
            log: log.clone(),
            ..Default::default()
        };
        let dictation = start(engine(Arc::new(marks)), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0)].concat());
        let err = dictation.finish().unwrap_err();
        assert!(format!("{err:#}").contains("poids tronqués"), "{err:#}");
        let events = said(&log);
        assert_eq!(events.first(), Some(&Event::Loading));
        assert!(
            matches!(events.last(), Some(Event::Failed(m)) if m.contains("poids tronqués")),
            "{events:?}"
        );
        assert!(!events.contains(&Event::Ready), "jamais prêt : {events:?}");
        assert!(reads(&log).is_empty());
    }

    #[test]
    fn less_than_a_tenth_of_a_second_ends_empty() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let marks = Marks {
            log: log.clone(),
            ..Default::default()
        };
        let dictation = start(engine(Arc::new(marks)), &clock, &log);
        dictation.push(&vec![0.01; MIN_SAMPLES - 1]);
        assert_eq!(dictation.finish().unwrap(), "");
        assert!(reads(&log).is_empty(), "rien à lire");
        assert_eq!(
            said(&log).last(),
            Some(&Event::Ended {
                text: String::new()
            })
        );
    }

    #[test]
    fn cancel_emits_nothing_more() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let marks = Arc::new(Marks {
            block_on: Some(2),
            ..Default::default()
        });
        let dictation = start(engine(marks.clone()), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0)].concat());
        assert!(wait_until(|| has_committed(&log, "w1")));
        dictation.push(&[sentence(2, 1.0), silence(1.0)].concat());
        // Une lecture est en cours quand l'annulation arrive.
        marks.entered.wait();
        dictation.cancel();
        let before = said(&log).len();
        marks.resume.open();
        dictation.push(&[sentence(3, 1.0), silence(1.0)].concat());
        clock.advance(Duration::from_secs(5));
        assert!(
            dictation.finish().is_err(),
            "une dictée annulée ne rend rien"
        );
        assert_eq!(
            said(&log).len(),
            before,
            "rien après l'annulation : {:?}",
            said(&log)
        );
    }

    /// Une nouvelle dictée commence pendant que l'ancienne lit sa fin.
    #[test]
    fn a_new_dictation_never_receives_the_text_of_the_previous_one() {
        let clock = ManualClock::new();
        let (first_log, second_log) = (Log::default(), Log::default());
        let marks = Arc::new(Marks {
            block_on: Some(2),
            ..Default::default()
        });
        let first = Arc::new(start(engine(marks.clone()), &clock, &first_log));
        first.push(
            &[
                sentence(1, 1.0),
                silence(1.0),
                sentence(2, 1.0),
                silence(1.0),
            ]
            .concat(),
        );
        marks.entered.wait();
        let ending = {
            let first = first.clone();
            std::thread::spawn(move || first.finish())
        };
        let second = start(engine(marks.clone()), &clock, &second_log);
        second.push(&[sentence(3, 1.0), silence(1.0)].concat());
        assert_eq!(second.finish().unwrap(), "w3");
        marks.resume.open();
        assert_eq!(ending.join().unwrap().unwrap(), "w1 w2");
        let text_of = |log: &Log| format!("{:?}", said(log));
        assert!(
            !text_of(&first_log).contains("w3"),
            "{}",
            text_of(&first_log)
        );
        let second_text = text_of(&second_log);
        assert!(
            !second_text.contains("w1") && !second_text.contains("w2"),
            "{second_text}"
        );
        assert_eq!(
            said(&first_log).last(),
            Some(&Event::Ended {
                text: "w1 w2".into()
            })
        );
        assert_eq!(
            said(&second_log).last(),
            Some(&Event::Ended { text: "w3".into() })
        );
    }

    #[test]
    fn a_segment_failure_is_reported_and_keeps_what_was_committed() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let marks = Marks {
            fail_on: Some(2),
            ..Default::default()
        };
        let dictation = start(engine(Arc::new(marks)), &clock, &log);
        for k in 1..=3 {
            dictation.push(&[sentence(k, 1.0), silence(1.0)].concat());
        }
        assert_eq!(dictation.finish().unwrap(), "w1 w3");
        assert_eq!(failures(&log), 1);
        let texts: Vec<String> = committed(&log).into_iter().map(|(t, _)| t).collect();
        assert_eq!(texts, ["w1", "w3"]);
        assert_eq!(
            said(&log).last(),
            Some(&Event::Ended {
                text: "w1 w3".into()
            })
        );
    }

    /// Une pause de 10 minutes au milieu d'une dictée, modèle gardé 5 s
    /// (`keep_voice_minutes` à 0).
    #[test]
    fn the_model_is_not_released_during_a_long_pause() {
        let (clock, log) = (ManualClock::new(), Log::default());
        let releases = Arc::new(AtomicUsize::new(0));
        let marks = Marks {
            releases: releases.clone(),
            ..Default::default()
        };
        let holder = Arc::new(Holder::with_clock(marks, idle_delay(0), clock.clone()));
        let dictation = start(engine(holder.clone()), &clock, &log);
        dictation.push(&[sentence(1, 1.0), silence(1.0)].concat());
        assert!(wait_until(|| has_committed(&log, "w1")));
        for _ in 0..600 {
            clock.advance(Duration::from_secs(1));
        }
        std::thread::sleep(ms(5));
        assert_eq!(
            releases.load(Ordering::SeqCst),
            0,
            "libéré au milieu de la dictée"
        );
        assert!(holder.loaded_directory().is_some());
        dictation.push(&[sentence(2, 1.0), silence(1.0)].concat());
        assert_eq!(dictation.finish().unwrap(), "w1 w2");
        assert_eq!(failures(&log), 0);
        clock.advance(SHORTEST_DELAY);
        assert!(
            wait_until(|| releases.load(Ordering::SeqCst) == 1),
            "libéré le délai écoulé, la dictée finie"
        );
    }
}
