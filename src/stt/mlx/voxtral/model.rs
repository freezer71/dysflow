//! Le modèle Voxtral Realtime (`VoxtralRealtime.swift`, pour le chemin du
//! flux) : chargement (clés renommées comme `sanitize`, quantification lue
//! dans `config.json`, chaque tenseur à son type du fichier, comme le Swift
//! qui ne convertit rien), le nombre de jetons d'un audio et du retard,
//! l'échelle du retard (`ensureAdaScales`) et le tirage glouton.

use anyhow::{anyhow, Result};
use mlx_rs::ops::indexing::argmax_axis;
use mlx_rs::Array;
use std::path::Path;

use super::audio::mel_filters;
use super::config::{Config, FRAMES_PER_TOKEN, HOP_LENGTH, SAMPLE_RATE};
use super::decoder::{time_embedding, Decoder};
use super::encoder::Encoder;
use super::tokenizer::Tokenizer;
use crate::stt::mlx::nn::Linear;
use crate::stt::mlx::weights::Weights;

/// Le modèle dans les messages.
pub const NAME: &str = "Voxtral";
/// Une clé de convolution du format d'origine, à transposer après lecture
/// (`sanitize` : `[sorties, entrées, noyau]` → `[sorties, noyau, entrées]`).
const TO_TRANSPOSE: &str = "#transposer";

/// Une `Linear` qui peut être quantifiée (`shouldQuantize`) : quantifiée si
/// le fichier a ses `scales`.
pub fn linear(
    w: &mut Weights,
    prefix: &str,
    input: i32,
    output: i32,
    bias: bool,
) -> Result<Linear> {
    Linear::load(w, prefix, input, output, bias)
}

pub struct Voxtral {
    pub config: Config,
    pub encoder: Encoder,
    pub decoder: Decoder,
    tokenizer: Tokenizer,
    filters: Array,
    /// Le retard (en jetons) des échelles calculées (`adaScaleDelay`).
    ada_delay: Option<i32>,
}

impl Voxtral {
    /// `fromDirectory` : `config.json`, `tekken.json`, puis les
    /// `*.safetensors` du dossier, chaque clé prise avec sa forme ; une clé
    /// absente ou en trop est une erreur qui la nomme. Les échelles du
    /// retard du fichier sont calculées.
    pub fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(dir)?;
        let tokenizer = Tokenizer::load(dir)?;
        let mut w = Weights::load_as_stored(dir, NAME, config.quantization, remap_key)?;
        for i in 0..2 {
            let key = format!("encoder.conv_layers_{i}_conv.conv.weight");
            if let Some((array, file)) = w.take_any(&format!("{key}{TO_TRANSPOSE}")) {
                let array = if array.ndim() == 3 {
                    array.transpose_axes(&[0, 2, 1])?
                } else {
                    array
                };
                w.insert(&key, array, &file);
            }
        }
        let mut model = Self::from_weights(config, tokenizer, w)?;
        let delay = model.config.transcription_delay_ms;
        model.ensure_ada_scales(delay)?;
        Ok(model)
    }

    pub fn from_weights(config: Config, tokenizer: Tokenizer, mut w: Weights) -> Result<Self> {
        let encoder = Encoder::load(&mut w, &config.encoder, config.decoder.dim)?;
        let decoder = Decoder::load(&mut w, &config.decoder)?;
        w.finish()?;
        let filters = mel_filters(&config.audio);
        Ok(Self {
            config,
            encoder,
            decoder,
            tokenizer,
            filters,
            ada_delay: None,
        })
    }

    /// `ensureMelFilters`.
    pub fn mel_filters(&self) -> Array {
        self.filters.clone()
    }

    /// `streamingTokenBytes`.
    pub fn token_bytes(&self, token: usize) -> &[u8] {
        self.tokenizer.token_bytes(token)
    }

    /// `ensureAdaScales` : les échelles du retard, recalculées s'il change.
    pub fn ensure_ada_scales(&mut self, delay_ms: i32) -> Result<()> {
        let tokens = num_delay_tokens(delay_ms);
        if self.ada_delay != Some(tokens) {
            let t_cond = time_embedding(tokens as f32, self.config.decoder.dim)?;
            self.decoder.precompute_ada_scales(&t_cond)?;
            self.ada_delay = Some(tokens);
        }
        Ok(())
    }

    /// `sample(logits:temperature: 0)` : le plus grand logit.
    pub fn sample(logits: &Array) -> Result<usize> {
        let index = argmax_axis(logits, -1, None)?.try_item_exact::<u32>()?;
        usize::try_from(index).map_err(|_| anyhow!("{NAME}: token {index}"))
    }
}

