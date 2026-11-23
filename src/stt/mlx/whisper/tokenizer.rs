//! Le vocabulaire de Whisper : `multilingual.tiktoken` (ids → octets) et les
//! ids spéciaux déduits de `n_vocab`. Port de `TiktokenDecoder.swift` et de
//! `WhisperTokenizer.swift` (Dysflow : sans swift-transformers). Pas
//! d'encodage : seuls l'invite (ids spéciaux) et le décodage servent.

use anyhow::{anyhow, bail, ensure, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Au-delà, `multilingual.tiktoken` est refusé avant d'être lu (le vrai fait
/// 816 730 octets).
pub const MAX_TIKTOKEN_BYTES: u64 = 64 << 20;

/// Le décodeur tiktoken minimal de Whisper (`TiktokenDecoder`) : une ligne
/// « jeton_base64 rang » par jeton, rangs 0, 1, 2… contigus. Tout id hors de
/// la table est spécial.
pub struct Tiktoken {
    /// Les octets de chaque jeton, par rang.
    bytes_by_rank: Vec<Vec<u8>>,
}

impl Tiktoken {
    /// Lit le fichier ; illisible, trop gros ou mal formé : une erreur qui le
    /// nomme.
    pub fn load(path: &Path) -> Result<Self> {
        let name = path.display();
        let size = std::fs::metadata(path)
            .with_context(|| crate::tr!(en = "reading {name}", fr = "lecture de {name}"))?
            .len();
        ensure!(
            size <= MAX_TIKTOKEN_BYTES,
            crate::tr!(
                en = "{name}: {size} bytes, more than the {MAX_TIKTOKEN_BYTES} allowed for a vocabulary",
                fr = "{name} : {size} octets, plus que les {MAX_TIKTOKEN_BYTES} permis pour un vocabulaire",
            )
        );
        let bytes = std::fs::read(path)
            .with_context(|| crate::tr!(en = "reading {name}", fr = "lecture de {name}"))?;
        let text = String::from_utf8(bytes).map_err(|_| {
            anyhow!(crate::tr!(
                en = "{name}: not UTF-8",
                fr = "{name} : pas en UTF-8",
            ))
        })?;
        Self::parse(&text)
            .with_context(|| crate::tr!(en = "vocabulary {name}", fr = "vocabulaire {name}"))
    }

    /// `init(contentsOf:)` sur le texte du fichier.
    pub fn parse(text: &str) -> Result<Self> {
        let mut table: Vec<Vec<u8>> = Vec::new();
        // Comme `split(whereSeparator: \.isNewline)` : les lignes vides sont
        // omises (« \r\n » ne fait qu'un séparateur).
        let lines = text
            .split(|c| {
                matches!(
                    c,
                    '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
                )
            })
            .filter(|line| !line.is_empty());
        for (index, line) in lines.enumerate() {
            let number = index + 1;
            let malformed = || {
                anyhow!(crate::tr!(
                    en = "tiktoken file: line {number} unreadable",
                    fr = "fichier tiktoken : ligne {number} illisible",
                ))
            };
            let (token, rank) = split_line(line).ok_or_else(malformed)?;
            let rank: i64 = rank.parse().map_err(|_| malformed())?;
            if usize::try_from(rank).ok() != Some(table.len()) {
                bail!(crate::tr!(
                    en = "tiktoken file: unexpected rank {rank} on line {number}",
                    fr = "fichier tiktoken : rang {rank} inattendu ligne {number}",
                ));
            }
            // Dernière ligne du fichier de Whisper : « = 50256 », un jeton vide.
            if token == "=" {
                table.push(Vec::new());
            } else {
                table.push(base64(token).ok_or_else(malformed)?);
            }
        }
        Ok(Self {
            bytes_by_rank: table,
        })
    }

    /// Le nombre de jetons ordinaires ; le premier id spécial.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.bytes_by_rank.len()
    }

    /// Les octets des jetons, à la suite, puis l'UTF-8 (une séquence invalide
    /// donne U+FFFD). Un id hors de la table : omis si `skip_special`, sinon
    /// écrit `<|id|>`. `drop_incomplete_tail` coupe un caractère inachevé à
    /// la fin.
    pub fn decode(&self, ids: &[i64], skip_special: bool, drop_incomplete_tail: bool) -> String {
        let mut out: Vec<u8> = Vec::with_capacity(ids.len() * 4);
        for &id in ids {
            match usize::try_from(id)
                .ok()
                .and_then(|i| self.bytes_by_rank.get(i))
            {
                Some(bytes) => out.extend_from_slice(bytes),
                None if !skip_special => out.extend_from_slice(format!("<|{id}|>").as_bytes()),
                None => {}
            }
        }
        if drop_incomplete_tail {
            out.truncate(complete_utf8_length(&out));
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// `line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences:
/// true)` en deux parties exactement : les espaces de tête sautés, le jeton
/// jusqu'à l'espace suivant, le reste tel quel (non vide).
fn split_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start_matches(' ');
    let (token, rest) = line.split_once(' ')?;
    (!rest.is_empty()).then_some((token, rest))
}

/// `Data(base64Encoded:)` : l'alphabet standard, longueur multiple de 4, au
/// plus deux `=` à la fin ; autre chose : `None`.
fn base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let padding = bytes.iter().rev().take_while(|&&b| b == b'=').count();
    if padding > 2 {
        return None;
    }
    let value = |b: u8| -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    // Un `=` avant le remplissage final : invalide. Vérifié avant le décodage,
    // qui compte au moins deux caractères réels par groupe de 4.
    let body = bytes.len() - padding;
    if bytes[..body].contains(&b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut word = 0u32;
        let mut real = 0;
        for &b in chunk {
            word <<= 6;
            if b != b'=' {
                word |= value(b)?;
                real += 1;
            }
        }
        out.extend_from_slice(&word.to_be_bytes()[1..]);
        if real < 4 {
            out.truncate(out.len().saturating_sub(4 - real));
        }
    }
    Some(out)
}

/// La longueur du plus long préfixe qui ne s'arrête pas au milieu d'un
/// caractère (`completeUTF8Length`).
pub fn complete_utf8_length(b: &[u8]) -> usize {
    let mut i = b.len();
    let mut continuation = 0;
    while i > 0 && continuation < 3 && b[i - 1] & 0xC0 == 0x80 {
        i -= 1;
        continuation += 1;
    }
    let Some(&lead) = i.checked_sub(1).and_then(|j| b.get(j)) else {
        return b.len();
    };
    let start = i - 1;
    let needed = if lead < 0x80 {
        1
    } else if lead & 0xE0 == 0xC0 {
        2
    } else if lead & 0xF0 == 0xE0 {
        3
    } else if lead & 0xF8 == 0xF0 {
        4
    } else {
        return b.len(); // octet invalide : laisser U+FFFD
    };
    if b.len() - start < needed {
        start
    } else {
        b.len()
    }
}

/// Les langues de Whisper, dans l'ordre officiel : les 99 premières pour
/// 51 865 jetons, les 100 pour 51 866 (`yue` en plus).
const LANGUAGE_CODES: [&str; 100] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su", "yue",
];

