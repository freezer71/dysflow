//! Les jetons de Parakeet en texte, comme `ParakeetTokenizer.swift`.

/// `<|…|>`, `<unk>` et `<pad>` : jamais dans le texte.
pub fn is_special(token: usize, vocabulary: &[String]) -> bool {
    vocabulary.get(token).is_some_and(|piece| {
        (piece.starts_with("<|") && piece.ends_with("|>")) || piece == "<unk>" || piece == "<pad>"
    })
}

/// Les morceaux à la suite, `▁` en espace ; hors vocabulaire et spéciaux
/// omis.
pub fn decode(tokens: &[usize], vocabulary: &[String]) -> String {
    tokens
        .iter()
        .filter(|&&t| t < vocabulary.len() && !is_special(t, vocabulary))
        .map(|&t| vocabulary[t].replace('▁', " "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morceaux_et_speciaux() {
        let vocabulary: Vec<String> = ["<unk>", "▁Bon", "jour", "<|fr|>", "<pad>", "."]
            .map(String::from)
            .to_vec();
        assert_eq!(decode(&[1, 2, 3, 0, 4, 5, 99], &vocabulary), " Bonjour.");
        assert!(
            is_special(3, &vocabulary)
                && is_special(0, &vocabulary)
                && !is_special(99, &vocabulary)
        );
    }
}
