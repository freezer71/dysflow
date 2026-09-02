//! Arithmétique du défilement, sans egui.
//!
//! La fenêtre compose les blocs à la demande ; en attendant, chacun a une
//! hauteur estimée. Ce module décide où placer la vue, comment l'y amener, et
//! comment absorber les corrections sans que le texte bouge sous les yeux.

/// Hauteur estimée d'un bloc jamais composé.
pub fn estimated_height(chars: usize, chars_per_line: f32, line_height: f32, gap: f32) -> f32 {
    let per_line = chars_per_line.max(1.0);
    let lines = (chars as f32 / per_line).ceil().max(1.0);
    lines * line_height + gap
}

/// Décalage qui tient `row_top` au tiers supérieur de la fenêtre.
///
/// Au tiers, pas au centre : on voit ce qui vient plus que ce qui est passé.
pub fn follow_target(row_top: f32, viewport_height: f32, content_height: f32) -> f32 {
    let target = row_top - viewport_height / 3.0;
    target.clamp(0.0, (content_height - viewport_height).max(0.0))
}

/// Rapproche le décalage de sa cible : en douceur de près, d'un bond de loin.
///
/// Glisser sur un document entier après un saut au sommaire prendrait des
/// secondes ; un bond dit mieux qu'on a changé de page.
pub fn approach(current: f32, target: f32, viewport_height: f32) -> f32 {
    let delta = target - current;
    if delta.abs() < 0.5 || delta.abs() > viewport_height {
        return target;
    }
    current + delta * 0.25
}

/// Absorbe le changement de hauteur d'un bloc entièrement au-dessus de la vue.
///
/// Ce qui est à l'écran ne doit pas bouger quand un bloc hors champ prend sa
/// hauteur réelle.
pub fn absorb(offset: f32, block_top: f32, old_height: f32, new_height: f32) -> f32 {
    if block_top + old_height <= offset {
        (offset + new_height - old_height).max(0.0)
    } else {
        offset
    }
}

/// Bloc dont le haut est le dernier à ne pas dépasser `offset`.
pub fn block_at(tops: &[f32], offset: f32) -> usize {
    let after = tops.partition_point(|&top| top <= offset);
    after.saturating_sub(1)
}

/// Vrai quand le décalage relu diffère de celui qu'on avait posé : quelqu'un
/// a défilé. Un pixel d'arrondi n'est pas un geste.
pub fn user_scrolled(set: f32, read: f32) -> bool {
    (set - read).abs() > 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_never_laid_out_is_estimated_from_its_length() {
        // 100 caractères à 40 par ligne : trois lignes, plus l'écart.
        assert_eq!(estimated_height(100, 40.0, 30.0, 10.0), 100.0);
        // Un bloc vide tient quand même une ligne.
        assert_eq!(estimated_height(0, 40.0, 30.0, 10.0), 40.0);
    }

    #[test]
    fn the_spoken_line_is_held_at_the_upper_third() {
        assert_eq!(follow_target(600.0, 300.0, 10_000.0), 500.0);
        // Jamais avant le début, jamais au-delà de la fin.
        assert_eq!(follow_target(50.0, 300.0, 10_000.0), 0.0);
        assert_eq!(follow_target(9_950.0, 300.0, 10_000.0), 9_700.0);
    }

    #[test]
    fn the_view_glides_when_close_and_jumps_when_far() {
        assert_eq!(approach(0.0, 100.0, 600.0), 25.0);
        assert_eq!(approach(0.0, 5_000.0, 600.0), 5_000.0);
        assert_eq!(approach(99.8, 100.0, 600.0), 100.0);
    }

    #[test]
    fn a_block_above_the_view_that_changes_height_does_not_move_the_text() {
        // Le bloc 100..200 devient 100..260 : la vue à 500 descend de 60.
        assert_eq!(absorb(500.0, 100.0, 100.0, 160.0), 560.0);
        // Un bloc sous la vue ne la déplace pas.
        assert_eq!(absorb(500.0, 800.0, 100.0, 160.0), 500.0);
        // Un bloc qui disparaît au-dessus ramène la vue d'autant, jamais au-delà
        // de zéro.
        assert_eq!(absorb(30.0, 0.0, 30.0, 0.0), 0.0);
        // Un bloc qui chevauche le haut de la vue n'est pas « au-dessus ».
        assert_eq!(absorb(20.0, 0.0, 100.0, 50.0), 20.0);
    }

    #[test]
    fn the_block_at_an_offset_is_the_last_one_starting_above_it() {
        let tops = [0.0, 100.0, 250.0, 400.0];
        assert_eq!(block_at(&tops, 0.0), 0);
        assert_eq!(block_at(&tops, 120.0), 1);
        assert_eq!(block_at(&tops, 250.0), 2);
        assert_eq!(block_at(&tops, 9_000.0), 3);
        assert_eq!(block_at(&[], 10.0), 0);
    }

    #[test]
    fn a_pixel_of_rounding_is_not_a_scroll() {
        assert!(!user_scrolled(100.0, 100.6));
        assert!(user_scrolled(100.0, 140.0));
    }
}
