//! `config.json` de Qwen3-ASR, comme `Qwen3ASRConfig.swift` : les mêmes clés
//! et les mêmes valeurs par défaut, lues sous `thinker_config` (le format de
//! Hugging Face) ou à la racine, la quantification affine globale de la
//! racine. Les tailles sont bornées : un fichier absurde est une erreur qui
//! le nomme, jamais une panique. Ce que le Swift ignore (`hidden_act`,
//! `activation_function` : il calcule toujours `silu` et `gelu`) ou ne porte
//! pas (l'aligneur forcé) est refusé par son nom ; `rope_scaling` est
//! ignoré comme dans le Swift (un RoPE ordinaire).

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use crate::stt::mlx::weights::Quantization;

pub const CONFIG_FILE: &str = "config.json";
/// Le modèle dans les messages.
pub const NAME: &str = "Qwen3-ASR";
/// `config.json` plus gros que cela : refusé sans être lu (ceux du
/// catalogue font 7 ko).
const MAX_CONFIG_BYTES: u64 = 1 << 20;
/// Une dimension au plus (couches, têtes, largeurs, positions).
const MAX_DIM: i64 = 1 << 18;
/// Le vocabulaire au plus (151 936 ici).
const MAX_VOCAB: i64 = 1 << 20;
/// Les bandes du mel au plus (128 ici).
const MAX_MEL_BINS: i64 = 1 << 10;

/// `Qwen3AudioEncoderConfig` : ce que l'encodeur lit.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioConfig {
    pub num_mel_bins: i32,
    pub encoder_layers: i32,
    pub encoder_attention_heads: i32,
    pub encoder_ffn_dim: i32,
    pub d_model: i32,
    pub max_source_positions: i32,
    pub n_window: i32,
    pub output_dim: i32,
    pub n_window_infer: i32,
    pub downsample_hidden_size: i32,
}

/// `Qwen3TextConfig` : ce que le décodeur lit.
#[derive(Debug, Clone, PartialEq)]
pub struct TextConfig {
    pub vocab_size: i32,
    pub hidden_size: i32,
    pub intermediate_size: i32,
    pub num_hidden_layers: i32,
    pub num_attention_heads: i32,
    pub num_key_value_heads: i32,
    pub head_dim: i32,
    pub rms_norm_eps: f32,
    pub tie_word_embeddings: bool,
    pub rope_theta: f32,
}

/// `Qwen3ASRConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub audio: AudioConfig,
    pub text: TextConfig,
    pub audio_token_id: i32,
    pub audio_start_token_id: i32,
    pub audio_end_token_id: i32,
    pub support_languages: Vec<String>,
    pub quantization: Option<Quantization>,
}

#[derive(Deserialize)]
#[serde(default)]
struct RawAudio {
    num_mel_bins: i64,
    encoder_layers: i64,
    encoder_attention_heads: i64,
    encoder_ffn_dim: i64,
    d_model: i64,
    activation_function: String,
    max_source_positions: i64,
    n_window: i64,
    output_dim: i64,
    n_window_infer: i64,
    downsample_hidden_size: i64,
}

