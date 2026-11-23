//! `config.json` de Nemotron 3.5 ASR Streaming (export NeMo
//! `EncDecRNNTBPEModelWithPrompt`), comme `NemotronASRConfig.swift` : les
//! mêmes clés et les mêmes valeurs par défaut, les nombres non standard
//! (`Infinity`, `NaN`) lus comme `null`, la quantification affine globale.
//! Les tailles sont bornées : un fichier absurde est une erreur qui le
//! nomme, jamais une panique. Seul le flux est porté (le moteur est un
//! `StreamingRecognizer`) : ce que le flux du Swift suppose sans le
//! vérifier est exigé ici.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::stt::mlx::nemo::config::{
    JointConfig, JointNetworkConfig, PredictConfig, PredictNetworkConfig,
};
use crate::stt::mlx::parakeet::config::PreprocessConfig;
use crate::stt::mlx::weights::Quantization;

pub const CONFIG_FILE: &str = "config.json";
/// `config.json` plus gros que cela : refusé sans être lu (celui du
/// catalogue fait 160 ko, vocabulaire compris).
const MAX_CONFIG_BYTES: u64 = 16 << 20;
/// Une dimension au plus (couches, têtes, vocabulaire, largeurs).
const MAX_DIM: i64 = 1 << 17;
/// La largeur de l'encodeur au plus : la table des positions est calculée
/// avant que la forme des poids soit vérifiée.
const MAX_D_MODEL: i64 = 4096;
/// Les positions de l'encodeur au plus (`pos_emb_max_len`, 5 000 ici).
const MAX_ENCODER_POSITIONS: i64 = 1 << 16;

/// `NemotronASRPreprocessConfig`, chaque clé à son défaut.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Preprocess {
    sample_rate: i64,
    features: i64,
    n_fft: i64,
    window_size: f32,
    window_stride: f32,
    window: String,
    preemph: f32,
    dither: f32,
    normalize: String,
    log_zero_guard_value: f32,
    pad_to: i64,
    pad_value: f32,
}

impl Default for Preprocess {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            features: 128,
            n_fft: 512,
            window_size: 0.025,
            window_stride: 0.01,
            window: "hann".into(),
            preemph: 0.97,
            dither: 1e-5,
            normalize: "NA".into(),
            log_zero_guard_value: 2f32.powi(-24),
            pad_to: 0,
            pad_value: 0.0,
        }
    }
}

/// `NemotronASRConvContextSize` : `"causal"` ou `[gauche, droite]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConvContext {
    Named(String),
    Explicit(Vec<i64>),
}

/// `NemotronASRConformerConfig`, chaque clé à son défaut.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RawEncoder {
    feat_in: i64,
    n_layers: i64,
    d_model: i64,
    n_heads: i64,
    ff_expansion_factor: i64,
    subsampling_factor: i64,
    subsampling_conv_channels: i64,
    conv_kernel_size: i64,
    causal_downsampling: bool,
    conv_context_size: Option<ConvContext>,
    conv_norm_type: String,
    self_attention_model: String,
    att_context_size: Vec<Vec<i64>>,
    pos_emb_max_len: i64,
    use_bias: bool,
    xscaling: bool,
}

impl Default for RawEncoder {
    fn default() -> Self {
        Self {
            feat_in: 128,
            n_layers: 24,
            d_model: 1024,
            n_heads: 8,
            ff_expansion_factor: 4,
            subsampling_factor: 8,
            subsampling_conv_channels: 256,
            conv_kernel_size: 9,
            causal_downsampling: true,
            conv_context_size: None,
            conv_norm_type: "layer_norm".into(),
            self_attention_model: "rel_pos".into(),
            att_context_size: vec![vec![56, 13]],
            pos_emb_max_len: 5000,
            use_bias: false,
            xscaling: false,
        }
    }
}

/// L'encodeur Conformer causal, tel que le flux le lit.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub feat_in: i32,
    pub n_layers: i32,
    pub d_model: i32,
    pub n_heads: i32,
    pub ff_expansion_factor: i32,
    pub subsampling_factor: i32,
    pub subsampling_conv_channels: i32,
    pub conv_kernel_size: i32,
    pub pos_emb_max_len: i32,
    pub use_bias: bool,
    pub xscaling: bool,
}

