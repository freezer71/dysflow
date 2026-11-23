//! Le taux d'erreur de mots (WER) : ce qu'une dictée a entendu, comparé au
//! texte lu.

/// Le taux d'erreur de mots de `heard` contre `reference` : substitutions,
/// suppressions et insertions (distance d'édition sur les mots), rapportées au
/// nombre de mots de la référence. Casse et ponctuation ignorées.
///
/// Référence vide : 0 si rien n'est entendu, 1 sinon (tout est inventé).
pub fn wer(reference: &str, heard: &str) -> f64 {
    let reference = words(reference);
    let heard = words(heard);
    if reference.is_empty() {
        return if heard.is_empty() { 0.0 } else { 1.0 };
    }
    // Distance d'édition, une ligne à la fois.
    let mut row: Vec<usize> = (0..=heard.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, h) in heard.iter().enumerate() {
            let replaced = diagonal + usize::from(r != h);
            diagonal = row[j + 1];
            row[j + 1] = replaced.min(row[j + 1] + 1).min(row[j] + 1);
        }
    }
    row[heard.len()] as f64 / reference.len() as f64
}

/// Les mots d'un texte, en minuscules, sans ponctuation. Une élision reste un
/// mot à part (« l'amazone » : « l' », « amazone »), l'apostrophe
/// typographique comprise ; un trait d'union sépare (« dit-il »).
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            word.push(c);
        } else if (c == '\'' || c == '’') && !word.is_empty() {
            word.push('\'');
            out.push(std::mem::take(&mut word));
        } else if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_and_punctuation_are_ignored() {
        assert_eq!(wer("Le chat dort.", "le chat, dort"), 0.0);
        assert_eq!(wer("« Bonjour ! » dit-il", "bonjour dit il"), 0.0);
        assert_eq!(wer("L’Amazone", "l'amazone"), 0.0);
    }

    #[test]
    fn one_word_in_three_replaced_is_a_third() {
        assert!((wer("le chat dort", "le chien dort") - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn an_empty_reference_and_an_empty_text_are_no_error() {
        assert_eq!(wer("", ""), 0.0);
        assert_eq!(wer(" . ", ""), 0.0);
    }

    #[test]
    fn missing_and_extra_words_count() {
        assert_eq!(wer("oui", ""), 1.0);
        assert_eq!(wer("oui", "oui oui"), 1.0);
        assert_eq!(wer("un deux trois quatre", "un trois quatre"), 0.25);
        assert_eq!(wer("", "bruit"), 1.0, "rien à entendre, un mot inventé");
    }
}