impl Default for RawAudio {
    fn default() -> Self {
        Self {
            num_mel_bins: 128,
            encoder_layers: 24,
            encoder_attention_heads: 16,
            encoder_ffn_dim: 4096,
            d_model: 1024,
            activation_function: "gelu".into(),
            max_source_positions: 1500,
            n_window: 50,
            output_dim: 2048,
            n_window_infer: 800,
            downsample_hidden_size: 480,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawText {
    vocab_size: i64,
    hidden_size: i64,
    intermediate_size: i64,
    num_hidden_layers: i64,
    num_attention_heads: i64,
    num_key_value_heads: i64,
    head_dim: i64,
    hidden_act: String,
    rms_norm_eps: f32,
    tie_word_embeddings: bool,
    rope_theta: f32,
}

impl Default for RawText {
    fn default() -> Self {
        Self {
            vocab_size: 151_936,
            hidden_size: 1024,
            intermediate_size: 3072,
            num_hidden_layers: 28,
            num_attention_heads: 16,
            num_key_value_heads: 8,
            head_dim: 128,
            hidden_act: "silu".into(),
            rms_norm_eps: 1e-6,
            tie_word_embeddings: true,
            rope_theta: 1_000_000.0,
        }
    }
}

/// Les champs lus sous `thinker_config` ou à la racine.
#[derive(Deserialize)]
#[serde(default)]
struct RawModel {
    audio_config: Option<RawAudio>,
    text_config: Option<RawText>,
    audio_token_id: i64,
    audio_start_token_id: i64,
    audio_end_token_id: i64,
    classify_num: Option<i64>,
    model_type: Option<String>,
}

impl Default for RawModel {
    fn default() -> Self {
        Self {
            audio_config: None,
            text_config: None,
            audio_token_id: 151_676,
            audio_start_token_id: 151_669,
            audio_end_token_id: 151_670,
            classify_num: None,
            model_type: None,
        }
    }
}

fn unreadable(path: &Path, err: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "unreadable {NAME} configuration ({}): {err}",
        fr = "configuration de {NAME} illisible ({}) : {err}",
        path.display(),
    ))
}

impl Config {
    /// `config.json` de `dir`, borné en taille, puis vérifié.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let file = std::fs::File::open(&path).map_err(|err| unreadable(&path, &err))?;
        let mut text = String::new();
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|err| unreadable(&path, &err))?;
        ensure!(
            text.len() as u64 <= MAX_CONFIG_BYTES,
            crate::tr!(
                en = "{NAME} configuration ({}) larger than {MAX_CONFIG_BYTES} bytes",
                fr = "configuration de {NAME} ({}) de plus de {MAX_CONFIG_BYTES} octets",
                path.display(),
            )
        );
        let json: Value = serde_json::from_str(&text).map_err(|err| unreadable(&path, &err))?;
        Self::from_json(&json).with_context(|| path.display().to_string())
    }

    pub fn from_json(json: &Value) -> Result<Self> {
        ensure!(
            json.is_object(),
            crate::tr!(
                "config.json is not an object",
                "config.json n'est pas un objet"
            )
        );
        let model_type = json.get("model_type").and_then(Value::as_str);
        // `thinker_config` d'abord, s'il est un objet (`nestedContainer`),
        // sinon la racine.
        let inner = json
            .get("thinker_config")
            .filter(|t| t.is_object())
            .unwrap_or(json);
        let raw = RawModel::deserialize(inner).context("config.json")?;
        let aligner = model_type == Some("qwen3_forced_aligner")
            || raw.model_type.as_deref() == Some("qwen3_forced_aligner")
            || raw.classify_num.is_some();
        ensure!(
            !aligner,
            crate::tr!(
                en = "{NAME}: the forced aligner (qwen3_forced_aligner) is not ported to Rust",
                fr = "{NAME} : l'aligneur forcé (qwen3_forced_aligner) n'est pas porté en Rust",
            )
        );
        let support_languages = match json.get("support_languages") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => Vec::<String>::deserialize(v).context("support_languages")?,
        };
        let audio = raw.audio_config.unwrap_or_default();
        let text = raw.text_config.unwrap_or_default();
        for (name, value, expected) in [
            (
                "audio_config.activation_function",
                &audio.activation_function,
                "gelu",
            ),
            ("text_config.hidden_act", &text.hidden_act, "silu"),
        ] {
            ensure!(
                value == expected,
                crate::tr!(
                    en = "{NAME}: {name} = {value:?} not ported to Rust (only {expected:?}, which the Swift always computes)",
                    fr = "{NAME} : {name} = {value:?} non porté en Rust (seulement {expected:?}, que le Swift calcule toujours)",
                )
            );
        }
        let text = TextConfig {
            vocab_size: bounded("text_config.vocab_size", text.vocab_size, 1, MAX_VOCAB)?,
            hidden_size: dim("text_config.hidden_size", text.hidden_size)?,
            intermediate_size: dim("text_config.intermediate_size", text.intermediate_size)?,
            num_hidden_layers: dim("text_config.num_hidden_layers", text.num_hidden_layers)?,
            num_attention_heads: dim("text_config.num_attention_heads", text.num_attention_heads)?,
            num_key_value_heads: dim("text_config.num_key_value_heads", text.num_key_value_heads)?,
            head_dim: dim("text_config.head_dim", text.head_dim)?,
            rms_norm_eps: text.rms_norm_eps,
            tie_word_embeddings: text.tie_word_embeddings,
            rope_theta: text.rope_theta,
        };
        let config = Self {
            audio: AudioConfig {
                num_mel_bins: bounded(
                    "audio_config.num_mel_bins",
                    audio.num_mel_bins,
                    1,
                    MAX_MEL_BINS,
                )?,
                encoder_layers: dim("audio_config.encoder_layers", audio.encoder_layers)?,
                encoder_attention_heads: dim(
                    "audio_config.encoder_attention_heads",
                    audio.encoder_attention_heads,
                )?,
                encoder_ffn_dim: dim("audio_config.encoder_ffn_dim", audio.encoder_ffn_dim)?,
                d_model: dim("audio_config.d_model", audio.d_model)?,
                max_source_positions: dim(
                    "audio_config.max_source_positions",
                    audio.max_source_positions,
                )?,
                n_window: dim("audio_config.n_window", audio.n_window)?,
                output_dim: dim("audio_config.output_dim", audio.output_dim)?,
                n_window_infer: dim("audio_config.n_window_infer", audio.n_window_infer)?,
                downsample_hidden_size: dim(
                    "audio_config.downsample_hidden_size",
                    audio.downsample_hidden_size,
                )?,
            },
            audio_token_id: token(&text, "audio_token_id", raw.audio_token_id)?,
            audio_start_token_id: token(&text, "audio_start_token_id", raw.audio_start_token_id)?,
            audio_end_token_id: token(&text, "audio_end_token_id", raw.audio_end_token_id)?,
            text,
            support_languages,
            quantization: quantization(json)?,
        };
        config.check()?;
        Ok(config)
    }

    /// Ce que le graphe suppose : sinon un `precondition` du Swift, une
    /// forme impossible ou une tranche hors de la table des positions.
    fn check(&self) -> Result<()> {
        let (a, t) = (&self.audio, &self.text);
        ensure!(
            a.d_model % a.encoder_attention_heads == 0
                && t.num_attention_heads % t.num_key_value_heads == 0
                && t.head_dim % 2 == 0
                && a.d_model % 2 == 0
                && a.d_model >= 4
                && t.rms_norm_eps.is_finite()
                && t.rms_norm_eps > 0.0
                && t.rope_theta.is_finite()
                && t.rope_theta > 0.0,
            crate::tr!(
                en = "inconsistent {NAME} configuration (heads, widths, rms_norm_eps or rope_theta)",
                fr = "configuration de {NAME} incohérente (têtes, largeurs, rms_norm_eps ou rope_theta)",
            )
        );
        // Une tranche de `2 · n_window` trames donne `tokens_per_chunk`
        // positions : la table des positions doit les avoir.
        let tokens = self.tokens_per_chunk();
        ensure!(
            tokens <= a.max_source_positions,
            crate::tr!(
                en = "{NAME}: max_source_positions = {}, fewer than the {tokens} positions of a chunk",
                fr = "{NAME} : max_source_positions = {}, moins que les {tokens} positions d'une tranche",
                a.max_source_positions,
            )
        );
        Ok(())
    }

    /// Les positions d'une tranche pleine de l'encodeur (13 pour 100 trames).
    pub fn tokens_per_chunk(&self) -> i32 {
        super::audio::length_after_conv_stem(self.audio.n_window.saturating_mul(2))
    }

    /// Les bandes après les trois convolutions de pas 2 (`freqAfterConv`).
    pub fn freq_after_conv(&self) -> i32 {
        super::audio::length_after_conv_stem(self.audio.num_mel_bins)
    }
}

