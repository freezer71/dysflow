//! L'encodeur BPE de Qwen2 (ByteLevel), comme `Qwen2BPETokenizer.swift` :
//! `vocab.json`, `merges.txt` et les jetons ajoutés de
//! `tokenizer_config.json`. Commun à Qwen3-ASR (qui encode son gabarit et
//! décode le texte reconnu) et à Qwen3-TTS (qui encode la phrase à dire) ;
//! il ne dépend d'aucun modèle, ni de MLX. Ne lit que : n'écrit jamais dans
//! le dossier du modèle.
//!
//! Mêmes étapes que le Swift : texte en NFC, coupe aux jetons ajoutés (le
//! plus proche, le plus long à égalité), pré-découpe de
//! `transformers.Qwen2Tokenizer` (`PATTERN`), octets vers la table GPT-2,
//! fusions de plus petit rang, ids du vocabulaire (un morceau inconnu est
//! omis). Le décodage rend le contenu des jetons ajoutés et les octets des
//! autres, décodés en UTF-8 une fois mis bout à bout.
//!
//! Les fichiers sont lus bornés en taille ; un fichier absent, trop gros,
//! illisible ou mal formé est une erreur bilingue qui le nomme, jamais une
//! panique.

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use unicode_normalization::UnicodeNormalization;

use crate::sync::LockExt;

pub const VOCAB_FILE: &str = "vocab.json";
pub const MERGES_FILE: &str = "merges.txt";
pub const CONFIG_FILE: &str = "tokenizer_config.json";

/// `vocab.json` et `merges.txt` plus gros que cela : refusés sans être lus
/// (ceux de Qwen3 font 2,8 et 1,7 Mo).
pub const MAX_TABLE_BYTES: u64 = 64 << 20;
/// `tokenizer_config.json` plus gros que cela : refusé (celui de Qwen3-ASR
/// fait 12 ko).
pub const MAX_CONFIG_BYTES: u64 = 16 << 20;
/// Les morceaux gardés en cache au plus ; au-delà, le cache repart de zéro
/// (le Swift ne le borne pas ; les ids n'en dépendent pas).
const MAX_CACHED_PIECES: usize = 1 << 16;

/// La pré-découpe de `transformers.Qwen2Tokenizer` (`Qwen2BPETokenizer.pattern`,
/// chiffres un à un : `\p{N}`), telle que le Swift la donne à
/// `NSRegularExpression`.
pub const PATTERN: &str = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";

/// `PATTERN` sans l'anticipation `(?!\S)`, que la crate `regex` n'a pas : la
/// dernière branche, `\s+`, est capturée, et `pieces` la raccourcit d'un
/// caractère quand `\s+(?!\S)` l'aurait fait (voir `split`). `\s` y est,
/// comme dans ICU, la propriété Unicode `White_Space`.
const SPLIT: &str = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|(\s+)";

/// Le BPE de Qwen2. Partageable entre fils (`Send + Sync`) : le cache des
/// morceaux et la table inverse sont sous verrou.
pub struct BpeTokenizer {
    vocab: HashMap<String, u32>,
    /// `a` puis `b` → rang de la fusion « a b » (plus petit = plus tôt).
    ranks: HashMap<String, HashMap<String, u32>>,
    /// Jetons ajoutés, du plus long au plus court (contenu, id).
    added: Vec<(String, u32)>,
    /// Jetons ajoutés par id, pour `decode`.
    added_by_id: HashMap<u32, String>,
    split: Regex,
    /// id → morceau, construit au premier décodage seulement.
    reverse: OnceLock<HashMap<u32, String>>,
    cache: Mutex<HashMap<String, Vec<u32>>>,
}

impl std::fmt::Debug for BpeTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BpeTokenizer")
            .field("vocab", &self.vocab.len())
            .field(
                "merges",
                &self.ranks.values().map(HashMap::len).sum::<usize>(),
            )
            .field("added", &self.added.len())
            .finish()
    }
}

fn unreadable(path: &Path, err: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "unreadable BPE vocabulary ({}): {err}",
        fr = "vocabulaire BPE illisible ({}) : {err}",
        path.display(),
    ))
}

