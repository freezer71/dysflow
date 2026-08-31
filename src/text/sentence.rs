//! Découpage en phrases, unité de synthèse du lecteur.
//!
//! Couper naïvement sur « . » casse les décimales, les abréviations et les
//! initiales : « Le prix est de 3.14 euros » devenait deux phrases, dont une
//! réduite à « 14 euros ». Ce module applique les exceptions usuelles du
//! français et de l'anglais, puis borne la longueur des phrases pour rester
//! sous la fenêtre de contexte des modèles TTS.

/// Abréviations qui ne terminent **jamais** une phrase.
///
/// Civilités et renvois : ce qui suit est toujours un nom propre ou une
/// référence, jamais une nouvelle phrase.
const TITLE_ABBREVIATIONS: &[&str] = &[
    "m", "mm", "mme", "mmes", "mlle", "mlles", "dr", "drs", "pr", "me", "mgr", "st", "ste",
    "mr", "mrs", "ms", "prof", "rev", "hon", "sr", "jr",
    "p", "pp", "art", "fig", "vol", "chap", "réf", "ref", "no", "nos", "num", "tél", "tel",
    "bd", "boul", "fbg",
];

/// Abréviations qui peuvent clore une phrase.
///
/// « etc. » en fin de phrase est courant ; « etc. et puis » ne l'est pas. On
/// exige donc une majuscule derrière — un chiffre ou une minuscule signalent
/// une continuation.
const GENERIC_ABBREVIATIONS: &[&str] = &[
    "etc", "al", "ibid", "cf", "ex", "approx", "vs", "op", "cit",
    "inc", "ltd", "co", "corp", "dept", "est", "ca", "apr", "av", "jc",
    "janv", "févr", "fevr", "avr", "juil", "sept", "oct", "nov", "déc", "dec",
    "min", "max", "sec", "env",
];

/// Longueur maximale d'une phrase avant redécoupage, en caractères.
///
/// Les modèles TTS dégradent (ou tronquent) au-delà de quelques centaines de
/// caractères ; couper nous-mêmes garantit aussi une latence régulière.
const MAX_SENTENCE_CHARS: usize = 320;

/// Découpe `text` en phrases prêtes pour la synthèse.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in split_on_terminators(text) {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !has_pronounceable_content(trimmed) {
            continue;
        }
        enforce_length(trimmed, &mut out);
    }
    out
}

/// Vrai si le fragment contient au moins une lettre ou un chiffre.
///
/// Écarte les phrases réduites à « . » ou « — » que produisait l'ancien
/// découpage, et qu'un moteur TTS lit comme un blanc ou un bruit.
fn has_pronounceable_content(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn split_on_terminators(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut sentences = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if !matches!(c, '.' | '!' | '?' | ';' | '…') {
            i += 1;
            continue;
        }

        // Absorbe une suite « ... » ou « !? » d'un seul tenant.
        let mut end = i + 1;
        while end < chars.len() && matches!(chars[end], '.' | '!' | '?' | '…') {
            end += 1;
        }
        let is_ellipsis = end - i >= 3 || c == '…';

        if c == '.' && !is_ellipsis && !terminates_sentence(&chars, i) {
            i = end;
            continue;
        }

        // Un point final est suivi d'une fin de texte, ou d'un blanc puis d'un
        // caractère qui n'ouvre pas une continuation en minuscule.
        if !followed_by_sentence_start(&chars, end) {
            i = end;
            continue;
        }

        sentences.push(chars[start..end].iter().collect::<String>());
        start = end;
        i = end;
    }

    if start < chars.len() {
        sentences.push(chars[start..].iter().collect::<String>());
    }
    sentences
}

/// Décide si le point en `dot` clôt réellement une phrase.
fn terminates_sentence(chars: &[char], dot: usize) -> bool {
    // Décimales et numéros de version : 3.14, 2.0.1
    let prev_is_digit = dot > 0 && chars[dot - 1].is_ascii_digit();
    let next_is_digit = chars.get(dot + 1).is_some_and(char::is_ascii_digit);
    if prev_is_digit && next_is_digit {
        return false;
    }

    // Mot précédant le point, en minuscules et sans ponctuation interne.
    let mut begin = dot;
    while begin > 0 {
        let c = chars[begin - 1];
        if c.is_alphanumeric() || c == '\'' || c == '-' || c == '.' {
            begin -= 1;
        } else {
            break;
        }
    }
    if begin == dot {
        return true;
    }
    let word: String = chars[begin..dot].iter().collect::<String>().to_lowercase();
    let word = word.trim_matches('.');

    // Initiale isolée : « J. Dupont », « J.-C. »
    let letters = word.trim_matches(|c: char| !c.is_alphanumeric());
    if letters.chars().count() == 1 && letters.chars().all(char::is_alphabetic) {
        return false;
    }

    if TITLE_ABBREVIATIONS.contains(&word) {
        return false;
    }
    if GENERIC_ABBREVIATIONS.contains(&word) {
        // Seule une majuscule ouvre franchement la phrase suivante.
        return next_significant(chars, dot + 1).is_some_and(char::is_uppercase);
    }
    true
}

