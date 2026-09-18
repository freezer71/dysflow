//! Position de la vue, distincte de la position de lecture.
//!
//! Suivre la voix des yeux suppose de pouvoir la lâcher : relire le paragraphe
//! précédent sans interrompre le son, puis revenir. La vue se détache donc de
//! la lecture au premier défilement, et s'y recolle sur demande.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewAnchor {
    /// Bloc sur lequel la vue est épinglée, ou `None` si elle suit la lecture.
    pinned: Option<usize>,
}

impl ViewAnchor {
    /// Bloc autour duquel dessiner, la lecture étant sur `current`.
    pub fn block(&self, current: usize) -> usize {
        self.pinned.unwrap_or(current)
    }

    /// Vrai tant que la vue suit la voix.
    pub fn is_following(&self) -> bool {
        self.pinned.is_none()
    }

    /// Déplace la vue de `delta` blocs, sans toucher à la lecture.
    ///
    /// Le premier défilement détache la vue : on ne fait pas défiler un texte
    /// pour se le voir reprendre sous les doigts à la phrase suivante.
    pub fn scroll(&mut self, delta: isize, current: usize, last: usize) {
        let from = self.pinned.unwrap_or(current) as isize;
        self.pinned = Some((from + delta).clamp(0, last as isize) as usize);
    }

    /// Recolle la vue sur la lecture.
    pub fn follow(&mut self) {
        self.pinned = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_view_follows_the_voice() {
        let view = ViewAnchor::default();
        assert!(view.is_following());
        assert_eq!(view.block(42), 42);
    }

    #[test]
    fn scrolling_detaches_the_view_from_the_voice() {
        let mut view = ViewAnchor::default();
        view.scroll(-3, 10, 99);
        assert!(!view.is_following());
        assert_eq!(view.block(10), 7);
        // La lecture continue d'avancer sans emporter la vue.
        assert_eq!(view.block(20), 7);
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut view = ViewAnchor::default();
        view.scroll(-100, 2, 99);
        assert_eq!(view.block(2), 0);
        view.scroll(1000, 2, 99);
        assert_eq!(view.block(2), 99);
    }

    #[test]
    fn following_again_snaps_back_to_the_voice() {
        let mut view = ViewAnchor::default();
        view.scroll(-5, 30, 99);
        view.follow();
        assert!(view.is_following());
        assert_eq!(view.block(30), 30);
    }

    #[test]
    fn an_empty_document_has_nowhere_to_scroll() {
        let mut view = ViewAnchor::default();
        view.scroll(5, 0, 0);
        assert_eq!(view.block(0), 0);
    }
}
