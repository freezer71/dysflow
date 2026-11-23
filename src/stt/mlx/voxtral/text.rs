//! Le texte d'un flux de Voxtral, recollé des octets de chaque jeton au fil
//! du décodage (`VoxtralRealtimeTranscriptText.swift`) : le coût d'un pas
//! ne grandit pas avec le texte.
//!
//! `text` vaut toujours le décodage UTF-8 de tous les octets reçus,
//! caractères de remplacement compris (`String(decoding:as:)` et
//! `String::from_utf8_lossy` remplacent les mêmes sous-suites fautives) :
//! les octets ne passent dans `settled` qu'une fois leur caractère fini.
//!
//! `delta` suit le Swift, qui compare des `String` : par caractères
//! (graphèmes étendus) et à équivalence canonique (`hasPrefix`), puis en
//! retire autant de caractères (`dropFirst`). Une marque combinante qui
//! s'accroche au dernier caractère change ce caractère : le texte d'avant
//! n'est plus un début, et `delta` rend tout le texte, comme le Swift.

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Default, Clone)]
pub struct Transcript {
    /// Le texte qu'aucun octet à venir ne peut changer.
    settled: String,
    /// Le début d'une suite UTF-8 dont la fin n'est pas encore arrivée.
    unsettled: Vec<u8>,
}

/// Une position du texte, d'où mesurer le morceau suivant.
#[derive(Debug, Clone)]
pub struct Mark {
    settled_len: usize,
    last_settled: String,
    unsettled_text: String,
}

impl Transcript {
    /// Tout le texte : il finit par U+FFFD tant qu'un caractère est
    /// incomplet.
    pub fn text(&self) -> String {
        if self.unsettled.is_empty() {
            self.settled.clone()
        } else {
            format!("{}{}", self.settled, self.unsettled_text())
        }
    }

    fn unsettled_text(&self) -> String {
        String::from_utf8_lossy(&self.unsettled).into_owned()
    }

    pub fn append(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut pending = std::mem::take(&mut self.unsettled);
        pending.extend_from_slice(bytes);
        let cut = unsettled_suffix_start(&pending);
        self.settled
            .push_str(&String::from_utf8_lossy(&pending[..cut]));
        self.unsettled = pending[cut..].to_vec();
    }

    pub fn mark(&self) -> Mark {
        Mark {
            settled_len: self.settled.len(),
            last_settled: self
                .settled
                .graphemes(true)
                .next_back()
                .unwrap_or("")
                .to_string(),
            unsettled_text: self.unsettled_text(),
        }
    }

    /// Le texte ajouté depuis `mark`, ou tout le texte si celui de `mark`
    /// n'en est plus un début (un U+FFFD final devenu un vrai caractère, ou
    /// un texte nouveau qui s'accroche au dernier caractère). Seuls ce
    /// dernier caractère et ce qui suit peuvent changer : seule cette queue
    /// est comparée.
    pub fn delta(&self, mark: &Mark) -> String {
        // `settled` ne fait que grandir, sur des fins de caractère : l'ancienne
        // longueur est une frontière.
        let newly = self.settled.get(mark.settled_len..).unwrap_or("");
        let old_tail = format!("{}{}", mark.last_settled, mark.unsettled_text);
        let new_tail = format!("{}{}{}", mark.last_settled, newly, self.unsettled_text());
        match strip_character_prefix(&new_tail, &old_tail) {
            Some(rest) => rest.to_string(),
            None => self.text(),
        }
    }
}

/// `String.hasPrefix` puis `dropFirst(prefix.count)` du Swift : `prefix`
/// est-il, caractère par caractère et à équivalence canonique, le début de
/// `text` ? Si oui, la suite de `text` après autant de caractères.
pub fn strip_character_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut chars = text.grapheme_indices(true);
    for expected in prefix.graphemes(true) {
        let (_, found) = chars.next()?;
        if !found.nfd().eq(expected.nfd()) {
            return None;
        }
    }
    Some(match chars.next() {
        Some((at, _)) => &text[at..],
        None => "",
    })
}

