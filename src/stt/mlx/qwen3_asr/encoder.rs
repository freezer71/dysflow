//! L'encodeur audio de Qwen3-ASR (`Qwen3ASRAudioEncoder`) : le mel coupé
//! en tranches de `2 · n_window` trames (complétées de zéros), trois
//! convolutions 2-D de pas 2 (`gelu`), une projection, les positions
//! sinusoïdales, puis les couches Transformer par fenêtres de
//! `n_window_infer / (2 · n_window)` tranches (attention pleine dans chaque
//! fenêtre, aucune entre fenêtres ; les fenêtres de même longueur passent en
//! un lot), la norme finale et deux projections. Mêmes opérations dans le
//! même ordre et aux mêmes points d'évaluation que le Swift ; les poids
//! restent au type du fichier (bf16), le mel en Float32 : la sortie est en
//! Float32, comme dans le Swift.

use anyhow::{anyhow, bail, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{nn, ops, Array, Dtype};
use std::collections::BTreeMap;

use super::audio::{length_after_conv_stem, window_lengths};
use super::config::{AudioConfig, Config, NAME};
use crate::stt::mlx::nn::{scalar, Conv2d, ConvShape, LayerNorm, Linear};
use crate::stt::mlx::weights::Weights;

/// Les tranches passées ensemble dans les convolutions (`convBatchSize`).
const CONV_BATCH: usize = 128;
/// Les fenêtres de même longueur passées ensemble dans les couches
/// (`encoderBatchSize`).
const ENCODER_BATCH: usize = 256;

/// Une couche dense de l'encodeur : jamais quantifiée (le Swift ne quantifie
/// pas `audio_tower`, et refuserait des `scales` en trop).
fn dense(w: &mut Weights, prefix: &str, input: i32, output: i32, bias: bool) -> Result<Linear> {
    if w.has(&format!("{prefix}.scales")) {
        bail!(crate::tr!(
            en = "{NAME}: {prefix} is quantized, but the Swift never quantizes the audio encoder",
            fr = "{NAME} : {prefix} est quantifié, mais le Swift ne quantifie jamais l'encodeur audio",
        ));
    }
    Linear::load(w, prefix, input, output, bias)
}

/// `Qwen3ASRSinusoidalPE` : `[longueur, canaux]`, sinus puis cosinus.
fn sinusoids(length: i32, channels: i32) -> Result<Array> {
    let half = channels / 2;
    let increment = 10_000f32.ln() / (half - 1) as f32;
    let inv = ops::exp(
        scalar(-increment)
            .multiply(Array::arange::<_, i32>(None, half, None)?.as_dtype(Dtype::Float32)?)?,
    )?;
    let positions = Array::arange::<_, i32>(None, length, None)?
        .as_dtype(Dtype::Float32)?
        .reshape(&[-1, 1])?;
    let scaled = positions.multiply(inv.reshape(&[1, -1])?)?;
    Ok(ops::concatenate(
        &[ops::sin(&scaled)?, ops::cos(&scaled)?],
        1,
    )?)
}

/// `Qwen3ASRAttention` : attention pleine, biais partout, sans masque.
struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    heads: i32,
    head_dim: i32,
    scale: f32,
}

impl Attention {
    fn load(w: &mut Weights, prefix: &str, c: &AudioConfig) -> Result<Self> {
        let d = c.d_model;
        let head_dim = d / c.encoder_attention_heads;
        Ok(Self {
            q: dense(w, &format!("{prefix}.q_proj"), d, d, true)?,
            k: dense(w, &format!("{prefix}.k_proj"), d, d, true)?,
            v: dense(w, &format!("{prefix}.v_proj"), d, d, true)?,
            out: dense(w, &format!("{prefix}.out_proj"), d, d, true)?,
            heads: c.encoder_attention_heads,
            head_dim,
            scale: (head_dim as f32).powf(-0.5),
        })
    }

