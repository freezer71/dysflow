//! L'encodeur audio de Voxtral en flux (`VoxtralRealtimeEncoder.swift` et
//! `feedIncremental`) : deux convolutions causales à retenue (`convStemStep`),
//! un Transformer causal à fenêtre glissante dont les caches repartent de
//! zéro à chaque bloc de `sliding_window` trames (RoPE est relatif : chaque
//! bloc refait `encodeChunked`), puis le sous-échantillonnage par 4 et la
//! projection vers le décodeur. Mêmes opérations, dans le même ordre ; les
//! types suivent ceux des poids, comme dans le Swift.

use anyhow::{anyhow, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{nn, ops, Array, Dtype};

use super::config::{EncoderConfig, MEL_BINS};
use super::model::linear;
use crate::stt::mlx::nn::{scalar, Conv1d, ConvShape, Linear, RmsNorm};
use crate::stt::mlx::weights::Weights;

/// `MLXArray(start..<end)` : des `int32`.
pub fn range(start: i32, end: i32) -> Array {
    let values: Vec<i32> = (start..end).collect();
    Array::from_slice(&values, &[values.len() as i32])
}

/// `voxtralComputeRopeFrequencies` (encodeur) : `exp(−log θ · i / d)`.
fn rope_frequencies(positions: &Array, head_dim: i32, theta: f32) -> Result<(Array, Array)> {
    let idx: Vec<i32> = (0..head_dim).step_by(2).collect();
    let idx = Array::from_slice(&idx, &[idx.len() as i32]).as_dtype(Dtype::Float32)?;
    let inv = ops::exp(scalar(-theta.ln()).multiply(idx.divide(scalar(head_dim as f32))?)?)?;
    let angles = positions
        .as_dtype(Dtype::Float32)?
        .expand_dims(1)?
        .multiply(inv.expand_dims(0)?)?;
    Ok((ops::cos(&angles)?, ops::sin(&angles)?))
}

/// `voxtralApplyInterleavedRoPE` : `[t, têtes · d]`, paires entrelacées.
pub fn apply_interleaved_rope(
    x: &Array,
    cos: &Array,
    sin: &Array,
    heads: i32,
    head_dim: i32,
) -> Result<Array> {
    let t = x.shape()[0];
    let pairs = x.reshape(&[t, heads, head_dim / 2, 2])?;
    let x1 = pairs.try_index((.., .., .., 0))?;
    let x2 = pairs.try_index((.., .., .., 1))?;
    let cos = cos.expand_dims(1)?.as_dtype(x.dtype())?;
    let sin = sin.expand_dims(1)?.as_dtype(x.dtype())?;
    let o1 = x1.multiply(&cos)?.subtract(x2.multiply(&sin)?)?;
    let o2 = x2.multiply(&cos)?.add(x1.multiply(&sin)?)?;
    let out = ops::concatenate(&[o1.expand_dims(-1)?, o2.expand_dims(-1)?], -1)?;
    Ok(out.reshape(&[t, heads * head_dim])?)
}

/// Le masque d'une attention (`ScaledDotProductAttentionMaskMode`).
pub enum Mask {
    None,
    Causal,
    Array(Array),
}

impl Mask {
    pub fn mode(&self) -> Option<ScaledDotProductAttentionMask<'_>> {
        match self {
            Self::None => None,
            Self::Causal => Some(ScaledDotProductAttentionMask::Causal),
            Self::Array(mask) => Some(ScaledDotProductAttentionMask::Array(mask)),
        }
    }
}

/// Le masque à fenêtre glissante des positions `positions` sur les clés
/// `[offset, offset + kv_len)` : 0 où permis, −1e9 ailleurs, au type des
/// requêtes.
pub fn window_mask(
    positions: &Array,
    offset: i32,
    kv_len: i32,
    window: i32,
    dtype: Dtype,
) -> Result<Array> {
    let q = positions.expand_dims(1)?;
    let k = range(offset, offset + kv_len).expand_dims(0)?;
    let causal = k.le(&q)?;
    let inside = k.ge(q.subtract(Array::from_int(window - 1))?)?;
    let allowed = ops::logical_and(&causal, &inside)?;
    Ok(ops::select(&allowed, scalar(0.0), scalar(-1e9))?.as_dtype(dtype)?)
}

/// Le cache d'une couche de l'encodeur.
#[derive(Clone)]
pub struct KvCache {
    pub keys: Array,
    pub values: Array,
    pub position_offset: i32,
}

/// Les retenues des convolutions (`VoxtralRealtimeConvStemState`).
#[derive(Default)]
pub struct ConvState {
    pub conv1: Option<Array>,
    pub conv2: Option<Array>,
}

