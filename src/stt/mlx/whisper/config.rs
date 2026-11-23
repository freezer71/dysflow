//! `config.json` de Whisper, comme `WhisperConfig` du Swift : les noms de
//! transformers (`d_model`…) ou ceux d'OpenAI / mlx-whisper (`n_audio_state`…),
//! avec les mêmes valeurs par défaut (celles de tiny), et la quantification
//! (`WhisperQuantizedModelConfig`). Les tailles sont bornées : un fichier
//! absurde est une erreur qui le nomme, jamais une panique.

use anyhow::{anyhow, ensure, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use super::tokenizer::SUPPORTED_VOCABULARY_SIZES;
use crate::stt::mlx::weights::Quantization;

/// Les dimensions du graphe (`WhisperConfig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub vocab_size: i32,
    pub num_mel_bins: i32,
    pub d_model: i32,
    pub encoder_layers: i32,
    pub encoder_attention_heads: i32,
    pub encoder_ffn_dim: i32,
    pub max_source_positions: i32,
    pub decoder_layers: i32,
    pub decoder_attention_heads: i32,
    pub decoder_ffn_dim: i32,
    pub max_target_positions: i32,
    /// `quantization` (`group_size`, `bits`) ; absente ou illisible : aucune,
    /// comme le `try?` du Swift.
    pub quantization: Option<Quantization>,
}

/// Une dimension au plus (couches, positions, largeur des MLP) : bien
/// au-delà de large-v3 (32 couches, 1 500 positions, 5 120).
const MAX_DIM: i64 = 1 << 16;
/// La largeur au plus : plus de trois fois celle de large-v3 (1 280). Les
/// positions sinusoïdales de l'encodeur (1 500 × `d_model`) sont calculées
/// avant que la forme des poids soit vérifiée : 4 096 les tient à ~50 Mo.
pub const MAX_D_MODEL: i64 = 4096;
/// La longueur de l'invite : `<|startoftranscript|> <|langue|>
/// <|transcribe|> <|notimestamps|>` (`Tokenizer::prompt`).
pub const PROMPT_LEN: i64 = 4;
/// Les positions du décodeur au moins : l'invite, puis un jeton.
pub const MIN_TARGET_POSITIONS: i64 = PROMPT_LEN + 2;
/// `config.json` plus gros que cela : refusé sans être lu.
const MAX_CONFIG_BYTES: u64 = 1 << 20;

