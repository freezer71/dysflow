//! Mise en runs d'un bloc : ce que chaque jeton du fichier porte à l'écran.
//!
//! Le pendant de `tui::ui::document_lines`, sans type de bibliothèque : la
//! fenêtre traduit ces runs en sections de texte, et c'est ici que se vérifie
//! qui porte le fond, la couleur, le soulignement.
//!
//! La composition vient du document — mêmes lignes, mêmes écarts ; le style
//! vient des mots, consommés dans l'ordre. Un jeton de plus que de mots garde
//! le ton du bloc.

use crate::app::search::Search;
use crate::doc::{Block, Document, Emphasis};

/// Couleur d'un jeton, par proximité avec la voix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Le mot prononcé : fond accent, texte sombre. Le seul à porter un fond.
    Cursor,
    /// La phrase en cours : la plus claire.
    Sentence,
    /// Le bloc lu, hors de la phrase en cours.
    Block,
    /// Le reste du document, atténué sans disparaître.
    Faded,
}

/// Ce qu'un jeton porte, hors de son texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub tone: Tone,
    /// Ce que le fichier dit : gras, italique, barré, lien, code.
    pub emphasis: Emphasis,
    /// Résultat de la recherche en cours.
    pub hit: bool,
}

/// Un jeton du fichier, prêt à être composé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    /// Espaces qui le précèdent sur sa ligne, tels que le fichier les écrit.
    pub gap: usize,
    /// Vrai s'il ouvre une ligne du fichier.
    pub newline: bool,
    pub mark: Mark,
    /// Phrase dont il fait partie, pour qu'un clic y envoie la voix.
    pub sentence: Option<usize>,
}

/// Où en est la voix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub sentence: usize,
    pub word: Option<usize>,
}

/// Les jetons d'un bloc de prose, dans l'ordre du fichier.
pub fn runs(doc: &Document, block: &Block, cursor: Cursor, search: Option<&Search>) -> Vec<Run> {
    let current_block = doc.sentence(cursor.sentence).map(|s| s.block_id) == Some(block.id);

    let mut words = Vec::new();
    for id in block.sentences.0..block.sentences.1 {
        let Some(sentence) = doc.sentence(id) else { continue };
        let in_sentence = id == cursor.sentence;
        for (i, word) in sentence.words.iter().enumerate() {
            let is_cursor = in_sentence && cursor.word == Some(i);
            let tone = if is_cursor {
                Tone::Cursor
            } else if in_sentence {
                Tone::Sentence
            } else if current_block {
                Tone::Block
            } else {
                Tone::Faded
            };
            // Un résultat se souligne — le fond plein reste au mot prononcé,
            // qui ne doit jamais se confondre.
            let hit = !is_cursor && search.is_some_and(|s| s.matches_word(word));
            words.push((Mark { tone, emphasis: word.emphasis, hit }, id));
        }
    }
    let base = Mark {
        tone: if current_block { Tone::Block } else { Tone::Faded },
        emphasis: Emphasis::default(),
        hit: false,
    };

    let source: Vec<&str> = if block.display_lines.is_empty() {
        vec![block.text.as_str()]
    } else {
        block.display_lines.iter().map(String::as_str).collect()
    };

    let mut out = Vec::new();
    let mut next = words.into_iter();
    for line in source {
        let mut opens = true;
        for (gap, text) in cells(line) {
            let (mark, sentence) = match next.next() {
                Some((mark, id)) => (mark, Some(id)),
                None => (base, None),
            };
            out.push(Run { text, gap, newline: opens, mark, sentence });
            opens = false;
        }
        // Une ligne vide du fichier reste vide à l'écran : elle en compose la
        // respiration.
        if opens {
            out.push(Run { text: String::new(), gap: 0, newline: true, mark: base, sentence: None });
        }
    }
    out
}

/// Découpe une ligne en jetons, chacun précédé de son écart.
///
/// L'écart est compté plutôt qu'écrasé : c'est lui qui aligne une colonne.
pub fn cells(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut gap = 0usize;
    let mut word = String::new();
    for c in source.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                out.push((gap, std::mem::take(&mut word)));
                gap = 0;
            }
            gap += 1;
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push((gap, word));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::builder::{parse_document, CodePolicy};

    const DOC: &str = "Première phrase ici. Encore une.\n\nDeuxième bloc là.\n";

    fn doc() -> Document {
        parse_document(DOC, CodePolicy::Announce)
    }

    fn at(sentence: usize, word: Option<usize>) -> Cursor {
        Cursor { sentence, word }
    }

    #[test]
    fn the_spoken_word_is_the_only_one_with_the_cursor_tone() {
        let doc = doc();
        let runs = runs(&doc, &doc.blocks[0], at(0, Some(1)), None);
        let cursors: Vec<&Run> = runs.iter().filter(|r| r.mark.tone == Tone::Cursor).collect();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].text, "phrase");
        assert!(runs
            .iter()
            .filter(|r| r.sentence == Some(0) && r.mark.tone != Tone::Cursor)
            .all(|r| r.mark.tone == Tone::Sentence));
    }

    #[test]
    fn the_rest_of_the_block_and_of_the_document_step_back() {
        let doc = doc();
        let first = runs(&doc, &doc.blocks[0], at(0, Some(0)), None);
        // « Encore une. » est dans le bloc lu, mais pas dans la phrase lue.
        assert!(first
            .iter()
            .filter(|r| r.sentence == Some(1))
            .all(|r| r.mark.tone == Tone::Block));
        let second = runs(&doc, &doc.blocks[1], at(0, Some(0)), None);
        assert!(second.iter().all(|r| r.mark.tone == Tone::Faded));
    }

    #[test]
    fn a_search_hit_is_marked_except_on_the_spoken_word() {
        let doc = doc();
        let search = crate::app::search::Search::new(&doc, "phrase", 0);
        let runs_spoken = runs(&doc, &doc.blocks[0], at(0, Some(1)), Some(&search));
        let spoken = runs_spoken.iter().find(|r| r.text == "phrase").unwrap();
        assert!(!spoken.mark.hit, "le fond plein suffit au mot prononcé");
        let other = runs(&doc, &doc.blocks[0], at(0, Some(0)), Some(&search));
        assert!(other.iter().find(|r| r.text == "phrase").unwrap().mark.hit);
    }

    #[test]
    fn tokens_follow_the_lines_and_the_gaps_of_the_file() {
        let doc = parse_document("un   deux\ntrois quatre\n", CodePolicy::Announce);
        let block = &doc.blocks[0];
        assert_eq!(block.display_lines.len(), 2, "le document garde ses lignes");
        let runs = runs(&doc, block, at(0, None), None);
        let texts: Vec<(&str, usize, bool)> =
            runs.iter().map(|r| (r.text.as_str(), r.gap, r.newline)).collect();
        assert_eq!(
            texts,
            vec![("un", 0, true), ("deux", 3, false), ("trois", 0, true), ("quatre", 1, false)]
        );
    }

    #[test]
    fn every_token_knows_its_sentence() {
        let doc = doc();
        let runs = runs(&doc, &doc.blocks[0], at(5, None), None);
        assert!(runs.iter().all(|r| r.sentence.is_some()));
        assert_eq!(runs.last().unwrap().sentence, Some(1));
    }

    #[test]
    fn cells_count_the_gap_before_each_word() {
        assert_eq!(
            cells("  a   b c"),
            vec![(2, "a".to_string()), (3, "b".to_string()), (1, "c".to_string())]
        );
        assert!(cells("   ").is_empty());
    }
}
