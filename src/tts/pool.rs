//! Ordonnanceur de synthèse : plusieurs sessions en parallèle, pilotées par la
//! position de lecture.
//!
//! Le point clé est que les workers ne synthétisent pas le document du début à
//! la fin, mais **une fenêtre glissante autour du curseur**. Trois propriétés en
//! découlent :
//!
//! * un saut ré-oriente immédiatement la synthèse, ce qui rend le seek possible ;
//! * la mémoire reste bornée, là où une file non bornée gardait tout le PCM ;
//! * un changement de vitesse invalide la fenêtre et la régénère, sans jamais
//!   ré-échantillonner l'audio déjà produit.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{PcmBuffer, TtsEngine, TtsParams};

/// Phrase à synthétiser.
#[derive(Debug, Clone)]
struct Job {
    text: String,
    pause_ms: u64,
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
}

/// Compteur affiché dans la barre d'état.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SynthesisStats {
    pub ready: usize,
    pub in_flight: usize,
    pub failed: usize,
    pub total: usize,
    pub workers: usize,
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
}

impl State {
    /// Prochaine phrase à synthétiser, au plus près du curseur.
    fn next_job(&self) -> Option<(usize, String, TtsParams, u64, u64)> {
        let end = (self.cursor + self.window).min(self.jobs.len());
        for id in self.cursor..end {
            if self.in_flight.contains(&id) || self.failures.contains_key(&id) {
                continue;
            }
            if self.cache.get(&id).is_some_and(|e| e.generation == self.generation) {
                continue;
            }
            let job = &self.jobs[id];
            return Some((
                id,
                job.text.clone(),
                self.params.clone(),
                self.generation,
                job.pause_ms,
            ));
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

    /// Ajoute une phrase à la fin du document.
    pub fn push(&self, text: String, pause_ms: u64) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.push(Job { text, pause_ms });
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
    /// Appelé notamment lors d'un changement de vitesse : plutôt que de
    /// transposer l'audio existant, on le régénère au bon débit.
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

    /// Redonne sa chance à une phrase en échec.
    pub fn retry(&self, id: usize) {
        let mut state = self.shared.state.lock().unwrap();
        state.failures.remove(&id);
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
        }
    }

}

impl Drop for Synthesizer {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
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
        let (id, text, params, generation, pause_ms) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(job) = state.next_job() {
                    state.in_flight.insert(job.0);
                    break job;
                }
                state = shared.wake.wait(state).unwrap();
            }
        };

        if session.is_none() {
            match shared.engine.open_session() {
                Ok(s) => session = Some(s),
                Err(err) => {
                    finish(&shared, id, Err(format!("ouverture de session : {err}")));
                    continue;
                }
            }
        }

        let outcome = session
            .as_mut()
            .expect("session ouverte juste au-dessus")
            .synthesize(&text, &params)
            .map_err(|e| e.to_string())
            .and_then(|pcm| {
                if pcm.is_empty() {
                    Err("aucun échantillon produit".to_string())
                } else {
                    Ok(with_trailing_silence(pcm, pause_ms))
                }
            });

        finish_generation(&shared, id, generation, outcome);
    }
}

/// Ajoute le silence de fin de bloc à la parole.
///
/// Ce silence porte la hiérarchie à l'oreille : une pause plus longue après un
/// titre qu'après un item de liste, sans jamais prononcer le moindre marqueur.
fn with_trailing_silence(speech: PcmBuffer, pause_ms: u64) -> Rendered {
    let speech_duration = speech.duration;
    if pause_ms == 0 {
        return Rendered {
            audio: Arc::new(speech),
            speech: speech_duration,
        };
    }
    let frames = (speech.sample_rate as u64 * pause_ms / 1000) as usize;
    let mut samples = Vec::with_capacity(speech.samples.len() + frames * speech.channels as usize);
    samples.extend_from_slice(&speech.samples);
    samples.resize(samples.len() + frames * speech.channels as usize, 0.0);

    Rendered {
        audio: Arc::new(PcmBuffer::new(samples, speech.sample_rate, speech.channels)),
        speech: speech_duration,
    }
}

fn finish(shared: &Arc<Shared>, id: usize, outcome: Result<Rendered, String>) {
    let generation = shared.state.lock().unwrap().generation;
    finish_generation(shared, id, generation, outcome);
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

    /// Moteur déterministe : la durée encode le débit demandé, ce qui permet de
    /// vérifier que la vitesse passe bien par la synthèse.
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
        fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
            let now = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(30));
            self.concurrent.fetch_sub(1, Ordering::SeqCst);

            // 1000 échantillons par mot, divisés par le débit.
            let words = text.split_whitespace().count().max(1);
            let count = (1000.0 * words as f32 / params.speaking_rate) as usize;
            Ok(PcmBuffer::new(vec![0.1; count], 1000, 1))
        }
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
    fn speed_change_regenerates_at_the_new_rate() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        synth.push("un deux trois quatre".to_string(), 0);
        assert!(wait_for(|| synth.get(0).is_some()));
        let slow = synth.get(0).unwrap().audio.samples.len();

        let mut params = synth.params();
        params.speaking_rate = 2.0;
        synth.set_params(params);

        assert!(wait_for(|| synth
            .get(0)
            .is_some_and(|r| r.audio.samples.len() != slow)));
        let fast = synth.get(0).unwrap().audio.samples.len();
        // Deux fois plus rapide : deux fois moins d'échantillons.
        assert!(fast < slow, "slow={slow} fast={fast}");
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

    #[test]
    fn trailing_silence_extends_audio_but_not_speech() {
        let pcm = PcmBuffer::new(vec![0.5; 1000], 1000, 1);
        let rendered = with_trailing_silence(pcm, 500);
        assert_eq!(rendered.speech, Duration::from_millis(1000));
        assert_eq!(rendered.audio.duration, Duration::from_millis(1500));
        assert_eq!(rendered.audio.samples.len(), 1500);
    }
}
