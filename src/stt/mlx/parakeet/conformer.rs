//! L'encodeur Conformer de Parakeet : sous-échantillonnage `dw_striding`,
//! positions relatives, puis les blocs. Port de `ParakeetConformer.swift`
//! (seule la variante de Parakeet v2 et v3 : `rel_pos`, `dw_striding`).

use anyhow::{ensure, Result};
use mlx_rs::{nn, ops, Array, Dtype};

use super::config::ConformerConfig;
use crate::stt::mlx::nemo::attention::{RelPositionMultiHeadAttention, RelPositionalEncoding};
use crate::stt::mlx::nn::{scalar_like, BatchNorm, Conv1d, Conv2d, ConvShape, LayerNorm, Linear};
use crate::stt::mlx::weights::Weights;

/// `linear2(silu(linear1(x)))`.
struct FeedForward {
    linear1: Linear,
    linear2: Linear,
}

impl FeedForward {
    fn load(w: &mut Weights, prefix: &str, d_model: i32, d_ff: i32, bias: bool) -> Result<Self> {
        Ok(Self {
            linear1: Linear::load(w, &format!("{prefix}.linear1"), d_model, d_ff, bias)?,
            linear2: Linear::load(w, &format!("{prefix}.linear2"), d_ff, d_model, bias)?,
        })
    }

    fn forward(&self, x: &Array) -> Result<Array> {
        self.linear2.forward(&nn::silu(self.linear1.forward(x)?)?)
    }
}

/// Le module de convolution : porte GLU, convolution par canal,
/// normalisation par lot, SiLU, projection.
struct Convolution {
    pointwise1: Conv1d,
    depthwise: Conv1d,
    batch_norm: BatchNorm,
    pointwise2: Conv1d,
}

impl Convolution {
    fn load(w: &mut Weights, prefix: &str, args: &ConformerConfig) -> Result<Self> {
        let (d, bias) = (args.d_model, args.use_bias);
        let pointwise = |output| ConvShape {
            input: d,
            output,
            kernel: 1,
            stride: 1,
            padding: 0,
            groups: 1,
            bias,
        };
        let k = args.conv_kernel_size;
        Ok(Self {
            pointwise1: Conv1d::load(w, &format!("{prefix}.pointwise_conv1"), pointwise(2 * d))?,
            depthwise: Conv1d::load(
                w,
                &format!("{prefix}.depthwise_conv"),
                ConvShape {
                    input: d,
                    output: d,
                    kernel: k,
                    stride: 1,
                    padding: (k - 1) / 2,
                    groups: d,
                    bias,
                },
            )?,
            batch_norm: BatchNorm::load(w, &format!("{prefix}.batch_norm"), d)?,
            pointwise2: Conv1d::load(w, &format!("{prefix}.pointwise_conv2"), pointwise(d))?,
        })
    }

    fn forward(&self, x: &Array) -> Result<Array> {
        let halves = self.pointwise1.forward(x)?.split_equal(2, 2)?;
        let [a, b] = halves.as_slice() else {
            anyhow::bail!("convolution : {} moitiés au lieu de 2", halves.len());
        };
        let gated = a.multiply(ops::sigmoid(b)?)?;
        let y = self.batch_norm.forward(&self.depthwise.forward(&gated)?)?;
        self.pointwise2.forward(&nn::silu(&y)?)
    }
}

/// `ParakeetDwStridingSubsampling` : une convolution 3 × 3 de pas 2, puis
/// des paires (par canal, ponctuelle), chacune suivie de ReLU ; enfin la
/// projection des canaux × bandes restants.
struct DwStridingSubsampling {
    conv0: Conv2d,
    depthwise: Vec<Conv2d>,
    pointwise: Vec<Conv2d>,
    out: Linear,
    sampling_num: i32,
}

const STRIDE: i32 = 2;
const KERNEL: i32 = 3;
const PADDING: i32 = 1;

