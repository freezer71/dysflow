//! Le modèle Whisper : chargement d'un dossier mlx-community (comme
//! `WhisperModel.fromDirectory` : `config.json`, `multilingual.tiktoken`, les
//! `*.safetensors` renommés comme `sanitizeMlxWhisper`, la quantification de
//! `config.json`), puis la lecture d'une fenêtre de 30 s (`transcribeChunk` :
//! encodeur, invite, décodage glouton avec les jetons interdits) et d'un audio
//! entier (`generate` : fenêtres de 30 s, textes collés).

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::Array;
use std::cell::Cell;
use std::path::Path;

use super::audio;
use super::config::{Config, GenerationConfig};
use super::layers::{Decoder, Encoder, LayerCache, NAME};
use super::tokenizer::{SpecialTokens, Tiktoken, Tokenizer};
use crate::stt::mlx::weights::Weights;

/// Les fichiers exigés du dossier.
pub const CONFIG_FILE: &str = "config.json";
pub const TIKTOKEN_FILE: &str = "multilingual.tiktoken";
/// La graine des positions de l'encodeur que mlx-whisper n'écrit pas.
const ENCODER_POSITIONS: &str = "model.encoder.embed_positions.weight";
/// Leur nombre (`whisperSinusoids(length: 1500, …)`).
const SINUSOID_LENGTH: usize = 1500;

/// Ce qu'une fenêtre a rendu : l'invite, les jetons gardés, leur texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowText {
    pub prompt: Vec<i64>,
    pub tokens: Vec<i64>,
    pub text: String,
}

pub struct Whisper {
    pub config: Config,
    pub encoder: Encoder,
    pub decoder: Decoder,
    pub tokenizer: Tokenizer,
    begin_suppress: Vec<i64>,
    suppress: Vec<i64>,
}

impl Whisper {
    /// `fromDirectory` : `config.json` et `multilingual.tiktoken` d'abord (un
    /// fichier absent ou illisible échoue avant les poids), puis les poids,
    /// chaque clé prise avec sa forme ; une clé en trop est une erreur.
    pub fn load(dir: &Path) -> Result<Self> {
        for required in [CONFIG_FILE, TIKTOKEN_FILE] {
            if !dir.join(required).is_file() {
                bail!(crate::tr!(
                    en = "Whisper: {required} is missing from {}",
                    fr = "Whisper : {required} absent de {}",
                    dir.display()
                ));
            }
        }
        let config = Config::load(dir)?;
        let special = SpecialTokens::new(config.vocab_size);
        let tokenizer = Tokenizer::new(Tiktoken::load(&dir.join(TIKTOKEN_FILE))?, special);
        // `generationConfig ?? special.generationConfig`, puis les `??` de
        // `transcribeChunk`.
        let (begin_suppress, suppress) = match GenerationConfig::load(dir) {
            Some(g) => (
                g.begin_suppress_tokens
                    .unwrap_or_else(|| vec![tokenizer.special.end_of_text]),
                g.suppress_tokens.unwrap_or_default(),
            ),
            None => (
                tokenizer.special.begin_suppress_tokens(),
                tokenizer.special.suppress_tokens(),
            ),
        };

        let blocks = Cell::new(false);
        let mut w = Weights::load_as_stored(dir, NAME, config.quantization, |key| {
            if key.contains(".blocks.") {
                blocks.set(true);
            }
            remap_key(key)
        })?;
        if !blocks.get() {
            bail!(crate::tr!(
                en = "Whisper: {} holds no mlx-whisper weights (*.blocks.*); the transformers layout is not supported",
                fr = "Whisper : {} n'a pas de poids mlx-whisper (*.blocks.*) ; la disposition de transformers n'est pas gérée",
                dir.display()
            ));
        }
        if !w.has(ENCODER_POSITIONS) && w.has("model.encoder.conv2.weight") {
            let values = sinusoids(SINUSOID_LENGTH, config.d_model as usize);
            let shape = [SINUSOID_LENGTH as i32, config.d_model];
            w.insert(ENCODER_POSITIONS, Array::from_slice(&values, &shape), dir);
        }
        let encoder = Encoder::load(&mut w, &config)?;
        let decoder = Decoder::load(&mut w, &config)?;
        w.finish()?;
        Ok(Self {
            config,
            encoder,
            decoder,
            tokenizer,
            begin_suppress,
            suppress,
        })
    }