/// Où commence la suite UTF-8 inachevée à la fin de `bytes`, ou
/// `bytes.len()` s'il n'y en a pas. Un octet de tête commence toujours un
/// nouveau caractère : un décodeur en une passe a fini tout ce qui précède.
pub fn unsettled_suffix_start(bytes: &[u8]) -> usize {
    let mut lead = bytes.len();
    let mut continuations = 0;
    while lead > 0 && continuations < 3 && bytes[lead - 1] & 0xC0 == 0x80 {
        lead -= 1;
        continuations += 1;
    }
    if lead == 0 {
        return bytes.len();
    }
    let expected = match bytes[lead - 1] {
        0xC2..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF4 => 3,
        // ASCII, une continuation isolée, ou jamais valide.
        _ => return bytes.len(),
    };
    if continuations < expected {
        lead - 1
    } else {
        bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appended(parts: &[&[u8]]) -> Transcript {
        let mut t = Transcript::default();
        for part in parts {
            t.append(part);
        }
        t
    }

    #[test]
    fn an_unfinished_character_is_held_back_then_settled() {
        let mut t = appended(&[b"caf", &[0xC3]]);
        assert_eq!(t.text(), "caf\u{FFFD}");
        let mark = t.mark();
        t.append(&[0xA9]);
        assert_eq!(t.text(), "café");
        // L'ancien texte finissait par U+FFFD : plus un début, tout le texte.
        assert_eq!(t.delta(&mark), "café");
        let mark = t.mark();
        t.append(b" noir");
        assert_eq!(t.delta(&mark), " noir");
    }

    /// `text` vaut toujours le décodage de tous les octets, fautes comprises.
    #[test]
    fn the_text_is_always_the_lossy_decoding_of_every_byte() {
        let parts: [&[u8]; 6] = [b"a", &[0xF0, 0x9F], &[0x98], &[0x80, 0xFF], &[0xE2], b"z"];
        let mut t = Transcript::default();
        let mut all = Vec::new();
        for part in parts {
            t.append(part);
            all.extend_from_slice(part);
            assert_eq!(t.text(), String::from_utf8_lossy(&all));
        }
    }

    #[test]
    fn the_unsettled_suffix_starts_at_an_unfinished_lead_byte() {
        assert_eq!(unsettled_suffix_start(b"abc"), 3);
        assert_eq!(unsettled_suffix_start(&[b'a', 0xC3]), 1);
        assert_eq!(unsettled_suffix_start(&[b'a', 0xC3, 0xA9]), 3);
        assert_eq!(unsettled_suffix_start(&[0xE2, 0x82]), 0);
        assert_eq!(unsettled_suffix_start(&[0xF0, 0x9F, 0x98]), 0);
        assert_eq!(
            unsettled_suffix_start(&[0x80, 0x80]),
            2,
            "continuations seules"
        );
        assert_eq!(unsettled_suffix_start(&[0xC0]), 1, "jamais valide");
        assert_eq!(unsettled_suffix_start(&[]), 0);
    }

    /// Le Swift compare par caractères : un accent combinant qui s'accroche
    /// au dernier caractère le change, et tout le texte est rendu ; un
    /// caractère précomposé et sa forme décomposée sont égaux.
    #[test]
    fn the_prefix_is_compared_by_characters_like_the_swift() {
        let mut t = appended(&[b"cafe"]);
        let mark = t.mark();
        t.append("\u{301} noir".as_bytes());
        assert_eq!(t.delta(&mark), "cafe\u{301} noir", "l'accent change le e");
        assert_eq!(strip_character_prefix("e\u{301}x", "\u{E9}"), Some("x"));
        assert_eq!(strip_character_prefix("ex", "e"), Some("x"));
        assert_eq!(strip_character_prefix("e\u{301}x", "e"), None);
        assert_eq!(strip_character_prefix("ab", "abc"), None);
        assert_eq!(strip_character_prefix("ab", ""), Some("ab"));
        assert_eq!(strip_character_prefix("ab", "ab"), Some(""));
    }

    #[test]
    fn deltas_put_end_to_end_give_the_text_when_it_only_grows() {
        let mut t = Transcript::default();
        let mut shown = String::new();
        for part in ["Bon", "jour", " à", " tous", "."] {
            let mark = t.mark();
            t.append(part.as_bytes());
            shown.push_str(&t.delta(&mark));
        }
        assert_eq!(shown, t.text());
        assert_eq!(shown, "Bonjour à tous.");
    }
}
