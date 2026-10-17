//! La voix de repli quand le moteur voulu échoue : la voix du système.
//!
//! Avant le 14 oct. 2026, un paquet audio.cpp installé (Supertonic) pouvait
//! l'être, chargé au premier besoin ; ce moteur est sorti du projet.

use std::sync::Arc;

use super::{EngineSettings, TtsEngine};

/// La voix de repli pour ces réglages : la voix du système.
pub fn fallback_engine(_settings: &EngineSettings) -> Arc<dyn TtsEngine> {
    Arc::new(super::system::SystemEngine::new())
}

/// La voix de repli qui dit pourquoi elle parle : un moteur voulu absent
/// (fichier principal manquant) se lit `Readiness::Fallback(note)`, pas
/// « prêt », pour que l'interface montre la raison.
pub fn explained(fallback: Arc<dyn TtsEngine>, note: String) -> Arc<dyn TtsEngine> {
    Arc::new(Explained { fallback, note })
}

struct Explained {
    fallback: Arc<dyn TtsEngine>,
    note: String,
}

impl TtsEngine for Explained {
    fn name(&self) -> String {
        format!("{} (repli)", self.fallback.name())
    }

    fn open_session(&self) -> anyhow::Result<Box<dyn super::TtsSession>> {
        self.fallback.open_session()
    }

    fn max_sessions(&self) -> usize {
        self.fallback.max_sessions()
    }

    fn readiness(&self) -> super::Readiness {
        super::Readiness::Fallback(self.note.clone())
    }
}
