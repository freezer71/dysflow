//! Le texte des jetons de Nemotron : les morceaux SentencePiece du
//! vocabulaire de `config.json`, `▁` en espace, jetons spéciaux retirés.
//! Port de `NemotronASRTokenizer.swift` (sans `detectedLanguage`, que le
//! flux ne lit pas). Et `tidy`, la réduction des espaces de
//! `NemotronSpeech.tidy` (l'app).

/// `<xx-YY>` : une étiquette de langue.
pub fn is_language_tag(piece: &str) -> bool {
    piece.starts_with('<') && piece.ends_with('>') && piece.contains('-')
}

/// Une étiquette de langue, `<|…|>`, `<unk>` ou `<pad>` ; un id hors du
/// vocabulaire n'est pas spécial.
pub fn is_special(token: usize, vocabulary: &[String]) -> bool {
    let Some(piece) = vocabulary.get(token) else {
        return false;
    };
    is_language_tag(piece)
        || (piece.starts_with("<|") && piece.ends_with("|>"))
        || piece == "<unk>"
        || piece == "<pad>"
}

/// Les morceaux de `tokens` à la suite, `▁` en espace ; les ids hors du
/// vocabulaire et les jetons spéciaux sont sautés.
pub fn decode(tokens: &[usize], vocabulary: &[String]) -> String {
    tokens
        .iter()
        .filter(|&&t| !is_special(t, vocabulary))
        .filter_map(|&t| vocabulary.get(t))
        .map(|piece| piece.replace('▁', " "))
        .collect()
}

/// `NemotronSpeech.tidy` : chaque suite de deux espaces ou plus devient une
/// seule (Nemotron en met deux entre deux phrases). Un préfixe reste un
/// préfixe : le texte réduit ne fait que s'allonger, lui aussi.
pub fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_space = false;
    for c in text.chars() {
        if c == ' ' && after_space {
            continue;
        }
        after_space = c == ' ';
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocabulary() -> Vec<String> {
        [
            "<unk>",
            "▁bon",
            "jour",
            "<en-US>",
            "<|nospeech|>",
            "<pad>",
            "▁<x>",
            "<fr>",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn special_tokens_are_tags_unk_and_pad() {
        let v = vocabulary();
        let special: Vec<usize> = (0..v.len() + 2).filter(|&t| is_special(t, &v)).collect();
        // `<fr>` sans tiret n'est pas une étiquette de langue ; hors du
        // vocabulaire, rien n'est spécial.
        assert_eq!(special, [0, 3, 4, 5]);
    }

    #[test]
    fn decoding_turns_the_marker_into_a_space_and_skips_the_rest() {
        let v = vocabulary();
        assert_eq!(decode(&[1, 2, 3, 4, 99, 6, 7], &v), " bonjour <x><fr>");
    }

    #[test]
    fn tidy_collapses_runs_of_spaces_and_keeps_prefixes() {
        assert_eq!(tidy("Bonjour.  Ça va ?   Oui"), "Bonjour. Ça va ? Oui");
        assert_eq!(tidy(" a"), " a");
        assert_eq!(tidy("a\t\tb"), "a\t\tb", "seulement les espaces");
        let (short, long) = ("Fin.  ", "Fin.  Début");
        assert!(tidy(long).starts_with(&tidy(short)));
    }
}
