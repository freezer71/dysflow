//! Cadence de lecture mesurée, d'où sortent les temps affichés.
//!
//! L'estimation ne repose pas sur une constante de mots par seconde mais sur ce
//! que le moteur a réellement produit : elle s'affine à mesure que la lecture
//! avance, et vaut pour la voix en cours plutôt que pour une moyenne.

/// Secondes de parole par mot, accumulées sur ce qui a été synthétisé.
///
/// Toujours à 1× : la synthèse ne connaît pas la vitesse. L'app divise par la
/// vitesse de lecture au moment d'afficher.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pace {
    seconds: f64,
    words: usize,
}

impl Pace {
    /// Enregistre une phrase synthétisée.
    pub fn record(&mut self, seconds: f64, words: usize) {
        // Une phrase sans mot ne dit rien de la cadence.
        if words == 0 {
            return;
        }
        self.seconds += seconds;
        self.words += words;
    }

    /// Secondes par mot, ou `None` tant que rien n'a été mesuré.
    pub fn seconds_per_word(&self) -> Option<f64> {
        (self.words > 0).then(|| self.seconds / self.words as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_nothing_before_the_first_measure() {
        assert_eq!(Pace::default().seconds_per_word(), None);
    }

    #[test]
    fn averages_over_everything_measured() {
        let mut pace = Pace::default();
        pace.record(4.0, 10);
        pace.record(2.0, 10);
        assert_eq!(pace.seconds_per_word(), Some(0.3));
    }

    #[test]
    fn ignores_a_sentence_without_words() {
        let mut pace = Pace::default();
        pace.record(1.0, 0);
        assert_eq!(pace.seconds_per_word(), None);
    }
}