/// `VoxtralRealtimeEncoderAttentionInputs` : RoPE et masque, communs à
/// toutes les couches d'un passage.
struct Inputs {
    cos: Array,
    sin: Array,
    mask: Mask,
}

fn inputs(
    positions: &Array,
    seq_len: i32,
    caches: &[Option<KvCache>],
    config: &EncoderConfig,
    dtype: Dtype,
) -> Result<Inputs> {
    let (cos, sin) = rope_frequencies(positions, config.head_dim, config.rope_theta)?;
    let first = caches.first().and_then(Option::as_ref);
    let cached_len = first.map_or(0, |c| c.keys.shape()[0]);
    let cached_offset = first.map_or(0, |c| c.position_offset);
    // Le Swift en fait un `precondition` : les couches avancent ensemble.
    let lockstep = caches.iter().all(|c| {
        c.is_none() == first.is_none()
            && c.as_ref().map_or(0, |c| c.keys.shape()[0]) == cached_len
            && c.as_ref().map_or(0, |c| c.position_offset) == cached_offset
    });
    if !lockstep {
        return Err(anyhow!(crate::tr!(
            "Voxtral: the encoder layer caches no longer advance together",
            "Voxtral : les caches des couches de l'encodeur n'avancent plus ensemble"
        )));
    }
    let window = config.sliding_window;
    let (mut offset, mut kv_len) = (cached_offset, cached_len + seq_len);
    if kv_len > window {
        offset += kv_len - window;
        kv_len = window;
    }
    let mask = if seq_len == 1 {
        Mask::None
    } else if first.is_none() && seq_len <= window {
        Mask::Causal
    } else {
        Mask::Array(window_mask(positions, offset, kv_len, window, dtype)?)
    };
    Ok(Inputs { cos, sin, mask })
}

struct Attention {
    wq: Linear,
    wk: Linear,
    wv: Linear,
    wo: Linear,
    heads: i32,
    head_dim: i32,
    window: i32,
    scale: f32,
}

impl Attention {
    fn load(w: &mut Weights, prefix: &str, c: &EncoderConfig) -> Result<Self> {
        let inner = c.n_heads * c.head_dim;
        Ok(Self {
            wq: linear(w, &format!("{prefix}.wq"), c.dim, inner, true)?,
            wk: linear(w, &format!("{prefix}.wk"), c.dim, inner, false)?,
            wv: linear(w, &format!("{prefix}.wv"), c.dim, inner, true)?,
            wo: linear(w, &format!("{prefix}.wo"), inner, c.dim, true)?,
            heads: c.n_heads,
            head_dim: c.head_dim,
            window: c.sliding_window,
            scale: (c.head_dim as f32).powf(-0.5),
        })
    }

    fn forward(
        &self,
        x: &Array,
        inputs: &Inputs,
        cache: Option<&KvCache>,
    ) -> Result<(Array, KvCache)> {
        let t = x.shape()[0];
        let (h, d) = (self.heads, self.head_dim);
        let q = apply_interleaved_rope(&self.wq.forward(x)?, &inputs.cos, &inputs.sin, h, d)?;
        let mut k = apply_interleaved_rope(&self.wk.forward(x)?, &inputs.cos, &inputs.sin, h, d)?;
        let mut v = self.wv.forward(x)?;
        let mut offset = cache.map_or(0, |c| c.position_offset);
        if let Some(cache) = cache {
            k = ops::concatenate(&[&cache.keys, &k], 0)?;
            v = ops::concatenate(&[&cache.values, &v], 0)?;
        }
        let mut kv_len = k.shape()[0];
        if kv_len > self.window {
            let trim = kv_len - self.window;
            k = k.try_index(trim..)?;
            v = v.try_index(trim..)?;
            kv_len = self.window;
            offset += trim;
        }
        let cache = KvCache {
            keys: k.clone(),
            values: v.clone(),
            position_offset: offset,
        };
        let q4 = q.reshape(&[1, t, h, d])?.transpose_axes(&[0, 2, 1, 3])?;
        let k4 = k
            .reshape(&[1, kv_len, h, d])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let v4 = v
            .reshape(&[1, kv_len, h, d])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let attn = fast::scaled_dot_product_attention(
            q4,
            k4,
            v4,
            self.scale,
            inputs.mask.mode(),
            None::<&Array>,
        )?;
        let out = attn.transpose_axes(&[0, 2, 1, 3])?.reshape(&[t, h * d])?;
        Ok((self.wo.forward(&out)?, cache))
    }
}

