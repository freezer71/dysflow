//! Ordonnanceur de synthèse : plusieurs sessions en parallèle, pilotées par la
//! position de lecture.
//!
//! Le point clé est que les workers ne synthétisent pas le document du début à
//! la fin, mais **une fenêtre glissante autour du curseur**. Trois propriétés en
//! découlent :
//!
//! * un saut ré-oriente immédiatement la synthèse, ce qui rend le seek possible ;
//! * la mémoire reste bornée, là où une file non bornée gardait tout le PCM ;
//! * l'audio est produit à 1× et sert à toutes les vitesses : la vitesse
//!   s'applique à la lecture (`audio::tempo`), un changement ne touche pas
//!   au cache.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{PcmBuffer, Synthesis, TtsEngine, TtsParams, WordSpan};

/// Phrase à synthétiser.
#[derive(Debug, Clone)]
struct Job {
    text: String,
    /// Formes parlées des mots affichés, pour le minutage.
    words: Vec<String>,
    pause_ms: u64,
}

/// Ce qu'un worker prend : la phrase, ses mots, et sous quels paramètres.
struct Picked {
    id: usize,
    text: String,
    words: Vec<String>,
    params: TtsParams,
    generation: u64,
    pause_ms: u64,
}

/// Quelle partie d'une phrase un audio porte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// La phrase entière.
    Whole,
}

/// Audio prêt à jouer pour une phrase.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Parole suivie du silence de fin de bloc.
    pub audio: Arc<PcmBuffer>,
    /// Durée de la parole seule, hors silence.
    ///
    /// Le curseur de mots s'appuie dessus : inclure le silence ferait traîner le
    /// dernier mot jusqu'au bloc suivant.
    pub speech: Duration,
    /// Temps de chaque mot affiché, quand le moteur le donne.
    pub words: Option<Arc<[WordSpan]>>,
    pub part: Part,
}

/// Compteur affiché dans la barre d'état.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SynthesisStats {
    pub ready: usize,
    pub in_flight: usize,
    pub failed: usize,
    pub total: usize,
    pub workers: usize,
    /// Temps passé à synthétiser, cumulé sur toutes les sessions.
    pub busy: Duration,
}

struct Entry {
    rendered: Rendered,
    generation: u64,
}

struct State {
    jobs: Vec<Job>,
    params: TtsParams,
    /// Incrémentée à chaque changement de paramètres ; les résultats produits
    /// sous une génération périmée sont jetés.
    generation: u64,
    cursor: usize,
    window: usize,
    keep_behind: usize,
    cache: HashMap<usize, Entry>,
    in_flight: HashSet<usize>,
    failures: HashMap<usize, String>,
    /// Temps passé à synthétiser, cumulé.
    busy: Duration,
}

impl State {
    /// Prochaine phrase à synthétiser, au plus près du curseur.
    fn next_job(&self) -> Option<Picked> {
        let end = (self.cursor + self.window).min(self.jobs.len());
        for id in self.cursor..end {
            if self.in_flight.contains(&id) || self.failures.contains_key(&id) {
                continue;
            }
            if self.cache.get(&id).is_some_and(|e| e.generation == self.generation) {
                continue;
            }
            let job = &self.jobs[id];
            return Some(Picked {
                id,
                text: job.text.clone(),
                words: job.words.clone(),
                params: self.params.clone(),
                generation: self.generation,
                pause_ms: job.pause_ms,
            });
        }
        None
    }

    /// Libère ce qui est trop loin du curseur pour resservir.
    fn evict(&mut self) {
        let low = self.cursor.saturating_sub(self.keep_behind);
        let high = self.cursor + self.window * 2;
        self.cache.retain(|&id, _| id >= low && id <= high);
    }
}

struct Shared {
    engine: Arc<dyn TtsEngine>,
    state: Mutex<State>,
    wake: Condvar,
    shutdown: AtomicBool,
}

pub struct Synthesizer {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
    worker_count: usize,
}

impl Synthesizer {
    /// Démarre `workers` threads de synthèse sur `engine`.
    pub fn new(
        engine: Arc<dyn TtsEngine>,
        params: TtsParams,
        workers: usize,
        window: usize,
    ) -> Self {
        let worker_count = workers.clamp(1, engine.max_sessions().max(1));
        let shared = Arc::new(Shared {
            engine,
            state: Mutex::new(State {
                jobs: Vec::new(),
                params,
                generation: 1,
                cursor: 0,
                window: window.max(2),
                keep_behind: 8,
                cache: HashMap::new(),
                in_flight: HashSet::new(),
                failures: HashMap::new(),
                busy: Duration::ZERO,
            }),
            wake: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });

        let handles = (0..worker_count)
            .map(|_| {
                let shared = Arc::clone(&shared);
                thread::spawn(move || worker_loop(shared))
            })
            .collect();

        Self {
            shared,
            workers: handles,
            worker_count,
        }
    }