impl Config {
    /// Lit `config.json` dans `dir`.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("config.json");
        let reading = || crate::tr!(en = "reading {}", fr = "lecture de {}", path.display());
        let size = std::fs::metadata(&path).with_context(reading)?.len();
        ensure!(
            size <= MAX_CONFIG_BYTES,
            crate::tr!(
                en = "Whisper configuration {}: {size} bytes, more than {MAX_CONFIG_BYTES}",
                fr = "configuration de Whisper {} : {size} octets, plus de {MAX_CONFIG_BYTES}",
                path.display()
            )
        );
        let mut text = String::new();
        std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_CONFIG_BYTES).read_to_string(&mut text))
            .with_context(reading)?;
        Self::parse(&text).with_context(|| {
            crate::tr!(
                en = "Whisper configuration {}",
                fr = "configuration de Whisper {}",
                path.display()
            )
        })
    }

    /// `init(from:)` sur le texte de `config.json`.
    pub fn parse(text: &str) -> Result<Self> {
        let json: Value = serde_json::from_str(text)?;
        let int = |key: &str| -> Result<Option<i64>> {
            match json.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v.as_i64().map(Some).ok_or_else(|| {
                    anyhow!(crate::tr!(
                        en = "{key} is not an integer",
                        fr = "{key} n'est pas un entier",
                    ))
                }),
            }
        };
        let first = |keys: &[&str], default: i64| -> Result<i64> {
            for key in keys {
                if let Some(v) = int(key)? {
                    return Ok(v);
                }
            }
            Ok(default)
        };
        let vocab_size = first(&["vocab_size", "n_vocab"], 51865)?;
        let num_mel_bins = first(&["num_mel_bins", "n_mels"], 80)?;
        let d_model = first(&["d_model", "n_audio_state", "n_text_state"], 384)?;
        let encoder_layers = first(&["encoder_layers", "n_audio_layer"], 4)?;
        let encoder_attention_heads = first(&["encoder_attention_heads", "n_audio_head"], 6)?;
        let encoder_ffn_dim = first(&["encoder_ffn_dim"], 4 * d_model.clamp(0, MAX_DIM))?;
        let max_source_positions = first(&["max_source_positions", "n_audio_ctx"], 1500)?;
        let decoder_layers = first(&["decoder_layers", "n_text_layer"], 4)?;
        let decoder_attention_heads = first(&["decoder_attention_heads", "n_text_head"], 6)?;
        let decoder_ffn_dim = first(&["decoder_ffn_dim"], 4 * d_model.clamp(0, MAX_DIM))?;
        let max_target_positions = first(&["max_target_positions", "n_text_ctx"], 448)?;

        let dim = |name: &str, v: i64| -> Result<i32> {
            ensure!(
                (1..=MAX_DIM).contains(&v),
                crate::tr!(
                    en = "{name} = {v}, outside 1…{MAX_DIM}",
                    fr = "{name} = {v}, hors de 1…{MAX_DIM}",
                )
            );
            Ok(v as i32)
        };
        ensure!(
            d_model <= MAX_D_MODEL,
            crate::tr!(
                en = "d_model = {d_model}, more than {MAX_D_MODEL}",
                fr = "d_model = {d_model}, plus de {MAX_D_MODEL}",
            )
        );
        ensure!(
            max_target_positions >= MIN_TARGET_POSITIONS,
            crate::tr!(
                en = "max_target_positions = {max_target_positions}, fewer than the prompt ({PROMPT_LEN} tokens) and one token ({MIN_TARGET_POSITIONS})",
                fr = "max_target_positions = {max_target_positions}, moins que l'invite ({PROMPT_LEN} jetons) et un jeton ({MIN_TARGET_POSITIONS})",
            )
        );
        let config = Self {
            vocab_size: vocab_size.try_into().unwrap_or(0),
            num_mel_bins: dim("num_mel_bins", num_mel_bins)?,
            d_model: dim("d_model", d_model)?,
            encoder_layers: dim("encoder_layers", encoder_layers)?,
            encoder_attention_heads: dim("encoder_attention_heads", encoder_attention_heads)?,
            encoder_ffn_dim: dim("encoder_ffn_dim", encoder_ffn_dim)?,
            max_source_positions: dim("max_source_positions", max_source_positions)?,
            decoder_layers: dim("decoder_layers", decoder_layers)?,
            decoder_attention_heads: dim("decoder_attention_heads", decoder_attention_heads)?,
            decoder_ffn_dim: dim("decoder_ffn_dim", decoder_ffn_dim)?,
            max_target_positions: dim("max_target_positions", max_target_positions)?,
            quantization: json
                .get("quantization")
                .and_then(|q| serde_json::from_value(q.clone()).ok()),
        };
        ensure!(
            SUPPORTED_VOCABULARY_SIZES.contains(&config.vocab_size),
            crate::tr!(
                en = "Whisper: a vocabulary of {vocab_size} tokens is not supported (multilingual only: 51865 or 51866)",
                fr = "Whisper : vocabulaire de {vocab_size} jetons non géré (multilingue seulement : 51865 ou 51866)",
            )
        );
        for (name, heads) in [
            ("encoder_attention_heads", config.encoder_attention_heads),
            ("decoder_attention_heads", config.decoder_attention_heads),
        ] {
            ensure!(
                config.d_model % heads == 0,
                crate::tr!(
                    en = "{name} = {heads} does not divide d_model = {}",
                    fr = "{name} = {heads} ne divise pas d_model = {}",
                    config.d_model
                )
            );
        }
        ensure!(
            config.d_model % 2 == 0,
            crate::tr!(
                en = "d_model = {} is odd: no sinusoidal positions",
                fr = "d_model = {} est impair : pas de positions sinusoïdales",
                config.d_model
            )
        );
        Ok(config)
    }

    /// Le plafond de jetons par défaut (`defaultGenerationParameters`).
    pub fn max_tokens(&self) -> i32 {
        self.max_target_positions - 16
    }
}

