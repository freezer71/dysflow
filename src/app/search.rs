//! Recherche dans le document.
//!
//! Dans un document long, retrouver un passage en le faisant défiler coûte
//! l'attention qu'on voulait garder pour le lire. La recherche ignore la casse
//! et les accents, et répond aux deux formes du document : celle qui est à
//! l'écran — chiffres écrits, source des blocs de code — et celle qui est dite.
//! On retrouve donc « 2027 » aussi bien en le tapant qu'en tapant « deux mille
//! vingt-sept », selon qu'on l'a lu ou entendu.

use crate::doc::{Document, Word};

/// Une recherche et son curseur dans les résultats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    pub query: String,
    /// Phrases où la requête apparaît, dans l'ordre du document.
    matches: Vec<usize>,
    current: usize,
    /// Requête réduite, comparée telle quelle à chaque mot rendu.
    needle: String,
}

impl Search {
    /// Cherche `query` dans le document, à partir de la phrase `from`.
    ///
    /// Le premier résultat retenu est celui qui suit le curseur de lecture :
    /// on cherche presque toujours vers l'avant.
    pub fn new(doc: &Document, query: &str, from: usize) -> Self {
        let needle = fold(query.trim());
        let mut search = Self {
            query: query.trim().to_string(),
            matches: Vec::new(),
            current: 0,
            needle,
        };
        if search.needle.is_empty() {
            return search;
        }

        for sentence in &doc.sentences {
            let seen = fold(&sentence.written()).contains(&search.needle);
            // La forme parlée n'est consultée qu'à défaut : elle coûte une
            // seconde réduction, et l'écran reste la référence.
            if seen || fold(&sentence.text).contains(&search.needle) {
                search.matches.push(sentence.id);
            }
        }
        // Un bloc de code n'est pas dit, mais il est lu : son source doit
        // répondre à la recherche comme le reste de l'écran.
        for block in &doc.blocks {
            if block
                .display_lines
                .iter()
                .any(|line| fold(line).contains(&search.needle))
            {
                search.matches.push(block.sentences.0);
            }
        }
        search.matches.sort_unstable();
        search.matches.dedup();
        search.current = search
            .matches
            .iter()
            .position(|&id| id >= from)
            .unwrap_or(0);
        search
    }

    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }

    /// Rang du résultat courant, à partir de un.
    pub fn position(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current + 1
        }
    }

    /// Phrase du résultat courant.
    pub fn target(&self) -> Option<usize> {
        self.matches.get(self.current).copied()
    }

    /// Passe au résultat suivant, en bouclant.
    pub fn advance(&mut self, delta: isize) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let count = self.matches.len() as isize;
        self.current = (self.current as isize + delta).rem_euclid(count) as usize;
        self.target()
    }

    /// Vrai si la phrase porte un résultat : un mot ne répond que dans une
    /// phrase qui répond elle-même.
    pub fn contains_sentence(&self, id: usize) -> bool {
        self.matches.binary_search(&id).is_ok()
    }

    /// Vrai si le mot fait partie de ce qui est recherché.
    ///
    /// Un nombre répond aussi à sa forme parlée : cherché comme on l'a
    /// entendu, il doit se souligner là où on le voit.
    pub fn matches_word(&self, word: &Word) -> bool {
        if self.needle.is_empty() {
            return false;
        }
        fold(&word.text).contains(&self.needle)
            || word
                .spoken
                .as_deref()
                .is_some_and(|said| fold(said).contains(&self.needle))
    }
}

/// Réduit un texte à une forme comparable : minuscules, sans accents.
///
/// Chercher « specification » doit trouver « Spécification » — pour qui écrit
/// avec effort, l'accent oublié est la règle, pas l'exception.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(deaccent)
        .collect()
}

/// Rend sa lettre de base à un caractère accentué.
fn deaccent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::builder::{parse_document, CodePolicy};

    const DOC: &str = "\
# Spécification

Le service expose deux routes.

La route des utilisateurs répond en JSON.

