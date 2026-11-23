//! `config.json` de Canary (export NeMo `EncDecMultiTaskModel`), comme
//! `CanaryConfig.swift` : les mêmes clés et les mêmes valeurs par défaut,
//! `transf_decoder` avant `decoder`, le vocabulaire SentencePiece en base64
//! (`tokenizer.model_base64`) et la quantification (`{"bits": 8}` seul : des
//! groupes de 64). Les tailles sont bornées : un fichier absurde est une
//! erreur qui le nomme, jamais une panique.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use crate::stt::mlx::parakeet::config::{ConformerConfig, PreprocessConfig};
use crate::stt::mlx::weights::Quantization;

pub const CONFIG_FILE: &str = "config.json";
/// `config.json` plus gros que cela : refusé sans être lu (celui de
/// canary-1b-v2 fait 673 ko, vocabulaire compris).
const MAX_CONFIG_BYTES: u64 = 16 << 20;
/// Une dimension au plus (couches, têtes, vocabulaire, largeur des MLP).
const MAX_DIM: i64 = 1 << 17;
/// La largeur au plus : la table des positions du décodeur (1 024 × largeur)
/// et celle de l'encodeur sont calculées avant que la forme des poids soit
/// vérifiée.
const MAX_D_MODEL: i64 = 4096;
/// Les positions de l'encodeur au plus (`pos_emb_max_len`, 5 000 dans
/// canary-1b-v2).
const MAX_ENCODER_POSITIONS: i64 = 1 << 16;
/// Le groupe d'une quantification qui ne le donne pas
/// (`CanaryFlatQuantization`).
const DEFAULT_GROUP_SIZE: i32 = 64;

/// Les langues de Canary 1b-v2 (`CanaryConfig.defaultSupportedLanguages`).
const DEFAULT_LANGUAGES: [&str; 25] = [
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

/// `CanaryPreprocessConfig`, chaque clé à son défaut.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Preprocess {
    sample_rate: i64,
    normalize: String,
    features: i64,
    n_fft: i64,
    window_size: f32,
    window_stride: f32,
    window: String,
    dither: f32,
    pad_to: i64,
    pad_value: f32,
    preemph: f32,
}

impl Default for Preprocess {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            normalize: "per_feature".into(),
            features: 128,
            n_fft: 512,
            window_size: 0.025,
            window_stride: 0.01,
            window: "hann".into(),
            dither: 0.0,
            pad_to: 0,
            pad_value: 0.0,
            preemph: 0.97,
        }
    }
}

/// `CanaryEncoderConfig`, chaque clé à son défaut.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Encoder {
    feat_in: i64,
    n_layers: i64,
    d_model: i64,
    n_heads: i64,
    ff_expansion_factor: i64,
    subsampling_factor: i64,
    self_attention_model: String,
    subsampling: String,
    conv_kernel_size: i64,
    subsampling_conv_channels: i64,
    pos_emb_max_len: i64,
    causal_downsampling: bool,
    use_bias: bool,
    xscaling: bool,
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            feat_in: 128,
            n_layers: 32,
            d_model: 1024,
            n_heads: 8,
            ff_expansion_factor: 4,
            subsampling_factor: 8,
            self_attention_model: "rel_pos".into(),
            subsampling: "dw_striding".into(),
            conv_kernel_size: 9,
            subsampling_conv_channels: 256,
            pos_emb_max_len: 5000,
            causal_downsampling: false,
            use_bias: true,
            xscaling: true,
        }
    }
}

/// `CanaryDecoderConfig`, chaque clé à son défaut ; les taux d'abandon
/// (`*_dropout`) ne servent qu'à l'entraînement.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RawDecoder {
    num_layers: i64,
    hidden_size: i64,
    num_attention_heads: i64,
    inner_size: i64,
}

impl Default for RawDecoder {
    fn default() -> Self {
        Self {
            num_layers: 8,
            hidden_size: 1024,
            num_attention_heads: 16,
            inner_size: 4096,
        }
    }
}

/// Les dimensions du décodeur Transformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderConfig {
    pub num_layers: i32,
    pub hidden_size: i32,
    pub num_attention_heads: i32,
    pub inner_size: i32,
}

