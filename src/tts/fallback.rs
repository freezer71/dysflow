//! La voix de repli quand le moteur voulu échoue : la voix du système.
//!
//! Compilé avec audio.cpp, `fallback_engine` cherche encore un paquet
//! audio.cpp installé *au catalogue*, chargé seulement s'il doit parler. Le
//! catalogue n'en propose plus aucun depuis le 14 oct. 2026 (Kokoro seul) :
//! la recherche ne trouve rien et la voix du système parle. Un Supertonic
//! déjà sur le disque ne sert donc plus de repli. Le code reste pour le jour
//! où un paquet audio.cpp reviendrait au catalogue.

use anyhow::Result;
use std::sync::{Arc, Mutex};

use super::{EngineSettings, TtsEngine, TtsSession};

type Loader = Box<dyn FnOnce() -> Result<Arc<dyn TtsEngine>> + Send>;

/// Un moteur chargé au premier besoin ; s'il échoue, `fallback` parle.
pub struct OnDemand {
    label: String,
    fallback: Arc<dyn TtsEngine>,
    load: Mutex<Option<Loader>>,
    engine: Mutex<Option<Arc<dyn TtsEngine>>>,
}

impl OnDemand {
    pub fn new<F>(label: String, fallback: Arc<dyn TtsEngine>, load: F) -> Self
    where
        F: FnOnce() -> Result<Arc<dyn TtsEngine>> + Send + 'static,
    {
        Self { label, fallback, load: Mutex::new(Some(Box::new(load))), engine: Mutex::new(None) }
    }

    /// Le chargement se fait sous le verrou de `load` seul (il sérialise les
    /// appels simultanés) : `engine` n'est tenu qu'un instant, et `name`,
    /// demandé à chaque trame du terminal, n'attend jamais les secondes du
    /// chargement.
    fn resolve(&self) -> Arc<dyn TtsEngine> {
        if let Some(engine) = self.engine.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return Arc::clone(engine);
        }
        let mut load = self.load.lock().unwrap_or_else(|e| e.into_inner());
        // Un autre appel a pu finir le chargement pendant l'attente.
        if let Some(engine) = self.engine.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return Arc::clone(engine);
        }
        let chosen = match load.take().map(|load| load()) {
            Some(Ok(e)) => e,
            _ => Arc::clone(&self.fallback),
        };
        *self.engine.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&chosen));
        chosen
    }
}

impl TtsEngine for OnDemand {
    fn name(&self) -> String {
        match self.engine.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(engine) => engine.name(),
            None => self.label.clone(),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        self.resolve().open_session()
    }

    fn max_sessions(&self) -> usize {
        1
    }
}

/// La voix de repli pour ces réglages : la voix du système (voir la doc du
/// module pour la recherche audio.cpp, aujourd'hui sans résultat).
pub fn fallback_engine(settings: &EngineSettings) -> Arc<dyn TtsEngine> {
    let system: Arc<dyn TtsEngine> = Arc::new(super::system::SystemEngine::new());
    #[cfg(feature = "audio-cpp")]
    {
        let installed = crate::models::available()
            .into_iter()
            .find(|p| p.runtime == crate::models::Runtime::AudioCpp && crate::models::is_installed(p));
        if let Some((package, path)) = installed.and_then(|p| crate::models::install_path(p).map(|path| (p, path))) {
            let audio_cpp = EngineSettings {
                model_path: path.to_string_lossy().into_owned(),
                family: package.family.to_string(),
                backend: settings.backend.clone(),
                threads: settings.threads,
                sessions: crate::models::sessions_for_family(package.family),
            };
            return Arc::new(OnDemand::new(package.display_name.to_string(), system, move || {
                super::try_audio_cpp(&audio_cpp)
            }));
        }
    }
    let _ = settings;
    system
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_fallback_loads_once_and_only_when_it_speaks() {
        let loads = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&loads);
        let engine = OnDemand::new("Supertonic 3 · F16".into(), Arc::new(SilentEngine), move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(SilentEngine) as Arc<dyn TtsEngine>)
        });
        assert_eq!(loads.load(Ordering::SeqCst), 0, "rien chargé d'avance");
        assert_eq!(engine.name(), "Supertonic 3 · F16");
        engine.open_session().unwrap();
        engine.open_session().unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_fallback_that_fails_to_load_gives_the_system_voice() {
        let engine = OnDemand::new("audio.cpp".into(), Arc::new(SilentEngine), || anyhow::bail!("fichier manquant"));
        engine.open_session().unwrap();
        assert_eq!(engine.name(), "silencieux");
    }

    #[test]
    fn the_name_does_not_wait_for_a_load_in_progress() {
        // Le terminal demande le nom à chaque trame : un chargement de
        // plusieurs secondes ne doit pas figer l'interface.
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel::<()>();
        let engine = Arc::new(OnDemand::new("Supertonic 3 · F16".into(), Arc::new(SilentEngine), move || {
            started_tx.send(()).unwrap();
            let _ = finish_rx.recv();
            Ok(Arc::new(SilentEngine) as Arc<dyn TtsEngine>)
        }));
        let loading = Arc::clone(&engine);
        let loader = std::thread::spawn(move || loading.open_session().map(|_| ()));
        started_rx.recv().unwrap();

        let (name_tx, name_rx) = std::sync::mpsc::channel();
        let asking = Arc::clone(&engine);
        std::thread::spawn(move || name_tx.send(asking.name()).unwrap());
        let name = name_rx.recv_timeout(std::time::Duration::from_secs(2));
        finish_tx.send(()).unwrap();
        loader.join().unwrap().unwrap();
        assert_eq!(name.expect("le nom a attendu la fin du chargement"), "Supertonic 3 · F16");
        assert_eq!(engine.name(), "silencieux");
    }
}
