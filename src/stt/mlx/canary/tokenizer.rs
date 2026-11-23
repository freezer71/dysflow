//! Le vocabulaire de Canary, comme `CanaryTokenizer` et la part de
//! `SentencePieceTokenizer.swift` qu'il emploie : le modèle SentencePiece
//! (protobuf, `tokenizer.model` ou `tokenizer.model_base64` de
//! `config.json` ; `tokenizer.json` sinon), les jetons de `tokens.txt` s'il
//! y en a, l'invite (`<|startofcontext|>`… langue source et cible…), la fin
//! (`<|endoftext|>`) et le texte des jetons. Puis `CanarySpeech.clean` de
//! l'app : les jetons spéciaux (`<|…|>`) que le décodage laisse passer sont
//! retirés. Seul le décodage est porté (Canary n'encode pas de texte).
//!
//! Un fichier malformé est une erreur, jamais une panique.

use anyhow::{anyhow, bail, ensure, Context, Result};
use std::collections::HashMap;
use std::path::Path;

use super::config::Config;

pub const TOKENS_FILE: &str = "tokens.txt";
pub const SENTENCEPIECE_FILE: &str = "tokenizer.model";
pub const TOKENIZER_JSON_FILE: &str = "tokenizer.json";
/// Un vocabulaire plus gros que cela : refusé sans être lu.
const MAX_FILE_BYTES: u64 = 64 << 20;

// ---------- le protobuf de SentencePiece ----------

/// `SentencePiecePieceType` : seuls les jetons de contrôle et inutilisés
/// sont omis du texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    Normal,
    Unknown,
    Control,
    UserDefined,
    Unused,
    Byte,
}