/// Tout ce que le graphe, l'invite et le vocabulaire lisent.
#[derive(Debug, Clone)]
pub struct Config {
    pub preprocessor: PreprocessConfig,
    pub encoder: ConformerConfig,
    pub decoder: DecoderConfig,
    pub vocab_size: i32,
    pub encoder_output_dim: i32,
    pub start_of_context_id: i64,
    pub start_of_transcript_id: i64,
    pub emotion_undefined_id: i64,
    pub end_of_text_id: i64,
    /// Lues comme le Swift, qui ne s'en sert pas pour la dictée.
    #[cfg_attr(not(test), expect(dead_code))]
    pub supported_languages: Vec<String>,
    pub tokenizer_model_base64: Option<String>,
    pub quantization: Option<Quantization>,
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let reading = || crate::tr!(en = "reading {}", fr = "lecture de {}", path.display());
        let size = std::fs::metadata(&path).with_context(reading)?.len();
        ensure!(
            size <= MAX_CONFIG_BYTES,
            crate::tr!(
                en = "Canary configuration {}: {size} bytes, more than {MAX_CONFIG_BYTES}",
                fr = "configuration de Canary {} : {size} octets, plus de {MAX_CONFIG_BYTES}",
                path.display()
            )
        );
        let mut text = String::new();
        std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_CONFIG_BYTES).read_to_string(&mut text))
            .with_context(reading)?;
        Self::parse(&text).with_context(|| {
            crate::tr!(
                en = "Canary configuration {}",
                fr = "configuration de Canary {}",
                path.display()
            )
        })
    }

    /// `init(from:)` sur le texte de `config.json`.
    pub fn parse(text: &str) -> Result<Self> {
        let json: Value = serde_json::from_str(text)?;
        let section = |key: &str| -> Option<&Value> { json.get(key).filter(|v| !v.is_null()) };
        let preprocess: Preprocess = match section("preprocessor") {
            Some(v) => Preprocess::deserialize(v).context("preprocessor")?,
            None => Preprocess::default(),
        };
        let encoder: Encoder = match section("encoder") {
            Some(v) => Encoder::deserialize(v).context("encoder")?,
            None => Encoder::default(),
        };
        // `transf_decoder` d'abord ; chacun peut ranger ses clés sous
        // `decoder`.
        let decoder = match section("transf_decoder").or_else(|| section("decoder")) {
            Some(v) => {
                let inner = v.get("decoder").filter(|d| !d.is_null()).unwrap_or(v);
                RawDecoder::deserialize(inner).context("transf_decoder")?
            }
            None => RawDecoder::default(),
        };
        let int = |key: &str, default: i64| -> Result<i64> {
            match section(key) {
                None => Ok(default),
                Some(v) => v.as_i64().ok_or_else(|| {
                    anyhow::anyhow!(crate::tr!(
                        en = "{key} is not an integer",
                        fr = "{key} n'est pas un entier",
                    ))
                }),
            }
        };
        let supported_languages = match section("supported_languages") {
            Some(v) => Vec::<String>::deserialize(v).context("supported_languages")?,
            None => DEFAULT_LANGUAGES.iter().map(|l| l.to_string()).collect(),
        };
        // `try? nestedContainer` : un `tokenizer` qui n'est pas un objet est
        // ignoré ; `model_base64` d'un autre type que texte est une erreur.
        let tokenizer_model_base64 = match json.get("tokenizer").and_then(Value::as_object) {
            Some(t) => match t.get("model_base64") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => bail!(crate::tr!(
                    en = "tokenizer.model_base64 is not a string",
                    fr = "tokenizer.model_base64 n'est pas un texte",
                )),
            },
            None => None,
        };
        let config = Self {
            preprocessor: preprocessor(&preprocess)?,
            encoder: conformer(&encoder)?,
            decoder: DecoderConfig {
                num_layers: dim("num_layers", decoder.num_layers)?,
                hidden_size: dim("hidden_size", decoder.hidden_size)?,
                num_attention_heads: dim("num_attention_heads", decoder.num_attention_heads)?,
                inner_size: dim("inner_size", decoder.inner_size)?,
            },
            vocab_size: dim("vocab_size", int("vocab_size", 16_384)?)?,
            encoder_output_dim: dim("enc_output_dim", int("enc_output_dim", 1024)?)?,
            start_of_context_id: int("startofcontext_id", 0)?,
            start_of_transcript_id: int("startoftranscript_id", 1)?,
            emotion_undefined_id: int("emo_undefined_id", 2)?,
            end_of_text_id: int("endoftext_id", 3)?,
            supported_languages,
            tokenizer_model_base64,
            quantization: quantization(&json)?,
        };
        config.check()?;
        Ok(config)
    }

    /// Ce que le graphe suppose : sinon une division par zéro, une forme
    /// impossible ou une couche que le Swift laisserait au hasard.
    fn check(&self) -> Result<()> {
        let (e, p, d) = (&self.encoder, &self.preprocessor, &self.decoder);
        ensure!(
            e.self_attention_model == "rel_pos"
                && e.subsampling == "dw_striding"
                && e.subsampling_factor > 1
                && !e.causal_downsampling,
            crate::tr!(
                en = "Canary encoder not ported to Rust ({}, {}, factor {}): only rel_pos and dw_striding are",
                fr = "encodeur de Canary non porté en Rust ({}, {}, facteur {}) : seuls rel_pos et dw_striding le sont",
                e.self_attention_model,
                e.subsampling,
                e.subsampling_factor,
            )
        );
        let encoder_fine = e.d_model % e.n_heads == 0
            && e.conv_kernel_size % 2 == 1
            && e.feat_in == p.features
            && p.hop_length() > 0
            && p.win_length() > 0
            && p.win_length() <= p.n_fft;
        ensure!(
            encoder_fine,
            crate::tr!(
                "inconsistent Canary encoder configuration",
                "configuration de l'encodeur de Canary incohérente"
            )
        );
        // Le Swift ajouterait une projection que ses poids ne remplissent
        // jamais (`encoder.*` va au Conformer) : laissée au hasard.
        ensure!(
            e.d_model == self.encoder_output_dim && d.hidden_size == self.encoder_output_dim,
            crate::tr!(
                en = "Canary: encoder width {}, decoder width {} and enc_output_dim {} differ; only equal widths are ported",
                fr = "Canary : largeur de l'encodeur {}, du décodeur {} et enc_output_dim {} différentes ; seules des largeurs égales sont portées",
                e.d_model,
                d.hidden_size,
                self.encoder_output_dim,
            )
        );
        ensure!(
            i64::from(d.hidden_size) <= MAX_D_MODEL && d.hidden_size % d.num_attention_heads == 0,
            crate::tr!(
                en = "Canary decoder: width {} with {} heads (at most {MAX_D_MODEL}, divisible by the heads)",
                fr = "décodeur de Canary : largeur {} pour {} têtes (au plus {MAX_D_MODEL}, divisible par les têtes)",
                d.hidden_size,
                d.num_attention_heads,
            )
        );
        Ok(())
    }
}