impl EncoderConfig {
    /// `Int(log2(subsamplingFactor))` : les étages de sous-échantillonnage.
    pub fn sampling_num(&self) -> i32 {
        (self.subsampling_factor.max(1) as f64).log2() as i32
    }
}

/// `NemotronASRPromptConfig` : l'invite de langue.
#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub num_prompts: i32,
    pub prompt_hidden: i32,
    pub dictionary: HashMap<String, i64>,
}

/// Tout ce que le graphe, l'invite, le décodeur et le vocabulaire lisent.
#[derive(Debug, Clone)]
pub struct Config {
    pub preprocessor: PreprocessConfig,
    pub encoder: EncoderConfig,
    /// `None` : sans `prompt` dans le fichier, aucune invite.
    pub prompt: Option<PromptConfig>,
    pub decoder: PredictConfig,
    pub joint: JointConfig,
    pub vocabulary: Vec<String>,
    pub default_language: String,
    /// `[gauche, droite]` : le cache d'attention (`gauche` trames) et la
    /// tranche native (`droite + 1` trames).
    pub default_att_context_size: Vec<i32>,
    pub max_symbols: usize,
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
                en = "Nemotron configuration {}: {size} bytes, more than {MAX_CONFIG_BYTES}",
                fr = "configuration de Nemotron {} : {size} octets, plus de {MAX_CONFIG_BYTES}",
                path.display()
            )
        );
        let mut text = String::new();
        std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_CONFIG_BYTES).read_to_string(&mut text))
            .with_context(reading)?;
        Self::parse(&text).with_context(|| {
            crate::tr!(
                en = "Nemotron configuration {}",
                fr = "configuration de Nemotron {}",
                path.display()
            )
        })
    }

    /// `init(from:)` sur le texte de `config.json`, nombres non standard
    /// lus comme `null` (`normalizedConfigData`).
    pub fn parse(text: &str) -> Result<Self> {
        let text = text
            .replace("-Infinity", "null")
            .replace("Infinity", "null")
            .replace("NaN", "null");
        let json: Value = serde_json::from_str(&text)?;
        let section = |key: &str| -> Option<&Value> { json.get(key).filter(|v| !v.is_null()) };
        let preprocess: Preprocess = match section("preprocessor") {
            Some(v) => Preprocess::deserialize(v).context("preprocessor")?,
            None => Preprocess::default(),
        };
        let raw: RawEncoder = match section("encoder") {
            Some(v) => RawEncoder::deserialize(v).context("encoder")?,
            None => RawEncoder::default(),
        };
        let prompt = match section("prompt") {
            Some(v) => Some(prompt(v)?),
            None => None,
        };
        let decoder = section("decoder").ok_or_else(|| missing("decoder"))?;
        let joint = section("joint").ok_or_else(|| missing("joint"))?;
        // Clés à plat, sinon sous `prednet` / `jointnet`.
        let nested = |v: &Value, inner: &str, key: &str| -> Option<i64> {
            v.get(key)
                .filter(|k| !k.is_null())
                .or_else(|| v.get(inner).and_then(|n| n.get(key)))
                .and_then(Value::as_i64)
        };
        let field = |v: &Value, inner: &str, key: &str| -> Result<i64> {
            nested(v, inner, key).ok_or_else(|| missing(key))
        };
        let vocab_size = dim("decoder.vocab_size", field(decoder, "", "vocab_size")?)?;
        let blank_as_pad = decoder
            .get("blank_as_pad")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let pred_hidden = dim("pred_hidden", field(decoder, "prednet", "pred_hidden")?)?;
        let pred_rnn_layers = dim(
            "pred_rnn_layers",
            field(decoder, "prednet", "pred_rnn_layers")?,
        )?;
        let activation = joint
            .get("activation")
            .or_else(|| joint.get("jointnet").and_then(|n| n.get("activation")))
            .and_then(Value::as_str)
            .ok_or_else(|| missing("joint.activation"))?
            .to_string();
        let vocabulary: Vec<String> = match section("vocabulary") {
            Some(v) => Vec::deserialize(v).context("vocabulary")?,
            None => match joint.get("vocabulary").filter(|v| !v.is_null()) {
                Some(v) => Vec::deserialize(v).context("joint.vocabulary")?,
                None => Vec::new(),
            },
        };
        let joint = JointConfig {
            num_classes: dim("joint.num_classes", field(joint, "", "num_classes")?)?,
            vocabulary: vocabulary.clone(),
            jointnet: JointNetworkConfig {
                joint_hidden: dim("joint_hidden", field(joint, "jointnet", "joint_hidden")?)?,
                activation,
                encoder_hidden: dim(
                    "encoder_hidden",
                    field(joint, "jointnet", "encoder_hidden")?,
                )?,
                pred_hidden: dim(
                    "joint pred_hidden",
                    field(joint, "jointnet", "pred_hidden")?,
                )?,
            },
            num_extra_outputs: 0,
        };
        let default_language = match section("default_language") {
            Some(v) => v
                .as_str()
                .ok_or_else(|| missing("default_language"))?
                .to_string(),
            None if prompt.is_some() => "auto".into(),
            None => "en".into(),
        };
        let default_att_context_size = match section("default_att_context_size") {
            Some(v) => Vec::<i64>::deserialize(v).context("default_att_context_size")?,
            None => raw
                .att_context_size
                .first()
                .cloned()
                .unwrap_or_else(|| vec![56, 13]),
        };
        let max_symbols = match section("max_symbols") {
            Some(v) => v.as_i64().ok_or_else(|| missing("max_symbols"))?,
            None => 0,
        };
        let config = Self {
            preprocessor: preprocessor(&preprocess)?,
            encoder: encoder(&raw)?,
            prompt,
            decoder: PredictConfig {
                blank_as_pad,
                vocab_size,
                prednet: PredictNetworkConfig {
                    pred_hidden,
                    pred_rnn_layers,
                    rnn_hidden_size: None,
                },
            },
            joint,
            vocabulary,
            default_language,
            default_att_context_size: context(&default_att_context_size)?,
            max_symbols: usize::try_from(max_symbols).unwrap_or(0),
            quantization: quantization(&json)?,
        };
        config.check(&preprocess.normalize)?;
        Ok(config)
    }

    /// Ce que le flux suppose : sinon une division par zéro, une forme
    /// impossible, un index hors des tableaux, une boucle sans fin ou un
    /// texte faux plus loin.
    fn check(&self, normalize: &str) -> Result<()> {
        // `NemotronASRStreamSession` : un `precondition`, ici une erreur.
        let normalize = normalize.to_lowercase();
        ensure!(
            normalize == "na" || normalize == "none",
            crate::tr!(
                en = "Nemotron streams only with an unnormalized mel (normalize \"NA\"), not {normalize:?}",
                fr = "Nemotron ne lit en flux qu'un mel non normalisé (normalize « NA »), pas {normalize:?}",
            )
        );
        let (e, p, d, j) = (
            &self.encoder,
            &self.preprocessor,
            &self.decoder,
            &self.joint,
        );
        let fine = e.d_model % e.n_heads == 0
            && e.feat_in == p.features
            && e.subsampling_factor > 1
            && e.subsampling_factor.count_ones() == 1
            && p.hop_length() > 0
            && p.win_length() > 0
            && p.n_fft >= 2
            && j.jointnet.encoder_hidden == e.d_model
            && j.jointnet.pred_hidden == d.prednet.pred_hidden
            && self.default_att_context_size.len() == 2;
        ensure!(
            fine,
            crate::tr!(
                "inconsistent Nemotron configuration",
                "configuration de Nemotron incohérente"
            )
        );
        // Le blanc (`vocab_size`) est la dernière sortie du réseau joint ;
        // tout autre jeton doit avoir son texte.
        ensure!(
            j.num_classes == d.vocab_size && self.vocabulary.len() == d.vocab_size as usize,
            crate::tr!(
                en = "Nemotron: {} joint classes and {} words for a prediction vocabulary of {}",
                fr = "Nemotron : {} classes jointes et {} mots pour un vocabulaire de prédiction de {}",
                j.num_classes,
                self.vocabulary.len(),
                d.vocab_size,
            )
        );
        // Sans plafond, un RNN-T qui n'émet jamais le blanc tournerait sans
        // fin sur une trame, sous le verrou du GPU (le Swift l'accepte).
        ensure!(
            self.max_symbols > 0,
            crate::tr!(
                "Nemotron configuration: max_symbols must be positive",
                "configuration de Nemotron : max_symbols doit être positif"
            )
        );
        // 80 ms par trame de l'encodeur : la tranche en millisecondes s'y
        // divise (`makeStreamSession`).
        ensure!(
            self.ms_per_frame() > 0,
            crate::tr!(
                "Nemotron configuration: an encoder frame shorter than 1 ms",
                "configuration de Nemotron : une trame de l'encodeur de moins de 1 ms"
            )
        );
        Ok(())
    }

    /// Le jeton blanc : juste après le vocabulaire (`decoder.vocab_size`).
    pub fn blank(&self) -> usize {
        self.decoder.vocab_size as usize
    }

    /// Les millisecondes d'une trame de l'encodeur, en entiers comme le
    /// Swift : `8 · 160 · 1000 / 16 000 = 80`.
    pub fn ms_per_frame(&self) -> i64 {
        let p = &self.preprocessor;
        i64::from(self.encoder.subsampling_factor) * p.hop_length() as i64 * 1000
            / i64::from(p.sample_rate)
    }

    /// Les secondes d'une trame de l'encodeur (dates des jetons).
    pub fn frame_seconds(&self) -> f64 {
        let p = &self.preprocessor;
        (self.encoder.subsampling_factor as usize * p.hop_length()) as f64
            / f64::from(p.sample_rate)
    }
}