/// `path`, au plus `limit` octets : plus gros, refusé sans être lu.
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|err| unreadable(path, &err))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| unreadable(path, &err))?;
    if bytes.len() as u64 > limit {
        return Err(anyhow!(crate::tr!(
            en = "BPE vocabulary ({}) larger than {limit} bytes",
            fr = "vocabulaire BPE ({}) de plus de {limit} octets",
            path.display(),
        )));
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct Added {
    content: String,
}

#[derive(Deserialize)]
struct TokenizerConfig {
    added_tokens_decoder: Option<HashMap<String, Added>>,
}

impl BpeTokenizer {
    /// `init(directory:)` : `vocab.json`, `merges.txt` et
    /// `tokenizer_config.json` de `dir`.
    pub fn from_dir(dir: &Path) -> Result<Self> {
        Self::from_files(
            &dir.join(VOCAB_FILE),
            &dir.join(MERGES_FILE),
            &dir.join(CONFIG_FILE),
        )
    }

    /// `init(vocab:merges:config:)`.
    pub fn from_files(vocab: &Path, merges: &Path, config: &Path) -> Result<Self> {
        let vocab = read_vocab(vocab)?;
        let ranks = read_merges(merges)?;
        let (added, added_by_id) = read_added(config)?;
        let split = Regex::new(SPLIT).map_err(|err| {
            anyhow!(crate::tr!(
                en = "BPE pre-tokenizer: {err}",
                fr = "pré-découpe du BPE : {err}",
            ))
        })?;
        Ok(Self {
            vocab,
            ranks,
            added,
            added_by_id,
            split,
            reverse: OnceLock::new(),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// L'id d'un jeton ajouté (`<|im_end|>`…), s'il existe.
    pub fn added_id(&self, content: &str) -> Option<u32> {
        self.added
            .iter()
            .find(|(c, _)| c == content)
            .map(|&(_, id)| id)
    }

    /// `encode(text:allowSpecial:)`. `allow_special` vrai (le défaut du
    /// Swift, comme `transformers`) : un jeton ajouté écrit en toutes lettres
    /// (`<|im_end|>`, `<think>`…) devient son id. Faux : tout est du texte
    /// ordinaire, pour la phrase à lire (un livre peut contenir ces
    /// chaînes) ; seul le gabarit garde ses jetons.
    pub fn encode(&self, text: &str, allow_special: bool) -> Vec<u32> {
        let text: String = text.nfc().collect();
        if !allow_special {
            return self.encode_ordinary(&text);
        }
        let mut ids = Vec::new();
        let mut rest = text.as_str();
        while !rest.is_empty() {
            // Le jeton ajouté le plus proche (le plus long à égalité : la
            // liste va du plus long au plus court, l'inégalité est stricte).
            let mut next: Option<(usize, usize, u32)> = None;
            for (content, id) in &self.added {
                if let Some(at) = rest.find(content.as_str()) {
                    if next.is_none_or(|(start, _, _)| at < start) {
                        next = Some((at, at + content.len(), *id));
                    }
                }
            }
            let Some((start, end, id)) = next else {
                ids.extend(self.encode_ordinary(rest));
                break;
            };
            ids.extend(self.encode_ordinary(&rest[..start]));
            ids.push(id);
            rest = &rest[end..];
        }
        ids
    }

    /// `decode(tokens:)` : les jetons ajoutés rendent leur contenu
    /// (`<asr_text>` sépare la langue reconnue du texte), les autres leurs
    /// octets (table GPT-2 inversée), décodés en UTF-8 une fois mis bout à
    /// bout (un caractère peut tenir sur deux jetons ; une suite invalide
    /// devient U+FFFD). Un id inconnu est omis.
    pub fn decode(&self, ids: &[u32]) -> String {
        let pieces = self.reverse.get_or_init(|| {
            let mut reverse = HashMap::with_capacity(self.vocab.len());
            for (piece, &id) in &self.vocab {
                reverse.entry(id).or_insert_with(|| piece.clone());
            }
            reverse
        });
        let decoder = byte_decoder();
        let mut text = String::new();
        let mut bytes: Vec<u8> = Vec::new();
        for id in ids {
            if let Some(special) = self.added_by_id.get(id) {
                text.push_str(&String::from_utf8_lossy(&bytes));
                bytes.clear();
                text.push_str(special);
            } else if let Some(piece) = pieces.get(id) {
                bytes.extend(piece.chars().filter_map(|c| decoder.get(&c).copied()));
            }
        }
        text.push_str(&String::from_utf8_lossy(&bytes));
        text
    }

    /// Le texte coupé par la pré-découpe, morceau par morceau.
    fn encode_ordinary(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::new();
        for piece in split(&self.split, text) {
            ids.extend(self.encode_piece(piece));
        }
        ids
    }

    fn encode_piece(&self, piece: &str) -> Vec<u32> {
        if let Some(hit) = self.cache.locked().get(piece) {
            return hit.clone();
        }
        let table = byte_encoder();
        let symbols: Vec<String> = piece
            .bytes()
            .map(|b| table[usize::from(b)].to_string())
            .collect();
        let ids: Vec<u32> = self
            .bpe(symbols)
            .iter()
            .filter_map(|s| self.vocab.get(s).copied())
            .collect();
        let mut cache = self.cache.locked();
        if cache.len() >= MAX_CACHED_PIECES {
            cache.clear();
        }
        cache.insert(piece.to_string(), ids.clone());
        ids
    }

    fn rank(&self, first: &str, second: &str) -> Option<u32> {
        self.ranks.get(first)?.get(second).copied()
    }

    /// Fusionne la paire de plus petit rang, toutes ses occurrences de gauche
    /// à droite, jusqu'à ce qu'aucune paire ne soit connue (GPT-2).
    fn bpe(&self, symbols: Vec<String>) -> Vec<String> {
        let mut parts = symbols;
        while parts.len() > 1 {
            let mut best: Option<(u32, usize)> = None;
            for (i, pair) in parts.windows(2).enumerate() {
                if let [a, b] = pair {
                    if let Some(r) = self.rank(a, b) {
                        if best.is_none_or(|(rank, _)| r < rank) {
                            best = Some((r, i));
                        }
                    }
                }
            }
            let Some((_, at)) = best else {
                break;
            };
            let (first, second) = match (parts.get(at), parts.get(at + 1)) {
                (Some(a), Some(b)) => (a.clone(), b.clone()),
                _ => break,
            };
            let mut merged = Vec::with_capacity(parts.len());
            let mut i = 0;
            while i < parts.len() {
                let pair = parts.get(i + 1).is_some_and(|b| *b == second) && parts[i] == first;
                if pair {
                    merged.push(format!("{first}{second}"));
                    i += 2;
                } else {
                    merged.push(parts[i].clone());
                    i += 1;
                }
            }
            parts = merged;
        }
        parts
    }
}

/// Les morceaux de la pré-découpe (`PATTERN`), de gauche à droite. Chaque
/// caractère commence un morceau d'une des branches, donc les morceaux se
/// suivent sans trou. Quand la dernière branche (`\s+`) prend une suite de
/// blancs de deux caractères ou plus suivie d'un non-blanc, `\s+(?!\S)` du
/// Swift en aurait laissé le dernier au morceau suivant : il y est rendu.
pub fn split<'t>(regex: &Regex, text: &'t str) -> Vec<&'t str> {
    let mut pieces = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let Some(caps) = regex.captures_at(text, at) else {
            break;
        };
        let Some(found) = caps.get(0) else {
            break;
        };
        let (start, mut end) = (found.start(), found.end());
        if caps.get(1).is_some() {
            let followed = text[end..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace());
            let mut chars = text[start..end].char_indices();
            let last = chars.next_back().map(|(i, _)| start + i);
            if followed && chars.next().is_some() {
                if let Some(last) = last {
                    end = last;
                }
            }
        }
        if end <= at {
            break;
        }
        pieces.push(&text[start..end]);
        at = end;
    }
    pieces
}

/// La table GPT-2 `bytes_to_unicode` : chaque octet devient un caractère
/// imprimable (les octets imprimables restent eux-mêmes, les autres vont à
/// 256, 257…, dans l'ordre).
pub fn byte_encoder() -> &'static [char; 256] {
    static TABLE: OnceLock<[char; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let printable = |b: u32| {
            (33..=126).contains(&b) || (161..=172).contains(&b) || (174..=255).contains(&b)
        };
        let mut table = [' '; 256];
        let mut n = 0u32;
        for (b, slot) in (0u32..).zip(table.iter_mut()) {
            let code = if printable(b) {
                b
            } else {
                n += 1;
                255 + n
            };
            *slot = char::from_u32(code).unwrap_or(' ');
        }
        table
    })
}