impl DwStridingSubsampling {
    fn load(w: &mut Weights, prefix: &str, args: &ConformerConfig) -> Result<Self> {
        let sampling_num = args.sampling_num();
        let channels = args.subsampling_conv_channels;
        let mut freq = args.feat_in;
        for _ in 0..sampling_num {
            freq = (((freq + 2 * PADDING - KERNEL) as f64 / STRIDE as f64).floor() + 1.0) as i32;
            freq = freq.max(1);
        }
        let conv0 = Conv2d::load(
            w,
            &format!("{prefix}.conv0"),
            ConvShape {
                input: 1,
                output: channels,
                kernel: 3,
                stride: 2,
                padding: 1,
                groups: 1,
                bias: true,
            },
        )?;
        let (mut depthwise, mut pointwise) = (Vec::new(), Vec::new());
        for i in 0..(sampling_num - 1).max(0) {
            depthwise.push(Conv2d::load(
                w,
                &format!("{prefix}.depthwise_layers.{i}"),
                ConvShape {
                    input: channels,
                    output: channels,
                    kernel: 3,
                    stride: 2,
                    padding: 1,
                    groups: channels,
                    bias: true,
                },
            )?);
            pointwise.push(Conv2d::load(
                w,
                &format!("{prefix}.pointwise_layers.{i}"),
                ConvShape {
                    input: channels,
                    output: channels,
                    kernel: 1,
                    stride: 1,
                    padding: 0,
                    groups: 1,
                    bias: true,
                },
            )?);
        }
        let out = Linear::load(
            w,
            &format!("{prefix}.out"),
            channels * freq,
            args.d_model,
            true,
        )?;
        Ok(Self {
            conv0,
            depthwise,
            pointwise,
            out,
            sampling_num,
        })
    }

    /// `x` : `[lot, trames, bandes]` ; rend `[lot, trames / 8, d]` et les
    /// longueurs, calculées en `f32` comme le Swift.
    fn forward(&self, x: &Array, lengths: &Array) -> Result<(Array, Array)> {
        let mut out_lengths = lengths.as_dtype(Dtype::Float32)?;
        for _ in 0..self.sampling_num {
            out_lengths = ops::floor(
                out_lengths
                    .add(Array::from_f32((2 * PADDING - KERNEL) as f32))?
                    .divide(Array::from_f32(STRIDE as f32))?,
            )?
            .add(Array::from_int(1))?;
        }
        let lengths = out_lengths.as_dtype(Dtype::Int32)?;

        let mut y = nn::relu(self.conv0.forward(&x.expand_dims(3)?)?)?;
        for (depthwise, pointwise) in self.depthwise.iter().zip(&self.pointwise) {
            y = nn::relu(pointwise.forward(&depthwise.forward(&y)?)?)?;
        }
        let s = y.shape().to_vec();
        let (batch, time, freq, channels) = (s[0], s[1], s[2], s[3]);
        let y = y
            .transpose_axes(&[0, 1, 3, 2])?
            .reshape(&[batch, time, channels * freq])?;
        Ok((self.out.forward(&y)?, lengths))
    }
}

/// Un bloc Conformer : demi-FFN, attention, convolution, demi-FFN, norme.
struct ConformerBlock {
    norm_ff1: LayerNorm,
    ff1: FeedForward,
    norm_att: LayerNorm,
    attn: RelPositionMultiHeadAttention,
    norm_conv: LayerNorm,
    conv: Convolution,
    norm_ff2: LayerNorm,
    ff2: FeedForward,
    norm_out: LayerNorm,
}

impl ConformerBlock {
    fn load(w: &mut Weights, prefix: &str, args: &ConformerConfig) -> Result<Self> {
        let (d, bias) = (args.d_model, args.use_bias);
        let ff = d * args.ff_expansion_factor;
        let norm = |w: &mut Weights, name: &str| LayerNorm::load(w, &format!("{prefix}.{name}"), d);
        Ok(Self {
            norm_ff1: norm(w, "norm_feed_forward1")?,
            ff1: FeedForward::load(w, &format!("{prefix}.feed_forward1"), d, ff, bias)?,
            norm_att: norm(w, "norm_self_att")?,
            attn: RelPositionMultiHeadAttention::load(
                w,
                &format!("{prefix}.self_attn"),
                args.n_heads,
                d,
                bias,
            )?,
            norm_conv: norm(w, "norm_conv")?,
            conv: Convolution::load(w, &format!("{prefix}.conv"), args)?,
            norm_ff2: norm(w, "norm_feed_forward2")?,
            ff2: FeedForward::load(w, &format!("{prefix}.feed_forward2"), d, ff, bias)?,
            norm_out: norm(w, "norm_out")?,
        })
    }

