//! L'arithmétique de la ligne de temps : phrases ↔ position, repères, horloge.
//!
//! Sans egui : c'est ce qui se vérifie. La ligne de temps compte en phrases,
//! pas en secondes — le temps n'est connu qu'une fois synthétisé, la phrase
//! l'est toujours.

/// Position de la phrase courante sur la ligne, de 0 à 1.
pub fn ratio(current: usize, total: usize) -> f32 {
    if total <= 1 {
        return 0.0;
    }
    (current.min(total - 1) as f32 / (total - 1) as f32).clamp(0.0, 1.0)
}

/// La phrase désignée par un point de la ligne.
pub fn sentence_at(ratio: f32, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    ((ratio.clamp(0.0, 1.0) * (total - 1) as f32).round() as usize).min(total - 1)
}

/// Les repères de section sur la ligne, hors celui du tout début.
pub fn ticks(firsts: impl IntoIterator<Item = usize>, total: usize) -> Vec<f32> {
    let mut ticks: Vec<f32> = firsts
        .into_iter()
        .filter(|&first| first > 0 && first < total)
        .map(|first| ratio(first, total))
        .collect();
    ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ticks.dedup();
    ticks
}

/// Des secondes en `M:SS`, ou `H:MM:SS` passé l'heure.
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sentence_is_at_the_start_and_the_last_at_the_end() {
        assert_eq!(ratio(0, 10), 0.0);
        assert_eq!(ratio(9, 10), 1.0);
        assert!((ratio(3, 7) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_single_sentence_sits_at_the_start() {
        assert_eq!(ratio(0, 1), 0.0);
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(sentence_at(0.7, 1), 0);
        assert_eq!(sentence_at(0.7, 0), 0);
    }

    #[test]
    fn a_point_on_the_line_names_a_sentence_and_back() {
        for current in 0..12 {
            assert_eq!(sentence_at(ratio(current, 12), 12), current);
        }
        assert_eq!(sentence_at(-0.5, 12), 0);
        assert_eq!(sentence_at(1.5, 12), 11);
    }

    #[test]
    fn section_ticks_skip_the_start_and_the_out_of_range() {
        let ticks = ticks([0, 5, 5, 20, 9], 10);
        assert_eq!(ticks.len(), 2);
        assert!((ticks[0] - ratio(5, 10)).abs() < 1e-6);
        assert!((ticks[1] - ratio(9, 10)).abs() < 1e-6);
    }

    #[test]
    fn the_clock_reads_like_a_player() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(62.4), "1:02");
        assert_eq!(clock(3661.0), "1:01:01");
        assert_eq!(clock(-3.0), "0:00");
    }
}
