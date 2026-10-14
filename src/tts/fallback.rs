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