    fn forward(&self, x: &Array, pos_emb: &Array) -> Result<Array> {
        let half = scalar_like(0.5, x)?;
        let mut y = x.add(half.multiply(self.ff1.forward(&self.norm_ff1.forward(x)?)?)?)?;
        let normed = self.norm_att.forward(&y)?;
        y = y.add(
            self.attn
                .forward(&normed, &normed, &normed, pos_emb, None)?,
        )?;
        y = y.add(self.conv.forward(&self.norm_conv.forward(&y)?)?)?;
        let half = scalar_like(0.5, &y)?;
        y = y.add(half.multiply(self.ff2.forward(&self.norm_ff2.forward(&y)?)?)?)?;
        self.norm_out.forward(&y)
    }
}

/// `ParakeetConformer` (clés `encoder.…`).
pub struct Conformer {
    pos_enc: RelPositionalEncoding,
    pre_encode: DwStridingSubsampling,
    layers: Vec<ConformerBlock>,
}

impl Conformer {
    pub fn load(w: &mut Weights, args: &ConformerConfig) -> Result<Self> {
        ensure!(args.n_layers > 0, "encodeur : {} couches", args.n_layers);
        Ok(Self {
            pos_enc: RelPositionalEncoding::new(args.d_model, args.pos_emb_max_len, args.xscaling)?,
            pre_encode: DwStridingSubsampling::load(w, "encoder.pre_encode", args)?,
            layers: (0..args.n_layers)
                .map(|i| ConformerBlock::load(w, &format!("encoder.layers.{i}"), args))
                .collect::<Result<_>>()?,
        })
    }

    /// `x` : `[lot, trames, bandes]` ; rend `[lot, trames / 8, d]` et les
    /// longueurs encodées.
    pub fn forward(&mut self, x: &Array, lengths: &Array) -> Result<(Array, Array)> {
        let (h, lengths) = self.pre_encode.forward(x, lengths)?;
        let (mut h, pos) = self.pos_enc.forward(&h)?;
        for layer in &self.layers {
            h = layer.forward(&h, &pos)?;
        }
        Ok((h, lengths))
    }
}

/// Chaque étage du mel et de l'encodeur relancé sur l'entrée exacte du
/// Swift (`macos/Tests/ParakeetStagesTests.swift` les écrit) et comparé
/// élément par élément, à l'octet : l'essai qui situe un écart que la parité
/// (`tests::parity`) ne fait que constater. Chaque étage part de l'entrée du
/// Swift : tous sont comparés, et l'échec les nomme tous (dans un autre
/// build, ce sont les noyaux qui diffèrent). Ignoré : il demande les poids
/// (`DYSFLOW_MODELS`), les étages du Swift (`DYSFLOW_PARAKEET_STAGES`) et le
/// build de `scripts/mlx-parite.sh` (noyaux du Swift), par exemple :
///
///   DYSFLOW_MODELS=… DYSFLOW_PARAKEET_STAGES=… ./scripts/mlx-parite.sh \
///       stt::mlx::parakeet::conformer::stages 2
#[cfg(test)]
mod stages {
    use super::super::model::Parakeet;
    use super::*;
    use crate::compute::mlx::Held;
    use std::path::{Path, PathBuf};

    /// Les étages d'un modèle qui diffèrent du Swift.
    struct Checker {
        id: String,
        differing: Vec<String>,
    }