/// La partie non spéciale de `suppress_tokens` (tiny comme large-v3-turbo).
const SUPPRESS_BASE: [i64; 82] = [
    1, 2, 7, 8, 9, 10, 14, 25, 26, 27, 28, 29, 31, 58, 59, 60, 61, 62, 63, 90, 91, 92, 93, 359,
    503, 522, 542, 873, 893, 902, 918, 922, 931, 1350, 1853, 1982, 2460, 2627, 3246, 3253, 3268,
    3536, 3846, 3961, 4183, 4667, 6585, 6647, 7273, 9061, 9383, 10428, 10929, 11938, 12033, 12331,
    12562, 13793, 14157, 14635, 15265, 15618, 16553, 16604, 18362, 18956, 20075, 21675, 22520,
    26130, 26161, 26435, 28279, 29464, 31650, 32302, 32470, 36865, 42863, 47425, 49870, 50254,
];

/// Les tailles de vocabulaire gérées (multilingue seulement).
pub const SUPPORTED_VOCABULARY_SIZES: [i32; 2] = [51865, 51866];

/// Les ids spéciaux de Whisper multilingue (`WhisperSpecialTokens`), déduits
/// de `n_vocab` : 51 866 (turbo) a une langue de plus, et tout ce qui suit
/// les langues est décalé de 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialTokens {
    pub end_of_text: i64,
    pub start_of_transcript: i64,
    pub language_to_id: BTreeMap<&'static str, i64>,
    pub translate: i64,
    pub transcribe: i64,
    pub start_of_lm: i64,
    pub start_of_prev: i64,
    pub no_speech: i64,
    pub no_timestamps: i64,
    /// Le premier horodatage `<|0.00|>` ; tout ce qui suit est horodatage.
    pub timestamp_begin: i64,
}

impl SpecialTokens {
    pub fn new(vocabulary_size: i32) -> Self {
        let languages: usize = if vocabulary_size >= 51866 { 100 } else { 99 };
        let language_to_id = LANGUAGE_CODES
            .iter()
            .take(languages)
            .zip(50259i64..)
            .map(|(&code, id)| (code, id))
            .collect();
        let translate = 50259 + languages as i64;
        Self {
            end_of_text: 50257,
            start_of_transcript: 50258,
            language_to_id,
            translate,
            transcribe: translate + 1,
            start_of_lm: translate + 2,
            start_of_prev: translate + 3,
            no_speech: translate + 4,
            no_timestamps: translate + 5,
            timestamp_begin: translate + 6,
        }
    }

