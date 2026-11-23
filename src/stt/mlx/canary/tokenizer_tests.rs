//! Les essais du vocabulaire de Canary (`tokenizer.rs`).

use super::*;

/// Un modèle SentencePiece minimal : les morceaux `(texte, type)`.
pub(in crate::stt::mlx::canary) fn model_bytes(pieces: &[(&str, u64)]) -> Vec<u8> {
    let varint = |mut v: u64, out: &mut Vec<u8>| loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    };
    let mut out = Vec::new();
    for (token, kind) in pieces {
        let mut piece = vec![0x0a];
        varint(token.len() as u64, &mut piece);
        piece.extend_from_slice(token.as_bytes());
        piece.extend_from_slice(&[0x15, 0, 0, 0, 0]); // score 0
        piece.push(0x18);
        varint(*kind, &mut piece);
        out.push(0x0a);
        varint(piece.len() as u64, &mut out);
        out.extend_from_slice(&piece);
    }
    // trainer_spec { model_type: 1 }
    out.extend_from_slice(&[0x12, 0x02, 0x18, 0x01]);
    out
}

fn config() -> Config {
    Config::parse("{}").unwrap()
}

#[test]
fn the_protobuf_pieces_and_their_types_are_read() {
    let sp = SentencePiece::parse(&model_bytes(&[
        ("<unk>", 2),
        ("<|endoftext|>", 3),
        ("▁Bon", 1),
        ("jour", 1),
        ("<0xC3>", 6),
        ("<0xA9>", 6),
        ("<|fr|>", 4),
        ("<pad>", 5),
    ]))
    .unwrap();
    assert_eq!(sp.len(), 8);
    assert_eq!(sp.id("jour"), Some(3));
    // Contrôle et inutilisés omis ; octets recollés ; `<|fr|>` (défini
    // par l'utilisateur) gardé ; espaces autour retirés.
    assert_eq!(sp.decode(&[2, 3, 1, 4, 5, 7, 6, 99]), "Bonjouré<|fr|>");
    // Un octet seul, invalide en UTF-8 : perdu.
    assert_eq!(sp.decode(&[2, 4]), "Bon");
    assert_eq!(byte_piece("<0xéA>"), None, "pas de panique hors ASCII");
    assert_eq!(byte_piece("<0x4Z>"), None);
}

#[test]
fn a_malformed_protobuf_is_an_error_not_a_panic() {
    for (data, what) in [
        (&[0x0a, 0x05, 0x0a][..], "tronqué"),
        (&[0x0a][..], "varint"),
        (&[0xff; 11][..], "varint"),
        (&[0x0b][..], "type"),
        (&[0x12, 0x00][..], "aucun"),
        (
            &[0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f][..],
            "tronqué",
        ),
    ] {
        let err = SentencePiece::parse(data).unwrap_err().to_string();
        assert!(
            !err.is_empty(),
            "{what} : {data:?} doit être refusé, sans paniquer"
        );
    }
}