struct Layer {
    attention_norm: RmsNorm,
    attention: Attention,
    ffn_norm: RmsNorm,
    w1: Linear,
    w3: Linear,
    w2: Linear,
}

impl Layer {
    fn load(w: &mut Weights, prefix: &str, c: &EncoderConfig) -> Result<Self> {
        Ok(Self {
            attention_norm: RmsNorm::load(
                w,
                &format!("{prefix}.attention_norm"),
                c.dim,
                c.norm_eps,
            )?,
            attention: Attention::load(w, &format!("{prefix}.attention"), c)?,
            ffn_norm: RmsNorm::load(w, &format!("{prefix}.ffn_norm"), c.dim, c.norm_eps)?,
            w1: linear(
                w,
                &format!("{prefix}.feed_forward_w1"),
                c.dim,
                c.hidden_dim,
                false,
            )?,
            w3: linear(
                w,
                &format!("{prefix}.feed_forward_w3"),
                c.dim,
                c.hidden_dim,
                false,
            )?,
            w2: linear(
                w,
                &format!("{prefix}.feed_forward_w2"),
                c.hidden_dim,
                c.dim,
                true,
            )?,
        })
    }

    fn forward(
        &self,
        x: &Array,
        inputs: &Inputs,
        cache: Option<&KvCache>,
    ) -> Result<(Array, KvCache)> {
        let h = self.attention_norm.forward(x)?;
        let (h, cache) = self.attention.forward(&h, inputs, cache)?;
        let out = x.add(&h)?;
        let h = self.ffn_norm.forward(&out)?;
        let gate = nn::silu(self.w1.forward(&h)?)?;
        let up = self.w3.forward(&h)?;
        let out = out.add(self.w2.forward(&gate.multiply(&up)?)?)?;
        Ok((out, cache))
    }
}

/// `VoxtralRealtimeCausalConv1d` : la convolution et sa marge causale.
struct CausalConv {
    conv: Conv1d,
    kernel: i32,
    stride: i32,
}

impl CausalConv {
    fn padding(&self) -> i32 {
        self.kernel - self.stride
    }
}

/// L'état de l'encodeur d'un flux (`VoxtralRealtimeStreamEncoderState`).
pub struct StreamState {
    pub caches: Vec<Option<KvCache>>,
    /// La trame de convolution (absolue) où commence le bloc en cours.
    pub block_base: i32,
    /// Les trames de convolution déjà passées au Transformer.
    pub consumed: i32,
}

/// `VoxtralRealtimeAudioEncoder`.
pub struct Encoder {
    pub config: EncoderConfig,
    conv0: CausalConv,
    conv1: CausalConv,
    layers: Vec<Layer>,
    norm: RmsNorm,
    projection0: Linear,
    projection2: Linear,
}

impl Encoder {
    pub fn load(w: &mut Weights, c: &EncoderConfig, decoder_dim: i32) -> Result<Self> {
        let conv = |w: &mut Weights, i: usize, input: i32, stride: i32| -> Result<CausalConv> {
            let shape = ConvShape {
                input,
                output: c.dim,
                kernel: 3,
                stride,
                padding: 0,
                groups: 1,
                bias: true,
            };
            Ok(CausalConv {
                conv: Conv1d::load(w, &format!("encoder.conv_layers_{i}_conv.conv"), shape)?,
                kernel: 3,
                stride,
            })
        };
        let conv0 = conv(w, 0, MEL_BINS, 1)?;
        let conv1 = conv(w, 1, c.dim, 2)?;
        let layers = (0..c.n_layers)
            .map(|i| Layer::load(w, &format!("encoder.transformer_layers.{i}"), c))
            .collect::<Result<_>>()?;
        let norm = RmsNorm::load(w, "encoder.transformer_norm", c.dim, c.norm_eps)?;
        // Jamais quantifiées (`shouldQuantize`) : des `Linear` en clair.
        let dense = |w: &mut Weights, i: usize, input: i32| -> Result<Linear> {
            let key = format!("encoder.audio_language_projection_{i}.weight");
            Ok(Linear::Dense {
                weight: w.take(&key, &[decoder_dim, input])?,
                bias: None,
            })
        };
        let projection0 = dense(w, 0, c.dim * c.downsample_factor)?;
        let projection2 = dense(w, 2, decoder_dim)?;
        Ok(Self {
            config: c.clone(),
            conv0,
            conv1,
            layers,
            norm,
            projection0,
            projection2,
        })
    }

    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Des zéros `[1, n, largeur]` au type des poids de la convolution.
    fn zeros(&self, n: i32, width: i32) -> Result<Array> {
        Ok(ops::zeros::<f32>(&[1, n, width])?.as_dtype(self.conv0.conv.dtype())?)
    }