    /// `suppressTokens` : la base, puis les ids de tâche.
    pub fn suppress_tokens(&self) -> Vec<i64> {
        let mut ids = SUPPRESS_BASE.to_vec();
        ids.extend([
            self.start_of_transcript,
            self.translate,
            self.transcribe,
            self.start_of_lm,
            self.start_of_prev,
            self.no_speech,
        ]);
        ids
    }

    /// `beginSuppressTokens` : l'espace seul et la fin.
    pub fn begin_suppress_tokens(&self) -> Vec<i64> {
        vec![220, self.end_of_text]
    }
}

/// `WhisperTokenizer` : l'invite et le décodage, sur le vocabulaire et les ids
/// spéciaux.
pub struct Tokenizer {
    inner: Tiktoken,
    pub special: SpecialTokens,
}

impl Tokenizer {
    pub fn new(inner: Tiktoken, special: SpecialTokens) -> Self {
        Self { inner, special }
    }

    /// L'invite d'une transcription dans `language` : début, langue, tâche,
    /// sans horodatage. Une langue inconnue est une erreur : Whisper ne la
    /// devine jamais ici (le Swift la laissait deviner).
    pub fn prompt(&self, language: &str) -> Result<Vec<i64>> {
        let id = self.resolve_language(language).ok_or_else(|| {
            anyhow!(crate::tr!(
                en = "Whisper: unknown language « {language} » (it is never guessed)",
                fr = "Whisper : langue « {language} » inconnue (elle n'est jamais devinée)",
            ))
        })?;
        Ok(vec![
            self.special.start_of_transcript,
            id,
            self.special.transcribe,
            self.special.no_timestamps,
        ])
    }

    /// `resolveLanguage` : un code ISO (« fr ») ou un nom anglais
    /// (« French »), sans casse, `<|fr|>` compris.
    pub fn resolve_language(&self, language: &str) -> Option<i64> {
        let raw = language.trim();
        if raw.is_empty() {
            return None;
        }
        let mut key = raw.to_lowercase();
        if key.starts_with("<|") && key.ends_with("|>") && key.len() >= 4 {
            key = key[2..key.len() - 2].to_string();
        }
        if let Some(&id) = self.special.language_to_id.get(key.as_str()) {
            return Some(id);
        }
        let code = FULL_NAME_TO_CODE
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, code)| *code)?;
        self.special.language_to_id.get(code).copied()
    }

    /// Les ids en texte, sans les ids spéciaux ni les horodatages.
    pub fn decode(&self, tokens: &[i64]) -> String {
        let special = self.special.end_of_text..self.special.timestamp_begin;
        let kept: Vec<i64> = tokens
            .iter()
            .copied()
            .filter(|&id| id >= 0 && id < self.special.timestamp_begin && !special.contains(&id))
            .collect();
        self.inner.decode(&kept, true, false)
    }
}