/// Un `tokenizer.json` mal formé, chaque cas refusé avec une erreur qui
/// nomme le champ ; le dossier qui le porte nomme le fichier.
#[test]
fn a_malformed_tokenizer_json_is_refused_by_name() {
    for (json, what) in [
        ("pas du json", ""),
        (r#"{"autre": 1}"#, "model"),
        (r#"{"model": {"vocab": []}}"#, "model.unk_id"),
        (r#"{"model": {"unk_id": 0}}"#, "model.vocab"),
        (
            r#"{"model": {"unk_id": 0, "vocab": [["a"]]}}"#,
            "model.vocab",
        ),
        (
            r#"{"model": {"unk_id": 0, "vocab": [[1, 0.5]]}}"#,
            "model.vocab",
        ),
    ] {
        let err = SentencePiece::from_tokenizer_json(json.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{json} accepté"))
            .to_string();
        assert!(err.contains(what), "{json} : {err}");
    }
    let ok = SentencePiece::from_tokenizer_json(
        r#"{"model": {"unk_id": 0, "vocab": [["<unk>", 0], ["▁oui", -1.5]]}}"#.as_bytes(),
    )
    .unwrap();
    assert_eq!(ok.decode(&[1]), "oui");

    let dir = crate::paths::test_scratch("stt-mlx-canary-tokenizer-json");
    std::fs::write(dir.join(TOKENIZER_JSON_FILE), r#"{"model": {}}"#).unwrap();
    let Err(err) = Tokenizer::from_dir(&dir, &config()) else {
        panic!("tokenizer.json mal formé accepté");
    };
    let err = err.to_string();
    assert!(err.contains(TOKENIZER_JSON_FILE), "{err}");
}

#[test]
fn the_prompt_has_both_languages_and_the_task_tokens() {
    let sp = SentencePiece::parse(&model_bytes(&[
        ("<unk>", 2),
        ("<|endoftext|>", 4),
        ("<|startofcontext|>", 4),
        ("<|startoftranscript|>", 4),
        ("<|emo:undefined|>", 4),
        ("<|fr|>", 4),
        ("<|en|>", 4),
        ("<|pnc|>", 4),
        ("<|noitn|>", 4),
        ("<|notimestamp|>", 4),
        ("<|nodiarize|>", 4),
    ]))
    .unwrap();
    let tokenizer = Tokenizer {
        sentence_piece: Some(sp),
        to_id: HashMap::new(),
        to_token: HashMap::new(),
    };
    let c = config();
    assert_eq!(
        tokenizer.prompt(&c, "fr", "fr"),
        [2, 3, 4, 5, 5, 7, 8, 9, 10]
    );
    assert_eq!(
        tokenizer.prompt(&c, "en", "fr"),
        [2, 3, 4, 6, 5, 7, 8, 9, 10]
    );
    // Une langue inconnue n'a pas de jeton : elle manque à l'invite.
    assert_eq!(tokenizer.prompt(&c, "xx", "xx"), [2, 3, 4, 7, 8, 9, 10]);
    assert_eq!(tokenizer.eos(&c), 1);
}

#[test]
fn without_special_tokens_the_config_ids_are_the_fallback() {
    let sp = SentencePiece::parse(&model_bytes(&[("<unk>", 2), ("a", 1)])).unwrap();
    let tokenizer = Tokenizer {
        sentence_piece: Some(sp),
        to_id: HashMap::new(),
        to_token: HashMap::new(),
    };
    let c = config();
    assert_eq!(tokenizer.prompt(&c, "fr", "fr"), [0, 1, 2]);
    assert_eq!(tokenizer.eos(&c), 3);
}

#[test]
fn tokens_txt_is_read_and_decoded_without_special_tokens() {
    let (to_id, to_token) = token_maps("<|fr|> 0\n▁Bon 1\njour 2\n 3\n\nmauvais\n ▁x 4\n");
    assert_eq!(to_id.get("<|fr|>"), Some(&0));
    assert_eq!(to_token.get(&3).map(String::as_str), Some(" "));
    assert_eq!(to_token.get(&4).map(String::as_str), Some(" ▁x"));
    let tokenizer = Tokenizer {
        sentence_piece: None,
        to_id,
        to_token,
    };
    assert_eq!(tokenizer.decode(&[0, 1, 2, 99]), "Bonjour");
}

/// `CanarySpeech.clean` : chaque `<|…|>` retiré, le reste tel quel.
#[test]
fn clean_removes_the_special_tokens() {
    assert_eq!(clean(" <|fr|>Bonjour <|pnc|>à tous. "), "Bonjour à tous.");
    assert_eq!(clean("a <| b |> c"), "a  c");
    assert_eq!(clean("a <|b|c|> d"), "a <|b|c|> d");
    assert_eq!(clean("<|<|x|>|>"), "<||>");
    assert_eq!(clean("pas de jeton"), "pas de jeton");
    assert_eq!(clean("<|fin"), "<|fin");
}

#[test]
fn the_base64_vocabulary_of_config_json_is_read() {
    let dir = crate::paths::test_scratch("canary-vocabulaire-base64");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = model_bytes(&[("<unk>", 2), ("▁oui", 1)]);
    let encoded = base64_encode(&bytes);
    let mut c = config();
    c.tokenizer_model_base64 = Some(encoded);
    let tokenizer = Tokenizer::from_dir(&dir, &c).unwrap().expect("vocabulaire");
    assert_eq!(tokenizer.decode(&[1]), "oui");
    // Base64 invalide et aucun fichier : pas de vocabulaire.
    c.tokenizer_model_base64 = Some("pas du base64".into());
    assert!(Tokenizer::from_dir(&dir, &c).unwrap().is_none());
    // Un tokenizer.model malformé : une erreur qui le nomme.
    std::fs::write(dir.join(SENTENCEPIECE_FILE), [0x0a, 0x05]).unwrap();
    let err = Tokenizer::from_dir(&dir, &c).unwrap_err().to_string();
    assert!(err.contains(SENTENCEPIECE_FILE), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

pub(in crate::stt::mlx::canary) fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
