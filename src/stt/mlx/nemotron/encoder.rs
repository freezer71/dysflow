//! L'encodeur Conformer causal de Nemotron, en flux : sous-échantillonnage
//! causal (`NemotronASRCausalDwStridingSubsampling`), blocs Conformer à
//! caches (`nemoStreamBlock` de `NemotronASRStreaming.swift`) et noyau de
//! l'invite de langue (`NemotronASRPromptKernel`). Port de
//! `NemotronASRConformer.swift` et `NemotronASRModel.swift`, pour le chemin
//! du flux seulement (sans masque d'attention : la tranche voit tout son
//! cache).

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{nn, ops, Array};

use super::config::{EncoderConfig, PromptConfig};
use crate::stt::mlx::nemo::attention::{RelPositionMultiHeadAttention, RelPositionalEncoding};
use crate::stt::mlx::nn::{scalar, scalar_like, Conv1d, Conv2d, ConvShape, LayerNorm, Linear};
use crate::stt::mlx::parakeet::conformer::FeedForward;
use crate::stt::mlx::weights::Weights;

/// Le sous-échantillonnage causal : noyau 3, pas 2, marge 2 avant et 1
/// après, sur le temps et les bandes.
const STRIDE: i32 = 2;
const KERNEL: i32 = 3;
const PAD_LEFT: i32 = 2;
const PAD_RIGHT: i32 = 1;

/// `NemotronASRCausalDwStridingSubsampling` : une convolution 3 × 3 de
/// pas 2, puis des paires (par canal, ponctuelle), chacune après la marge
/// causale et suivie de ReLU ; enfin la projection des canaux × bandes.
struct Subsampling {
    conv0: Conv2d,
    depthwise: Vec<Conv2d>,
    pointwise: Vec<Conv2d>,
    out: Linear,
}