    /// Ajoute une phrase à la fin du document, sans mots à minuter.
    pub fn push(&self, text: String, pause_ms: u64) {
        self.push_with_words(text, Vec::new(), pause_ms);
    }

    /// Ajoute une phrase et les formes parlées de ses mots affichés.
    pub fn push_with_words(&self, text: String, words: Vec<String>, pause_ms: u64) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.push(Job { text, words, pause_ms });
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Déplace la fenêtre de synthèse.
    pub fn set_cursor(&self, cursor: usize) {
        let mut state = self.shared.state.lock().unwrap();
        if state.cursor == cursor {
            return;
        }
        state.cursor = cursor;
        state.evict();
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Change les paramètres de synthèse et invalide ce qui a été produit.
    ///
    /// Voix, langue, échantillonnage… La vitesse n'en fait pas partie : elle
    /// s'applique au son, et ne vide jamais le cache.
    pub fn set_params(&self, params: TtsParams) {
        let mut state = self.shared.state.lock().unwrap();
        if state.params == params {
            return;
        }
        state.params = params;
        state.generation += 1;
        state.cache.clear();
        state.failures.clear();
        drop(state);
        self.shared.wake.notify_all();
    }

    pub fn params(&self) -> TtsParams {
        self.shared.state.lock().unwrap().params.clone()
    }

    /// Audio d'une phrase, s'il est prêt et à jour.
    pub fn get(&self, id: usize) -> Option<Rendered> {
        let state = self.shared.state.lock().unwrap();
        state
            .cache
            .get(&id)
            .filter(|e| e.generation == state.generation)
            .map(|e| e.rendered.clone())
    }

    /// Message d'erreur si la synthèse de cette phrase a échoué.
    pub fn failure(&self, id: usize) -> Option<String> {
        self.shared.state.lock().unwrap().failures.get(&id).cloned()
    }

    /// Redonne leur chance aux phrases en échec à partir de `from`.
    ///
    /// Un refus du moteur condamnait la phrase pour la session entière : y
    /// revenir exprès ne la faisait pas réentendre. Or c'est précisément le
    /// geste de quelqu'un qui veut réécouter un passage.
    pub fn retry_from(&self, from: usize) {
        let mut state = self.shared.state.lock().unwrap();
        let before = state.failures.len();
        state.failures.retain(|&id, _| id < from);
        if state.failures.len() == before {
            return;
        }
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Nom courant du moteur.
    ///
    /// Interrogé à chaque trame : il évolue pendant le chargement différé.
    pub fn engine_name(&self) -> String {
        self.shared.engine.name()
    }

    /// Vrai si le moteur exige un WAV de référence pour parler.
    pub fn requires_reference(&self) -> bool {
        self.shared.engine.requires_reference()
    }

    /// Le moteur, pour ouvrir une session hors de l'ordonnanceur.
    ///
    /// Sert aux aperçus : ils doivent parler avec le même modèle et les mêmes
    /// poids que la lecture, sans pour autant occuper un de ses workers.
    pub fn engine(&self) -> Arc<dyn TtsEngine> {
        Arc::clone(&self.shared.engine)
    }

    /// Repart d'un document vide, threads et sessions gardés ouverts.
    ///
    /// Les synthèses encore en cours portent l'ancienne génération : elles
    /// seront jetées à leur arrivée, et libéreront alors leur place.
    pub fn reset(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.clear();
        state.cache.clear();
        state.failures.clear();
        state.cursor = 0;
        state.generation += 1;
        drop(state);
        self.shared.wake.notify_all();
    }

    pub fn stats(&self) -> SynthesisStats {
        let state = self.shared.state.lock().unwrap();
        SynthesisStats {
            ready: state
                .cache
                .values()
                .filter(|e| e.generation == state.generation)
                .count(),
            in_flight: state.in_flight.len(),
            failed: state.failures.len(),
            total: state.jobs.len(),
            workers: self.worker_count,
            busy: state.busy,
        }
    }

}

impl Drop for Synthesizer {
    fn drop(&mut self) {
        // Le drapeau se lève sous le verrou. Levé à côté, il pouvait tomber
        // entre la vérification d'un worker et son endormissement : le réveil
        // partait dans le vide, et la fermeture attendait un thread que plus
        // rien ne réveillerait — « q » restait suspendu.
        {
            let _state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.shutdown.store(true, Ordering::SeqCst);
            self.shared.wake.notify_all();
        }
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(shared: Arc<Shared>) {
    // La session est ouverte au premier travail : un moteur qui échoue à
    // s'initialiser ne doit pas empêcher le lecteur de démarrer.
    let mut session = None;

    loop {
        let job = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(job) = state.next_job() {
                    state.in_flight.insert(job.id);
                    break job;
                }
                state = shared.wake.wait(state).unwrap();
            }
        };

        if session.is_none() {
            match shared.engine.open_session() {
                Ok(s) => session = Some(s),
                Err(err) => {
                    finish_generation(
                        &shared,
                        job.id,
                        job.generation,
                        Err(format!("ouverture de session : {err}")),
                    );
                    continue;
                }
            }
        }

        let session = session.as_mut().expect("session ouverte juste au-dessus");
        let outcome = synthesize(&shared, session.as_mut(), &job.text, &job.words, &job.params)
            .map(|s| with_trailing_silence(s, job.words.len(), job.pause_ms));
        finish_generation(&shared, job.id, job.generation, outcome);
    }
}

/// Une synthèse, chronométrée : le temps occupé est la mesure du filtre de
/// charge. Un son vide est un échec.
fn synthesize(
    shared: &Arc<Shared>,
    session: &mut dyn super::TtsSession,
    text: &str,
    words: &[String],
    params: &TtsParams,
) -> Result<Synthesis, String> {
    let started = std::time::Instant::now();
    let outcome = session.synthesize_timed(text, words, params);
    shared.state.lock().unwrap().busy += started.elapsed();
    match outcome {
        Ok(s) if s.pcm.is_empty() => Err("aucun échantillon produit".to_string()),
        Ok(s) => Ok(s),
        Err(e) => Err(e.to_string()),
    }
}

/// Ajoute le silence de fin de bloc à la parole.
///
/// Ce silence porte la hiérarchie à l'oreille : une pause plus longue après un
/// titre qu'après un item de liste, sans jamais prononcer le moindre marqueur.
/// Un minutage qui ne couvre pas exactement les mots est écarté.
fn with_trailing_silence(synthesis: Synthesis, expected_words: usize, pause_ms: u64) -> Rendered {
    let speech = synthesis.pcm;
    let words = synthesis
        .words
        .filter(|w| expected_words > 0 && w.len() == expected_words)
        .map(Arc::from);
    let speech_duration = speech.duration;
    if pause_ms == 0 {
        return Rendered { audio: Arc::new(speech), speech: speech_duration, words, part: Part::Whole };
    }
    let frames = (speech.sample_rate as u64 * pause_ms / 1000) as usize;
    let mut samples = Vec::with_capacity(speech.samples.len() + frames * speech.channels as usize);
    samples.extend_from_slice(&speech.samples);
    samples.resize(samples.len() + frames * speech.channels as usize, 0.0);
    Rendered {
        audio: Arc::new(PcmBuffer::new(samples, speech.sample_rate, speech.channels)),
        speech: speech_duration,
        words,
        part: Part::Whole,
    }
}

fn finish_generation(
    shared: &Arc<Shared>,
    id: usize,
    generation: u64,
    outcome: Result<Rendered, String>,
) {
    let mut state = shared.state.lock().unwrap();
    state.in_flight.remove(&id);
    // Un résultat produit sous d'anciens paramètres n'a plus lieu d'être joué.
    if generation == state.generation {
        match outcome {
            Ok(rendered) => {
                state.cache.insert(id, Entry { rendered, generation });
                state.evict();
            }
            Err(message) => {
                state.failures.insert(id, message);
            }
        }
    }
    drop(state);
    shared.wake.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::TtsSession;
    use std::sync::atomic::AtomicUsize;

    /// Moteur déterministe : la durée suit le nombre de mots.
    struct FakeEngine {
        calls: Arc<AtomicUsize>,
        concurrent: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    struct FakeSession {
        calls: Arc<AtomicUsize>,
        concurrent: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    impl TtsEngine for FakeEngine {
        fn name(&self) -> String {
            "fake".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(FakeSession {
                calls: Arc::clone(&self.calls),
                concurrent: Arc::clone(&self.concurrent),
                peak: Arc::clone(&self.peak),
            }))
        }
        fn max_sessions(&self) -> usize {
            8
        }
    }

    impl TtsSession for FakeSession {
        fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            let now = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(30));
            self.concurrent.fetch_sub(1, Ordering::SeqCst);

            // 1000 échantillons par mot.
            let words = text.split_whitespace().count().max(1);
            let count = 1000 * words;
            Ok(PcmBuffer::new(vec![0.1; count], 1000, 1))
        }
    }

    /// Moteur qui répond sans attendre : les workers enchaînent alors
    /// prise de travail et sommeil aussi vite que possible.
    struct InstantEngine;
    struct InstantSession;

    impl TtsEngine for InstantEngine {
        fn name(&self) -> String {
            "instantané".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(InstantSession))
        }
        fn max_sessions(&self) -> usize {
            8
        }
    }

