//! Le vocabulaire Tekken de Voxtral (`tekken.json`), comme
//! `VoxtralRealtimeTokenizer.swift` : chaque jeton est une suite d'octets
//! en base64, décalée des `default_num_special_tokens` premiers numéros ;
//! un jeton spécial, ou hors du vocabulaire, n'écrit rien. Seuls les octets
//! d'un jeton servent au flux (`tokenBytes`), le texte se recolle ailleurs
//! (`text::Transcript`).

use anyhow::{ensure, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

pub const TEKKEN_FILE: &str = "tekken.json";
/// `tekken.json` plus gros que cela : refusé sans être lu (celui du
/// catalogue fait 15 Mo).
const MAX_TEKKEN_BYTES: u64 = 64 << 20;
/// `defaultNumSpecialTokens` absent : 1 000, comme le Swift.
const DEFAULT_SPECIAL: usize = 1000;

#[derive(Deserialize)]
struct File {
    vocab: Vec<Entry>,
    config: Option<FileConfig>,
    special_tokens: Option<Vec<Special>>,
}

#[derive(Deserialize)]
struct Entry {
    token_bytes: String,
}

#[derive(Deserialize)]
struct FileConfig {
    default_num_special_tokens: Option<i64>,
}

#[derive(Deserialize)]
struct Special {
    rank: Option<i64>,
}

pub struct Tokenizer {
    /// Les octets de chaque entrée du vocabulaire (vides si la base64 est
    /// fausse, comme `Data(base64Encoded:) ?? Data()`).
    vocab: Vec<Vec<u8>>,
    n_special: usize,
    special: HashSet<i64>,
}

fn unreadable(path: &Path, err: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(crate::tr!(
        en = "unreadable Voxtral vocabulary ({}): {err}",
        fr = "vocabulaire de Voxtral illisible ({}) : {err}",
        path.display(),
    ))
}

impl Tokenizer {
    /// `fromModelDirectory` : `tekken.json` du dossier, borné en taille.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(TEKKEN_FILE);
        let file = std::fs::File::open(&path).map_err(|err| unreadable(&path, &err))?;
        let mut text = Vec::new();
        file.take(MAX_TEKKEN_BYTES + 1)
            .read_to_end(&mut text)
            .map_err(|err| unreadable(&path, &err))?;
        ensure!(
            text.len() as u64 <= MAX_TEKKEN_BYTES,
            crate::tr!(
                en = "Voxtral vocabulary ({}) larger than {MAX_TEKKEN_BYTES} bytes",
                fr = "vocabulaire de Voxtral ({}) de plus de {MAX_TEKKEN_BYTES} octets",
                path.display(),
            )
        );
        let parsed: File = serde_json::from_slice(&text).map_err(|err| unreadable(&path, &err))?;
        Ok(Self::from_parts(
            parsed
                .vocab
                .iter()
                .map(|e| base64(&e.token_bytes).unwrap_or_default())
                .collect(),
            parsed.config.and_then(|c| c.default_num_special_tokens),
            parsed
                .special_tokens
                .unwrap_or_default()
                .iter()
                .filter_map(|s| s.rank)
                .collect(),
        ))
    }

    fn from_parts(vocab: Vec<Vec<u8>>, n_special: Option<i64>, special: HashSet<i64>) -> Self {
        // Un nombre négatif : aucun jeton n'est spécial par son numéro (le
        // Swift compare `tokenId < nSpecial`).
        let n_special = match n_special {
            None => DEFAULT_SPECIAL,
            Some(n) => usize::try_from(n).unwrap_or(0),
        };
        Self {
            vocab,
            n_special,
            special,
        }
    }

    /// `tokenBytes(for:)` : vides pour un jeton spécial ou hors du
    /// vocabulaire.
    pub fn token_bytes(&self, token: usize) -> &[u8] {
        let special = i64::try_from(token).map_or(true, |t| self.special.contains(&t));
        if token < self.n_special || special {
            return &[];
        }
        self.vocab
            .get(token - self.n_special)
            .map_or(&[], Vec::as_slice)
    }
}

/// `Data(base64Encoded:)` : l'alphabet standard, complété par `=` à un
/// multiple de 4 ; toute autre forme rend `None`.
fn base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.len() / 4;
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && index + 1 != chunks) {
            return None;
        }
        let mut word = 0u32;
        for &c in &chunk[..4 - pad] {
            word = (word << 6) | value(c)?;
        }
        word <<= 6 * pad as u32;
        let decoded = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
        out.extend_from_slice(&decoded[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> Tokenizer {
        Tokenizer::from_parts(
            vec![b"Bon".to_vec(), b"jour".to_vec(), vec![0xC3], vec![0xA9]],
            Some(4),
            HashSet::from([5]),
        )
    }

    /// Les jetons sous `default_num_special_tokens`, ceux qui sont nommés
    /// spéciaux et ceux hors du vocabulaire n'écrivent rien.
    #[test]
    fn special_and_unknown_tokens_have_no_bytes() {
        let t = tiny();
        assert_eq!(t.token_bytes(4), b"Bon");
        assert_eq!(t.token_bytes(5), b"", "spécial par son rang");
        assert_eq!(t.token_bytes(6), [0xC3]);
        assert_eq!(t.token_bytes(3), b"", "sous les spéciaux");
        assert_eq!(t.token_bytes(8), b"", "hors du vocabulaire");
        assert_eq!(t.token_bytes(usize::MAX), b"");
        let default = Tokenizer::from_parts(vec![b"x".to_vec()], None, HashSet::new());
        assert_eq!(default.token_bytes(999), b"");
        assert_eq!(default.token_bytes(1000), b"x");
    }

    #[test]
    fn base64_is_read_like_foundation() {
        assert_eq!(base64("Qm9u").as_deref(), Some(&b"Bon"[..]));
        assert_eq!(base64("am91cg==").as_deref(), Some(&b"jour"[..]));
        assert_eq!(base64("w6k=").as_deref(), Some(&[0xC3, 0xA9][..]));
        assert_eq!(base64("").as_deref(), Some(&[][..]));
        assert_eq!(base64("Qm9"), None, "pas un multiple de 4");
        assert_eq!(base64("Q=9u"), None, "un = au milieu");
        assert_eq!(base64("Qm9u Qm9u"), None, "une espace");
        assert_eq!(base64("Qm*u"), None);
        assert_eq!(base64("Q==="), None);
    }

    #[test]
    fn a_vocabulary_file_is_read_and_its_faults_are_named() {
        let dir = crate::paths::test_scratch("stt-mlx-voxtral-tekken");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = format!("{:#}", Tokenizer::load(&dir).err().unwrap());
        assert!(err.contains(TEKKEN_FILE), "{err}");
        let file = serde_json::json!({
            "vocab": [{"rank": 0, "token_bytes": "Qm9u", "token_str": "Bon"}, {"token_bytes": "%%%%"}],
            "config": {"default_num_special_tokens": 2},
            "special_tokens": [{"rank": 0, "token_str": "<unk>"}, {"rank": 1}]
        });
        std::fs::write(dir.join(TEKKEN_FILE), file.to_string()).unwrap();
        let t = Tokenizer::load(&dir).unwrap();
        assert_eq!(t.token_bytes(2), b"Bon");
        assert_eq!(t.token_bytes(3), b"", "base64 fausse : rien");
        std::fs::write(dir.join(TEKKEN_FILE), "{\"vocab\": 3}").unwrap();
        let err = format!("{:#}", Tokenizer::load(&dir).err().unwrap());
        assert!(err.contains(TEKKEN_FILE), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