impl Subsampling {
    fn load(w: &mut Weights, prefix: &str, args: &EncoderConfig) -> Result<Self> {
        let sampling_num = args.sampling_num();
        let channels = args.subsampling_conv_channels;
        let mut freq = args.feat_in;
        for _ in 0..sampling_num {
            freq = (((freq + PAD_LEFT + PAD_RIGHT - KERNEL) as f64 / STRIDE as f64).floor() + 1.0)
                as i32;
            freq = freq.max(1);
        }
        let conv = |input, kernel, stride, groups| ConvShape {
            input,
            output: channels,
            kernel,
            stride,
            padding: 0,
            groups,
            bias: true,
        };
        let conv0 = Conv2d::load(w, &format!("{prefix}.conv0"), conv(1, 3, 2, 1))?;
        let (mut depthwise, mut pointwise) = (Vec::new(), Vec::new());
        for i in 0..(sampling_num - 1).max(0) {
            depthwise.push(Conv2d::load(
                w,
                &format!("{prefix}.depthwise_layers.{i}"),
                conv(channels, 3, 2, channels),
            )?);
            pointwise.push(Conv2d::load(
                w,
                &format!("{prefix}.pointwise_layers.{i}"),
                conv(channels, 1, 1, 1),
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
        })
    }

    fn causal_pad(x: &Array) -> Result<Array> {
        let pad = (PAD_LEFT, PAD_RIGHT);
        Ok(ops::pad(x, &[(0, 0), pad, pad, (0, 0)], None, None)?)
    }

    /// `x` : `[1, trames, bandes]` ; rend `[1, trames sous-échantillonnées,
    /// d]` (la longueur, que le flux ignore, n'est pas calculée).
    fn forward(&self, x: &Array) -> Result<Array> {
        let mut y = x.expand_dims(3)?;
        y = Self::causal_pad(&y)?;
        y = nn::relu(self.conv0.forward(&y)?)?;
        for (depthwise, pointwise) in self.depthwise.iter().zip(&self.pointwise) {
            y = Self::causal_pad(&y)?;
            y = nn::relu(pointwise.forward(&depthwise.forward(&y)?)?)?;
        }
        let s = y.shape();
        let (batch, time, freq, channels) = (s[0], s[1], s[2], s[3]);
        let y = y
            .transpose_axes(&[0, 1, 3, 2])?
            .reshape(&[batch, time, channels * freq])?;
        self.out.forward(&y)
    }
}

/// `NemotronASRConvolution` : porte GLU, convolution causale par canal,
/// `LayerNorm` (clé `batch_norm`), SiLU, projection. En flux, la marge
/// causale est le cache des `noyau − 1` dernières sorties de la porte.
struct Convolution {
    pointwise1: Conv1d,
    depthwise: Conv1d,
    norm: LayerNorm,
    pointwise2: Conv1d,
}

impl Convolution {
    fn load(w: &mut Weights, prefix: &str, args: &EncoderConfig) -> Result<Self> {
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
        Ok(Self {
            pointwise1: Conv1d::load(w, &format!("{prefix}.pointwise_conv1"), pointwise(2 * d))?,
            depthwise: Conv1d::load(
                w,
                &format!("{prefix}.depthwise_conv"),
                ConvShape {
                    input: d,
                    output: d,
                    kernel: args.conv_kernel_size,
                    stride: 1,
                    padding: 0,
                    groups: d,
                    bias,
                },
            )?,
            norm: LayerNorm::load(w, &format!("{prefix}.batch_norm"), d)?,
            pointwise2: Conv1d::load(w, &format!("{prefix}.pointwise_conv2"), pointwise(d))?,
        })
    }
}

/// Un bloc Conformer : demi-FFN, attention, convolution, demi-FFN, norme.
struct Block {
    norm_ff1: LayerNorm,
    ff1: FeedForward,
    norm_att: LayerNorm,
    attention: RelPositionMultiHeadAttention,
    norm_conv: LayerNorm,
    conv: Convolution,
    norm_ff2: LayerNorm,
    ff2: FeedForward,
    norm_out: LayerNorm,
}

/// Ce qu'un bloc laisse à la tranche suivante : les dernières entrées de
/// l'attention et les dernières sorties de la porte de la convolution.
#[derive(Clone, Default)]
pub struct LayerCache {
    pub attention: Option<Array>,
    pub conv: Option<Array>,
}

impl Block {
    fn load(w: &mut Weights, prefix: &str, args: &EncoderConfig) -> Result<Self> {
        let d = args.d_model;
        let ff_hidden = d * args.ff_expansion_factor;
        let norm = |w: &mut Weights, name: &str| LayerNorm::load(w, &format!("{prefix}.{name}"), d);
        Ok(Self {
            norm_ff1: norm(w, "norm_feed_forward1")?,
            ff1: FeedForward::load(
                w,
                &format!("{prefix}.feed_forward1"),
                d,
                ff_hidden,
                args.use_bias,
            )?,
            norm_att: norm(w, "norm_self_att")?,
            attention: RelPositionMultiHeadAttention::load(
                w,
                &format!("{prefix}.self_attn"),
                args.n_heads,
                d,
                args.use_bias,
            )?,
            norm_conv: norm(w, "norm_conv")?,
            conv: Convolution::load(w, &format!("{prefix}.conv"), args)?,
            norm_ff2: norm(w, "norm_feed_forward2")?,
            ff2: FeedForward::load(
                w,
                &format!("{prefix}.feed_forward2"),
                d,
                ff_hidden,
                args.use_bias,
            )?,
            norm_out: norm(w, "norm_out")?,
        })
    }