/// `numAudioTokens` : les jetons d'un audio de `samples` échantillons,
/// arrondis comme le Swift (en `Double`).
pub fn num_audio_tokens(samples: usize) -> i32 {
    let hop = HOP_LENGTH as usize;
    let frames = if !samples.is_multiple_of(hop) {
        (samples as f64 / hop as f64 - 1.0).ceil()
    } else {
        (samples / hop) as f64
    };
    (frames / f64::from(FRAMES_PER_TOKEN)).ceil() as i32
}

/// `numDelayTokens` : 480 ms → 6 jetons.
pub fn num_delay_tokens(delay_ms: i32) -> i32 {
    let samples = (f64::from(delay_ms) / 1000.0 * f64::from(SAMPLE_RATE)) as usize;
    num_audio_tokens(samples)
}

/// `sanitize` : les clés du format d'origine (Mistral) vers celles du
/// graphe ; les autres, dont celles du paquet (déjà converties), restent
/// telles quelles. Une convolution d'origine est marquée, pour être
/// transposée après lecture.
pub fn remap_key(key: &str) -> Option<String> {
    const ENC: &str = "mm_streams_embeddings.embedding_module.whisper_encoder";
    const ADAPTER: &str = "mm_streams_embeddings.embedding_module";
    let ff = |path: &str| {
        path.replace("feed_forward.w1.", "feed_forward_w1.")
            .replace("feed_forward.w2.", "feed_forward_w2.")
            .replace("feed_forward.w3.", "feed_forward_w3.")
    };
    let mapped = if key == format!("{ADAPTER}.tok_embeddings.weight") {
        Some("decoder.tok_embeddings.weight".to_string())
    } else if key == "norm.weight" {
        Some("decoder.norm.weight".to_string())
    } else if let Some(rest) = key.strip_prefix(&format!("{ENC}.conv_layers.")) {
        match rest.splitn(3, '.').collect::<Vec<_>>()[..] {
            [layer, _, param] => {
                let mapped = format!("encoder.conv_layers_{layer}_conv.conv.{param}");
                Some(if param == "weight" {
                    format!("{mapped}{TO_TRANSPOSE}")
                } else {
                    mapped
                })
            }
            _ => None,
        }
    } else if let Some(rest) = key.strip_prefix(&format!("{ENC}.transformer.layers.")) {
        match rest.splitn(2, '.').collect::<Vec<_>>()[..] {
            [layer, path] => Some(format!("encoder.transformer_layers.{layer}.{}", ff(path))),
            _ => None,
        }
    } else if let Some(rest) = key.strip_prefix(&format!("{ENC}.transformer.norm.")) {
        Some(format!("encoder.transformer_norm.{rest}"))
    } else if let Some(rest) = key.strip_prefix(&format!("{ADAPTER}.audio_language_projection.")) {
        match rest.splitn(2, '.').collect::<Vec<_>>()[..] {
            [index, param] => Some(format!("encoder.audio_language_projection_{index}.{param}")),
            _ => None,
        }
    } else if let Some(rest) = key.strip_prefix("layers.") {
        match rest.splitn(2, '.').collect::<Vec<_>>()[..] {
            [layer, path] => {
                let path = ff(path)
                    .replace("ada_rms_norm_t_cond.0.", "ada_rms_norm_t_cond.ada_down.")
                    .replace("ada_rms_norm_t_cond.2.", "ada_rms_norm_t_cond.ada_up.");
                Some(format!("decoder.layers.{layer}.{path}"))
            }
            _ => None,
        }
    } else {
        None
    };
    Some(mapped.unwrap_or_else(|| key.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `numAudioTokens` et `numDelayTokens` du Swift : 480 ms → 7 680
    /// échantillons → 48 trames → 6 jetons.
    #[test]
    fn audio_and_delay_tokens_are_counted_like_the_swift() {
        assert_eq!(num_delay_tokens(480), 6);
        assert_eq!(num_delay_tokens(0), 0);
        assert_eq!(num_delay_tokens(80), 1);
        assert_eq!(num_audio_tokens(0), 0);
        assert_eq!(num_audio_tokens(1_280), 1);
        assert_eq!(num_audio_tokens(1_281), 1, "⌈1281/160 − 1⌉ = 8 trames");
        assert_eq!(num_audio_tokens(1_441), 2, "⌈1441/160 − 1⌉ = 9 trames");
        assert_eq!(num_audio_tokens(1_440), 2);
        assert_eq!(num_audio_tokens(159), 0);
        assert_eq!(num_audio_tokens(161), 1);
    }

    #[test]
    fn keys_are_renamed_like_the_swift_sanitize() {
        let enc = "mm_streams_embeddings.embedding_module.whisper_encoder";
        let cases = [
            (
                "mm_streams_embeddings.embedding_module.tok_embeddings.weight".to_string(),
                "decoder.tok_embeddings.weight",
            ),
            ("norm.weight".into(), "decoder.norm.weight"),
            (
                format!("{enc}.conv_layers.0.conv.weight"),
                "encoder.conv_layers_0_conv.conv.weight#transposer",
            ),
            (
                format!("{enc}.conv_layers.1.conv.bias"),
                "encoder.conv_layers_1_conv.conv.bias",
            ),
            (
                format!("{enc}.conv_layers.1"),
                "mm_streams_embeddings.embedding_module.whisper_encoder.conv_layers.1",
            ),
            (
                format!("{enc}.transformer.layers.3.feed_forward.w2.bias"),
                "encoder.transformer_layers.3.feed_forward_w2.bias",
            ),
            (
                format!("{enc}.transformer.layers.3.attention.wq.weight"),
                "encoder.transformer_layers.3.attention.wq.weight",
            ),
            (
                format!("{enc}.transformer.norm.weight"),
                "encoder.transformer_norm.weight",
            ),
            (
                "mm_streams_embeddings.embedding_module.audio_language_projection.2.weight".into(),
                "encoder.audio_language_projection_2.weight",
            ),
            (
                "layers.7.ada_rms_norm_t_cond.0.weight".into(),
                "decoder.layers.7.ada_rms_norm_t_cond.ada_down.weight",
            ),
            (
                "layers.7.ada_rms_norm_t_cond.2.weight".into(),
                "decoder.layers.7.ada_rms_norm_t_cond.ada_up.weight",
            ),
            (
                "layers.7.feed_forward.w1.scales".into(),
                "decoder.layers.7.feed_forward_w1.scales",
            ),
            // Le paquet du catalogue : déjà au format du graphe.
            (
                "decoder.layers.0.attention.wq.weight".into(),
                "decoder.layers.0.attention.wq.weight",
            ),
            (
                "encoder.conv_layers_0_conv.conv.weight".into(),
                "encoder.conv_layers_0_conv.conv.weight",
            ),
        ];
        for (key, expected) in cases {
            assert_eq!(remap_key(&key).as_deref(), Some(expected), "{key}");
        }
    }
}