    fn forward(&self, x: &Array) -> Result<Array> {
        let (b, l) = match x.shape() {
            [b, l, _] => (*b, *l),
            shape => bail!(crate::tr!(
                en = "{NAME}: encoder input {shape:?}, three axes expected",
                fr = "{NAME} : entrée de l'encodeur {shape:?}, trois axes attendus",
            )),
        };
        let split = |y: Array| -> Result<Array> {
            Ok(y.reshape(&[b, l, self.heads, self.head_dim])?
                .transpose_axes(&[0, 2, 1, 3])?)
        };
        let q = split(self.q.forward(x)?)?;
        let k = split(self.k.forward(x)?)?;
        let v = split(self.v.forward(x)?)?;
        let attended = fast::scaled_dot_product_attention(
            q,
            k,
            v,
            self.scale,
            None::<ScaledDotProductAttentionMask>,
            None::<&Array>,
        )?;
        let merged =
            attended
                .transpose_axes(&[0, 2, 1, 3])?
                .reshape(&[b, l, self.heads * self.head_dim])?;
        self.out.forward(&merged)
    }
}

/// `Qwen3ASRAudioEncoderLayer` : normes en tête, attention puis MLP `gelu`.
struct Layer {
    attn: Attention,
    attn_norm: LayerNorm,
    fc1: Linear,
    fc2: Linear,
    final_norm: LayerNorm,
}

impl Layer {
    fn load(w: &mut Weights, prefix: &str, c: &AudioConfig) -> Result<Self> {
        let d = c.d_model;
        Ok(Self {
            attn: Attention::load(w, &format!("{prefix}.self_attn"), c)?,
            attn_norm: LayerNorm::load(w, &format!("{prefix}.self_attn_layer_norm"), d)?,
            fc1: dense(w, &format!("{prefix}.fc1"), d, c.encoder_ffn_dim, true)?,
            fc2: dense(w, &format!("{prefix}.fc2"), c.encoder_ffn_dim, d, true)?,
            final_norm: LayerNorm::load(w, &format!("{prefix}.final_layer_norm"), d)?,
        })
    }

    fn forward(&self, x: &Array) -> Result<Array> {
        let h = x.add(self.attn.forward(&self.attn_norm.forward(x)?)?)?;
        let y = self
            .fc2
            .forward(&nn::gelu(self.fc1.forward(&self.final_norm.forward(&h)?)?)?)?;
        Ok(h.add(y)?)
    }
}

pub struct Encoder {
    conv1: Conv2d,
    conv2: Conv2d,
    conv3: Conv2d,
    conv_out: Linear,
    positions: Array,
    layers: Vec<Layer>,
    ln_post: LayerNorm,
    proj1: Linear,
    proj2: Linear,
    n_mels: i32,
    chunk: i32,
    chunks_per_window: usize,
}

