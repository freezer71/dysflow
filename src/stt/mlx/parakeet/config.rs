//! `config.json` de Parakeet (export NeMo), comme `ParakeetConfig.swift`,
//! `ParakeetVariantResolver` et `ParakeetQuantizationConfig` : seule la
//! variante TDT (Parakeet v2 et v3) est portée ici.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::path::Path;

use crate::stt::mlx::nemo::config::{JointConfig, PredictConfig};
use crate::stt::mlx::weights::Quantization;

pub const CONFIG_FILE: &str = "config.json";
/// La cible NeMo d'un transducteur (RNN-T, et TDT avec `tdt_durations`).
const RNNT_TARGET: &str = "nemo.collections.asr.models.rnnt_bpe_models.EncDecRNNTBPEModel";

#[derive(Debug, Clone, Deserialize)]
pub struct PreprocessConfig {
    pub sample_rate: u32,
    pub normalize: String,
    pub window_size: f32,
    pub window_stride: f32,
    pub window: String,
    pub features: i32,
    pub n_fft: usize,
    /// Exigé, jamais appliqué (le Swift non plus).
    #[allow(dead_code)]
    pub dither: f32,
    #[serde(default)]
    pub pad_to: usize,
    #[serde(default)]
    pub pad_value: f32,
    #[serde(default = "defaults::preemph")]
    pub preemph: f32,
    #[serde(default = "defaults::log_zero_guard_value")]
    pub log_zero_guard_value: f32,
}

impl PreprocessConfig {
    /// `Int(windowSize * Float(sampleRate))`, en `f32` comme le Swift.
    pub fn win_length(&self) -> usize {
        (self.window_size * self.sample_rate as f32) as usize
    }