impl PieceType {
    /// Une valeur inconnue : `.normal`, comme le Swift.
    fn from_raw(raw: u64) -> Self {
        match raw {
            2 => Self::Unknown,
            3 => Self::Control,
            4 => Self::UserDefined,
            5 => Self::Unused,
            6 => Self::Byte,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub token: String,
    pub kind: PieceType,
}

/// `SentencePieceProtobufReader`.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

fn malformed_varint() -> anyhow::Error {
    anyhow!(crate::tr!(
        "Malformed SentencePiece protobuf varint",
        "varint du protobuf SentencePiece malformé"
    ))
}

fn truncated() -> anyhow::Error {
    anyhow!(crate::tr!(
        "Truncated SentencePiece protobuf field",
        "champ du protobuf SentencePiece tronqué"
    ))
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn at_end(&self) -> bool {
        self.at >= self.data.len()
    }

    fn varint(&mut self) -> Result<u64> {
        let (mut value, mut shift) = (0u64, 0u32);
        while self.at < self.data.len() && shift < 64 {
            let byte = self.data[self.at];
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
        Err(malformed_varint())
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(len)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(truncated)?;
        let slice = &self.data[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn length_delimited(&mut self) -> Result<&'a [u8]> {
        let len = usize::try_from(self.varint()?).map_err(|_| truncated())?;
        self.take(len)
    }

    fn fixed32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn skip(&mut self, wire: u64) -> Result<()> {
        match wire {
            0 => self.varint().map(drop),
            1 => self.take(8).map(drop),
            2 => self.length_delimited().map(drop),
            5 => self.fixed32().map(drop),
            _ => bail!(crate::tr!(
                en = "Unsupported SentencePiece protobuf wire type: {wire}",
                fr = "type de champ du protobuf SentencePiece non géré : {wire}",
            )),
        }
    }

    /// La clé d'un champ : numéro et type.
    fn key(&mut self) -> Result<(u64, u64)> {
        let key = self.varint()?;
        Ok((key >> 3, key & 0x7))
    }
}

/// `parsePiece` : le texte (champ 1, décodé en UTF-8 avec remplacement), le
/// type (champ 3) ; sans texte, `None`.
fn parse_piece(data: &[u8]) -> Result<Option<Piece>> {
    let mut reader = Reader::new(data);
    let (mut token, mut kind) = (None, PieceType::Normal);
    while !reader.at_end() {
        match reader.key()? {
            (1, 2) => {
                token = Some(String::from_utf8_lossy(reader.length_delimited()?).into_owned())
            }
            (2, 5) => drop(reader.fixed32()?),
            (3, 0) => kind = PieceType::from_raw(reader.varint()?),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(token.map(|token| Piece { token, kind }))
}

/// `parseTrainerSpecModelType` : lu (et donc vérifié) comme le Swift ; seul
/// le décodage, qui ne dépend pas du type de modèle, est porté.
fn check_trainer_spec(data: &[u8]) -> Result<()> {
    let mut reader = Reader::new(data);
    while !reader.at_end() {
        match reader.key()? {
            (3, 0) => return reader.varint().map(drop),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(())
}

/// `SentencePieceTokenizer`, pour le décodage : les morceaux dans l'ordre
/// des ids.
#[derive(Debug, Clone)]
pub struct SentencePiece {
    pieces: Vec<Piece>,
    ids: HashMap<String, usize>,
}

impl SentencePiece {
    fn new(pieces: Vec<Piece>) -> Self {
        // `mapping[tok.token] = i` : le dernier l'emporte.
        let ids = pieces
            .iter()
            .enumerate()
            .map(|(i, p)| (p.token.clone(), i))
            .collect();
        Self { pieces, ids }
    }

    /// `SentencePieceModelParser.parsePieces` : les morceaux (champ 1) ; aucun
    /// est une erreur.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(data);
        let mut pieces = Vec::new();
        while !reader.at_end() {
            match reader.key()? {
                (1, 2) => {
                    if let Some(piece) = parse_piece(reader.length_delimited()?)? {
                        pieces.push(piece);
                    }
                }
                (2, 2) => check_trainer_spec(reader.length_delimited()?)?,
                (_, wire) => reader.skip(wire)?,
            }
        }
        ensure!(
            !pieces.is_empty(),
            crate::tr!(
                "SentencePiece model did not contain any vocabulary pieces",
                "le modèle SentencePiece n'a aucun morceau de vocabulaire"
            )
        );
        Ok(Self::new(pieces))
    }

    /// `init(tokenizerJSON:)` : `model.unk_id`, `model.vocab` en paires
    /// `[morceau, score]` ; chaque morceau est normal.
    pub fn from_tokenizer_json(bytes: &[u8]) -> Result<Self> {
        let missing = |field: &str| {
            anyhow!(crate::tr!(
                en = "{field} missing from tokenizer.json",
                fr = "{field} absent de tokenizer.json",
            ))
        };
        let json: serde_json::Value = serde_json::from_slice(bytes)?;
        let model = json
            .get("model")
            .and_then(|m| m.as_object())
            .ok_or_else(|| missing("model"))?;
        model
            .get("unk_id")
            .and_then(|u| u.as_i64())
            .ok_or_else(|| missing("model.unk_id"))?;
        let vocab = model
            .get("vocab")
            .and_then(|v| v.as_array())
            .ok_or_else(|| missing("model.vocab"))?;
        let mut pieces = Vec::with_capacity(vocab.len());
        for entry in vocab {
            match entry.as_array().map(Vec::as_slice) {
                Some([token, score]) if token.is_string() && score.is_number() => {
                    pieces.push(Piece {
                        token: token.as_str().unwrap_or_default().to_string(),
                        kind: PieceType::Normal,
                    })
                }
                _ => bail!(crate::tr!(
                    en = "tokenizer.json: a model.vocab entry is not [piece, score]",
                    fr = "tokenizer.json : une entrée de model.vocab n'est pas [morceau, score]",
                )),
            }
        }
        Ok(Self::new(pieces))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    /// `tokenID(for:)`.
    pub fn id(&self, token: &str) -> Option<usize> {
        self.ids.get(token).copied()
    }

    /// `decode` : contrôle et inutilisés omis, octets `<0x..>` recollés en
    /// UTF-8 (une suite invalide est perdue), `▁` en espace, espaces autour
    /// retirés.
    pub fn decode(&self, ids: &[usize]) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        let mut pieces: Vec<String> = Vec::new();
        let flush = |bytes: &mut Vec<u8>, pieces: &mut Vec<String>| {
            if !bytes.is_empty() {
                if let Ok(text) = String::from_utf8(std::mem::take(bytes)) {
                    pieces.push(text);
                }
            }
        };
        for &id in ids {
            let Some(piece) = self.pieces.get(id) else {
                continue;
            };
            if matches!(piece.kind, PieceType::Control | PieceType::Unused) {
                continue;
            }
            if let Some(byte) = byte_piece(&piece.token) {
                bytes.push(byte);
                continue;
            }
            flush(&mut bytes, &mut pieces);
            pieces.push(piece.token.clone());
        }
        flush(&mut bytes, &mut pieces);
        let joined = pieces.concat().replace('▁', " ");
        trim_swift_whitespaces(&joined).to_string()
    }
}

/// `<0xAB>` : l'octet, sinon `None`.
fn byte_piece(token: &str) -> Option<u8> {
    if token.starts_with("<0x") && token.ends_with('>') && token.chars().count() == 6 {
        let hex: String = token.chars().skip(3).take(2).collect();
        u8::from_str_radix(&hex, 16).ok()
    } else {
        None
    }
}

/// `trimmingCharacters(in: .whitespaces)` : espaces (catégorie Zs) et
/// tabulation, sans les fins de ligne.
fn trim_swift_whitespaces(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() && !is_newline(c))
}

/// Les fins de ligne de `CharacterSet.newlines`.
fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\u{000A}'..='\u{000D}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

// ---------- CanaryTokenizer ----------

/// `loadTokenMaps` : « morceau id » par ligne (un morceau qui commence par
/// une espace la garde), ou « id » seul pour l'espace.
fn token_maps(text: &str) -> (HashMap<String, usize>, HashMap<usize, String>) {
    let (mut to_id, mut to_token) = (HashMap::new(), HashMap::new());
    for line in text.lines() {
        let trimmed = trim_swift_whitespaces(line);
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        let (token, id) = match fields.as_slice() {
            [token, id] => match id.parse::<usize>() {
                Ok(id) if line.starts_with(' ') => (format!(" {token}"), id),
                Ok(id) => (token.to_string(), id),
                Err(_) => continue,
            },
            [id] => match id.parse::<usize>() {
                Ok(id) => (" ".to_string(), id),
                Err(_) => continue,
            },
            _ => continue,
        };
        to_id.insert(token.clone(), id);
        to_token.insert(id, token);
    }
    (to_id, to_token)
}

/// Un fichier du dossier, au plus `MAX_FILE_BYTES`.
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let size = std::fs::metadata(path)
        .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))?
        .len();
    ensure!(
        size <= MAX_FILE_BYTES,
        crate::tr!(
            en = "Canary vocabulary {}: {size} bytes, more than {MAX_FILE_BYTES}",
            fr = "vocabulaire de Canary {} : {size} octets, plus de {MAX_FILE_BYTES}",
            path.display()
        )
    );
    std::fs::read(path)
        .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))
}

/// `CanaryTokenizer`.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    sentence_piece: Option<SentencePiece>,
    /// `tokens.txt` (vide sans lui).
    to_id: HashMap<String, usize>,
    to_token: HashMap<usize, String>,
}