    /// `generate` : chaque fenêtre de 30 s lue seule, ses jetons et son
    /// texte ; `stop` est consulté après chaque calcul évalué.
    pub fn transcribe(
        &self,
        samples: &[f32],
        language: &str,
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<WindowText>> {
        let prompt = self.tokenizer.prompt(language)?;
        audio::windows(samples)
            .into_iter()
            .map(|window| self.window(window, &prompt, stop))
            .collect()
    }

    /// Le texte de l'app : celui de chaque fenêtre sans espaces autour, les
    /// non vides collés par une espace.
    pub fn text(windows: &[WindowText]) -> String {
        windows
            .iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `transcribeChunk` : une fenêtre, décodage glouton.
    fn window(
        &self,
        samples: &[f32],
        prompt: &[i64],
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<WindowText> {
        let features = audio::features(samples, self.config.num_mel_bins)?;
        let encoder_hidden = self.encoder.forward(&features)?;
        let mut caches: Vec<LayerCache> = (0..self.config.decoder_layers)
            .map(|_| LayerCache::default())
            .collect();

        let ids: Vec<i32> = prompt.iter().map(|&id| id as i32).collect();
        let prompt_array = Array::from_slice(&ids, &[1, ids.len() as i32]);
        let hidden = self
            .decoder
            .forward(&prompt_array, 0, &encoder_hidden, &mut caches)?;
        let mut logits = self.decoder.project(&hidden.try_index((0, -1))?)?;
        logits.eval()?;
        stop()?;

        let prompt_len = prompt.len() as i32;
        let max_tokens = self
            .config
            .max_tokens()
            .min(self.config.max_target_positions - prompt_len - 1)
            .max(1);
        // Les masques des jetons interdits, construits une fois et ajoutés
        // dans l'ordre du Swift.
        let begin_mask = mask(&logits, &self.begin_suppress)?;
        let always_mask = mask(&logits, &self.suppress)?;
        let timestamp_mask = mask_from(&logits, self.tokenizer.special.timestamp_begin)?;

        let end = self.tokenizer.special.end_of_text;
        let mut generated: Vec<i64> = Vec::new();
        for step in 0..max_tokens {
            let mut step_logits = logits.clone();
            if step == 0 {
                if let Some(m) = &begin_mask {
                    step_logits = step_logits.add(m)?;
                }
            }
            if let Some(m) = &always_mask {
                step_logits = step_logits.add(m)?;
            }
            if let Some(m) = &timestamp_mask {
                step_logits = step_logits.add(m)?;
            }
            let next = i64::from(argmax_axis(&step_logits, -1, None)?.try_item_exact::<u32>()?);
            if next == end {
                break;
            }
            generated.push(next);

            let token = Array::from_slice(&[next as i32], &[1, 1]);
            let hidden =
                self.decoder
                    .forward(&token, prompt_len + step, &encoder_hidden, &mut caches)?;
            logits = self.decoder.project(&hidden.try_index((0, -1))?)?;
            logits.eval()?;
            if generated.len().is_multiple_of(256) {
                mlx_rs::memory::clear_cache()?;
            }
            stop()?;
        }
        let text = self.tokenizer.decode(&generated);
        Ok(WindowText {
            prompt: prompt.to_vec(),
            tokens: generated,
            text,
        })
    }
}

/// `suppressMask` : −1e9 sur `ids` (ceux de la largeur des logits), 0
/// ailleurs, au type des logits ; `None` si `ids` est vide.
fn mask(logits: &Array, ids: &[i64]) -> Result<Option<Array>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let length = logits.shape().last().copied().unwrap_or(0).max(0);
    let mut values = vec![0f32; length as usize];
    for &id in ids {
        if let Some(cell) = usize::try_from(id).ok().and_then(|i| values.get_mut(i)) {
            *cell = -1e9;
        }
    }
    Ok(Some(
        Array::from_slice(&values, &[length]).as_dtype(logits.dtype())?,
    ))
}

/// `suppressFromIndexMask` : −1e9 à partir de `from` ; `None` s'il n'y a
/// rien à masquer.
fn mask_from(logits: &Array, from: i64) -> Result<Option<Array>> {
    let length = logits.shape().last().copied().unwrap_or(0).max(0);
    if from >= i64::from(length) {
        return Ok(None);
    }
    let from = from.max(0) as usize;
    let values: Vec<f32> = (0..length as usize)
        .map(|i| if i >= from { -1e9 } else { 0.0 })
        .collect();
    Ok(Some(
        Array::from_slice(&values, &[length]).as_dtype(logits.dtype())?,
    ))
}

/// `whisperSinusoids` : `[longueur, canaux]`, sinus puis cosinus, calculés en
/// `f64` puis arrondis en `f32`, comme le Swift.
pub fn sinusoids(length: usize, channels: usize) -> Vec<f32> {
    let half = channels / 2;
    let increment = 10000f64.ln() / (half.saturating_sub(1).max(1)) as f64;
    let mut values = vec![0f32; length * channels];
    for pos in 0..length {
        for i in 0..half {
            let scaled = pos as f64 * (-increment * i as f64).exp();
            values[pos * channels + i] = scaled.sin() as f32;
            values[pos * channels + half + i] = scaled.cos() as f32;
        }
    }
    values
}

/// `remapMlxWhisperKey` : les clés de mlx-whisper vers celles du graphe ;
/// `None` pour une clé ignorée (`alignment_heads`, ou inconnue : le Swift la
/// laisse de côté).
pub fn remap_key(raw: &str) -> Option<String> {
    match raw {
        "encoder.positional_embedding" => return Some(ENCODER_POSITIONS.to_string()),
        "decoder.positional_embedding" => {
            return Some("model.decoder.embed_positions.weight".to_string())
        }
        "encoder.conv1.weight"
        | "encoder.conv1.bias"
        | "encoder.conv2.weight"
        | "encoder.conv2.bias" => return Some(format!("model.{raw}")),
        _ => {}
    }
    if let Some(rest) = raw.strip_prefix("decoder.token_embedding.") {
        return Some(format!("model.decoder.embed_tokens.{rest}"));
    }
    if let Some(rest) = raw.strip_prefix("encoder.ln_post.") {
        return Some(format!("model.encoder.layer_norm.{rest}"));
    }
    if let Some(rest) = raw.strip_prefix("decoder.ln.") {
        return Some(format!("model.decoder.layer_norm.{rest}"));
    }
    for stem in ["encoder", "decoder"] {
        let Some(rest) = raw.strip_prefix(&format!("{stem}.blocks.")) else {
            continue;
        };
        let (layer, suffix) = rest.split_once('.')?;
        let mapped = remap_block(suffix, stem == "decoder")?;
        return Some(format!("model.{stem}.layers.{layer}.{mapped}"));
    }
    None
}

/// `remapBlockSuffix`.
fn remap_block(suffix: &str, decoder: bool) -> Option<String> {
    if let Some(rest) = suffix.strip_prefix("attn_ln.") {
        return Some(format!("self_attn_layer_norm.{rest}"));
    }
    if decoder {
        if let Some(rest) = suffix.strip_prefix("cross_attn_ln.") {
            return Some(format!("encoder_attn_layer_norm.{rest}"));
        }
    }
    let renamed = [
        ("mlp_ln.", "final_layer_norm."),
        ("mlp1.", "fc1."),
        ("mlp2.", "fc2."),
    ];
    for (from, to) in renamed {
        if let Some(rest) = suffix.strip_prefix(from) {
            return Some(format!("{to}{rest}"));
        }
    }
    if let Some(rest) = suffix.strip_prefix("attn.") {
        return remap_attention(rest, "self_attn");
    }
    if decoder {
        if let Some(rest) = suffix.strip_prefix("cross_attn.") {
            return remap_attention(rest, "encoder_attn");
        }
    }
    None
}

/// `remapAttnSuffix` : `query` → `q_proj`, `key`, `value`, `out`.
fn remap_attention(suffix: &str, container: &str) -> Option<String> {
    let (projection, tail) = suffix.split_once('.')?;
    let mapped = match projection {
        "query" => "q_proj",
        "key" => "k_proj",
        "value" => "v_proj",
        "out" => "out_proj",
        _ => return None,
    };
    Some(format!("{container}.{mapped}.{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_renamed_like_the_swift() {
        let cases = [
            ("encoder.conv1.weight", Some("model.encoder.conv1.weight")),
            (
                "encoder.ln_post.bias",
                Some("model.encoder.layer_norm.bias"),
            ),
            ("decoder.ln.weight", Some("model.decoder.layer_norm.weight")),
            (
                "decoder.token_embedding.scales",
                Some("model.decoder.embed_tokens.scales"),
            ),
            (
                "decoder.positional_embedding",
                Some("model.decoder.embed_positions.weight"),
            ),
            (
                "encoder.positional_embedding",
                Some("model.encoder.embed_positions.weight"),
            ),
            (
                "encoder.blocks.3.attn.query.biases",
                Some("model.encoder.layers.3.self_attn.q_proj.biases"),
            ),
            (
                "decoder.blocks.0.cross_attn.key.weight",
                Some("model.decoder.layers.0.encoder_attn.k_proj.weight"),
            ),
            (
                "decoder.blocks.1.cross_attn_ln.bias",
                Some("model.decoder.layers.1.encoder_attn_layer_norm.bias"),
            ),
            (
                "encoder.blocks.0.mlp_ln.weight",
                Some("model.encoder.layers.0.final_layer_norm.weight"),
            ),
            (
                "encoder.blocks.0.mlp2.bias",
                Some("model.encoder.layers.0.fc2.bias"),
            ),
            ("encoder.blocks.0.cross_attn.key.weight", None),
            ("encoder.blocks.0.attn.other.weight", None),
            ("encoder.blocks.0", None),
            ("alignment_heads", None),
            ("proj_out.weight", None),
        ];
        for (raw, expected) in cases {
            assert_eq!(remap_key(raw).as_deref(), expected, "{raw}");
        }
    }

    #[test]
    fn sinusoids_are_sines_then_cosines() {
        let s = sinusoids(3, 4);
        assert_eq!(&s[..4], &[0.0, 0.0, 1.0, 1.0]);
        // Position 1 : sin(1), sin(≈1e-4) ; cos(1), cos(≈1e-4).
        assert_eq!(s[4], 1f64.sin() as f32);
        assert!((s[5] - 1e-4).abs() < 1e-9, "{}", s[5]);
        assert_eq!(s[6], 1f64.cos() as f32);
    }
}