/// L'inverse de `byte_encoder`.
fn byte_decoder() -> &'static HashMap<char, u8> {
    static TABLE: OnceLock<HashMap<char, u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0u8..=255)
            .zip(byte_encoder().iter().copied())
            .map(|(b, c)| (c, b))
            .collect()
    })
}

/// `vocab.json` : morceau → id.
fn read_vocab(path: &Path) -> Result<HashMap<String, u32>> {
    let bytes = read_bounded(path, MAX_TABLE_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|err| unreadable(path, &err))
}

/// `merges.txt` : une fusion « a b » par ligne non vide, dans l'ordre des
/// rangs ; une première ligne `#version…` est sautée, une ligne qui n'a pas
/// exactement deux parties aussi (sans prendre de rang). Une fusion donnée
/// deux fois garde son dernier rang, comme le dictionnaire du Swift.
fn read_merges(path: &Path) -> Result<HashMap<String, HashMap<String, u32>>> {
    let bytes = read_bounded(path, MAX_TABLE_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|err| unreadable(path, &err))?;
    let mut ranks: HashMap<String, HashMap<String, u32>> = HashMap::new();
    let mut rank = 0u32;
    for (index, line) in text.split('\n').filter(|l| !l.is_empty()).enumerate() {
        if index == 0 && line.starts_with("#version") {
            continue;
        }
        let mut parts = line.split(' ');
        let (Some(a), Some(b), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        ranks
            .entry(a.to_string())
            .or_default()
            .insert(b.to_string(), rank);
        rank = rank.saturating_add(1);
    }
    Ok(ranks)
}

/// Les jetons ajoutés de `tokenizer_config.json` (`added_tokens_decoder`,
/// id en clé), du plus long au plus court, à longueur égale par id ; une
/// clé qui n'est pas un nombre est ignorée, comme `Int(key)` du Swift, un
/// id hors de 0…2³²−1 refusé.
#[allow(clippy::type_complexity)]
fn read_added(path: &Path) -> Result<(Vec<(String, u32)>, HashMap<u32, String>)> {
    let bytes = read_bounded(path, MAX_CONFIG_BYTES)?;
    let config: TokenizerConfig =
        serde_json::from_slice(&bytes).map_err(|err| unreadable(path, &err))?;
    let mut added = Vec::new();
    for (key, token) in config.added_tokens_decoder.unwrap_or_default() {
        let Ok(id) = key.parse::<i64>() else {
            continue;
        };
        let id = u32::try_from(id).map_err(|_| {
            anyhow!(crate::tr!(
                en = "BPE vocabulary ({}): added token id {id} out of range",
                fr = "vocabulaire BPE ({}) : id de jeton ajouté {id} hors limites",
                path.display(),
            ))
        })?;
        added.push((token.content, id));
    }
    added.sort_by(|a, b| {
        b.0.chars()
            .count()
            .cmp(&a.0.chars().count())
            .then(a.1.cmp(&b.1))
    });
    let mut by_id = HashMap::new();
    for (content, id) in &added {
        by_id.entry(*id).or_insert_with(|| content.clone());
    }
    // Un contenu vide ne se trouve jamais dans le texte (`range(of: "")` du
    // Swift ne rend rien) ; il se décode toujours (en rien).
    added.retain(|(content, _)| !content.is_empty());
    Ok((added, by_id))
}

#[cfg(test)]
#[path = "bpe_tests.rs"]
mod tests;
