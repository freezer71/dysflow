//! Le rythme de la relecture provisoire, et l'horloge de la dictée.
//!
//! Port de `RollingTranscription.swift` : logique pure, l'heure et l'état
//! « en cours » viennent de l'appelant. Dans la découpe aux silences, la
//! relecture ne porte que sur l'énoncé ouvert (`Dictation`).

use std::time::{Duration, Instant};

/// Le plus court écart entre deux lectures : un modèle rapide ne relit pas
/// plus souvent.
pub const FASTEST: Duration = Duration::from_millis(400);
/// L'écart d'avant la première mesure.
pub const INITIAL: Duration = Duration::from_millis(1500);

/// L'heure de la dictée et de `Holder`, simulée dans les tests.
pub trait Clock: Send + Sync {
    /// Le temps écoulé depuis la création de l'horloge.
    fn now(&self) -> Duration;
    /// Le temps réel à attendre avant de relire `now`, pour une échéance
    /// dans `due`.
    fn pause(&self, due: Duration) -> Duration;
}

/// L'horloge monotone du système.
pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn pause(&self, due: Duration) -> Duration {
        due
    }
}

/// Le rythme des relectures : jamais deux à la fois, une par `interval` au
/// plus, la première tout de suite.
#[derive(Debug, Clone)]
pub struct Rolling {
    /// Entre le début d'une lecture et celui de la suivante.
    interval: Duration,
    last_start: Option<Duration>,
}

impl Rolling {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_start: None,
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Règle le rythme sur la dernière lecture : trois fois sa durée, jamais
    /// moins de `FASTEST`, sans plafond. Un modèle rapide relit souvent (le
    /// texte suit la voix) ; un lent espace ses lectures : la puce n'est
    /// jamais occupée plus d'un tiers du temps.
    pub fn adapt(&mut self, last_read: Duration) {
        self.interval = FASTEST.max(last_read * 3);
    }

    /// Vrai s'il faut lancer une lecture maintenant : aucune n'est en cours,
    /// et `interval` s'est écoulé depuis le début de la précédente (la
    /// première, tout de suite). Vrai compte comme un début de lecture.
    pub fn should_read(&mut self, now: Duration, busy: bool) -> bool {
        if busy {
            return false;
        }
        if self.due().is_some_and(|due| now < due) {
            return false;
        }
        self.last_start = Some(now);
        true
    }

    /// L'heure à partir de laquelle `should_read` peut rendre vrai.
    pub fn due(&self) -> Option<Duration> {
        self.last_start.map(|start| start + self.interval)
    }
}

/// Une horloge que le test avance à la main.
#[cfg(test)]
pub(crate) struct ManualClock(std::sync::Mutex<Duration>);

#[cfg(test)]
impl ManualClock {
    pub(crate) fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self(std::sync::Mutex::new(Duration::ZERO)))
    }

    pub(crate) fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }

    /// Avance jusqu'à `at` ; ne recule jamais.
    pub(crate) fn advance_to(&self, at: Duration) {
        let mut now = self.0.lock().unwrap();
        *now = (*now).max(at);
    }
}

#[cfg(test)]
impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap()
    }

    /// Le temps n'avance que par le test : on regarde de nouveau très vite.
    fn pause(&self, _due: Duration) -> Duration {
        Duration::from_millis(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn waits_for_the_interval_and_never_overlaps() {
        let t0 = Duration::ZERO;
        let mut r = Rolling::new(ms(1500));
        assert!(r.should_read(t0, false), "première lecture tout de suite");
        assert!(!r.should_read(t0 + ms(500), false));
        assert!(!r.should_read(t0 + ms(2000), true), "jamais deux à la fois");
        assert!(r.should_read(t0 + ms(2000), false));
    }

    #[test]
    fn the_rhythm_follows_the_model_speed() {
        let mut r = Rolling::new(INITIAL);
        r.adapt(ms(100));
        assert_eq!(
            r.interval(),
            FASTEST,
            "un modèle rapide relit toutes les 0,4 s"
        );
        r.adapt(ms(300));
        assert_eq!(
            r.interval(),
            ms(900),
            "trois fois la lecture : un tiers du temps occupé"
        );
        r.adapt(Duration::from_secs(2));
        assert_eq!(
            r.interval(),
            Duration::from_secs(6),
            "une lecture lente : toujours trois fois, sans plafond"
        );
    }

    #[test]
    fn the_next_read_is_due_one_interval_after_the_last_start() {
        let mut r = Rolling::new(ms(1500));
        assert_eq!(r.due(), None, "aucune lecture : tout de suite");
        assert!(r.should_read(ms(200), false));
        assert_eq!(r.due(), Some(ms(1700)));
        assert!(!r.should_read(ms(1699), false));
        assert!(r.should_read(ms(1700), false));
    }
}