/// Une dimension dans 1…`MAX_DIM`.
fn dim(name: &str, v: i64) -> Result<i32> {
    ensure!(
        (1..=MAX_DIM).contains(&v),
        crate::tr!(
            en = "{name} = {v}, outside 1…{MAX_DIM}",
            fr = "{name} = {v}, hors de 1…{MAX_DIM}",
        )
    );
    Ok(v as i32)
}

/// `parakeetConfig` : les réglages du mel de Parakeet (la garde du
/// logarithme à son défaut, 2⁻²⁴).
fn preprocessor(p: &Preprocess) -> Result<PreprocessConfig> {
    Ok(PreprocessConfig {
        sample_rate: dim("sample_rate", p.sample_rate)? as u32,
        normalize: p.normalize.clone(),
        window_size: p.window_size,
        window_stride: p.window_stride,
        window: p.window.clone(),
        features: dim("features", p.features)?,
        n_fft: dim("n_fft", p.n_fft)? as usize,
        dither: p.dither,
        pad_to: usize::try_from(p.pad_to.clamp(0, MAX_DIM)).unwrap_or(0),
        pad_value: p.pad_value,
        preemph: p.preemph,
        log_zero_guard_value: 2f32.powi(-24),
    })
}

/// `parakeetConfig` : le Conformer de Parakeet.
fn conformer(e: &Encoder) -> Result<ConformerConfig> {
    ensure!(
        e.d_model <= MAX_D_MODEL && e.pos_emb_max_len <= MAX_ENCODER_POSITIONS,
        crate::tr!(
            en = "Canary encoder: d_model = {}, pos_emb_max_len = {} (at most {MAX_D_MODEL} and {MAX_ENCODER_POSITIONS})",
            fr = "encodeur de Canary : d_model = {}, pos_emb_max_len = {} (au plus {MAX_D_MODEL} et {MAX_ENCODER_POSITIONS})",
            e.d_model,
            e.pos_emb_max_len,
        )
    );
    Ok(ConformerConfig {
        feat_in: dim("feat_in", e.feat_in)?,
        n_layers: dim("n_layers", e.n_layers)?,
        d_model: dim("d_model", e.d_model)?,
        n_heads: dim("n_heads", e.n_heads)?,
        ff_expansion_factor: dim("ff_expansion_factor", e.ff_expansion_factor)?,
        subsampling_factor: dim("subsampling_factor", e.subsampling_factor)?,
        self_attention_model: e.self_attention_model.clone(),
        subsampling: e.subsampling.clone(),
        conv_kernel_size: dim("conv_kernel_size", e.conv_kernel_size)?,
        subsampling_conv_channels: dim("subsampling_conv_channels", e.subsampling_conv_channels)?,
        pos_emb_max_len: dim("pos_emb_max_len", e.pos_emb_max_len)?,
        causal_downsampling: e.causal_downsampling,
        use_bias: e.use_bias,
        xscaling: e.xscaling,
    })
}