fn missing(key: &str) -> anyhow::Error {
    anyhow::anyhow!(crate::tr!(
        en = "{key} is missing or of the wrong type",
        fr = "{key} manque ou n'a pas le bon type",
    ))
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

/// `[gauche, droite]` : un cache d'au moins une trame, une droite positive
/// ou nulle, bornés.
fn context(values: &[i64]) -> Result<Vec<i32>> {
    let fine = values.len() == 2
        && (1..=MAX_DIM).contains(&values[0])
        && (0..=MAX_DIM).contains(&values[1]);
    ensure!(
        fine,
        crate::tr!(
            en = "default_att_context_size = {values:?}: [left ≥ 1, right ≥ 0] expected",
            fr = "default_att_context_size = {values:?} : [gauche ≥ 1, droite ≥ 0] attendu",
        )
    );
    Ok(values.iter().map(|&v| v as i32).collect())
}

/// Les réglages du mel, au format de Parakeet (mêmes opérations).
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
        log_zero_guard_value: p.log_zero_guard_value,
    })
}

/// Le Conformer causal du flux. Le Swift construit toujours le même graphe
/// (sous-échantillonnage causal, attention à positions relatives,
/// `LayerNorm` dans le module de convolution, convolution causale) quoi
/// que dise le fichier : un autre réglage est refusé plutôt que lu faux.
fn encoder(e: &RawEncoder) -> Result<EncoderConfig> {
    let k = e.conv_kernel_size;
    let causal_conv = match &e.conv_context_size {
        None => true,
        Some(ConvContext::Named(name)) => name.eq_ignore_ascii_case("causal"),
        Some(ConvContext::Explicit(v)) => {
            v.first() == Some(&(k - 1)) && v.get(1).is_none_or(|r| *r == 0)
        }
    };
    ensure!(
        e.causal_downsampling
            && causal_conv
            && e.self_attention_model == "rel_pos"
            && e.conv_norm_type == "layer_norm",
        crate::tr!(
            en = "Nemotron encoder not ported to Rust (causal downsampling {}, convolution context {:?}, {}, {}): only the causal, rel_pos, layer_norm streaming encoder is",
            fr = "encodeur de Nemotron non porté en Rust (sous-échantillonnage causal {}, contexte de convolution {:?}, {}, {}) : seul l'encodeur causal en flux, rel_pos, layer_norm l'est",
            e.causal_downsampling,
            e.conv_context_size,
            e.self_attention_model,
            e.conv_norm_type,
        )
    );
    ensure!(
        e.d_model <= MAX_D_MODEL && e.pos_emb_max_len <= MAX_ENCODER_POSITIONS,
        crate::tr!(
            en = "Nemotron encoder: d_model = {}, pos_emb_max_len = {} (at most {MAX_D_MODEL} and {MAX_ENCODER_POSITIONS})",
            fr = "encodeur de Nemotron : d_model = {}, pos_emb_max_len = {} (au plus {MAX_D_MODEL} et {MAX_ENCODER_POSITIONS})",
            e.d_model,
            e.pos_emb_max_len,
        )
    );
    Ok(EncoderConfig {
        feat_in: dim("feat_in", e.feat_in)?,
        n_layers: dim("n_layers", e.n_layers)?,
        d_model: dim("d_model", e.d_model)?,
        n_heads: dim("n_heads", e.n_heads)?,
        ff_expansion_factor: dim("ff_expansion_factor", e.ff_expansion_factor)?,
        subsampling_factor: dim("subsampling_factor", e.subsampling_factor)?,
        subsampling_conv_channels: dim("subsampling_conv_channels", e.subsampling_conv_channels)?,
        conv_kernel_size: dim("conv_kernel_size", k)?,
        pos_emb_max_len: dim("pos_emb_max_len", e.pos_emb_max_len)?,
        use_bias: e.use_bias,
        xscaling: e.xscaling,
    })
}

