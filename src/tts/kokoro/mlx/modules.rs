//! Les briques de Kokoro (`Blocks/` du Swift : `Linear`, convolutions
//! normalisées, BiLSTM, AdaIN, blocs résiduels), l'encodeur de texte et le
//! prédicteur de prosodie (`KokoroModules.swift`). Mêmes opérations, dans le
//! même ordre que le Swift ; aucune opérateur surchargé de `mlx-rs` (ils
//! paniquent) : chaque opération rend son `Result`.

use anyhow::Result;
use mlx_rs::ops::indexing::{Ellipsis, TryIndexOp};
use mlx_rs::{fast, nn, ops, Array};

use super::config::{Config, Weights};

pub fn scalar(value: f32) -> Array {
    Array::from_f32(value)
}

/// `Linear` de MLXNN : `addMM(bias, x, weightᵀ)`.
pub struct Linear {
    weight: Array,
    bias: Array,
}

impl Linear {
    pub fn load(w: &mut Weights, prefix: &str, input: i32, output: i32) -> Result<Self> {
        Ok(Self {
            weight: w.take(&format!("{prefix}.weight"), &[output, input])?,
            bias: w.take(&format!("{prefix}.bias"), &[output])?,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        Ok(ops::addmm(&self.bias, x, self.weight.transpose()?, None, None)?)
    }
}

/// `LayerNorm` de MLXNN (`MLXFast.layerNorm`).
pub struct LayerNorm {
    weight: Array,
    bias: Array,
    eps: f32,
}

impl LayerNorm {
    /// `names` : (« weight », « bias ») ou (« gamma », « beta ») selon le
    /// fichier ; `sanitize` renomme les seconds.
    pub fn load(w: &mut Weights, prefix: &str, dim: i32, eps: f32, names: (&str, &str)) -> Result<Self> {
        Ok(Self {
            weight: w.take(&format!("{prefix}.{}", names.0), &[dim])?,
            bias: w.take(&format!("{prefix}.{}", names.1), &[dim])?,
            eps,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        Ok(fast::layer_norm(x, &self.weight, &self.bias, self.eps)?)
    }
}

/// `Embedding` de MLXNN : une ligne par identifiant.
pub struct Embedding {
    weight: Array,
}

impl Embedding {
    pub fn load(w: &mut Weights, prefix: &str, count: i32, dim: i32) -> Result<Self> {
        Ok(Self { weight: w.take(&format!("{prefix}.weight"), &[count, dim])? })
    }

    pub fn forward(&self, ids: &Array) -> Result<Array> {
        Ok(self.weight.take_axis(ids, 0)?)
    }
}

/// `Conv1d` de MLXNN, poids `[sortie, noyau, entrée]`.
pub struct Conv1d {
    weight: Array,
    bias: Array,
    stride: i32,
    padding: i32,
}

impl Conv1d {
    pub fn load(w: &mut Weights, prefix: &str, shape: ConvShape, stride: i32, padding: i32) -> Result<Self> {
        Ok(Self {
            weight: w.take_conv(&format!("{prefix}.weight"), [shape.output, shape.input, shape.kernel])?,
            bias: w.take(&format!("{prefix}.bias"), &[shape.output])?,
            stride,
            padding,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        Ok(ops::conv1d(x, &self.weight, self.stride, self.padding, 1, 1)?.add(&self.bias)?)
    }
}

/// Entrées, sorties et noyau d'une convolution, comme les initialiseurs
/// Swift les nomment.
#[derive(Clone, Copy)]
pub struct ConvShape {
    pub input: i32,
    pub output: i32,
    pub kernel: i32,
}

pub fn shape(input: i32, output: i32, kernel: i32) -> ConvShape {
    ConvShape { input, output, kernel }
}

/// `WeightNormedConv` : le poids `v / (‖v‖ + 1e-7) · g` recalculé à chaque
/// appel, comme en Swift.
pub struct WeightNormedConv {
    weight_v: Array,
    weight_g: Array,
    bias: Option<Array>,
    pub stride: i32,
    pub padding: i32,
    pub dilation: i32,
    pub groups: i32,
}

/// Les réglages d'une `WeightNormedConv`, avec les défauts du Swift.
#[derive(Clone, Copy)]
pub struct ConvOptions {
    pub stride: i32,
    pub padding: i32,
    pub dilation: i32,
    pub groups: i32,
    pub bias: bool,
    /// `encode` du Swift : le biais a la taille de l'entrée (`ups`).
    pub encode: bool,
}

impl ConvOptions {
    pub const fn padded(padding: i32) -> Self {
        Self { stride: 1, padding, dilation: 1, groups: 1, bias: true, encode: false }
    }
}

impl WeightNormedConv {
    pub fn load(w: &mut Weights, prefix: &str, shape: ConvShape, options: ConvOptions) -> Result<Self> {
        let ConvShape { input, output, kernel } = shape;
        let bias = if options.bias {
            Some(w.take(&format!("{prefix}.bias"), &[if options.encode { input } else { output }])?)
        } else {
            None
        };
        Ok(Self {
            weight_v: w.take_weight_v(&format!("{prefix}.weight_v"), [output, input, kernel])?,
            weight_g: w.take(&format!("{prefix}.weight_g"), &[output, 1, 1])?,
            bias,
            stride: options.stride,
            padding: options.padding,
            dilation: options.dilation,
            groups: options.groups,
        })
    }

    pub fn conv(&self, x: &Array) -> Result<Array> {
        self.apply(x, false)
    }

    pub fn conv_transpose(&self, x: &Array) -> Result<Array> {
        self.apply(x, true)
    }

    fn apply(&self, x: &Array, transpose: bool) -> Result<Array> {
        let v = &self.weight_v;
        let norm = v.multiply(v)?.sum_axes(&[1, 2], true)?.sqrt()?;
        let weight = v.divide(norm.add(scalar(1e-7))?)?.multiply(&self.weight_g)?;
        let last = |a: &Array| a.shape().last().copied();
        let weight = if last(x) == last(&weight) || self.groups > 1 { weight } else { weight.transpose()? };
        let result = if transpose {
            ops::conv_transpose1d(x, &weight, self.stride, self.padding, self.dilation, 0, self.groups)?
        } else {
            ops::conv1d(x, &weight, self.stride, self.padding, self.dilation, self.groups)?
        };
        match &self.bias {
            Some(bias) => Ok(result.add(bias.reshape(&[1, 1, -1])?)?),
            None => Ok(result),
        }
    }
}

/// `conv(x.swappedAxes(2, 1)).swappedAxes(2, 1)`, la forme de presque tous
/// les appels : canaux au milieu dehors, en dernier pour MLX.
pub fn conv_channels_first(conv: &WeightNormedConv, x: &Array) -> Result<Array> {
    Ok(conv.conv(&x.swap_axes(2, 1)?)?.swap_axes(2, 1)?)
}

pub fn leaky_relu(x: &Array, slope: f32) -> Result<Array> {
    Ok(nn::leaky_relu(x, slope)?)
}

/// Un LSTM dans chaque sens (`BiLSTM.swift`).
pub struct BiLstm {
    forward: Direction,
    backward: Direction,
}

struct Direction {
    wx: Array,
    wh: Array,
    bias_ih: Array,
    bias_hh: Array,
}

impl BiLstm {
    pub fn load(w: &mut Weights, prefix: &str, input: i32, hidden: i32) -> Result<Self> {
        let mut direction = |suffix: &str| -> Result<Direction> {
            Ok(Direction {
                wx: w.take(&format!("{prefix}.weight_ih_l0{suffix}"), &[4 * hidden, input])?,
                wh: w.take(&format!("{prefix}.weight_hh_l0{suffix}"), &[4 * hidden, hidden])?,
                bias_ih: w.take(&format!("{prefix}.bias_ih_l0{suffix}"), &[4 * hidden])?,
                bias_hh: w.take(&format!("{prefix}.bias_hh_l0{suffix}"), &[4 * hidden])?,
            })
        };
        Ok(Self { forward: direction("")?, backward: direction("_reverse")? })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        let input = if x.ndim() == 2 { x.expand_dims(0)? } else { x.clone() };
        let forward = self.forward.run(&input, false)?;
        let backward = self.backward.run(&input, true)?;
        Ok(ops::concatenate(&[forward, backward], -1)?)
    }
}

impl Direction {
    fn run(&self, x: &Array, reverse: bool) -> Result<Array> {
        let projected = ops::addmm(self.bias_ih.add(&self.bias_hh)?, x, self.wx.transpose()?, None, None)?;
        let steps = x.shape()[x.ndim() - 2];
        let batch = if x.ndim() == 3 { x.shape()[0] } else { 1 };
        let hidden_size = self.wh.shape()[1];
        let wh = self.wh.transpose()?;
        let mut hidden = Array::zeros::<f32>(&[batch, hidden_size])?;
        let mut cell = Array::zeros::<f32>(&[batch, hidden_size])?;
        let mut all = Vec::with_capacity(steps as usize);
        let order: Vec<i32> = if reverse { (0..steps).rev().collect() } else { (0..steps).collect() };
        for index in order {
            let ifgo = projected.try_index((Ellipsis, index, ..))?.add(hidden.matmul(&wh)?)?;
            let gates = ifgo.split_equal(4, -1)?;
            let [i, f, g, o] = gates.as_slice() else {
                anyhow::bail!("BiLSTM : {} portes au lieu de 4", gates.len());
            };
            let (i, f, g, o) = (ops::sigmoid(i)?, ops::sigmoid(f)?, ops::tanh(g)?, ops::sigmoid(o)?);
            cell = f.multiply(&cell)?.add(i.multiply(&g)?)?;
            hidden = o.multiply(ops::tanh(&cell)?)?;
            all.push(hidden.clone());
        }
        if reverse {
            all.reverse();
        }
        Ok(ops::stack(&all, -2)?)
    }
}

/// `InstanceNorm1d` (et la normalisation d'`AdaLayerNorm`) : sur le dernier
/// axe, variance sans correction.
fn instance_norm(x: &Array) -> Result<Array> {
    let mean = x.mean_axis(-1, true)?;
    let variance = x.var_axis(-1, true, 0)?;
    Ok(x.subtract(mean)?.divide(variance.add(scalar(1e-5))?.sqrt()?)?)
}

fn halves(h: &Array) -> Result<(Array, Array)> {
    let parts = h.split_equal(2, 1)?;
    match <[Array; 2]>::try_from(parts) {
        Ok([gamma, beta]) => Ok((gamma, beta)),
        Err(parts) => anyhow::bail!("AdaIN : {} parts au lieu de 2", parts.len()),
    }
}

/// `AdaIN1d`.
pub struct AdaIn {
    fc: Linear,
}

impl AdaIn {
    pub fn load(w: &mut Weights, prefix: &str, style: i32, features: i32) -> Result<Self> {
        Ok(Self { fc: Linear::load(w, &format!("{prefix}.fc"), style, features * 2)? })
    }

    pub fn forward(&self, x: &Array, s: &Array) -> Result<Array> {
        let h = self.fc.forward(s)?.expand_dims(2)?;
        let (gamma, beta) = halves(&h)?;
        Ok(scalar(1.0).add(gamma)?.multiply(instance_norm(x)?)?.add(beta)?)
    }
}

/// `AdaLayerNorm`.
pub struct AdaLayerNorm {
    fc: Linear,
}

impl AdaLayerNorm {
    pub fn load(w: &mut Weights, prefix: &str, style: i32, channels: i32) -> Result<Self> {
        Ok(Self { fc: Linear::load(w, &format!("{prefix}.fc"), style, channels * 2)? })
    }

    pub fn forward(&self, x: &Array, s: &Array) -> Result<Array> {
        let h = self.fc.forward(s)?;
        let h = h.reshape(&[h.shape()[0], h.shape()[1], 1])?;
        let (gamma, beta) = halves(&h)?;
        let gamma = gamma.transpose_axes(&[2, 0, 1])?;
        let beta = beta.transpose_axes(&[2, 0, 1])?;
        Ok(scalar(1.0).add(gamma)?.multiply(instance_norm(x)?)?.add(beta)?)
    }
}

/// `Upsample(scaleFactor: k, mode: .nearest)` sur l'axe 1 d'un `[B, L, C]` :
/// chaque trame répétée `k` fois.
pub fn upsample_nearest(x: &Array, factor: i32) -> Result<Array> {
    let s = x.shape().to_vec();
    let repeated = ops::broadcast_to(x.expand_dims(2)?, &[s[0], s[1], factor, s[2]])?;
    Ok(repeated.reshape(&[s[0], s[1] * factor, s[2]])?)
}

/// `AdainResBlock1d`.
pub struct AdainResBlock {
    conv1: WeightNormedConv,
    conv2: WeightNormedConv,
    norm1: AdaIn,
    norm2: AdaIn,
    conv1x1: Option<WeightNormedConv>,
    pool: Option<WeightNormedConv>,
}

impl AdainResBlock {
    pub fn load(w: &mut Weights, prefix: &str, dim_in: i32, dim_out: i32, style: i32, upsample: bool) -> Result<Self> {
        let p = |name: &str| format!("{prefix}.{name}");
        let conv1x1 = if dim_in != dim_out {
            let options = ConvOptions { bias: false, ..ConvOptions::padded(0) };
            Some(WeightNormedConv::load(w, &p("conv1x1"), shape(dim_in, dim_out, 1), options)?)
        } else {
            None
        };
        let pool = if upsample {
            let options = ConvOptions { stride: 2, groups: dim_in, ..ConvOptions::padded(0) };
            Some(WeightNormedConv::load(w, &p("pool"), shape(1, dim_in, 3), options)?)
        } else {
            None
        };
        Ok(Self {
            conv1: WeightNormedConv::load(w, &p("conv1"), shape(dim_in, dim_out, 3), ConvOptions::padded(1))?,
            conv2: WeightNormedConv::load(w, &p("conv2"), shape(dim_out, dim_out, 3), ConvOptions::padded(1))?,
            norm1: AdaIn::load(w, &p("norm1"), style, dim_in)?,
            norm2: AdaIn::load(w, &p("norm2"), style, dim_out)?,
            conv1x1,
            pool,
        })
    }

    pub fn upsamples(&self) -> bool {
        self.pool.is_some()
    }

    fn shortcut(&self, x: &Array) -> Result<Array> {
        let mut h = x.swap_axes(2, 1)?;
        if self.upsamples() {
            h = upsample_nearest(&h, 2)?;
        }
        h = h.swap_axes(2, 1)?;
        if let Some(conv1x1) = &self.conv1x1 {
            h = conv_channels_first(conv1x1, &h)?;
        }
        Ok(h)
    }

    fn residual(&self, x: &Array, s: &Array) -> Result<Array> {
        let mut h = self.norm1.forward(x, s)?;
        h = leaky_relu(&h, 0.2)?;
        if let Some(pool) = &self.pool {
            // torch ConvTranspose1d(k=3, stride=2, padding=1, output_padding=1) :
            // T → 2T en ôtant un échantillon à gauche (voir le Swift).
            h = pool.conv_transpose(&h.swap_axes(2, 1)?)?.try_index((.., 1.., ..))?;
            h = h.swap_axes(2, 1)?;
        }
        h = conv_channels_first(&self.conv1, &h)?;
        h = self.norm2.forward(&h, s)?;
        h = leaky_relu(&h, 0.2)?;
        conv_channels_first(&self.conv2, &h)
    }

    pub fn forward(&self, x: &Array, s: &Array) -> Result<Array> {
        let out = self.residual(x, s)?;
        Ok(out.add(self.shortcut(x)?)?.divide(scalar(2f32.sqrt()))?)
    }
}

/// `AdaINResBlock1` (activation Snake), pour le générateur.
pub struct AdaInResBlock1 {
    convs1: Vec<WeightNormedConv>,
    convs2: Vec<WeightNormedConv>,
    adain1: Vec<AdaIn>,
    adain2: Vec<AdaIn>,
    alpha1: Vec<Array>,
    alpha2: Vec<Array>,
}

impl AdaInResBlock1 {
    pub fn load(w: &mut Weights, prefix: &str, channels: i32, kernel: i32, dilations: &[i32], style: i32) -> Result<Self> {
        anyhow::ensure!(
            dilations.len() == 3,
            crate::tr!("inconsistent Kokoro configuration", "configuration de Kokoro incohérente")
        );
        let p = |name: &str, i: usize| format!("{prefix}.{name}.{i}");
        let mut block = Self {
            convs1: Vec::new(),
            convs2: Vec::new(),
            adain1: Vec::new(),
            adain2: Vec::new(),
            alpha1: Vec::new(),
            alpha2: Vec::new(),
        };
        let same = shape(channels, channels, kernel);
        for (i, &d) in dilations.iter().enumerate() {
            let options = ConvOptions { dilation: d, ..ConvOptions::padded((kernel * d - d) / 2) };
            block.convs1.push(WeightNormedConv::load(w, &p("convs1", i), same, options)?);
            block.convs2.push(WeightNormedConv::load(w, &p("convs2", i), same, ConvOptions::padded((kernel - 1) / 2))?);
            block.adain1.push(AdaIn::load(w, &p("adain1", i), style, channels)?);
            block.adain2.push(AdaIn::load(w, &p("adain2", i), style, channels)?);
            block.alpha1.push(w.take(&p("alpha1", i), &[1, channels, 1])?);
            block.alpha2.push(w.take(&p("alpha2", i), &[1, channels, 1])?);
        }
        Ok(block)
    }

    /// `x + (1 / a) · sin(a · x)²`.
    fn snake(x: &Array, alpha: &Array) -> Result<Array> {
        let wave = alpha.multiply(x)?.sin()?.power(scalar(2.0))?;
        Ok(x.add(scalar(1.0).divide(alpha)?.multiply(wave)?)?)
    }

    pub fn forward(&self, x: &Array, s: &Array) -> Result<Array> {
        let mut h = x.clone();
        for i in 0..self.convs1.len() {
            let mut xt = self.adain1[i].forward(&h, s)?;
            xt = Self::snake(&xt, &self.alpha1[i])?;
            xt = conv_channels_first(&self.convs1[i], &xt)?;
            xt = self.adain2[i].forward(&xt, s)?;
            xt = Self::snake(&xt, &self.alpha2[i])?;
            xt = conv_channels_first(&self.convs2[i], &xt)?;
            h = xt.add(&h)?;
        }
        Ok(h)
    }
}

/// `KokoroTextEncoder`.
pub struct TextEncoder {
    embedding: Embedding,
    cnn: Vec<(WeightNormedConv, LayerNorm)>,
    lstm: BiLstm,
}

impl TextEncoder {
    pub fn load(w: &mut Weights, config: &Config) -> Result<Self> {
        let channels = config.hidden_dim;
        let kernel = config.text_encoder_kernel_size;
        let mut cnn = Vec::new();
        for i in 0..config.n_layer {
            let p = format!("text_encoder.cnn.{i}");
            let conv = WeightNormedConv::load(
                w,
                &format!("{p}.0"),
                shape(channels, channels, kernel),
                ConvOptions::padded((kernel - 1) / 2),
            )?;
            cnn.push((conv, LayerNorm::load(w, &format!("{p}.1"), channels, 1e-5, ("gamma", "beta"))?));
        }
        Ok(Self {
            embedding: Embedding::load(w, "text_encoder.embedding", config.n_token, channels)?,
            cnn,
            lstm: BiLstm::load(w, "text_encoder.lstm", channels, channels / 2)?,
        })
    }

    pub fn forward(&self, ids: &Array, mask: &Array) -> Result<Array> {
        let zero = scalar(0.0);
        let mut h = self.embedding.forward(ids)?.transpose_axes(&[0, 2, 1])?;
        let m = mask.expand_dims(1)?;
        h = ops::select(&m, &zero, &h)?;
        for (conv, norm) in &self.cnn {
            h = conv_channels_first(conv, &h)?;
            h = norm.forward(&h.swap_axes(2, 1)?)?.swap_axes(2, 1)?;
            h = ops::select(&m, &zero, &h)?;
            h = leaky_relu(&h, 0.2)?;
            h = ops::select(&m, &zero, &h)?;
        }
        h = h.swap_axes(2, 1)?;
        h = self.lstm.forward(&h)?.swap_axes(2, 1)?;
        // Le Swift recopie `h` dans des zéros de la longueur du masque : la
        // même, une seule séquence pleine.
        Ok(ops::select(&m, &zero, &h)?)
    }
}

/// `KokoroProsodyPredictor`, avec son `KokoroDurationEncoder`.
pub struct Predictor {
    lstms: Vec<(BiLstm, AdaLayerNorm)>,
    pub lstm: BiLstm,
    pub duration_proj: Linear,
    shared: BiLstm,
    f0: Vec<AdainResBlock>,
    n: Vec<AdainResBlock>,
    f0_proj: Conv1d,
    n_proj: Conv1d,
}

impl Predictor {
    pub fn load(w: &mut Weights, config: &Config) -> Result<Self> {
        let (style, hidden) = (config.style_dim, config.hidden_dim);
        let mut lstms = Vec::new();
        for i in 0..config.n_layer {
            let p = |j: i32| format!("predictor.text_encoder.lstms.{j}");
            lstms.push((
                BiLstm::load(w, &p(2 * i), hidden + style, hidden / 2)?,
                AdaLayerNorm::load(w, &p(2 * i + 1), style, hidden)?,
            ));
        }
        let mut blocks = |name: &str| -> Result<Vec<AdainResBlock>> {
            let p = |i: usize| format!("predictor.{name}.{i}");
            Ok(vec![
                AdainResBlock::load(w, &p(0), hidden, hidden, style, false)?,
                AdainResBlock::load(w, &p(1), hidden, hidden / 2, style, true)?,
                AdainResBlock::load(w, &p(2), hidden / 2, hidden / 2, style, false)?,
            ])
        };
        let (f0, n) = (blocks("F0")?, blocks("N")?);
        Ok(Self {
            lstms,
            lstm: BiLstm::load(w, "predictor.lstm", hidden + style, hidden / 2)?,
            duration_proj: Linear::load(w, "predictor.duration_proj.linear_layer", hidden, config.max_dur)?,
            shared: BiLstm::load(w, "predictor.shared", hidden + style, hidden / 2)?,
            f0,
            n,
            f0_proj: Conv1d::load(w, "predictor.F0_proj", shape(hidden / 2, 1, 1), 1, 0)?,
            n_proj: Conv1d::load(w, "predictor.N_proj", shape(hidden / 2, 1, 1), 1, 0)?,
        })
    }

    /// `KokoroDurationEncoder.callAsFunction`.
    pub fn encode_durations(&self, x: &Array, style: &Array, mask: &Array) -> Result<Array> {
        let zero = scalar(0.0);
        let mut h = x.transpose_axes(&[2, 0, 1])?;
        let style_dim = style.shape()[style.ndim() - 1];
        let s = ops::broadcast_to(style, &[h.shape()[0], h.shape()[1], style_dim])?;
        h = ops::concatenate(&[&h, &s], -1)?;
        let mask_expanded = mask.expand_dims(-1)?.transpose_axes(&[1, 0, 2])?;
        h = ops::select(&mask_expanded, &zero, &h)?;
        h = h.transpose_axes(&[1, 2, 0])?;
        let s_t = s.transpose_axes(&[1, 2, 0])?;
        let mask_t = mask.expand_dims(-1)?.transpose_axes(&[0, 2, 1])?;
        for (lstm, norm) in &self.lstms {
            let input = h.transpose_axes(&[0, 2, 1])?.try_index(0)?;
            h = lstm.forward(&input)?.transpose_axes(&[0, 2, 1])?;
            h = norm.forward(&h.transpose_axes(&[0, 2, 1])?, style)?.transpose_axes(&[0, 2, 1])?;
            h = ops::concatenate(&[&h, &s_t], 1)?;
            h = ops::select(&mask_t, &zero, &h)?;
        }
        Ok(h.transpose_axes(&[0, 2, 1])?)
    }

    /// `predict` : la hauteur (F0) et l'énergie (N) par trame.
    pub fn predict(&self, x: &Array, s: &Array) -> Result<(Array, Array)> {
        let shared = self.shared.forward(&x.transpose_axes(&[0, 2, 1])?)?;
        let run = |blocks: &[AdainResBlock], proj: &Conv1d| -> Result<Array> {
            let mut h = shared.transpose_axes(&[0, 2, 1])?;
            for block in blocks {
                h = block.forward(&h, s)?;
            }
            h = proj.forward(&h.swap_axes(2, 1)?)?.swap_axes(2, 1)?;
            Ok(h.squeeze_axes(&[1])?)
        };
        Ok((run(&self.f0, &self.f0_proj)?, run(&self.n, &self.n_proj)?))
    }
}