    impl Checker {
        /// Identique à l'octet, ou l'étage noté avec son écart ; une forme ou
        /// un type différent est une erreur.
        fn check(&mut self, name: &str, got: &Array, want: &Array) -> Result<()> {
            let id = &self.id;
            let differ = difference(id, name, got, want)?;
            if differ > 0.0 {
                self.differing
                    .push(format!("{name} ({differ} éléments sur {})", got.size()));
            }
            Ok(())
        }
    }

    /// Le nombre d'éléments qui diffèrent, imprimé avec l'écart le plus
    /// grand ; une forme ou un type différent est une erreur.
    fn difference(id: &str, name: &str, got: &Array, want: &Array) -> Result<f32> {
        ensure!(
            got.shape() == want.shape(),
            "{id} {name} : formes {:?} et {:?}",
            got.shape(),
            want.shape()
        );
        ensure!(
            got.dtype() == want.dtype(),
            "{id} {name} : types {:?} et {:?}",
            got.dtype(),
            want.dtype()
        );
        let (g, w) = (
            got.as_dtype(Dtype::Float32)?,
            want.as_dtype(Dtype::Float32)?,
        );
        let diff = ops::abs(g.subtract(&w)?)?;
        let differ = diff
            .gt(Array::from_f32(0.0))?
            .as_dtype(Dtype::Float32)?
            .sum(None)?;
        let max = diff.max(None)?;
        mlx_rs::transforms::eval([&differ, &max])?;
        let (differ, max): (f32, f32) = (differ.try_item_exact()?, max.try_item_exact()?);
        eprintln!(
            "{id} {name:10} {:?} : {differ} éléments différents, max|Δ| {max:.3e}",
            got.shape()
        );
        Ok(differ)
    }