```rust
fn authenticate() {}
```
";

    fn doc() -> Document {
        parse_document(DOC, CodePolicy::Announce)
    }

    #[test]
    fn folding_ignores_case_and_accents() {
        assert_eq!(fold("Spécification"), "specification");
        assert_eq!(fold("À CÔTÉ"), "a cote");
        assert_eq!(fold("Straße"), "straße");
    }

    #[test]
    fn a_search_finds_every_occurrence_in_reading_order() {
        let doc = doc();
        let search = Search::new(&doc, "route", 0);
        assert_eq!(search.count(), 2);
        assert_eq!(search.position(), 1);
    }

    #[test]
    fn a_search_ignores_the_missing_accent() {
        let doc = doc();
        assert_eq!(Search::new(&doc, "specification", 0).count(), 1);
    }

    #[test]
    fn a_search_finds_the_figures_that_are_on_screen() {
        // On cherche ce qu'on voit : « 2027 » est à l'écran, même si la voix
        // dit « deux mille vingt-sept ».
        let doc = parse_document("La règle s'applique à partir de 2027.\n", CodePolicy::Announce);
        assert_eq!(Search::new(&doc, "2027", 0).count(), 1);
    }

    #[test]
    fn a_search_also_answers_to_what_was_heard() {
        // On tape aussi bien ce qu'on a entendu que ce qu'on a sous les yeux :
        // la phrase montre « 2027 » et dit « deux mille vingt-sept ».
        let doc = parse_document("La règle s'applique à partir de 2027.\n", CodePolicy::Announce);
        assert_eq!(Search::new(&doc, "deux mille vingt-sept", 0).count(), 1);
    }

    #[test]
    fn the_underline_reaches_the_figure_that_was_heard() {
        let doc = parse_document("La règle s'applique à partir de 2027.\n", CodePolicy::Announce);
        let search = Search::new(&doc, "deux mille", 0);
        let word = doc.sentences[0]
            .words
            .iter()
            .find(|w| w.text.starts_with("2027"))
            .expect("le nombre est à l'écran");
        assert!(search.matches_word(word), "le résultat trouvé doit se voir");
    }

    #[test]
    fn a_search_reaches_the_source_of_a_code_block() {
        let doc = doc();
        let search = Search::new(&doc, "authenticate", 0);
        assert_eq!(search.count(), 1, "le code affiché doit être cherchable");
        let target = search.target().expect("un résultat");
        let block = doc.sentences[target].block_id;
        assert!(matches!(
            doc.blocks[block].kind,
            crate::doc::BlockKind::Code { .. }
        ));
    }

    #[test]
    fn the_first_result_is_the_one_ahead_of_the_cursor() {
        let doc = doc();
        let all = Search::new(&doc, "route", 0);
        let first = all.target().expect("un résultat");
        let later = Search::new(&doc, "route", first + 1);
        assert!(later.target().unwrap() > first, "la recherche part du curseur");
        // Les résultats précédents restent atteignables en bouclant.
        assert_eq!(later.count(), 2);
    }

    #[test]
    fn results_wrap_around_in_both_directions() {
        let doc = doc();
        let mut search = Search::new(&doc, "route", 0);
        let first = search.target().unwrap();
        search.advance(1);
        assert_ne!(search.target().unwrap(), first);
        search.advance(1);
        assert_eq!(search.target().unwrap(), first, "la liste boucle");
        search.advance(-1);
        assert_ne!(search.target().unwrap(), first);
    }

    #[test]
    fn an_unknown_word_yields_nothing_rather_than_a_wrong_jump() {
        let doc = doc();
        let mut search = Search::new(&doc, "kubernetes", 0);
        assert!(search.is_empty());
        assert_eq!(search.target(), None);
        assert_eq!(search.advance(1), None);
        assert_eq!(search.position(), 0);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        let doc = doc();
        assert!(Search::new(&doc, "   ", 0).is_empty());
    }

    fn word(text: &str) -> Word {
        Word {
            text: text.to_string(),
            index: 0,
            punctuation: None,
            spoken: None,
            emphasis: Default::default(),
        }
    }

    #[test]
    fn a_word_is_highlighted_when_it_carries_the_query() {
        let doc = doc();
        let search = Search::new(&doc, "route", 0);
        assert!(search.matches_word(&word("routes.")));
        assert!(search.matches_word(&word("Route")));
        assert!(!search.matches_word(&word("service")));
    }
}
