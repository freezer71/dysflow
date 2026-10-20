//! Ce que le lecteur montre pendant les secondes qui précèdent le premier son.
//!
//! Charger un GGUF et compiler les noyaux Metal prend environ trois secondes,
//! et dix au premier lancement d'une machine froide. Ce délai ne se raccourcit
//! pas depuis ici — il appartient au moteur. Ce qui se corrige, c'est le
//! silence : sans retour, l'outil paraît figé.
//!
//! Deux phases se succèdent, l'une et l'autre déduites de ce que le lecteur
//! sait déjà — le nom du moteur et les compteurs de synthèse. Rien n'est
//! ajouté au contrat du moteur pour les distinguer.

use std::time::Duration;

use crate::tts::SynthesisStats;

/// Étape en cours avant le premier son.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Le moteur lit son modèle : c'est le gros de l'attente.
    LoadingModel,
    /// Modèle prêt ; reste l'ouverture de session et la première phrase.
    PreparingFirstSentence,
}

impl Phase {
    pub fn label(self, family: &str) -> String {
        match self {
            Phase::LoadingModel => crate::tr!(en = "loading the {family} model…", fr = "chargement du modèle {family}…"),
            Phase::PreparingFirstSentence => {
                crate::tr!("synthesizing the first sentence…", "synthèse de la première phrase…").to_string()
            }
        }
    }
}

/// Phase de démarrage, ou `None` quand il n'y a plus rien à annoncer.
///
/// `already_playing` retient qu'un son a déjà été joué : sans lui, un
/// changement de voix — qui vide le cache — ferait réapparaître l'écran de
/// démarrage en pleine lecture.
pub fn phase(
    engine_name: &str,
    stats: SynthesisStats,
    already_playing: bool,
) -> Option<Phase> {
    // Un échec est annoncé ailleurs : maintenir le sablier ferait attendre un
    // son qui ne viendra pas.
    if already_playing || stats.total == 0 || stats.ready > 0 || stats.failed > 0 {
        return None;
    }
    // Le moteur se nomme « … · chargement… » (« … · loading… ») tant qu'il lit
    // son modèle : c'est la seule chose qui distingue les deux phases, et elle
    // suffit.
    if engine_name.contains("chargement") || engine_name.contains("loading") {
        return Some(Phase::LoadingModel);
    }
    Some(Phase::PreparingFirstSentence)
}

/// Referme le verrou dès qu'une phrase a été produite.
///
/// Un changement de voix vide le cache en pleine lecture ; sans ce verrou,
/// l'écran de démarrage se rejouerait à chaque fois.
pub fn latch(previous: bool, stats: SynthesisStats) -> bool {
    previous || stats.ready > 0
}

/// Sablier braille : dix images, un tour complet en huit dixièmes de seconde.
const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Image du sablier pour `elapsed`.
pub fn spinner(elapsed: Duration) -> char {
    FRAMES[(elapsed.as_millis() / 80) as usize % FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(ready: usize, total: usize, failed: usize) -> SynthesisStats {
        SynthesisStats {
            ready,
            in_flight: 0,
            failed,
            total,
            workers: 3,
            busy: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn announces_the_model_while_the_engine_says_it_is_loading() {
        let phase = phase("Kokoro · chargement…", stats(0, 12, 0), false);
        assert_eq!(phase, Some(Phase::LoadingModel));
    }

    #[test]
    fn announces_the_first_sentence_once_the_model_is_loaded() {
        // Session à ouvrir puis phrase à synthétiser : presque une seconde
        // pendant laquelle rien n'était annoncé.
        let phase = phase("Supertonic 3", stats(0, 12, 0), false);
        assert_eq!(phase, Some(Phase::PreparingFirstSentence));
    }

    #[test]
    fn goes_quiet_as_soon_as_a_sentence_is_ready() {
        assert_eq!(phase("Supertonic 3", stats(1, 12, 0), false), None);
    }

    #[test]
    fn stays_quiet_once_playback_has_started() {
        // Un changement de voix vide le cache en pleine lecture : le
        // démarrage ne doit pas se rejouer pour autant.
        assert_eq!(phase("Supertonic 3", stats(0, 12, 0), true), None);
    }

    #[test]
    fn stays_quiet_without_a_document() {
        // Écran de configuration : il n'y a rien à préparer.
        assert_eq!(phase("Kokoro · chargement…", stats(0, 0, 0), false), None);
    }

    #[test]
    fn gives_up_when_the_first_sentence_failed() {
        // L'échec est déjà signalé ailleurs ; laisser tourner le sablier
        // ferait attendre un son qui ne viendra pas.
        assert_eq!(phase("Supertonic 3", stats(0, 12, 1), false), None);
    }

    #[test]
    fn names_the_family_being_loaded() {
        assert_eq!(
            Phase::LoadingModel.label("supertonic"),
            "chargement du modèle supertonic…"
        );
        assert_eq!(
            Phase::PreparingFirstSentence.label("supertonic"),
            "synthèse de la première phrase…"
        );
    }

    #[test]
    fn the_latch_closes_on_the_first_ready_sentence_and_stays_closed() {
        assert!(!latch(false, stats(0, 12, 0)), "rien n'est encore prêt");
        assert!(latch(false, stats(1, 12, 0)), "une phrase est prête");
        // Cache vidé par un changement de voix : le verrou tient.
        assert!(latch(true, stats(0, 12, 0)));
    }

    #[test]
    fn the_spinner_turns_with_time() {
        let first = spinner(Duration::ZERO);
        let later = spinner(Duration::from_millis(400));
        assert_ne!(first, later, "le sablier doit tourner");
        // Et il boucle plutôt que de s'arrêter.
        assert_eq!(first, spinner(Duration::from_millis(800)));
    }
}