impl Tokenizer {
    /// `fromModelDirectory` : `tokens.txt`, puis SentencePiece de
    /// `tokenizer.model`, de `tokenizer.json` ou de `config.json` (base64) ;
    /// sans aucun : `None`.
    pub fn from_dir(dir: &Path, config: &Config) -> Result<Option<Self>> {
        let tokens = dir.join(TOKENS_FILE);
        let (to_id, to_token) = if tokens.is_file() {
            let bytes = read_bounded(&tokens)?;
            let text = String::from_utf8(bytes).map_err(|_| {
                anyhow!(crate::tr!(
                    en = "Canary vocabulary {}: not UTF-8",
                    fr = "vocabulaire de Canary {} : pas en UTF-8",
                    tokens.display()
                ))
            })?;
            token_maps(&text)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let unreadable = |source: String| {
            move |err: anyhow::Error| {
                anyhow!(crate::tr!(
                    en = "unreadable Canary vocabulary ({source}): {err:#}",
                    fr = "vocabulaire de Canary illisible ({source}) : {err:#}",
                ))
            }
        };
        let model = dir.join(SENTENCEPIECE_FILE);
        let json = dir.join(TOKENIZER_JSON_FILE);
        let sentence_piece = if model.is_file() {
            Some(
                SentencePiece::parse(&read_bounded(&model)?)
                    .map_err(unreadable(model.display().to_string()))?,
            )
        } else if json.is_file() {
            Some(
                SentencePiece::from_tokenizer_json(&read_bounded(&json)?)
                    .map_err(unreadable(json.display().to_string()))?,
            )
        } else {
            // `Data(base64Encoded:)` invalide : pas de vocabulaire, comme le
            // Swift.
            match config
                .tokenizer_model_base64
                .as_deref()
                .and_then(crate::stt::mlx::whisper::tokenizer::base64)
            {
                Some(data) => Some(
                    SentencePiece::parse(&data)
                        .map_err(unreadable("config.json, tokenizer.model_base64".into()))?,
                ),
                None => None,
            }
        };
        if sentence_piece.is_none() && to_id.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            sentence_piece,
            to_id,
            to_token,
        }))
    }

    /// `tokenToId[token] ?? tokenizer?.tokenID(for: token)`.
    fn lookup(&self, token: &str) -> Option<usize> {
        self.to_id
            .get(token)
            .copied()
            .or_else(|| self.sentence_piece.as_ref()?.id(token))
    }

    /// `buildPromptTokens` : contexte, transcription, émotion indéfinie (ou
    /// leurs ids de `config.json`), puis, s'ils existent, la langue source, la
    /// langue cible, la ponctuation, sans normalisation, sans horodatage,
    /// sans locuteurs.
    pub fn prompt(&self, config: &Config, source: &str, target: &str) -> Vec<i64> {
        let id = |token: &str, fallback: i64| self.lookup(token).map_or(fallback, |id| id as i64);
        let mut tokens = vec![
            id("<|startofcontext|>", config.start_of_context_id),
            id("<|startoftranscript|>", config.start_of_transcript_id),
            id("<|emo:undefined|>", config.emotion_undefined_id),
        ];
        for token in [
            format!("<|{source}|>"),
            format!("<|{target}|>"),
            "<|pnc|>".to_string(),
            "<|noitn|>".to_string(),
            "<|notimestamp|>".to_string(),
            "<|nodiarize|>".to_string(),
        ] {
            if let Some(id) = self.lookup(&token) {
                tokens.push(id as i64);
            }
        }
        tokens
    }

    /// `eosTokenId`.
    pub fn eos(&self, config: &Config) -> i64 {
        self.lookup("<|endoftext|>")
            .map_or(config.end_of_text_id, |id| id as i64)
    }

    /// `decode` : par SentencePiece, sinon par `tokens.txt` (`<|…|>` omis,
    /// `▁` en espace, espaces autour retirés).
    pub fn decode(&self, ids: &[i64]) -> String {
        let ids: Vec<usize> = ids
            .iter()
            .filter_map(|&id| usize::try_from(id).ok())
            .collect();
        if let Some(sp) = &self.sentence_piece {
            return sp.decode(&ids);
        }
        let joined: String = ids
            .iter()
            .filter_map(|id| self.to_token.get(id))
            .filter(|t| !(t.starts_with("<|") && t.ends_with("|>")))
            .map(String::as_str)
            .collect();
        joined.replace('▁', " ").trim().to_string()
    }
}

/// `CanarySpeech.clean` : `<|…|>` (sans `|` dedans) retirés, puis les
/// espaces et fins de ligne autour.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        let after = &rest[start + 2..];
        match after.find('|') {
            Some(bar) if after[bar + 1..].starts_with('>') => {
                out.push_str(&rest[..start]);
                rest = &after[bar + 2..];
            }
            _ => {
                out.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
#[path = "tokenizer_tests.rs"]
pub(super) mod tests;
