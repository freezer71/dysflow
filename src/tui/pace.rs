//! Cadence de lecture mesurée, d'où sortent les temps affichés.
//!
//! L'estimation ne repose pas sur une constante de mots par seconde mais sur ce
//! que le moteur a réellement produit : elle s'affine à mesure que la lecture
//! avance, et vaut pour la voix en cours plutôt que pour une moyenne.

/// Secondes de parole par mot, accumulées sur ce qui a été synthétisé.
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

    /// Transpose les mesures d'un débit à un autre.
    ///
    /// Le débit est appliqué à la synthèse : parler deux fois plus vite tient
    /// la même parole en deux fois moins de temps. Sans cette transposition,
    /// l'estimation resterait celle de l'ancienne vitesse jusqu'à ce que
    /// suffisamment de phrases neuves aient dilué les anciennes mesures.
    pub fn rescale(&mut self, from: f32, to: f32) {
        if to <= 0.0 || from <= 0.0 {
            return;
        }
        self.seconds *= from as f64 / to as f64;
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

    #[test]
    fn doubling_the_speed_halves_the_estimate() {
        let mut pace = Pace::default();
        pace.record(4.0, 10);
        pace.rescale(1.0, 2.0);
        assert_eq!(pace.seconds_per_word(), Some(0.2));
        // Et le retour au débit initial rend l'estimation d'origine.
        pace.rescale(2.0, 1.0);
        assert_eq!(pace.seconds_per_word(), Some(0.4));
    }

    #[test]
    fn rescaling_without_a_measure_stays_quiet() {
        let mut pace = Pace::default();
        pace.rescale(1.0, 2.0);
        assert_eq!(pace.seconds_per_word(), None);
    }

    #[test]
    fn refuses_a_speed_that_would_divide_by_zero() {
        let mut pace = Pace::default();
        pace.record(4.0, 10);
        pace.rescale(1.0, 0.0);
        // Mesure inchangée plutôt qu'infinie.
        assert_eq!(pace.seconds_per_word(), Some(0.4));
    }
}