/// `fullNameToCode`.
const FULL_NAME_TO_CODE: [(&str, &str); 37] = [
    ("english", "en"),
    ("chinese", "zh"),
    ("mandarin", "zh"),
    ("cantonese", "yue"),
    ("japanese", "ja"),
    ("korean", "ko"),
    ("french", "fr"),
    ("german", "de"),
    ("spanish", "es"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("russian", "ru"),
    ("polish", "pl"),
    ("turkish", "tr"),
    ("dutch", "nl"),
    ("arabic", "ar"),
    ("hindi", "hi"),
    ("indonesian", "id"),
    ("vietnamese", "vi"),
    ("thai", "th"),
    ("ukrainian", "uk"),
    ("swedish", "sv"),
    ("czech", "cs"),
    ("romanian", "ro"),
    ("hungarian", "hu"),
    ("danish", "da"),
    ("finnish", "fi"),
    ("norwegian", "no"),
    ("hebrew", "he"),
    ("greek", "el"),
    ("tagalog", "tl"),
    ("filipino", "tl"),
    ("malay", "ms"),
    ("tamil", "ta"),
    ("telugu", "te"),
    ("bengali", "bn"),
    ("urdu", "ur"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn mini() -> Tiktoken {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("macos/Tests/Fixtures/mini.tiktoken");
        Tiktoken::load(&path).unwrap()
    }

    // ---------- TiktokenDecoderTests.swift ----------

    #[test]
    fn decodes_bytes_in_order() {
        assert_eq!(mini().decode(&[0, 1], true, false), "Hello world");
    }

    #[test]
    fn joins_utf8_split_across_tokens() {
        // « é » = C3 A9 : un jeton peut n'en porter qu'une partie.
        assert_eq!(mini().decode(&[0, 2], true, false), "Hello é");
    }

    #[test]
    fn special_ids_are_skipped() {
        assert_eq!(mini().decode(&[50258, 0, 50257], true, false), "Hello");
    }

    #[test]
    fn turbo_shifts_the_task_tokens() {
        let base = SpecialTokens::new(51865);
        let turbo = SpecialTokens::new(51866);
        assert_eq!(base.transcribe, 50359);
        assert_eq!(turbo.transcribe, 50360);
        assert_eq!(turbo.no_timestamps, 50364);
        assert_eq!(base.language_to_id["fr"], 50265);
        assert_eq!(turbo.language_to_id["fr"], 50265);
    }

    // ---------- en plus ----------

    #[test]
    fn special_ids_are_written_unless_skipped() {
        assert_eq!(mini().decode(&[0, 50257], false, false), "Hello<|50257|>");
        assert_eq!(mini().decode(&[-1, 0], false, false), "<|-1|>Hello");
    }

    #[test]
    fn an_incomplete_tail_is_dropped_on_request() {
        let t = Tiktoken::parse("SGVsbG8= 0\nIMM= 1\n").unwrap(); // « Hello », « \u{20}\xC3 »
        assert_eq!(t.decode(&[0, 1], true, true), "Hello ");
        assert_eq!(t.decode(&[0, 1], true, false), "Hello \u{FFFD}");
        assert_eq!(complete_utf8_length(&[]), 0);
        assert_eq!(complete_utf8_length(&[0x80, 0x80]), 2);
        assert_eq!(complete_utf8_length(&[b'a', 0xE2, 0x82]), 1);
        assert_eq!(complete_utf8_length(&[b'a', 0xE2, 0x82, 0xAC]), 4);
    }

    #[test]
    fn the_whisper_empty_last_token_reads() {
        let t = Tiktoken::parse("SGVsbG8= 0\r\n= 1\n\n").unwrap();
        assert_eq!(t.count(), 2);
        assert_eq!(t.decode(&[1, 0], true, false), "Hello");
    }

    #[test]
    fn malformed_lines_are_refused_with_their_number() {
        for (text, needle) in [
            ("SGVsbG8= 0\nnope\n", "2"),
            ("SGVsbG8= 0\nIHdvcmxk 5\n", "5"),
            ("SGVsbG8= x\n", "1"),
            ("SGVsbG8 0\n", "1"),
            ("SG=sbG8= 0\n", "1"),
            ("====AAAA 0\n", "1"),
            ("SGVsbG8= 0\nAA==AAAA 1\n", "2"),
        ] {
            let err = Tiktoken::parse(text).err().expect(text).to_string();
            assert!(
                err.contains("tiktoken") && err.contains(needle),
                "{text:?} : {err}"
            );
        }
    }

    #[test]
    fn a_vocabulary_too_large_is_refused_before_reading() {
        let dir = crate::paths::test_scratch("whisper-tiktoken-trop-gros");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("multilingual.tiktoken");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_TIKTOKEN_BYTES + 1).unwrap();
        let err = Tiktoken::load(&path).err().expect("trop gros").to_string();
        assert!(err.contains("multilingual.tiktoken"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn languages_resolve_by_code_or_name_and_never_by_guess() {
        let tokenizer = Tokenizer::new(mini(), SpecialTokens::new(51865));
        assert_eq!(tokenizer.resolve_language("fr"), Some(50265));
        assert_eq!(tokenizer.resolve_language(" French "), Some(50265));
        assert_eq!(tokenizer.resolve_language("<|EN|>"), Some(50259));
        assert_eq!(
            tokenizer.resolve_language("cantonese"),
            None,
            "yue : 51 866 seulement"
        );
        assert_eq!(
            tokenizer.prompt("fr").unwrap(),
            vec![50258, 50265, 50359, 50363]
        );
        assert_eq!(
            tokenizer.prompt("en").unwrap().len() as i64,
            super::super::config::PROMPT_LEN
        );
        for unknown in ["", "fr-FR", "xx"] {
            let err = tokenizer.prompt(unknown).unwrap_err().to_string();
            assert!(err.contains("Whisper"), "{err}");
        }
    }

    #[test]
    fn decoding_drops_special_and_timestamp_ids() {
        let special = SpecialTokens::new(51865);
        let tokenizer = Tokenizer::new(mini(), special.clone());
        let tokens = [
            special.start_of_transcript,
            0,
            special.timestamp_begin + 3,
            1,
            special.end_of_text,
        ];
        assert_eq!(tokenizer.decode(&tokens), "Hello world");
        assert_eq!(special.suppress_tokens().len(), 88);
        assert_eq!(special.begin_suppress_tokens(), vec![220, 50257]);
    }
}
