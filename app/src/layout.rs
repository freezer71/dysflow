//! Mise en page du document en lignes, sans GPUI : la largeur des mots vient
//! d'un `Measure`, que la vue implémente. Sert à placer le surlignage, à
//! viser un mot à la souris et à passer d'une ligne affichée à l'autre.

use dysflow::reader::view::{BlockKindView, DocView, WordView};

/// Une boîte de mot sur une ligne ; `(sentence, word)` est le couple que
/// `SeekToWord` attend. Un mot plus large que la ligne donne
/// plusieurs boîtes de même `(sentence, word)`.
#[derive(Debug, Clone, PartialEq)]
pub struct WordBox {
    /// `SentenceView.id`, l'identifiant global de la phrase.
    pub sentence: usize,
    /// Position du mot dans sa phrase.
    pub word: usize,
    /// `WordView.index`, l'index global du mot.
    pub index: usize,
    pub x: f32,
    pub width: f32,
}

/// Une ligne affichée. Une ligne de code ou de tableau (montrée telle
/// quelle) n'a pas de mots suivis : `words` est vide.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub block: usize,
    pub y: f32,
    pub height: f32,
    pub words: Vec<WordBox>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub height: f32,
}

/// Ce que la vue sait mesurer.
pub trait Measure {
    fn width(&self, text: &str, word: &WordView, kind: &BlockKindView) -> f32;
    fn line_height(&self, kind: &BlockKindView) -> f32;
}

fn verbatim(kind: &BlockKindView) -> bool {
    matches!(
        kind,
        BlockKindView::Code { .. } | BlockKindView::TableRow { .. }
    )
}