/// `quantization` (ou `quantization_config`) avec `group_size` et `bits`,
/// sinon `quantization` à plat (`bits` seul : groupes de 64) ; illisible :
/// aucune, comme les `try?` du Swift. Seule la quantification affine est
/// portée ; un réglage par couche (lu seulement avec `model_type`) ne l'est
/// pas.
fn quantization(json: &Value) -> Result<Option<Quantization>> {
    #[derive(Deserialize)]
    struct Full {
        group_size: i32,
        bits: i32,
    }
    #[derive(Deserialize)]
    struct Flat {
        bits: i32,
        group_size: Option<i32>,
    }
    let full = |key: &str| {
        json.get(key)
            .and_then(|q| Full::deserialize(q).ok())
            .map(|q| Quantization {
                group_size: q.group_size,
                bits: q.bits,
            })
    };
    let flat = || {
        json.get("quantization")
            .and_then(|q| Flat::deserialize(q).ok())
            .map(|q| Quantization {
                group_size: q.group_size.unwrap_or(DEFAULT_GROUP_SIZE),
                bits: q.bits,
            })
    };
    let chosen = full("quantization")
        .or_else(|| full("quantization_config"))
        .or_else(flat);
    if let Some(q) = json.get("quantization").and_then(Value::as_object) {
        if let Some(mode) = q.get("mode").and_then(Value::as_str) {
            ensure!(
                mode == "affine",
                crate::tr!(
                    en = "Canary: quantization mode {mode:?} not ported to Rust (affine only)",
                    fr = "Canary : quantification {mode:?} non portée en Rust (affine seulement)",
                )
            );
        }
        let per_layer = json.get("model_type").is_some()
            && q.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "group_size"
                        | "bits"
                        | "mode"
                        | "quant_method"
                        | "linear_class"
                        | "quantization_mode"
                )
            });
        ensure!(
            !per_layer,
            crate::tr!(
                "Canary: per-layer quantization not ported to Rust",
                "Canary : quantification par couche non portée en Rust"
            )
        );
    }
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les clés de canary-1b-v2 (sans vocabulaire) : `transf_decoder`, 8 bits
    /// à plat.
    const CANARY: &str = r#"{
        "preprocessor": {"sample_rate": 16000, "normalize": "per_feature", "window_size": 0.025,
            "window_stride": 0.01, "window": "hann", "features": 128, "n_fft": 512,
            "dither": 1e-05, "pad_to": 0, "pad_value": 0.0, "preemph": 0.97, "mag_power": 2.0},
        "encoder": {"feat_in": 128, "n_layers": 32, "d_model": 1024, "n_heads": 8,
            "ff_expansion_factor": 4, "subsampling_factor": 8, "self_attention_model": "rel_pos",
            "subsampling": "dw_striding", "conv_kernel_size": 9, "subsampling_conv_channels": 256,
            "pos_emb_max_len": 5000, "causal_downsampling": false, "use_bias": true, "xscaling": false},
        "transf_decoder": {"vocab_size": 16384, "hidden_size": 1024, "inner_size": 4096,
            "num_layers": 8, "num_attention_heads": 8, "pre_ln": true},
        "tokenizer": {"model_base64": "AAAA"},
        "quantization": {"bits": 8}
    }"#;

    #[test]
    fn the_canary_keys_are_read_with_the_swift_defaults() {
        let c = Config::parse(CANARY).unwrap();
        assert_eq!(
            c.decoder,
            DecoderConfig {
                num_layers: 8,
                hidden_size: 1024,
                num_attention_heads: 8,
                inner_size: 4096
            }
        );
        assert_eq!((c.vocab_size, c.encoder_output_dim), (16_384, 1024));
        assert_eq!(
            (
                c.start_of_context_id,
                c.start_of_transcript_id,
                c.emotion_undefined_id,
                c.end_of_text_id
            ),
            (0, 1, 2, 3)
        );
        assert_eq!(
            c.quantization,
            Some(Quantization {
                group_size: 64,
                bits: 8
            })
        );
        assert!(!c.encoder.xscaling);
        assert_eq!(c.encoder.n_layers, 32);
        assert_eq!(c.preprocessor.features, 128);
        assert_eq!(c.preprocessor.log_zero_guard_value, 2f32.powi(-24));
        assert_eq!(c.supported_languages.len(), 25);
        assert_eq!(c.tokenizer_model_base64.as_deref(), Some("AAAA"));
    }

    #[test]
    fn an_empty_configuration_takes_the_swift_defaults() {
        let c = Config::parse("{}").unwrap();
        assert_eq!(c.decoder.num_attention_heads, 16);
        assert_eq!(c.decoder.num_layers, 8);
        assert!(c.encoder.xscaling, "xscaling : vrai par défaut");
        assert_eq!(c.quantization, None);
        assert_eq!(c.tokenizer_model_base64, None);
        assert!(c.supported_languages.iter().any(|l| l == "fr"));
    }

    #[test]
    fn a_full_quantization_and_a_nested_decoder_are_read() {
        let c = Config::parse(
            r#"{"decoder": {"decoder": {"num_layers": 2}}, "quantization_config": {"group_size": 32, "bits": 4}}"#,
        )
        .unwrap();
        assert_eq!(c.decoder.num_layers, 2);
        assert_eq!(
            c.quantization,
            Some(Quantization {
                group_size: 32,
                bits: 4
            })
        );
    }

    #[test]
    fn absurd_or_unported_configurations_are_refused_by_name() {
        for (text, name) in [
            (r#"{"transf_decoder": {"num_layers": 0}}"#, "num_layers"),
            (r#"{"transf_decoder": {"num_attention_heads": 7}}"#, "7"),
            (r#"{"vocab_size": -1}"#, "vocab_size"),
            (r#"{"vocab_size": "beaucoup"}"#, "vocab_size"),
            (r#"{"enc_output_dim": 512}"#, "enc_output_dim"),
            (r#"{"encoder": {"d_model": 8192}}"#, "d_model"),
            (
                r#"{"encoder": {"self_attention_model": "abs_pos"}}"#,
                "abs_pos",
            ),
            (r#"{"encoder": {"n_heads": 0}}"#, "n_heads"),
            (r#"{"preprocessor": {"features": 80}}"#, "Canary"),
            (r#"{"tokenizer": {"model_base64": 3}}"#, "model_base64"),
            (
                r#"{"quantization": {"bits": 4, "group_size": 64, "mode": "mxfp4"}}"#,
                "mxfp4",
            ),
            (
                r#"{"model_type": "canary", "quantization": {"bits": 4, "group_size": 64, "decoder.output_proj": false}}"#,
                "Canary",
            ),
        ] {
            let err = format!("{:#}", Config::parse(text).unwrap_err());
            assert!(err.contains(name), "{text} : {err}");
        }
    }
}