    /// `nemoStreamBlock` : la tranche `x` `[1, c, d]` attend sur `[cache,
    /// tranche]` (positions décalées de la longueur du cache, aucun masque),
    /// et sa convolution causale lit le cache de la porte au lieu de zéros.
    /// Rend la sortie et les caches suivants (`left_cache` entrées de
    /// l'attention, `conv_left` sorties de la porte).
    fn stream(
        &self,
        x: &Array,
        cache: &LayerCache,
        pos_enc: &mut RelPositionalEncoding,
        left_cache: i32,
        conv_left: i32,
    ) -> Result<(Array, LayerCache)> {
        let half = |x: &Array| scalar_like(0.5, x);
        let mut residual =
            x.add(half(x)?.multiply(self.ff1.forward(&self.norm_ff1.forward(x)?)?)?)?;

        let xn = self.norm_att.forward(&residual)?;
        let (kv, cache_len) = match &cache.attention {
            Some(c) => (ops::concatenate(&[c, &xn], 1)?, c.shape()[1]),
            None => (xn.clone(), 0),
        };
        let (_, pos_emb) = pos_enc.forward_offset(&xn, cache_len)?;
        residual = residual.add(self.attention.forward(&xn, &kv, &kv, &pos_emb, None)?)?;
        let kv_len = kv.shape()[1];
        let attention = kv.try_index((.., (kv_len - left_cache).max(0)..kv_len, ..))?;

        let xc = self.norm_conv.forward(&residual)?;
        let halves = self.conv.pointwise1.forward(&xc)?.split_equal(2, 2)?;
        let [a, b] = halves.as_slice() else {
            bail!("convolution : {} moitiés au lieu de 2", halves.len());
        };
        let gated = a.multiply(ops::sigmoid(b)?)?;
        let previous = match &cache.conv {
            Some(c) => c.clone(),
            None => {
                let s = gated.shape();
                ops::zeros_dtype(&[s[0], conv_left, s[2]], gated.dtype())?
            }
        };
        let din = ops::concatenate(&[&previous, &gated], 1)?;
        let dw = self.conv.depthwise.forward(&din)?;
        let din_len = din.shape()[1];
        let conv = din.try_index((.., (din_len - conv_left).max(0)..din_len, ..))?;
        let y = nn::silu(self.conv.norm.forward(&dw)?)?;
        residual = residual.add(self.conv.pointwise2.forward(&y)?)?;

        let ff2 = self.ff2.forward(&self.norm_ff2.forward(&residual)?)?;
        residual = residual.add(half(&residual)?.multiply(ff2)?)?;
        let next = LayerCache {
            attention: Some(attention),
            conv: Some(conv),
        };
        Ok((self.norm_out.forward(&residual)?, next))
    }
}

/// `NemotronASRConformer` : positions relatives, sous-échantillonnage,
/// blocs.
pub struct Encoder {
    pos_enc: RelPositionalEncoding,
    pre_encode: Subsampling,
    layers: Vec<Block>,
}

impl Encoder {
    pub fn load(w: &mut Weights, args: &EncoderConfig) -> Result<Self> {
        let layers = (0..args.n_layers)
            .map(|i| Block::load(w, &format!("encoder.layers.{i}"), args))
            .collect::<Result<_>>()?;
        Ok(Self {
            pos_enc: RelPositionalEncoding::new(args.d_model, args.pos_emb_max_len, args.xscaling)?,
            pre_encode: Subsampling::load(w, "encoder.pre_encode", args)?,
            layers,
        })
    }

    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// `preEncode(win, lengths:).0` : `[1, trames, bandes]` en `[1, k, d]`.
    pub fn pre_encode(&self, window: &Array) -> Result<Array> {
        self.pre_encode.forward(window)
    }

    /// Les blocs sur une tranche `[1, c, d]`, chacun avec son cache, mis à
    /// jour.
    pub fn stream_layers(
        &mut self,
        x: &Array,
        caches: &mut [LayerCache],
        left_cache: i32,
        conv_left: i32,
    ) -> Result<Array> {
        let mut h = x.clone();
        for (layer, cache) in self.layers.iter().zip(caches.iter_mut()) {
            let (out, next) = layer.stream(&h, cache, &mut self.pos_enc, left_cache, conv_left)?;
            h = out;
            *cache = next;
        }
        Ok(h)
    }
}

/// `NemotronASRPromptKernel` et `applyPrompt` : l'encodeur et l'invite
/// (une ligne de l'identité, `num_prompts` colonnes) mis côte à côte, puis
/// `linear2(relu(linear0(·)))`.
pub struct Prompt {
    linear0: Linear,
    linear2: Linear,
    num_prompts: i32,
}

impl Prompt {
    pub fn load(w: &mut Weights, d_model: i32, config: &PromptConfig) -> Result<Self> {
        Ok(Self {
            linear0: Linear::load(
                w,
                "prompt_kernel.linear0",
                d_model + config.num_prompts,
                config.prompt_hidden,
                true,
            )?,
            linear2: Linear::load(
                w,
                "prompt_kernel.linear2",
                config.prompt_hidden,
                d_model,
                true,
            )?,
            num_prompts: config.num_prompts,
        })
    }

    /// `encoded` `[lot, temps, d]` conditionné par l'invite `index`.
    pub fn apply(&self, encoded: &Array, index: i32) -> Result<Array> {
        let s = encoded.shape();
        let (batch, time) = (s[0], s[1]);
        let count = usize::try_from(batch * time).unwrap_or(0);
        let ids = Array::from_slice(&vec![index; count], &[batch, time]).expand_dims(2)?;
        let range = Array::arange::<_, i32>(None, self.num_prompts, None)?.reshape(&[
            1,
            1,
            self.num_prompts,
        ])?;
        let one_hot =
            ops::select(&range.eq(&ids)?, scalar(1.0), scalar(0.0))?.as_dtype(encoded.dtype())?;
        let conditioned = ops::concatenate(&[encoded, &one_hot], 2)?;
        self.linear2
            .forward(&nn::relu(self.linear0.forward(&conditioned)?)?)
    }
}