/// `NemotronASRPromptConfig`, chaque clé à son défaut.
fn prompt(v: &Value) -> Result<PromptConfig> {
    #[derive(Deserialize)]
    #[serde(default)]
    struct Raw {
        num_prompts: i64,
        prompt_hidden: i64,
        prompt_dictionary: HashMap<String, i64>,
    }
    impl Default for Raw {
        fn default() -> Self {
            Self {
                num_prompts: 128,
                prompt_hidden: 2048,
                prompt_dictionary: HashMap::new(),
            }
        }
    }
    let raw = Raw::deserialize(v).context("prompt")?;
    Ok(PromptConfig {
        num_prompts: dim("num_prompts", raw.num_prompts)?,
        prompt_hidden: dim("prompt_hidden", raw.prompt_hidden)?,
        dictionary: raw.prompt_dictionary,
    })
}

/// `quantization` avec `group_size` et `bits`, comme
/// `NemotronASRQuantizationConfig` ; absente : aucune. Seule la
/// quantification affine globale est portée : un mode autre qu'affine, ou
/// un réglage par couche (une autre clé que celles que le Swift ignore), est
/// refusé par son nom.
fn quantization(json: &Value) -> Result<Option<Quantization>> {
    let Some(q) = json.get("quantization").filter(|q| !q.is_null()) else {
        return Ok(None);
    };
    let Some(object) = q.as_object() else {
        bail!(crate::tr!(
            "Nemotron: quantization is not an object",
            "Nemotron : quantization n'est pas un objet"
        ));
    };
    const IGNORED: [&str; 6] = [
        "group_size",
        "bits",
        "mode",
        "quant_method",
        "linear_class",
        "quantization_mode",
    ];
    if let Some(layer) = object.keys().find(|k| !IGNORED.contains(&k.as_str())) {
        bail!(crate::tr!(
            en = "Nemotron: per-layer quantization ({layer}) not ported to Rust",
            fr = "Nemotron : quantification par couche ({layer}) non portée en Rust",
        ));
    }
    if let Some(mode) = object.get("mode").and_then(Value::as_str) {
        ensure!(
            mode == "affine",
            crate::tr!(
                en = "Nemotron: quantization mode {mode:?} not ported to Rust (affine only)",
                fr = "Nemotron : quantification {mode:?} non portée en Rust (affine seulement)",
            )
        );
    }
    let quantization = Quantization::deserialize(q).context("quantization")?;
    ensure!(
        quantization.bits > 0 && quantization.group_size > 0,
        crate::tr!(
            en = "Nemotron: quantization of {} bits by groups of {}",
            fr = "Nemotron : quantification de {} bits par groupes de {}",
            quantization.bits,
            quantization.group_size,
        )
    );
    Ok(Some(quantization))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