impl Encoder {
    /// Les poids sous `audio_tower.`, chaque clé avec sa forme.
    pub fn load(w: &mut Weights, config: &Config) -> Result<Self> {
        let c = &config.audio;
        let hidden = c.downsample_hidden_size;
        let conv = |w: &mut Weights, name: &str, input: i32| {
            Conv2d::load(
                w,
                &format!("audio_tower.{name}"),
                ConvShape {
                    input,
                    output: hidden,
                    kernel: 3,
                    stride: 2,
                    padding: 1,
                    groups: 1,
                    bias: true,
                },
            )
        };
        let conv1 = conv(w, "conv2d1", 1)?;
        let conv2 = conv(w, "conv2d2", hidden)?;
        let conv3 = conv(w, "conv2d3", hidden)?;
        let flat = hidden
            .checked_mul(config.freq_after_conv())
            .ok_or_else(|| {
                anyhow!(crate::tr!(
                    en = "{NAME}: convolution width too large",
                    fr = "{NAME} : largeur des convolutions trop grande",
                ))
            })?;
        let conv_out = dense(w, "audio_tower.conv_out", flat, c.d_model, false)?;
        let positions = sinusoids(c.max_source_positions, c.d_model)?;
        positions.eval()?;
        let layers = (0..c.encoder_layers)
            .map(|i| Layer::load(w, &format!("audio_tower.layers.{i}"), c))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            conv1,
            conv2,
            conv3,
            conv_out,
            positions,
            layers,
            ln_post: LayerNorm::load(w, "audio_tower.ln_post", c.d_model)?,
            proj1: dense(w, "audio_tower.proj1", c.d_model, c.d_model, true)?,
            proj2: dense(w, "audio_tower.proj2", c.d_model, c.output_dim, true)?,
            n_mels: c.num_mel_bins,
            chunk: c.n_window * 2,
            chunks_per_window: usize::try_from((c.n_window_infer / (c.n_window * 2)).max(1))
                .unwrap_or(1),
        })
    }

    /// `callAsFunction` d'un mel `[1, bandes, trames]` (une entrée, toutes
    /// ses trames valides) : `[positions, output_dim]`.
    pub fn forward(&self, features: &Array) -> Result<Array> {
        let frames = match features.shape() {
            [1, mels, frames] if *mels == self.n_mels => *frames,
            shape => bail!(crate::tr!(
                en = "{NAME}: encoder input {shape:?} instead of [1, {}, frames]",
                fr = "{NAME} : entrée de l'encodeur {shape:?} au lieu de [1, {}, trames]",
                self.n_mels,
            )),
        };
        let feat = features.try_index(0)?;
        let chunk = self.chunk;
        let count = (frames + chunk - 1) / chunk;
        let mut lengths = Vec::new();
        let mut padded = Vec::new();
        let mut pos = 0;
        for j in 0..count {
            let clen = if j == count - 1 && frames % chunk != 0 {
                frames % chunk
            } else {
                chunk
            };
            let piece = feat.try_index((.., pos..pos + clen))?;
            padded.push(if clen < chunk {
                ops::pad(&piece, &[(0, 0), (0, chunk - clen)][..], None, None)?
            } else {
                piece
            });
            lengths.push(clen);
            pos += clen;
        }
        let after_cnn: Vec<i32> = lengths.iter().map(|&l| length_after_conv_stem(l)).collect();

        // Les convolutions, par lots de 128 tranches, évaluées chacune.
        let mut hidden = Vec::new();
        let mut index = 0usize;
        for batch in padded.chunks(CONV_BATCH) {
            let n = batch.len() as i32;
            let mut x = ops::stack(batch, 0)?.expand_dims(-1)?;
            x = nn::gelu(self.conv1.forward(&x)?)?;
            x = nn::gelu(self.conv2.forward(&x)?)?;
            x = nn::gelu(self.conv3.forward(&x)?)?;
            let [_, f, t, c] = match x.shape() {
                [b, f, t, c] => [*b, *f, *t, *c],
                shape => bail!(crate::tr!(
                    en = "{NAME}: convolution output {shape:?}",
                    fr = "{NAME} : sortie des convolutions {shape:?}",
                )),
            };
            x = x.transpose_axes(&[0, 2, 3, 1])?.reshape(&[n, t, c * f])?;
            x = self.conv_out.forward(&x)?;
            let time = x.shape()[1];
            let positions = self.positions.try_index(0..time)?;
            x = x.add(positions.expand_dims(0)?)?;
            x.eval()?;
            for i in 0..n {
                let valid = after_cnn.get(index).copied().unwrap_or(0);
                hidden.push(x.try_index((i, 0..valid))?);
                index += 1;
            }
        }
        let states = ops::concatenate(&hidden, 0)?;

        // Les couches, par fenêtres ; les fenêtres de même longueur en un lot.
        let windows = window_lengths(&after_cnn, &[lengths.len()], self.chunks_per_window);
        let total = states.shape()[0];
        let mut by_length: BTreeMap<i32, Vec<(usize, Array)>> = BTreeMap::new();
        let (mut offset, mut window_index) = (0, 0usize);
        for length in windows {
            let end = (offset + length).min(total);
            if offset >= end {
                continue;
            }
            by_length
                .entry(end - offset)
                .or_default()
                .push((window_index, states.try_index(offset..end)?));
            offset = end;
            window_index += 1;
        }
        if offset < total {
            by_length
                .entry(total - offset)
                .or_default()
                .push((window_index, states.try_index(offset..total)?));
        }
        let mut processed: Vec<(usize, Array)> = Vec::new();
        for group in by_length.values() {
            for batch in group.chunks(ENCODER_BATCH) {
                let items: Vec<&Array> = batch.iter().map(|(_, a)| a).collect();
                let mut x = ops::stack(&items, 0)?;
                for layer in &self.layers {
                    x = layer.forward(&x)?;
                }
                x.eval()?;
                for (j, (window, _)) in batch.iter().enumerate() {
                    processed.push((*window, x.try_index(j as i32)?));
                }
            }
        }
        processed.sort_by_key(|(window, _)| *window);
        let parts: Vec<Array> = processed.into_iter().map(|(_, a)| a).collect();
        let states = ops::concatenate(&parts, 0)?;

        let states = self.ln_post.forward(&states)?;
        let states = nn::gelu(self.proj1.forward(&states)?)?;
        self.proj2.forward(&states)
    }
}