/// Premier caractère non blanc à partir de `from`.
fn next_significant(chars: &[char], from: usize) -> Option<char> {
    chars[from.min(chars.len())..]
        .iter()
        .copied()
        .find(|c| !c.is_whitespace())
}

/// Vrai si ce qui suit la ponctuation ressemble à un début de phrase.
fn followed_by_sentence_start(chars: &[char], after: usize) -> bool {
    let mut i = after;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    match chars.get(i) {
        // Fin du fragment : la phrase est bien close.
        None => true,
        // Une minuscule signale une continuation (« etc. et puis »).
        Some(c) => !c.is_lowercase(),
    }
}

/// Redécoupe les phrases trop longues, en préférant les frontières naturelles.
fn enforce_length(sentence: &str, out: &mut Vec<String>) {
    if sentence.chars().count() <= MAX_SENTENCE_CHARS {
        out.push(sentence.to_string());
        return;
    }

    let chars: Vec<char> = sentence.chars().collect();
    let mut start = 0usize;
    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= MAX_SENTENCE_CHARS {
            let tail: String = chars[start..].iter().collect();
            let tail = tail.trim();
            if !tail.is_empty() {
                out.push(tail.to_string());
            }
            break;
        }

        let limit = start + MAX_SENTENCE_CHARS;
        // Une virgule ou un tiret respire mieux qu'une coupe au milieu d'un mot.
        let cut = last_index_of(&chars, start, limit, &[',', ';', ':', '—', '–'])
            .or_else(|| last_index_of(&chars, start, limit, &[' ']))
            .map(|i| i + 1)
            .unwrap_or(limit);

        let chunk: String = chars[start..cut].iter().collect();
        let chunk = chunk.trim();
        if !chunk.is_empty() {
            out.push(chunk.to_string());
        }
        start = cut;
    }
}

fn last_index_of(chars: &[char], start: usize, end: usize, targets: &[char]) -> Option<usize> {
    (start..end.min(chars.len()))
        .rev()
        .find(|&i| targets.contains(&chars[i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_decimals_and_versions_intact() {
        let s = split_sentences("Le prix est de 3.14 euros. La version 2.0.1 est sortie.");
        assert_eq!(s, vec!["Le prix est de 3.14 euros.", "La version 2.0.1 est sortie."]);
    }

    #[test]
    fn keeps_abbreviations_intact() {
        let s = split_sentences("M. Dupont est arrivé. Mme Durand aussi.");
        assert_eq!(s, vec!["M. Dupont est arrivé.", "Mme Durand aussi."]);
    }

    #[test]
    fn keeps_etc_glued_to_its_sentence() {
        let s = split_sentences("Des pommes, des poires, etc. Puis on rentre.");
        assert_eq!(s, vec!["Des pommes, des poires, etc.", "Puis on rentre."]);
    }

    #[test]
    fn keeps_initials_intact() {
        let s = split_sentences("J. R. R. Tolkien a écrit ce livre.");
        assert_eq!(s, vec!["J. R. R. Tolkien a écrit ce livre."]);
    }

    #[test]
    fn treats_ellipsis_as_one_terminator() {
        let s = split_sentences("Il hésitait... Puis il partit.");
        assert_eq!(s, vec!["Il hésitait...", "Puis il partit."]);
        // Aucune phrase réduite à de la ponctuation.
        assert!(s.iter().all(|x| x.chars().any(char::is_alphanumeric)));
    }

    #[test]
    fn splits_on_question_and_exclamation() {
        let s = split_sentences("Vraiment ? Oui ! Bien sûr.");
        assert_eq!(s, vec!["Vraiment ?", "Oui !", "Bien sûr."]);
    }

    #[test]
    fn drops_punctuation_only_fragments() {
        assert!(split_sentences("... ; . !").is_empty());
    }

    #[test]
    fn splits_overlong_sentences_at_commas() {
        let long = format!("{} et voilà.", "un segment de texte, ".repeat(40));
        let s = split_sentences(&long);
        assert!(s.len() > 1);
        assert!(s.iter().all(|x| x.chars().count() <= MAX_SENTENCE_CHARS));
    }

    #[test]
    fn handles_utf8_without_panicking() {
        let s = split_sentences("Élan… Œuvre ! Ça va ? Très bien.");
        assert_eq!(s.len(), 4);
    }
}