/// Ce que `generation_config.json` peut changer (`WhisperGenerationConfig`) :
/// les jetons interdits. Le fichier est facultatif et lu comme le `try?` du
/// Swift : absent ou illisible, ce sont les ids déduits de `n_vocab`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct GenerationConfig {
    pub suppress_tokens: Option<Vec<i64>>,
    pub begin_suppress_tokens: Option<Vec<i64>>,
}

impl GenerationConfig {
    /// Plus que cela : le fichier est tenu pour illisible.
    const MAX_BYTES: u64 = 1 << 20;

    pub fn load(dir: &Path) -> Option<Self> {
        let path = dir.join("generation_config.json");
        let size = std::fs::metadata(&path).ok()?.len();
        if size > Self::MAX_BYTES {
            return None;
        }
        serde_json::from_slice(&std::fs::read(&path).ok()?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mlx_whisper_names_are_read() {
        let config = Config::parse(
            r#"{"n_mels": 128, "n_audio_ctx": 1500, "n_audio_state": 1280, "n_audio_head": 20,
                "n_audio_layer": 32, "n_vocab": 51866, "n_text_ctx": 448, "n_text_state": 1280,
                "n_text_head": 20, "n_text_layer": 4,
                "quantization": {"group_size": 64, "bits": 4}, "model_type": "whisper"}"#,
        )
        .unwrap();
        assert_eq!(config.num_mel_bins, 128);
        assert_eq!(config.d_model, 1280);
        assert_eq!(config.encoder_layers, 32);
        assert_eq!(config.decoder_layers, 4);
        assert_eq!(config.encoder_ffn_dim, 5120);
        assert_eq!(config.decoder_attention_heads, 20);
        assert_eq!(config.max_tokens(), 432);
        assert_eq!(
            config.quantization,
            Some(Quantization {
                group_size: 64,
                bits: 4
            })
        );
    }

    #[test]
    fn missing_fields_take_the_swift_defaults() {
        let config = Config::parse("{}").unwrap();
        assert_eq!(
            (
                config.vocab_size,
                config.num_mel_bins,
                config.d_model,
                config.encoder_layers
            ),
            (51865, 80, 384, 4)
        );
        assert_eq!(config.encoder_ffn_dim, 1536);
        assert_eq!(config.max_source_positions, 1500);
        assert_eq!(config.max_target_positions, 448);
        assert_eq!(config.quantization, None);
        let bad_quantization = Config::parse(r#"{"quantization": {"bits": "4"}}"#).unwrap();
        assert_eq!(
            bad_quantization.quantization, None,
            "comme le try? du Swift"
        );
    }

    #[test]
    fn an_english_only_vocabulary_is_refused() {
        let err = Config::parse(r#"{"n_vocab": 51864}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("51864") && err.contains("51865"), "{err}");
    }

    #[test]
    fn absurd_sizes_are_refused_by_name() {
        for (text, name) in [
            (r#"{"n_audio_state": 0}"#, "d_model"),
            (r#"{"n_audio_layer": 100000}"#, "encoder_layers"),
            (r#"{"n_audio_head": 7}"#, "encoder_attention_heads"),
            (r#"{"n_mels": -3}"#, "num_mel_bins"),
            (r#"{"n_text_ctx": "beaucoup"}"#, "n_text_ctx"),
            (r#"{"n_audio_state": 8192, "n_audio_head": 8}"#, "d_model"),
            (r#"{"n_text_ctx": 5}"#, "max_target_positions"),
        ] {
            let err = format!("{:#}", Config::parse(text).unwrap_err());
            assert!(err.contains(name), "{text} : {err}");
        }
    }
}