/// Une mesure sûre : jamais négative, jamais NaN.
fn clean(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

/// Met le document en lignes de `width` pixels au plus. L'espace entre deux
/// mots (même d'une phrase à l'autre) est mesurée comme `" "` ; elle ne se
/// compte pas en fin de ligne. `spacing` lignes vides suivent chaque ligne.
///
/// Un caractère seul plus large que la ligne déborde : c'est le seul cas où
/// une boîte dépasse `width`, faute de pouvoir couper plus.
pub fn lay_out(doc: &DocView, width: f32, spacing: usize, measure: &dyn Measure) -> Layout {
    let width = clean(width);
    let mut lines: Vec<Line> = Vec::new();
    let mut y = 0.0f32;

    for (b, block) in doc.blocks.iter().enumerate() {
        let kind = &block.kind;
        let line_height = clean(measure.line_height(kind));
        let advance = line_height * (1 + spacing) as f32;
        let mut push = |words: Vec<WordBox>, y: &mut f32| {
            lines.push(Line {
                block: b,
                y: *y,
                height: advance,
                words,
            });
            *y += advance;
        };

        if verbatim(kind) && !block.display_lines.is_empty() {
            for _ in &block.display_lines {
                push(Vec::new(), &mut y);
            }
            continue;
        }

        let mut current: Vec<WordBox> = Vec::new();
        let mut x = 0.0f32;
        for sentence in &block.sentences {
            for (wi, word) in sentence.words.iter().enumerate() {
                let gap = if current.is_empty() {
                    0.0
                } else {
                    clean(measure.width(" ", word, kind))
                };
                let w = clean(measure.width(&word.text, word, kind));
                if current.is_empty() || x + gap + w <= width {
                    if w <= width {
                        let at = x + gap;
                        current.push(WordBox {
                            sentence: sentence.id,
                            index: word.index,
                            word: wi,
                            x: at,
                            width: w,
                        });
                        x = at + w;
                        continue;
                    }
                } else {
                    push(std::mem::take(&mut current), &mut y);
                    x = 0.0;
                    if w <= width {
                        current.push(WordBox {
                            sentence: sentence.id,
                            index: word.index,
                            word: wi,
                            x: 0.0,
                            width: w,
                        });
                        x = w;
                        continue;
                    }
                }
                // Mot plus large que la ligne : on le coupe, au moins un
                // caractère par boîte pour toujours avancer.
                if !current.is_empty() {
                    push(std::mem::take(&mut current), &mut y);
                }
                let chars: Vec<char> = word.text.chars().collect();
                let mut start = 0;
                while start < chars.len() {
                    let mut end = start + 1;
                    let mut piece: String = chars[start..end].iter().collect();
                    let mut piece_w = clean(measure.width(&piece, word, kind));
                    while end < chars.len() {
                        let mut longer = piece.clone();
                        longer.push(chars[end]);
                        let lw = clean(measure.width(&longer, word, kind));
                        if lw > width {
                            break;
                        }
                        piece = longer;
                        piece_w = lw;
                        end += 1;
                    }
                    let b = WordBox {
                        sentence: sentence.id,
                        index: word.index,
                        word: wi,
                        x: 0.0,
                        width: piece_w,
                    };
                    start = end;
                    if start < chars.len() {
                        push(vec![b], &mut y);
                    } else {
                        // Le dernier morceau continue la ligne.
                        x = piece_w;
                        current.push(b);
                    }
                }
            }
        }
        if !current.is_empty() {
            push(current, &mut y);
        }
    }
    Layout { lines, height: y }
}

impl Layout {
    /// La première ligne qui montre ce mot.
    pub fn line_of(&self, sentence: usize, word: usize) -> Option<usize> {
        self.lines.iter().position(|l| {
            l.words
                .iter()
                .any(|b| b.sentence == sentence && b.word == word)
        })
    }

    /// Le premier mot de la ligne voisine (seul le signe de `delta` compte),
    /// comme `LineNavigator.swift` : les lignes sans mot suivi (code,
    /// tableaux) sont enjambées, et un mot coupé sur deux lignes ne retient
    /// pas sur place.
    pub fn neighbour(&self, sentence: usize, word: usize, delta: isize) -> Option<(usize, usize)> {
        if delta == 0 {
            return None;
        }
        let mut i = self.line_of(sentence, word)? as isize;
        let step = delta.signum();
        loop {
            i += step;
            let line = self.lines.get(usize::try_from(i).ok()?)?;
            if let Some(first) = line.words.first() {
                if (first.sentence, first.word) != (sentence, word) {
                    return Some((first.sentence, first.word));
                }
            }
        }
    }

    /// Le mot sous le point. Entre deux mots, c'est le mot de gauche ; à
    /// gauche du premier ou à droite du dernier mot d'une ligne, ou hors de
    /// toute ligne : rien.
    pub fn hit(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let line = self.lines.iter().find(|l| y >= l.y && y < l.y + l.height)?;
        let first = line.words.first()?;
        let last = line.words.last()?;
        if x < first.x || x > last.x + last.width {
            return None;
        }
        let b = line.words.iter().rev().find(|b| b.x <= x)?;
        Some((b.sentence, b.word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dysflow::reader::view::{BlockView, SentenceView};

    /// 10 px par caractère, une espace comprise ; lignes de 20 px.
    struct Mono;
    impl Measure for Mono {
        fn width(&self, text: &str, _: &WordView, _: &BlockKindView) -> f32 {
            text.chars().count() as f32 * 10.0
        }
        fn line_height(&self, _: &BlockKindView) -> f32 {
            20.0
        }
    }

    fn para(sentences: &[&str]) -> BlockView {
        BlockView {
            sentences: sentences
                .iter()
                .enumerate()
                .map(|(i, s)| SentenceView {
                    id: i,
                    words: s
                        .split(' ')
                        .enumerate()
                        .map(|(j, t)| WordView {
                            index: j,
                            text: t.into(),
                            ..Default::default()
                        })
                        .collect(),
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Numérote comme un vrai document : ids de phrase et index de mot
    /// globaux et croissants d'un bloc à l'autre.
    fn doc(mut blocks: Vec<BlockView>) -> DocView {
        let (mut sid, mut widx) = (0, 0);
        for block in &mut blocks {
            for sentence in &mut block.sentences {
                sentence.id = sid;
                sid += 1;
                for word in &mut sentence.words {
                    word.index = widx;
                    widx += 1;
                }
            }
        }
        DocView {
            blocks,
            ..Default::default()
        }
    }

    fn texts(layout: &Layout, d: &DocView, line: usize) -> Vec<String> {
        layout.lines[line]
            .words
            .iter()
            .map(|b| {
                let block = &d.blocks[layout.lines[line].block];
                let sentence = block.sentences.iter().find(|s| s.id == b.sentence).unwrap();
                assert_eq!(sentence.words[b.word].index, b.index);
                sentence.words[b.word].text.clone()
            })
            .collect()
    }

    #[test]
    fn wraps_at_width() {
        let d = doc(vec![para(&["un deux trois"])]);
        let l = lay_out(&d, 80.0, 0, &Mono);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(texts(&l, &d, 0), ["un", "deux"]);
        assert_eq!(texts(&l, &d, 1), ["trois"]);
        // L'espace se mesure entre les mots, pas en fin de ligne.
        assert_eq!(l.lines[0].words[1].x, 30.0);
        assert_eq!(l.lines[1].y, 20.0);
        assert_eq!(l.height, 40.0);
    }

    #[test]
    fn overlong_word_is_split_never_overflows() {
        let d = doc(vec![para(&[&"a".repeat(30)])]);
        let l = lay_out(&d, 100.0, 0, &Mono);
        assert_eq!(l.lines.len(), 3);
        for line in &l.lines {
            for b in &line.words {
                assert!(b.x + b.width <= 100.0, "{b:?}");
                assert_eq!((b.sentence, b.word), (0, 0));
            }
        }
        let total: f32 = l.lines.iter().flat_map(|l| &l.words).map(|b| b.width).sum();
        assert_eq!(total, 300.0);
    }

    #[test]
    fn a_word_after_a_split_continues_the_last_piece_line() {
        let d = doc(vec![para(&["aaaaaaaaaaaaaaa b"])]);
        let l = lay_out(&d, 100.0, 0, &Mono);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[1].words.len(), 2);
        assert_eq!(l.lines[1].words[1].x, 60.0);
    }

    #[test]
    fn width_too_small_for_one_char_neither_loops_nor_panics() {
        let d = doc(vec![para(&["abc de"])]);
        for w in [5.0, 0.0, -10.0, f32::NAN] {
            let l = lay_out(&d, w, 0, &Mono);
            assert_eq!(l.lines.len(), 5, "largeur {w}");
            assert!(l.lines.iter().all(|line| line.words.len() == 1));
        }
        struct Zero;
        impl Measure for Zero {
            fn width(&self, _: &str, _: &WordView, _: &BlockKindView) -> f32 {
                0.0
            }
            fn line_height(&self, _: &BlockKindView) -> f32 {
                0.0
            }
        }
        let l = lay_out(&d, 100.0, 0, &Zero);
        assert_eq!(l.lines.len(), 1);
    }

    #[test]
    fn neighbour_moves_by_displayed_line() {
        let d = doc(vec![para(&["un deux trois"])]);
        let l = lay_out(&d, 80.0, 0, &Mono);
        assert_eq!(l.neighbour(0, 1, 1), Some((0, 2)));
        assert_eq!(l.neighbour(0, 0, 1), Some((0, 2)));
        assert_eq!(l.neighbour(0, 2, 1), None);
        assert_eq!(l.neighbour(0, 2, -1), Some((0, 0)));
        assert_eq!(l.neighbour(0, 0, -1), None);
        assert_eq!(l.neighbour(0, 0, 0), None);
        assert_eq!(l.neighbour(9, 9, 1), None);
        assert_eq!(l.line_of(0, 2), Some(1));
    }

    #[test]
    fn neighbour_crosses_blocks() {
        let d = doc(vec![para(&["un deux"]), para(&["trois quatre"])]);
        let l = lay_out(&d, 200.0, 0, &Mono);
        assert_eq!(l.lines.len(), 2);
        // Ids globaux : la phrase du 2e bloc est la phrase 1, pas la 0.
        assert_eq!(l.lines[1].block, 1);
        assert_eq!(l.line_of(1, 0), Some(1));
        assert_eq!(l.line_of(0, 0), Some(0));
        assert_eq!(l.neighbour(0, 1, 1), Some((1, 0)));
        assert_eq!(l.neighbour(1, 1, -1), Some((0, 0)));
        assert_eq!(l.lines[1].words[1].index, 3);
    }

    #[test]
    fn neighbour_skips_verbatim_lines_and_split_continuations() {
        let code = BlockView {
            kind: BlockKindView::Code { language: None },
            display_lines: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let d = doc(vec![para(&["a", "un"]), code, para(&["deux"])]);
        let l = lay_out(&d, 200.0, 0, &Mono);
        assert_eq!(l.lines.len(), 4);
        assert_eq!(l.neighbour(1, 0, 1), Some((2, 0)));
        assert_eq!(l.lines[2].words.len(), 0);
        let long = doc(vec![para(&[&"a".repeat(25), "zzzzzzzz"])]);
        let l = lay_out(&long, 100.0, 0, &Mono);
        // Le mot 0 occupe les lignes 0 à 2 ; la ligne voisine est celle de « zzzzzzzz ».
        assert_eq!(l.neighbour(0, 0, 1), Some((1, 0)));
    }

    #[test]
    fn hit_finds_the_word_under_the_pointer() {
        let d = doc(vec![para(&["un deux trois"])]);
        let l = lay_out(&d, 80.0, 0, &Mono);
        assert_eq!(l.hit(5.0, 5.0), Some((0, 0)));
        assert_eq!(l.hit(35.0, 5.0), Some((0, 1)));
        assert_eq!(
            l.hit(25.0, 5.0),
            Some((0, 0)),
            "entre deux mots : celui de gauche"
        );
        assert_eq!(l.hit(10.0, 25.0), Some((0, 2)));
        // Hors du texte.
        assert_eq!(l.hit(10.0, 500.0), None);
        assert_eq!(l.hit(10.0, -1.0), None);
        assert_eq!(
            l.hit(75.0, 25.0),
            None,
            "à droite du dernier mot de la ligne"
        );
        assert_eq!(l.hit(69.0, 5.0), Some((0, 1)));
    }

    #[test]
    fn spacing_adds_blank_lines() {
        let d = doc(vec![para(&["un"])]);
        let plain = lay_out(&d, 100.0, 0, &Mono);
        let spaced = lay_out(&d, 100.0, 1, &Mono);
        assert_eq!(spaced.height, plain.height * 2.0);
        assert_eq!(spaced.lines[0].height, 40.0);
    }

    #[test]
    fn empty_document_has_no_lines() {
        let l = lay_out(&DocView::default(), 100.0, 2, &Mono);
        assert!(l.lines.is_empty());
        assert_eq!(l.height, 0.0);
        assert_eq!(l.hit(0.0, 0.0), None);
        assert_eq!(l.neighbour(0, 0, 1), None);
    }
}
