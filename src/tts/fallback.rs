//! La voix de repli quand le moteur voulu échoue : le premier modèle
//! audio.cpp installé, chargé seulement s'il doit parler, sinon la voix du
//! système.

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

    fn resolve(&self) -> Arc<dyn TtsEngine> {
        let mut engine = self.engine.lock().unwrap();
        if let Some(engine) = engine.as_ref() {
            return Arc::clone(engine);
        }
        let loaded = self.load.lock().unwrap().take().map(|load| load());
        let chosen = match loaded {
            Some(Ok(e)) => e,
            _ => Arc::clone(&self.fallback),
        };
        *engine = Some(Arc::clone(&chosen));
        chosen
    }
}

impl TtsEngine for OnDemand {
    fn name(&self) -> String {
        match self.engine.lock().unwrap().as_ref() {
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

/// La voix de repli pour ces réglages.
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
}