    fn compare(id: &str, open: fn(&Path) -> Result<Parakeet>) {
        let base = PathBuf::from(
            std::env::var_os("DYSFLOW_PARAKEET_STAGES").expect("DYSFLOW_PARAKEET_STAGES"),
        );
        let models = PathBuf::from(std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS"));
        let (dir, file, shown) = (
            models.join(id),
            base.join(format!("{id}-etages.safetensors")),
            id.to_string(),
        );
        assert!(file.is_file(), "{} absent", file.display());
        let held = Held::load(move || {
            let parakeet = open(&dir)?;
            Ok((parakeet.encoder, parakeet.config))
        })
        .unwrap();
        let differing = held
            .with(move |(enc, config)| {
                let mut stage = Checker {
                    id: shown,
                    differing: Vec::new(),
                };
                let d = Array::load_safetensors(&file)?;
                let get = |k: &str| d.get(k).ok_or_else(|| anyhow::anyhow!("étage {k} absent"));
                let wav = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("macos/Tests/Fixtures/dictee-charge.wav");
                let samples = crate::stt::file::read_wav(&wav)?;
                let audio = Array::from_slice(&samples, &[samples.len() as i32]);
                let mel = super::super::audio::log_mel_spectrogram(&audio, &config.preprocessor)?;
                stage.check("mel", &mel, get("mel")?)?;
                stage.check(
                    "features",
                    &mel.as_dtype(Dtype::Bfloat16)?,
                    get("features")?,
                )?;
                let x = get("features")?;
                let p = &enc.pre_encode;
                stage.check(
                    "s_conv0",
                    &nn::relu(p.conv0.forward(&x.expand_dims(3)?)?)?,
                    get("s_conv0")?,
                )?;
                stage.check(
                    "s_dw0",
                    &p.depthwise[0].forward(get("s_conv0")?)?,
                    get("s_dw0")?,
                )?;
                stage.check(
                    "s_pw0",
                    &nn::relu(p.pointwise[0].forward(get("s_dw0")?)?)?,
                    get("s_pw0")?,
                )?;
                stage.check(
                    "s_dw1",
                    &p.depthwise[1].forward(get("s_pw0")?)?,
                    get("s_dw1")?,
                )?;
                stage.check(
                    "s_pw1",
                    &nn::relu(p.pointwise[1].forward(get("s_dw1")?)?)?,
                    get("s_pw1")?,
                )?;
                let s = get("s_pw1")?.shape().to_vec();
                let flat = get("s_pw1")?.transpose_axes(&[0, 1, 3, 2])?.reshape(&[
                    s[0],
                    s[1],
                    s[3] * s[2],
                ])?;
                stage.check("pre", &p.out.forward(&flat)?, get("pre")?)?;
                let (h, pos) = enc.pos_enc.forward(get("pre")?)?;
                stage.check("pos_x", &h, get("pos_x")?)?;
                stage.check("pos", &pos, get("pos")?)?;

                let (h, pos) = (get("pos_x")?, get("pos")?);
                let l = &enc.layers[0];
                stage.check("n1", &l.norm_ff1.forward(h)?, get("n1")?)?;
                stage.check("f1", &l.ff1.forward(get("n1")?)?, get("f1")?)?;
                stage.check(
                    "y1",
                    &h.add(scalar_like(0.5, h)?.multiply(get("f1")?)?)?,
                    get("y1")?,
                )?;
                stage.check("n2", &l.norm_att.forward(get("y1")?)?, get("n2")?)?;
                let n2 = get("n2")?;
                stage.check("att", &l.attn.forward(n2, n2, n2, pos, None)?, get("att")?)?;
                stage.check("y2", &get("y1")?.add(get("att")?)?, get("y2")?)?;
                stage.check("n3", &l.norm_conv.forward(get("y2")?)?, get("n3")?)?;
                let c = &l.conv;
                stage.check("c_pw1", &c.pointwise1.forward(get("n3")?)?, get("c_pw1")?)?;
                let halves = get("c_pw1")?.split_equal(2, 2)?;
                stage.check(
                    "c_gated",
                    &halves[0].multiply(ops::sigmoid(&halves[1])?)?,
                    get("c_gated")?,
                )?;
                stage.check("c_dw", &c.depthwise.forward(get("c_gated")?)?, get("c_dw")?)?;
                stage.check("c_bn", &c.batch_norm.forward(get("c_dw")?)?, get("c_bn")?)?;
                stage.check("c_silu", &nn::silu(get("c_bn")?)?, get("c_silu")?)?;
                stage.check(
                    "c_pw2",
                    &c.pointwise2.forward(get("c_silu")?)?,
                    get("c_pw2")?,
                )?;
                stage.check("cv", &c.forward(get("n3")?)?, get("cv")?)?;
                stage.check("y3", &get("y2")?.add(get("cv")?)?, get("y3")?)?;
                stage.check("n4", &l.norm_ff2.forward(get("y3")?)?, get("n4")?)?;
                stage.check("f2", &l.ff2.forward(get("n4")?)?, get("f2")?)?;
                let y3 = get("y3")?;
                stage.check(
                    "y4",
                    &y3.add(scalar_like(0.5, y3)?.multiply(get("f2")?)?)?,
                    get("y4")?,
                )?;
                stage.check("layer0", &l.norm_out.forward(get("y4")?)?, get("layer0")?)?;
                // L'encodeur entier, depuis les mêmes caractéristiques.
                let lengths = Array::from_slice(&[x.shape()[1]], &[1]);
                let (full, _) = enc.forward(x, &lengths)?;
                stage.check("encoder", &full, get("encoder")?)?;
                Ok(stage.differing)
            })
            .unwrap();
        assert!(differing.is_empty(), "{id} : {}", differing.join(" ; "));
    }

    #[test]
    #[ignore = "demande les poids et les étages du Swift, dans le build de scripts/mlx-parite.sh"]
    fn every_stage_matches_the_swift() {
        compare("parakeet_tdt_v3", Parakeet::load);
        compare("parakeet_tdt_v2", Parakeet::load);
    }

    /// Phonon-2 (tâche 16) : le graphe de Parakeet, ses couches à cinq
    /// valeurs (FFN, attention, convolutions ponctuelles) lues dans l'archive.
    #[test]
    #[ignore = "demande les poids et les étages du Swift, dans le build de scripts/mlx-parite.sh"]
    fn every_stage_of_phonon_matches_the_swift() {
        compare("phonon_2", crate::stt::mlx::phonon::load);
    }
}