    impl TtsSession for InstantSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.1; 10], 1000, 1))
        }
    }

    /// Compte les sessions ouvertes, et rend un son proportionnel au texte.
    struct OpeningCounter {
        opened: Arc<AtomicUsize>,
    }
    struct LengthSession;

    impl TtsEngine for OpeningCounter {
        fn name(&self) -> String {
            "compteur".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(LengthSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for LengthSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    fn wait_ready(synth: &Synthesizer, id: usize) -> Rendered {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = synth.get(id) {
                return r;
            }
            assert!(std::time::Instant::now() < deadline, "phrase {id} jamais prête");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn reset_keeps_the_session_and_forgets_the_text() {
        let opened = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(OpeningCounter { opened: Arc::clone(&opened) });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 40);

        synth.reset();
        assert_eq!(synth.stats().total, 0);
        synth.push("bb".into(), 0);
        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    /// Moteur dont la première ouverture de session attend un feu vert, puis
    /// échoue ; les suivantes réussissent.
    struct SlowFailingOpen {
        opens: AtomicUsize,
        started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
        gate: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl TtsEngine for SlowFailingOpen {
        fn name(&self) -> String {
            "ouverture lente".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            if self.opens.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.lock().unwrap().send(()).unwrap();
                self.gate.lock().unwrap().recv().unwrap();
                anyhow::bail!("refusée");
            }
            Ok(Box::new(LengthSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    #[test]
    fn a_failed_open_for_the_old_text_does_not_poison_the_new_one() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let engine = Arc::new(SlowFailingOpen {
            opens: AtomicUsize::new(0),
            started: std::sync::Mutex::new(started_tx),
            gate: std::sync::Mutex::new(gate),
        });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        synth.reset();
        synth.push("bb".into(), 0);
        release.send(()).unwrap();

        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(synth.failure(0), None);
    }

    /// Moteur dont chaque synthèse annonce son départ puis attend un feu vert.
    struct GatedSynthesis {
        calls: Arc<AtomicUsize>,
        started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }
    struct GatedSession {
        calls: Arc<AtomicUsize>,
        started: std::sync::mpsc::Sender<()>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl TtsEngine for GatedSynthesis {
        fn name(&self) -> String {
            "à feu vert".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(GatedSession {
                calls: Arc::clone(&self.calls),
                started: self.started.lock().unwrap().clone(),
                gate: Arc::clone(&self.gate),
            }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for GatedSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.send(()).unwrap();
            self.gate.lock().unwrap().recv().unwrap();
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    #[test]
    fn a_synthesis_in_flight_across_a_reset_is_dropped_and_the_id_reused() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(GatedSynthesis {
            calls: Arc::clone(&calls),
            started: std::sync::Mutex::new(started_tx),
            gate: Arc::new(std::sync::Mutex::new(gate)),
        });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        synth.reset();
        synth.push("bb".into(), 0);
        release.send(()).unwrap();
        release.send(()).unwrap();

        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "une fois l'ancien, une fois le nouveau");
    }

    #[test]
    fn dropping_the_pool_never_hangs() {
        // Le réveil de fermeture se perdait quand il tombait entre la
        // vérification du drapeau par un worker et son endormissement : la
        // fermeture attendait alors un thread que plus rien ne réveillerait,
        // et « q » restait suspendu. La fenêtre est étroite : on la vise en
        // fermant des pools par milliers, workers en pleine activité.
        let (done, finished) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let engine: Arc<dyn TtsEngine> = Arc::new(InstantEngine);
            for _ in 0..60_000 {
                let synth = Synthesizer::new(Arc::clone(&engine), TtsParams::default(), 3, 4);
                for i in 0..4 {
                    synth.push(format!("phrase {i}"), 0);
                }
                drop(synth);
            }
            let _ = done.send(());
        });
        finished
            .recv_timeout(Duration::from_secs(30))
            .expect("la fermeture d'un pool est restée bloquée");
    }

    fn engine() -> (Arc<FakeEngine>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(FakeEngine {
            calls: Arc::clone(&calls),
            concurrent: Arc::new(AtomicUsize::new(0)),
            peak: Arc::clone(&peak),
        });
        (engine, calls, peak)
    }

    fn wait_for(mut predicate: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn synthesizes_ahead_of_the_cursor() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        for i in 0..10 {
            synth.push(format!("phrase numéro {i}"), 0);
        }
        assert!(wait_for(|| synth.get(0).is_some() && synth.get(3).is_some()));
        // Rien n'est produit au-delà de la fenêtre.
        assert!(synth.get(9).is_none());
    }

    #[test]
    fn runs_sessions_in_parallel() {
        let (engine, _, peak) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 4, 8);
        for i in 0..8 {
            synth.push(format!("phrase {i}"), 0);
        }
        assert!(wait_for(|| synth.stats().ready >= 8));
        assert!(peak.load(Ordering::SeqCst) > 1, "la synthèse est restée séquentielle");
    }

    #[test]
    fn seeking_redirects_synthesis() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 3);
        for i in 0..40 {
            synth.push(format!("phrase {i}"), 0);
        }
        assert!(wait_for(|| synth.get(0).is_some()));
        assert!(synth.get(30).is_none());

        synth.set_cursor(30);
        assert!(wait_for(|| synth.get(30).is_some()), "le saut n'a pas réorienté la synthèse");
    }

    #[test]
    fn a_change_of_voice_settings_regenerates_the_window() {
        // Voix, langue, échantillonnage : le son change, le cache se refait.
        // (La vitesse, elle, n'est pas un paramètre de synthèse.)
        let (engine, calls, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        synth.push("un deux trois quatre".to_string(), 0);
        assert!(wait_for(|| synth.get(0).is_some()));
        let before = calls.load(Ordering::SeqCst);

        let mut params = synth.params();
        params.voice = "F2".into();
        synth.set_params(params);

        assert!(wait_for(|| calls.load(Ordering::SeqCst) > before && synth.get(0).is_some()));
    }

    #[test]
    fn memory_stays_bounded_while_reading_forward() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        for i in 0..200 {
            synth.push(format!("phrase {i}"), 0);
        }
        for cursor in 0..60 {
            synth.set_cursor(cursor);
            thread::sleep(Duration::from_millis(5));
        }
        let cached = synth.shared.state.lock().unwrap().cache.len();
        assert!(cached <= 32, "le cache a grossi sans borne : {cached}");
    }

    /// Un mot = 100 ms ; le minutage rendu suit les mots reçus. Avec
    /// `wrong`, il en oublie un.
    struct TimedEngine {
        wrong: bool,
    }
    struct TimedSession {
        wrong: bool,
    }

    impl TtsEngine for TimedEngine {
        fn name(&self) -> String {
            "minuté".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(TimedSession { wrong: self.wrong }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for TimedSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            let words = text.split_whitespace().count().max(1);
            Ok(PcmBuffer::new(vec![0.1; words * 100], 1000, 1))
        }
        fn synthesize_timed(&mut self, text: &str, words: &[String], p: &TtsParams) -> Result<super::super::Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let count = if self.wrong { words.len().saturating_sub(1) } else { words.len() };
            let spans = (0..count)
                .map(|i| crate::doc::WordSpan { start: i as f32 * 0.1, end: (i + 1) as f32 * 0.1 })
                .collect();
            Ok(super::super::Synthesis { pcm, words: Some(spans) })
        }
    }

    #[test]
    fn the_timing_of_each_word_reaches_the_rendered_audio() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: false }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux trois".into(), vec!["un".into(), "deux".into(), "trois".into()], 0);
        let rendered = wait_ready(&synth, 0);
        let words = rendered.words.expect("minutage rendu");
        assert_eq!(words.len(), 3);
        assert_eq!(words[2].end, 0.3);
        assert_eq!(rendered.part, Part::Whole);
    }

    #[test]
    fn a_timing_that_does_not_cover_the_words_is_dropped() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: true }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux".into(), vec!["un".into(), "deux".into()], 0);
        assert!(wait_ready(&synth, 0).words.is_none());
    }

    #[test]
    fn the_time_spent_synthesizing_is_counted() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("une phrase".into(), 0);
        wait_ready(&synth, 0);
        assert!(synth.stats().busy >= Duration::from_millis(25), "{:?}", synth.stats().busy);
    }

    #[test]
    fn trailing_silence_extends_audio_but_not_speech() {
        let pcm = PcmBuffer::new(vec![0.5; 1000], 1000, 1);
        let rendered = with_trailing_silence(Synthesis { pcm, words: None }, 0, 500);
        assert_eq!(rendered.speech, Duration::from_millis(1000));
        assert_eq!(rendered.audio.duration, Duration::from_millis(1500));
        assert_eq!(rendered.audio.samples.len(), 1500);
    }
}
