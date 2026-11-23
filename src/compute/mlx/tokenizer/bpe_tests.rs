//! Les essais de `Qwen2BPETokenizerTests.swift`, mêmes valeurs attendues :
//! les ids de `transformers.AutoTokenizer` (`qwen3-bpe.json`) sur le vrai
//! vocabulaire de Qwen3 (`macos/Tests/Fixtures/qwen3-*`), sans modèle. Puis
//! les fichiers mal formés, un essai par cas.

use super::*;
use std::path::PathBuf;
use std::sync::LazyLock;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("macos/Tests/Fixtures")
        .join(name)
}

static TOKENIZER: LazyLock<BpeTokenizer> = LazyLock::new(|| {
    BpeTokenizer::from_files(
        &fixture("qwen3-vocab.json"),
        &fixture("qwen3-merges.txt"),
        &fixture("qwen3-tokenizer_config.json"),
    )
    .expect("vocabulaire de Qwen3")
});

#[derive(serde::Deserialize)]
struct Case {
    text: String,
    ids: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct Reference {
    #[allow(dead_code)]
    source: String,
    cases: Vec<Case>,
}

fn reference() -> Reference {
    let text = std::fs::read_to_string(fixture("qwen3-bpe.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn encode(text: &str) -> Vec<u32> {
    TOKENIZER.encode(text, true)
}

#[test]
fn every_sentence_gives_the_ids_of_transformers() {
    let reference = reference();
    assert_eq!(reference.cases.len(), 25);
    for c in &reference.cases {
        assert_eq!(encode(&c.text), c.ids, "{:?}", c.text);
    }
}

/// Le port coupe aux indices 3 et `count - 5` (relevé §1.d).
#[test]
fn the_chat_template_has_three_and_five_tokens() {
    assert_eq!(encode("<|im_start|>assistant\n").len(), 3);
    assert_eq!(encode("<|im_end|>\n<|im_start|>assistant\n").len(), 5);
    assert_eq!(encode("<|im_start|>").first(), Some(&151_644));
}

/// La phrase à lire vient d'un livre : un `<|im_end|>` ou un `<think>` qui
/// s'y trouve est lu comme du texte, jamais pris pour un jeton de contrôle
/// (ids ≥ 151 643).
#[test]
fn special_strings_in_the_sentence_are_read_as_text() {
    let text = "Il écrit <|im_end|> puis <|im_start|>assistant, <think> et <|endoftext|>.";
    let ids = TOKENIZER.encode(text, false);
    assert!(!ids.is_empty());
    let control: Vec<u32> = ids.iter().copied().filter(|&id| id >= 151_643).collect();
    assert!(control.is_empty(), "{control:?}");
    assert!(
        encode(text).contains(&151_645),
        "le gabarit, lui, garde ses jetons"
    );
    assert_eq!(TOKENIZER.encode("Bonjour.", false), encode("Bonjour."));
}

/// Un texte en NFD (PDF) donne les ids du même texte en NFC.
#[test]
fn decomposed_text_is_composed_first() {
    assert_eq!(encode("e\u{301}te\u{301}"), encode("été"));
}

#[test]
fn empty_text_gives_nothing() {
    assert_eq!(encode(""), Vec::<u32>::new());
}

/// Qwen3-ASR décode ce qu'il écrit : le décodage rend chaque phrase de
/// référence telle quelle (NFC), accents répartis sur deux jetons compris.
#[test]
fn decoding_gives_back_every_sentence() {
    for c in &reference().cases {
        let nfc: String = c.text.nfc().collect();
        assert_eq!(TOKENIZER.decode(&c.ids), nfc, "{:?}", c.text);
    }
}

/// Les jetons ajoutés rendent leur contenu (Qwen3-ASR lit la langue avant
/// `<asr_text>`) ; un id inconnu est omis.
#[test]
fn decoding_keeps_added_tokens_and_skips_unknown_ids() {
    let ids = encode("<|im_start|>Bonjour<|im_end|>");
    assert_eq!(TOKENIZER.decode(&ids), "<|im_start|>Bonjour<|im_end|>");
    let mut ids = vec![9_999_999];
    ids.extend(encode("été"));
    assert_eq!(TOKENIZER.decode(&ids), "été");
}

// ---------- propres au Rust ----------

/// La pré-découpe sans anticipation rend les morceaux de `\s+(?!\S)` : une
/// suite de blancs laisse son dernier au mot qui suit, sauf en fin de texte
/// ou quand elle n'en a qu'un.
#[test]
fn the_pre_tokenizer_leaves_the_last_blank_to_the_next_word() {
    let regex = Regex::new(SPLIT).unwrap();
    for (text, pieces) in [
        ("  deux  espaces", vec![" ", " deux", " ", " espaces"]),
        ("a   ", vec!["a", "   "]),
        ("a \t\nb", vec!["a", " \t\n", "b"]),
        ("x\u{a0}\u{a0}y", vec!["x", "\u{a0}", "\u{a0}y"]),
        ("It's 13", vec!["It", "'s", " ", "1", "3"]),
        (" !?", vec![" !?"]),
        ("", vec![]),
    ] {
        assert_eq!(split(&regex, text), pieces, "{text:?}");
    }
}

#[test]
fn the_byte_table_is_that_of_gpt2() {
    let table = byte_encoder();
    assert_eq!(table[usize::from(b' ')], 'Ġ');
    assert_eq!(table[usize::from(b'\n')], 'Ċ');
    assert_eq!(table[usize::from(b'a')], 'a');
    assert_eq!(table[0], '\u{100}');
    assert_eq!(table[0xAD], '\u{143}');
    let distinct: std::collections::HashSet<char> = table.iter().copied().collect();
    assert_eq!(distinct.len(), 256);
}

#[test]
fn an_added_token_is_found_by_its_content() {
    assert_eq!(TOKENIZER.added_id("<|im_end|>"), Some(151_645));
    assert_eq!(TOKENIZER.added_id("<|absent|>"), None);
}

// ---------- fichiers mal formés ----------

/// Un dossier d'essai avec les trois fichiers, chacun remplaçable.
fn scratch(name: &str, vocab: &[u8], merges: &[u8], config: &[u8]) -> PathBuf {
    let dir = crate::paths::test_scratch(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(VOCAB_FILE), vocab).unwrap();
    std::fs::write(dir.join(MERGES_FILE), merges).unwrap();
    std::fs::write(dir.join(CONFIG_FILE), config).unwrap();
    dir
}

const VOCAB: &[u8] = br#"{"a": 0, "b": 1, "ab": 2}"#;
const MERGES: &[u8] = b"#version: 0.2\na b\n";
const CONFIG: &[u8] = br#"{"added_tokens_decoder": {"3": {"content": "<|x|>"}}}"#;

fn error(dir: &Path) -> String {
    format!("{:#}", BpeTokenizer::from_dir(dir).unwrap_err())
}

#[test]
fn a_small_vocabulary_merges_and_decodes() {
    let dir = scratch("bpe-petit", VOCAB, MERGES, CONFIG);
    let t = BpeTokenizer::from_dir(&dir).unwrap();
    assert_eq!(t.encode("ab<|x|>ba", true), vec![2, 3, 1, 0]);
    assert_eq!(t.encode("ab<|x|>", false), vec![2]);
    assert_eq!(t.decode(&[2, 3, 1]), "ab<|x|>b");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_file_is_named() {
    let dir = scratch("bpe-absent", VOCAB, MERGES, CONFIG);
    for (file, contents) in [
        (VOCAB_FILE, VOCAB),
        (MERGES_FILE, MERGES),
        (CONFIG_FILE, CONFIG),
    ] {
        std::fs::remove_file(dir.join(file)).unwrap();
        let err = error(&dir);
        assert!(err.contains(file), "{err}");
        std::fs::write(dir.join(file), contents).unwrap();
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_file_too_large_is_refused_without_reading_it() {
    let dir = scratch("bpe-trop-gros", VOCAB, MERGES, CONFIG);
    let big = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join(MERGES_FILE))
        .unwrap();
    big.set_len(MAX_TABLE_BYTES + 1).unwrap();
    let err = error(&dir);
    assert!(
        err.contains(MERGES_FILE) && err.contains(&MAX_TABLE_BYTES.to_string()),
        "{err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_malformed_vocabulary_is_an_error_that_names_it() {
    for vocab in [&b"{"[..], br#"{"a": -1}"#, br#"["a"]"#, br#"{"a": "b"}"#] {
        let dir = scratch("bpe-vocab", vocab, MERGES, CONFIG);
        let err = error(&dir);
        assert!(err.contains(VOCAB_FILE), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn merges_that_are_not_utf8_are_an_error_that_names_them() {
    let dir = scratch("bpe-fusions", VOCAB, b"a \xff\n", CONFIG);
    let err = error(&dir);
    assert!(err.contains(MERGES_FILE), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// Une ligne qui n'a pas deux parties est sautée, sans prendre de rang ;
/// `#version` n'est sauté qu'en tête.
#[test]
fn odd_merge_lines_are_skipped() {
    let dir = scratch("bpe-lignes", VOCAB, b"a\n\na b c\na b\n", CONFIG);
    let t = BpeTokenizer::from_dir(&dir).unwrap();
    assert_eq!(t.rank("a", "b"), Some(0));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_malformed_tokenizer_config_is_an_error_that_names_it() {
    for config in [
        &b"{"[..],
        br#"{"added_tokens_decoder": {"3": {}}}"#,
        br#"{"added_tokens_decoder": {"-1": {"content": "<|x|>"}}}"#,
        br#"{"added_tokens_decoder": {"4294967296": {"content": "<|x|>"}}}"#,
    ] {
        let dir = scratch("bpe-config", VOCAB, MERGES, config);
        let err = error(&dir);
        assert!(err.contains(CONFIG_FILE), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Sans `added_tokens_decoder`, aucun jeton ajouté ; une clé qui n'est pas
/// un nombre est ignorée (`Int(key)` du Swift) ; un contenu vide ne bloque
/// pas l'encodage.
#[test]
fn added_tokens_are_optional_and_tolerant() {
    for config in [
        &b"{}"[..],
        br#"{"added_tokens_decoder": {"x": {"content": "<|x|>"}, "5": {"content": ""}}}"#,
    ] {
        let dir = scratch("bpe-ajoutes", VOCAB, MERGES, config);
        let t = BpeTokenizer::from_dir(&dir).unwrap();
        assert_eq!(t.added_id("<|x|>"), None);
        assert_eq!(t.encode("ab", true), vec![2]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