    /// `convStemStep` : de nouvelles colonnes de mel `[bandes, n]` → chaque
    /// ligne de convolution qu'elles complètent, `[lignes, largeur]`, ou
    /// rien. La première convolution (noyau 3, pas 1) retient ses 2 dernières
    /// entrées ; la seconde (noyau 3, pas 2) la suite depuis le début de sa
    /// fenêtre suivante.
    pub fn conv_stem_step(&self, mel: &Array, state: &mut ConvState) -> Result<Option<Array>> {
        if mel.shape()[1] == 0 {
            return Ok(None);
        }
        let x = mel
            .as_dtype(self.conv0.conv.dtype())?
            .transpose_axes(&[1, 0])?
            .expand_dims(0)?;
        let pad1 = self.conv0.padding();
        let carry1 = match state.conv1.take() {
            Some(carry) => carry,
            None => self.zeros(pad1, x.shape()[2])?,
        };
        let in1 = ops::concatenate(&[&carry1, &x], 1)?;
        let len1 = in1.shape()[1];
        state.conv1 = Some(in1.try_index((.., (len1 - pad1).., ..))?);
        let h = nn::gelu(self.conv0.conv.forward(&in1)?)?;

        let pad2 = self.conv1.padding();
        let carry2 = match state.conv2.take() {
            Some(carry) => carry,
            None => self.zeros(pad2, h.shape()[2])?,
        };
        let in2 = ops::concatenate(&[&carry2, &h], 1)?;
        let len2 = in2.shape()[1];
        let (kernel, stride) = (self.conv1.kernel, self.conv1.stride);
        let rows = if len2 >= kernel {
            (len2 - kernel) / stride + 1
        } else {
            0
        };
        state.conv2 = Some(in2.try_index((.., (rows * stride).., ..))?);
        if rows == 0 {
            return Ok(None);
        }
        Ok(Some(
            nn::gelu(self.conv1.conv.forward(&in2)?)?.squeeze_axes(&[0])?,
        ))
    }

    /// `encodeIncremental` : un bloc de lignes aux positions
    /// `[start, start + n)` par le Transformer, avec les caches de chaque
    /// couche ; rend les lignes normées (avant le sous-échantillonnage).
    fn encode_incremental(
        &self,
        block: &Array,
        start: i32,
        caches: &mut [Option<KvCache>],
    ) -> Result<Array> {
        let n = block.shape()[0];
        let positions = range(start, start + n);
        let inputs = inputs(&positions, n, caches, &self.config, block.dtype())?;
        let mut x = block.clone();
        for (layer, cache) in self.layers.iter().zip(caches.iter_mut()) {
            let (next, updated) = layer.forward(&x, &inputs, cache.as_ref())?;
            x = next;
            *cache = Some(updated);
        }
        self.norm.forward(&x)
    }

    /// `feedIncremental` : les lignes `[consumed, up_to)` (`rows` commence à
    /// la ligne absolue `start`), par blocs alignés sur `sliding_window`,
    /// caches remis à zéro à chaque frontière ; rend les lignes normées
    /// nouvelles.
    pub fn feed_incremental(
        &self,
        rows: &Array,
        start: i32,
        up_to: i32,
        state: &mut StreamState,
    ) -> Result<Option<Array>> {
        let window = self.config.sliding_window;
        let mut pieces = Vec::new();
        while state.consumed < up_to {
            let block_end = state.block_base + window;
            let end = up_to.min(block_end);
            let block = rows.try_index((state.consumed - start)..(end - start))?;
            let relative = state.consumed - state.block_base;
            pieces.push(self.encode_incremental(&block, relative, &mut state.caches)?);
            state.consumed = end;
            if state.consumed == block_end {
                state.caches = (0..self.layers.len()).map(|_| None).collect();
                state.block_base = block_end;
            }
        }
        Ok(match pieces.len() {
            0 => None,
            1 => pieces.pop(),
            _ => Some(ops::concatenate(&pieces, 0)?),
        })
    }

    /// `downsampleAndProject` : 4 lignes par ligne, puis
    /// `projection2(gelu(projection0(x)))`.
    pub fn downsample_and_project(&self, encoded: &Array) -> Result<Array> {
        let ds = self.config.downsample_factor;
        let len = encoded.shape()[0] / ds;
        if len == 0 {
            return Ok(encoded.try_index(0..0)?);
        }
        let x = encoded
            .try_index(0..len * ds)?
            .reshape(&[len, self.config.dim * ds])?;
        let x = nn::gelu(self.projection0.forward(&x)?)?;
        self.projection2.forward(&x)
    }
}
