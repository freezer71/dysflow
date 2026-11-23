//! La dictée : reconnaissance de la parole au terminal.

pub mod detector;
pub mod dictation;
pub mod file;
pub mod holder;
pub mod recognizer;
pub mod rolling;
pub mod segmenter;
pub mod wer;

#[cfg(feature = "stt-sherpa")]
pub mod sherpa;

use crate::models::ModelPackage;
use anyhow::Result;
use dictation::Engine;

/// Le moteur de dictée d'un paquet installé, prêt à passer à
/// `Dictation::start`. Rien n'est chargé ici : `Dictation` charge le modèle.
///
/// Le détecteur de parole se choisit ici, et seulement ici (R46). Aucun
/// paquet installé ne porte de modèle Silero (`silero_vad.onnx` n'est que
/// celui de la porte, `scripts/ort-gate.sh`) : `EnergyDetector` partout. Quand
/// la porte de la tâche 10 aura tranché (D5), Silero pour un reconnaisseur
/// sherpa se met dans l'embranchement de `Runtime::ParakeetSherpa`.
pub fn engine_for(package: &ModelPackage, threads: u32) -> Result<Engine> {
    match package.runtime {
        #[cfg(feature = "stt-sherpa")]
        crate::models::Runtime::ParakeetSherpa => Ok(Engine::Segmented(
            std::sync::Arc::new(sherpa::parakeet::ParakeetSherpa::new(threads)),
            Box::new(detector::EnergyDetector::new()),
        )),
        // Un moteur que ce binaire ne sait pas calculer : seul l'app, ou un
        // binaire construit avec la feature, le fait.
        runtime => {
            let _ = threads;
            anyhow::bail!(crate::tr!(
                en = "this build cannot run {} (engine {runtime:?})",
                fr = "ce binaire ne sait pas lancer {} (moteur {runtime:?})",
                package.display_name.get()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CATALOG;

    fn package(id: &str) -> &'static ModelPackage {
        CATALOG
            .iter()
            .find(|p| p.id == id)
            .expect("paquet du catalogue")
    }

    #[test]
    fn a_sherpa_package_gets_a_segmented_engine_where_the_build_can_run_it() {
        let engine = engine_for(package("parakeet_tdt_v3_int8"), 4);
        if cfg!(feature = "stt-sherpa") {
            assert!(matches!(engine, Ok(Engine::Segmented(..))));
        } else {
            let err = engine.err().expect("refusé sans stt-sherpa").to_string();
            assert!(err.contains("Parakeet"), "{err}");
        }
    }

    #[test]
    fn an_engine_this_binary_cannot_compute_is_refused_by_name() {
        let err = engine_for(package("parakeet_tdt_v3"), 4)
            .err()
            .expect("MLX : pas ici")
            .to_string();
        assert!(err.contains("ParakeetMlx"), "{err}");
    }
}
