//! Les pas des décodeurs de NeMo, sans MLX : RNN-T, TDT et les tronçons
//! d'un CTC glouton. Port de `NemoDecodingLogic.swift`.

/// Le pas suivant d'un RNN-T glouton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RnntStep {
    pub next_time: usize,
    pub next_new_symbols: usize,
    pub emitted_token: bool,
}

/// Le pas suivant d'un TDT glouton : `jump`, la durée prédite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TdtStep {
    pub next_time: usize,
    pub next_new_symbols: usize,
    pub jump: usize,
    pub emitted_token: bool,
}

/// Un jeton CTC et ses trames `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtcSpan {
    pub token: usize,
    pub start_frame: usize,
    pub end_frame: usize,
}

/// RNN-T : un blanc avance d'une trame ; un jeton reste sur la trame, sauf
/// après `max_symbols` jetons de suite.
pub fn rnnt_step(
    predicted: usize,
    blank: usize,
    time: usize,
    new_symbols: usize,
    max_symbols: Option<usize>,
) -> RnntStep {
    if predicted == blank {
        return RnntStep {
            next_time: time + 1,
            next_new_symbols: 0,
            emitted_token: false,
        };
    }
    let next = new_symbols + 1;
    match max_symbols {
        Some(max) if next >= max => RnntStep {
            next_time: time + 1,
            next_new_symbols: 0,
            emitted_token: true,
        },
        _ => RnntStep {
            next_time: time,
            next_new_symbols: next,
            emitted_token: true,
        },
    }
}

/// TDT : avance de la durée prédite (`durations[decision]`, 1 hors de la
/// table) ; une durée nulle reste sur la trame, sauf après `max_symbols`
/// pas de suite.
pub fn tdt_step(
    predicted: usize,
    blank: usize,
    decision: usize,
    durations: &[usize],
    time: usize,
    new_symbols: usize,
    max_symbols: Option<usize>,
) -> TdtStep {
    let jump = durations.get(decision).copied().unwrap_or(1);
    let mut next_time = time + jump;
    let mut next_new_symbols = new_symbols + 1;
    if jump != 0 {
        next_new_symbols = 0;
    } else if max_symbols.is_some_and(|max| next_new_symbols >= max) {
        next_time += 1;
        next_new_symbols = 0;
    }
    TdtStep {
        next_time,
        next_new_symbols,
        jump,
        emitted_token: predicted != blank,
    }
}

/// Les tronçons d'un CTC glouton : répétitions fusionnées, blancs retirés ;
/// le dernier finit après sa dernière trame non blanche.
pub fn ctc_spans(best: &[usize], blank: usize) -> Vec<CtcSpan> {
    let mut spans = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for (t, &token) in best.iter().enumerate() {
        if token == blank {
            if let Some((previous, start)) = current.take() {
                spans.push(CtcSpan {
                    token: previous,
                    start_frame: start,
                    end_frame: t,
                });
            }
            continue;
        }
        if current.is_some_and(|(previous, _)| previous == token) {
            continue;
        }
        if let Some((previous, start)) = current.take() {
            spans.push(CtcSpan {
                token: previous,
                start_frame: start,
                end_frame: t,
            });
        }
        current = Some((token, t));
    }
    if let Some((previous, start)) = current {
        let last = (start..best.len())
            .rev()
            .find(|&t| best[t] != blank)
            .unwrap_or(best.len() - 1);
        spans.push(CtcSpan {
            token: previous,
            start_frame: start,
            end_frame: last + 1,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    const DURATIONS: [usize; 5] = [0, 1, 2, 3, 4];

    #[test]
    fn tdt_avance_de_la_duree() {
        let step = tdt_step(5, 8192, 2, &DURATIONS, 10, 3, Some(10));
        assert_eq!(
            step,
            TdtStep {
                next_time: 12,
                next_new_symbols: 0,
                jump: 2,
                emitted_token: true
            }
        );
        let blank = tdt_step(8192, 8192, 1, &DURATIONS, 10, 0, Some(10));
        assert_eq!(
            blank,
            TdtStep {
                next_time: 11,
                next_new_symbols: 0,
                jump: 1,
                emitted_token: false
            }
        );
    }

    #[test]
    fn tdt_duree_nulle_bornee_par_max_symbols() {
        let stay = tdt_step(5, 8192, 0, &DURATIONS, 4, 8, Some(10));
        assert_eq!((stay.next_time, stay.next_new_symbols), (4, 9));
        let forced = tdt_step(5, 8192, 0, &DURATIONS, 4, 9, Some(10));
        assert_eq!((forced.next_time, forced.next_new_symbols), (5, 0));
        let unbounded = tdt_step(5, 8192, 0, &DURATIONS, 4, 99, None);
        assert_eq!((unbounded.next_time, unbounded.next_new_symbols), (4, 100));
    }

    #[test]
    fn tdt_decision_hors_table() {
        assert_eq!(tdt_step(5, 8192, 9, &DURATIONS, 0, 0, Some(10)).jump, 1);
    }

    #[test]
    fn rnnt_pas() {
        assert_eq!(rnnt_step(3, 3, 7, 2, Some(5)).next_time, 8);
        let stay = rnnt_step(1, 3, 7, 2, Some(5));
        assert_eq!(
            (stay.next_time, stay.next_new_symbols, stay.emitted_token),
            (7, 3, true)
        );
        let forced = rnnt_step(1, 3, 7, 4, Some(5));
        assert_eq!((forced.next_time, forced.next_new_symbols), (8, 0));
    }

    #[test]
    fn ctc_troncons() {
        let blank = 0;
        assert!(ctc_spans(&[], blank).is_empty());
        let spans = ctc_spans(&[0, 3, 3, 0, 3, 5, 5, 0, 0], blank);
        assert_eq!(
            spans,
            vec![
                CtcSpan {
                    token: 3,
                    start_frame: 1,
                    end_frame: 3
                },
                CtcSpan {
                    token: 3,
                    start_frame: 4,
                    end_frame: 5
                },
                CtcSpan {
                    token: 5,
                    start_frame: 5,
                    end_frame: 7
                },
            ]
        );
        assert_eq!(
            ctc_spans(&[2, 2], blank),
            vec![CtcSpan {
                token: 2,
                start_frame: 0,
                end_frame: 2
            }]
        );
    }
}