/// Une dimension dans 1…`MAX_DIM`.
fn dim(name: &str, v: i64) -> Result<i32> {
    bounded(name, v, 1, MAX_DIM)
}

fn bounded(name: &str, v: i64, low: i64, high: i64) -> Result<i32> {
    ensure!(
        (low..=high).contains(&v),
        crate::tr!(
            en = "{NAME} configuration: {name} = {v}, outside {low}…{high}",
            fr = "configuration de {NAME} : {name} = {v}, hors de {low}…{high}",
        )
    );
    Ok(v as i32)
}

/// Un id de jeton du vocabulaire.
fn token(text: &TextConfig, name: &str, v: i64) -> Result<i32> {
    bounded(name, v, 0, i64::from(text.vocab_size) - 1)
}

/// `quantization` avec `group_size` et `bits` ; absente : aucune. Seule la
/// quantification affine globale est portée : un mode autre qu'affine, ou
/// un réglage par couche (une autre clé que celles que le Swift lit ou
/// ignore), est refusé par son nom.
fn quantization(json: &Value) -> Result<Option<Quantization>> {
    let Some(q) = json.get("quantization").filter(|q| !q.is_null()) else {
        return Ok(None);
    };
    let Some(object) = q.as_object() else {
        bail!(crate::tr!(
            en = "{NAME}: quantization is not an object",
            fr = "{NAME} : quantization n'est pas un objet",
        ));
    };
    const KNOWN: [&str; 6] = [
        "group_size",
        "bits",
        "mode",
        "quant_method",
        "linear_class",
        "quantization_mode",
    ];
    if let Some(layer) = object.keys().find(|k| !KNOWN.contains(&k.as_str())) {
        bail!(crate::tr!(
            en = "{NAME}: per-layer quantization ({layer}) not ported to Rust",
            fr = "{NAME} : quantification par couche ({layer}) non portée en Rust",
        ));
    }
    if let Some(mode) = object.get("mode").and_then(Value::as_str) {
        ensure!(
            mode == "affine",
            crate::tr!(
                en = "{NAME}: quantization mode {mode:?} not ported to Rust (affine only)",
                fr = "{NAME} : quantification {mode:?} non portée en Rust (affine seulement)",
            )
        );
    }
    let quantization = Quantization::deserialize(q).context("quantization")?;
    ensure!(
        quantization.bits > 0 && quantization.group_size > 0,
        crate::tr!(
            en = "{NAME}: quantization of {} bits by groups of {}",
            fr = "{NAME} : quantification de {} bits par groupes de {}",
            quantization.bits,
            quantization.group_size,
        )
    );
    Ok(Some(quantization))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn error(json: Value) -> String {
        format!("{:#}", Config::from_json(&json).unwrap_err())
    }

    #[test]
    fn the_defaults_are_those_of_the_swift() {
        let c = Config::from_json(&json!({})).unwrap();
        assert_eq!(
            c.audio,
            AudioConfig {
                num_mel_bins: 128,
                encoder_layers: 24,
                encoder_attention_heads: 16,
                encoder_ffn_dim: 4096,
                d_model: 1024,
                max_source_positions: 1500,
                n_window: 50,
                output_dim: 2048,
                n_window_infer: 800,
                downsample_hidden_size: 480,
            }
        );
        assert_eq!(
            c.text,
            TextConfig {
                vocab_size: 151_936,
                hidden_size: 1024,
                intermediate_size: 3072,
                num_hidden_layers: 28,
                num_attention_heads: 16,
                num_key_value_heads: 8,
                head_dim: 128,
                rms_norm_eps: 1e-6,
                tie_word_embeddings: true,
                rope_theta: 1_000_000.0,
            }
        );
        assert_eq!(
            (
                c.audio_token_id,
                c.audio_start_token_id,
                c.audio_end_token_id
            ),
            (151_676, 151_669, 151_670)
        );
        assert!(c.support_languages.is_empty());
        assert_eq!(c.quantization, None);
        assert_eq!((c.tokens_per_chunk(), c.freq_after_conv()), (13, 16));
    }

    /// Sous `thinker_config` (Hugging Face) ou à la racine ; les langues et
    /// la quantification à la racine.
    #[test]
    fn the_model_is_read_under_the_thinker_or_at_the_root() {
        let inner = json!({"audio_config": {"d_model": 896}, "text_config": {"hidden_size": 2048}, "audio_token_id": 7});
        let thinker = json!({
            "thinker_config": inner,
            "support_languages": ["French", "English"],
            "quantization": {"group_size": 64, "bits": 8, "mode": "affine"},
            "audio_config": {"d_model": 512},
        });
        for json in [thinker, inner] {
            let c = Config::from_json(&json).unwrap();
            assert_eq!(
                (c.audio.d_model, c.text.hidden_size, c.audio_token_id),
                (896, 2048, 7)
            );
        }
        let c = Config::from_json(&json!({
            "thinker_config": {},
            "support_languages": ["French"],
            "quantization": {"group_size": 64, "bits": 4}
        }))
        .unwrap();
        assert_eq!(c.support_languages, ["French"]);
        assert_eq!(
            c.quantization,
            Some(Quantization {
                group_size: 64,
                bits: 4
            })
        );
        // `thinker_config` qui n'est pas un objet : la racine.
        let c = Config::from_json(&json!({"thinker_config": 3, "audio_token_id": 9})).unwrap();
        assert_eq!(c.audio_token_id, 9);
    }

    /// Un cas par ligne : le message nomme ce qui est refusé.
    #[test]
    fn absurd_or_unported_configurations_are_refused_by_name() {
        for (json, named) in [
            (json!([]), "objet"),
            (
                json!({"model_type": "qwen3_forced_aligner"}),
                "qwen3_forced_aligner",
            ),
            (
                json!({"thinker_config": {"classify_num": 5000}}),
                "qwen3_forced_aligner",
            ),
            (
                json!({"thinker_config": {"model_type": "qwen3_forced_aligner"}}),
                "qwen3_forced_aligner",
            ),
            (
                json!({"audio_config": {"activation_function": "relu"}}),
                "activation_function",
            ),
            (json!({"text_config": {"hidden_act": "gelu"}}), "hidden_act"),
            (json!({"text_config": {"vocab_size": 0}}), "vocab_size"),
            (
                json!({"text_config": {"vocab_size": 2_000_000_000_000i64}}),
                "vocab_size",
            ),
            (json!({"text_config": {"hidden_size": -1}}), "hidden_size"),
            (
                json!({"text_config": {"num_key_value_heads": 0}}),
                "num_key_value_heads",
            ),
            (
                json!({"text_config": {"num_key_value_heads": 3}}),
                "incohérente",
            ),
            (json!({"text_config": {"head_dim": 127}}), "incohérente"),
            (json!({"text_config": {"rope_theta": 0.0}}), "incohérente"),
            (
                json!({"text_config": {"rms_norm_eps": -1.0}}),
                "incohérente",
            ),
            (
                json!({"audio_config": {"encoder_attention_heads": 7}}),
                "incohérente",
            ),
            (
                json!({"audio_config": {"num_mel_bins": 100_000}}),
                "num_mel_bins",
            ),
            (json!({"audio_config": {"n_window": 0}}), "n_window"),
            (
                json!({"audio_config": {"max_source_positions": 12}}),
                "max_source_positions",
            ),
            (json!({"audio_token_id": 151_936}), "audio_token_id"),
            (json!({"audio_start_token_id": -1}), "audio_start_token_id"),
            (json!({"support_languages": "French"}), "support_languages"),
            (json!({"quantization": 4}), "quantization"),
            (
                json!({"quantization": {"group_size": 64, "bits": 4, "mode": "mxfp4"}}),
                "mxfp4",
            ),
            (
                json!({"quantization": {"group_size": 64, "bits": 4, "model.layers.0.mlp.up_proj": {"bits": 8}}}),
                "model.layers.0.mlp.up_proj",
            ),
            (json!({"quantization": {"group_size": 0, "bits": 4}}), "0"),
            (
                json!({"text_config": {"vocab_size": "grand"}}),
                "config.json",
            ),
        ] {
            let err = crate::i18n::with_ui_lang(crate::i18n::UiLang::Fr, || error(json.clone()));
            assert!(err.contains(named), "{json} : {err}");
        }
    }

    #[test]
    fn a_config_too_large_or_unreadable_is_refused_by_name() {
        let dir = crate::paths::test_scratch("qwen3-asr-config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(CONFIG_FILE), "{err}");
        std::fs::write(dir.join(CONFIG_FILE), b"{").unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(CONFIG_FILE), "{err}");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join(CONFIG_FILE))
            .unwrap();
        file.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(&MAX_CONFIG_BYTES.to_string()), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