    pub fn hop_length(&self) -> usize {
        (self.window_stride * self.sample_rate as f32) as usize
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConformerConfig {
    pub feat_in: i32,
    pub n_layers: i32,
    pub d_model: i32,
    pub n_heads: i32,
    pub ff_expansion_factor: i32,
    pub subsampling_factor: i32,
    pub self_attention_model: String,
    pub subsampling: String,
    pub conv_kernel_size: i32,
    pub subsampling_conv_channels: i32,
    pub pos_emb_max_len: i32,
    #[serde(default)]
    pub causal_downsampling: bool,
    #[serde(default = "defaults::yes")]
    pub use_bias: bool,
    #[serde(default)]
    pub xscaling: bool,
}

impl ConformerConfig {
    /// `Int(log2(subsamplingFactor))` : les étages de sous-échantillonnage.
    pub fn sampling_num(&self) -> i32 {
        (self.subsampling_factor.max(1) as f64).log2() as i32
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Greedy {
    #[serde(default)]
    max_symbols: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct TdtDecoding {
    /// Exigé, comme en Swift ; non lu.
    #[allow(dead_code)]
    model_type: String,
    durations: Vec<usize>,
    #[serde(default)]
    greedy: Option<Greedy>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelDefaults {
    #[serde(default)]
    tdt_durations: Option<Vec<usize>>,
}

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    model_defaults: Option<ModelDefaults>,
    preprocessor: PreprocessConfig,
    encoder: ConformerConfig,
    decoder: serde_json::Value,
    #[serde(default)]
    joint: Option<JointConfig>,
    decoding: serde_json::Value,
    #[serde(default)]
    quantization: Option<serde_json::Value>,
}

/// Un Parakeet TDT : tout ce que le graphe et le décodeur lisent.
#[derive(Debug, Clone)]
pub struct Config {
    pub preprocessor: PreprocessConfig,
    pub encoder: ConformerConfig,
    pub decoder: PredictConfig,
    pub joint: JointConfig,
    pub durations: Vec<usize>,
    pub max_symbols: Option<usize>,
    pub quantization: Option<Quantization>,
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))?;
        Self::parse(&text).with_context(|| {
            crate::tr!(
                en = "unreadable Parakeet configuration ({})",
                fr = "configuration de Parakeet illisible ({})",
                path.display()
            )
        })
    }

    /// Le texte de `config.json`. Les nombres non standard de certains
    /// exports NeMo (`-Infinity`, `Infinity`, `NaN`) deviennent `null`,
    /// comme `normalizedConfigData` du Swift.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text
            .replace("-Infinity", "null")
            .replace("Infinity", "null")
            .replace("NaN", "null");
        let raw: Raw = serde_json::from_str(&text)?;
        let target = raw.target.as_deref().unwrap_or("");
        let tdt = raw
            .model_defaults
            .as_ref()
            .is_some_and(|d| d.tdt_durations.is_some());
        if target != RNNT_TARGET || !tdt {
            bail!(crate::tr!(
                en = "Parakeet variant not ported to Rust (target {target:?}, TDT: {tdt}): only TDT is",
                fr = "variante de Parakeet non portée en Rust (cible {target:?}, TDT : {tdt}) : seul TDT l'est",
            ));
        }
        let joint = raw.joint.context("joint")?;
        let decoder: PredictConfig = serde_json::from_value(raw.decoder).context("decoder")?;
        let decoding: TdtDecoding = serde_json::from_value(raw.decoding).context("decoding")?;
        let quantization = quantization(raw.quantization)?;
        let config = Self {
            preprocessor: raw.preprocessor,
            encoder: raw.encoder,
            decoder,
            joint,
            durations: decoding.durations,
            max_symbols: decoding.greedy.and_then(|g| g.max_symbols),
            quantization,
        };
        config.check()?;
        Ok(config)
    }

    /// Ce que le graphe suppose : sinon une division par zéro, une forme
    /// impossible ou un index hors des tableaux plus loin.
    fn check(&self) -> Result<()> {
        let (e, p, j) = (&self.encoder, &self.preprocessor, &self.joint);
        ensure!(
            e.self_attention_model == "rel_pos"
                && e.subsampling == "dw_striding"
                && e.subsampling_factor > 1
                && !e.causal_downsampling,
            crate::tr!(
                en = "Parakeet encoder not ported to Rust ({}, {}, factor {}): only rel_pos and dw_striding are",
                fr = "encodeur de Parakeet non porté en Rust ({}, {}, facteur {}) : seuls rel_pos et dw_striding le sont",
                e.self_attention_model,
                e.subsampling,
                e.subsampling_factor,
            )
        );
        let fine = e.n_heads > 0
            && e.d_model % e.n_heads == 0
            && e.n_layers > 0
            && e.conv_kernel_size % 2 == 1
            && e.subsampling_conv_channels > 0
            && e.feat_in == p.features
            && e.pos_emb_max_len > 0
            && p.features > 0
            && p.n_fft > 0
            && p.sample_rate > 0
            && p.hop_length() > 0
            && p.win_length() > 0
            && !self.durations.is_empty()
            && self.decoder.prednet.pred_rnn_layers > 0
            && j.vocabulary.len() == j.num_classes as usize
            && j.jointnet.encoder_hidden == e.d_model
            && j.jointnet.pred_hidden == self.decoder.prednet.pred_hidden
            && self.durations.len() == j.num_extra_outputs as usize;
        ensure!(
            fine,
            crate::tr!(
                "inconsistent Parakeet configuration",
                "configuration de Parakeet incohérente"
            )
        );
        // Une durée 0 laisse le décodeur sur la même trame : sans plafond de
        // symboles, tdt_step pourrait y boucler sans fin, sous le verrou du GPU.
        ensure!(
            !self.durations.contains(&0) || self.max_symbols.is_some_and(|m| m > 0),
            crate::tr!(
                "Parakeet configuration: a duration of 0 needs decoding.greedy.max_symbols",
                "configuration de Parakeet : une durée 0 demande decoding.greedy.max_symbols"
            )
        );
        // Le blanc (vocab_size) est la ligne de remplissage de l'embedding de
        // la prédiction : sans blank_as_pad, il serait lu hors de la table.
        ensure!(
            self.decoder.blank_as_pad,
            crate::tr!(
                "Parakeet decoder without blank_as_pad: not ported to Rust",
                "décodeur de Parakeet sans blank_as_pad : non porté en Rust"
            )
        );
        Ok(())
    }

    /// Le jeton blanc : juste après le vocabulaire.
    pub fn blank(&self) -> usize {
        self.joint.vocabulary.len()
    }
}

/// `"quantization": {"group_size": …, "bits": …}` ; illisible : une erreur
/// (le Swift l'ignorerait, puis refuserait les clés quantifiées).
fn quantization(value: Option<serde_json::Value>) -> Result<Option<Quantization>> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => Ok(Some(serde_json::from_value(value).context("quantization")?)),
    }
}

mod defaults {
    pub fn preemph() -> f32 {
        0.97
    }
    pub fn log_zero_guard_value() -> f32 {
        2f32.powi(-24)
    }
    pub fn yes() -> bool {
        true
    }
}
