//! Chargement différé du moteur.
//!
//! Charger un GGUF et compiler les noyaux Metal prend quelques secondes. Fait
//! avant l'ouverture de l'interface, ce délai laissait l'écran noir : l'outil
//! paraissait lent alors qu'il ne faisait qu'attendre son modèle.
//!
//! Le chargement part donc dans un thread, et l'interface s'affiche
//! immédiatement. Les threads de synthèse, eux, peuvent attendre sans nuire :
//! ils bloquent sur l'ouverture de leur session jusqu'à ce que le modèle soit
//! prêt. Rien n'est perdu — la première phrase n'aurait de toute façon pas pu
//! être prononcée avant.

use anyhow::{anyhow, Result};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use super::{TtsEngine, TtsSession};

enum State {
    Loading,
    Ready(Arc<dyn TtsEngine>),
    Failed(String),
}

struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    /// Moteur de repli, utilisé si le chargement échoue.
    fallback: Arc<dyn TtsEngine>,
    label: String,
    sessions: usize,
}

/// Moteur dont le chargement se fait en arrière-plan.
pub struct LazyEngine {
    shared: Arc<Shared>,
}

impl LazyEngine {
    /// Lance le chargement et rend la main aussitôt.
    ///
    /// `load` est exécuté dans un thread dédié ; `fallback` prend le relais si
    /// ce chargement échoue, pour que le lecteur reste utilisable.
    pub fn spawn<F>(label: String, sessions: usize, fallback: Arc<dyn TtsEngine>, load: F) -> Self
    where
        F: FnOnce() -> Result<Arc<dyn TtsEngine>> + Send + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::Loading),
            ready: Condvar::new(),
            fallback,
            label,
            sessions,
        });

        let worker = Arc::clone(&shared);
        thread::spawn(move || {
            let outcome = load();
            let mut state = worker.state.lock().unwrap();
            *state = match outcome {
                Ok(engine) => State::Ready(engine),
                Err(err) => State::Failed(err.to_string()),
            };
            drop(state);
            worker.ready.notify_all();
        });

        Self { shared }
    }

    /// Vrai tant que le modèle n'est pas prêt.
    pub fn is_loading(&self) -> bool {
        matches!(*self.shared.state.lock().unwrap(), State::Loading)
    }

    /// Message d'échec, si le chargement a échoué.
    pub fn failure(&self) -> Option<String> {
        match &*self.shared.state.lock().unwrap() {
            State::Failed(err) => Some(err.clone()),
            _ => None,
        }
    }

    /// Attend la fin du chargement et rend le moteur retenu.
    fn resolve(&self) -> std::result::Result<Arc<dyn TtsEngine>, String> {
        let mut state = self.shared.state.lock().unwrap();
        while matches!(*state, State::Loading) {
            state = self.shared.ready.wait(state).unwrap();
        }
        match &*state {
            State::Ready(engine) => Ok(Arc::clone(engine)),
            State::Failed(err) => Err(err.clone()),
            State::Loading => unreachable!("la boucle ci-dessus l'exclut"),
        }
    }
}

impl TtsEngine for LazyEngine {
    fn name(&self) -> String {
        match &*self.shared.state.lock().unwrap() {
            State::Loading => format!("{} · chargement…", self.shared.label),
            State::Ready(engine) => engine.name(),
            // L'échec est nommé : l'utilisateur doit savoir pourquoi la voix
            // n'est pas celle qu'il attendait.
            State::Failed(_) => format!("{} (repli)", self.shared.fallback.name()),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        match self.resolve() {
            Ok(engine) => engine.open_session(),
            Err(_) => self
                .shared
                .fallback
                .open_session()
                .map_err(|e| anyhow!("moteur et repli indisponibles : {e}")),
        }
    }

    fn max_sessions(&self) -> usize {
        // Connu d'avance : le pool se dimensionne sans attendre le modèle.
        self.shared.sessions.max(1)
    }

    /// Ne se prononce qu'une fois le modèle chargé.
    ///
    /// Tant qu'il charge, répondre « oui » afficherait un avertissement qui
    /// pourrait se révéler faux ; on attend de savoir.
    fn requires_reference(&self) -> bool {
        match &*self.shared.state.lock().unwrap() {
            State::Ready(engine) => engine.requires_reference(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::{PcmBuffer, TtsParams};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct Stub(&'static str, Arc<AtomicUsize>);
    struct StubSession;

    impl TtsEngine for Stub {
        fn name(&self) -> String {
            self.0.to_string()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            self.1.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(StubSession))
        }
        fn max_sessions(&self) -> usize {
            4
        }
    }

    impl TtsSession for StubSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.0; 10], 100, 1))
        }
    }

    fn fallback(counter: &Arc<AtomicUsize>) -> Arc<dyn TtsEngine> {
        Arc::new(Stub("repli", Arc::clone(counter)))
    }

    #[test]
    fn construction_does_not_wait_for_the_model() {
        let counter = Arc::new(AtomicUsize::new(0));
        let start = Instant::now();
        let engine = LazyEngine::spawn("audio.cpp".into(), 3, fallback(&counter), || {
            thread::sleep(Duration::from_millis(400));
            Ok(Arc::new(Stub("réel", Arc::new(AtomicUsize::new(0)))) as Arc<dyn TtsEngine>)
        });
        // Le point décisif : rendre la main sans attendre le chargement.
        assert!(start.elapsed() < Duration::from_millis(100), "{:?}", start.elapsed());
        assert!(engine.is_loading());
        assert!(engine.name().contains("chargement"));
        assert_eq!(engine.max_sessions(), 3);

        // La session, elle, attend — et obtient le vrai moteur.
        engine.open_session().unwrap();
        assert!(!engine.is_loading());
        assert_eq!(engine.name(), "réel");
    }

    #[test]
    fn a_failed_load_falls_back_instead_of_going_silent() {
        let counter = Arc::new(AtomicUsize::new(0));
        let engine = LazyEngine::spawn("audio.cpp".into(), 2, fallback(&counter), || {
            Err(anyhow!("modèle introuvable"))
        });
        engine.open_session().expect("le repli doit prendre le relais");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(engine.failure().as_deref(), Some("modèle introuvable"));
        assert!(engine.name().contains("repli"));
    }

    #[test]
    fn several_sessions_share_a_single_load() {
        let loads = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&loads);
        let counter = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(LazyEngine::spawn(
            "audio.cpp".into(),
            4,
            fallback(&counter),
            move || {
                counted.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(100));
                Ok(Arc::new(Stub("réel", Arc::new(AtomicUsize::new(0)))) as Arc<dyn TtsEngine>)
            },
        ));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let engine = Arc::clone(&engine);
                thread::spawn(move || engine.open_session().is_ok())
            })
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap());
        }
        // Le modèle n'est chargé qu'une fois, quel que soit le nombre de threads.
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
