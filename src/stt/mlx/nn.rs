//! Les couches de MLXNN qu'emploient les modèles de dictée : `Linear` (et
//! `QuantizedLinear`, et le `FiveValueLinear` de Phonon-2), `Embedding` (et
//! `QuantizedEmbedding`), `LayerNorm`, `RMSNorm`, `BatchNorm`, `Conv1d`, `Conv2d`,
//! `LSTM`, et le cache d'attention `KVCacheSimple`. Mêmes opérations que mlx-swift
//! 0.32 ; aucun opérateur surchargé de `mlx-rs` (ils paniquent) : chaque
//! opération rend son `Result`.

use anyhow::{anyhow, ensure, Result};
use mlx_rs::ops::indexing::{Ellipsis, TryIndexMutOp, TryIndexOp};
use mlx_rs::ops::{DequantizeOptions, QuantizedMatmulOptions};
use mlx_rs::{fast, ops, Array};

use super::weights::{Quantization, Weights};

/// Un flottant comme mlx-swift le met face à un tableau (`x * 0.5`) : du
/// type de `x` s'il est flottant, sinon `f32`.
pub fn scalar_like(value: f32, x: &Array) -> Result<Array> {
    let scalar = Array::from_f32(value);
    if x.dtype().is_float() {
        Ok(scalar.as_dtype(x.dtype())?)
    } else {
        Ok(scalar)
    }
}

/// `MLXArray(Float(value))` : un `f32`, sans conversion.
pub fn scalar(value: f32) -> Array {
    Array::from_f32(value)
}

/// La quantification d'une couche dont le fichier a les `scales` : celle de
/// `config.json`, sinon une erreur (le Swift laisserait la couche en clair,
/// puis refuserait ses clés en trop).
fn quantization_of(w: &Weights, prefix: &str) -> Result<Option<Quantization>> {
    if !w.has(&format!("{prefix}.scales")) {
        return Ok(None);
    }
    w.quantization.map(Some).ok_or_else(|| {
        anyhow!(crate::tr!(
            en = "{prefix} is quantized but config.json has no \"quantization\"",
            fr = "{prefix} est quantifié mais config.json n'a pas de « quantization »",
        ))
    })
}

/// Les formes d'une matrice `[sorties, entrées]` quantifiée : mots `uint32`
/// et échelles par groupe.
fn quantized_shapes(
    rows: i32,
    columns: i32,
    q: Quantization,
    prefix: &str,
) -> Result<([i32; 2], [i32; 2])> {
    ensure!(
        q.bits > 0
            && q.group_size > 0
            && (columns * q.bits) % 32 == 0
            && columns % q.group_size == 0,
        crate::tr!(
            en = "{prefix}: {columns} columns cannot be quantized in {} bits by groups of {}",
            fr = "{prefix} : {columns} colonnes ne se quantifient pas en {} bits par groupes de {}",
            q.bits,
            q.group_size,
        )
    );
    Ok((
        [rows, columns * q.bits / 32],
        [rows, columns / q.group_size],
    ))
}

/// Les groupes d'une couche à cinq valeurs (`FiveValueLinear.groupSize`).
pub const FIVE_VALUE_GROUP_SIZE: i32 = 128;
/// Le plan des magnitudes hautes d'une couche à cinq valeurs est rangé sous
/// `{préfixe}.hi.weight`, `.hi.scales` et `.hi.biases` ; celui des basses,
/// sous les clés d'une couche quantifiée.
pub const FIVE_VALUE_HI: &str = "hi";

/// Une matrice quantifiée : mots, échelles et décalages par groupe.
pub struct Packed {
    weight: Array,
    scales: Array,
    biases: Array,
    q: Quantization,
}

impl Packed {
    fn load(
        w: &mut Weights,
        prefix: &str,
        rows: i32,
        columns: i32,
        q: Quantization,
    ) -> Result<Self> {
        let (words, groups) = quantized_shapes(rows, columns, q, prefix)?;
        Ok(Self {
            weight: w.take(&format!("{prefix}.weight"), &words)?,
            scales: w.take(&format!("{prefix}.scales"), &groups)?,
            biases: w.take(&format!("{prefix}.biases"), &groups)?,
            q,
        })
    }

    /// `quantizedMM(x, weight, scales:, biases:, transpose: true, …)`.
    fn matmul(&self, x: &Array) -> Result<Array> {
        let options = QuantizedMatmulOptions {
            transpose: true,
            group_size: Some(self.q.group_size),
            bits: Some(self.q.bits),
            ..Default::default()
        };
        Ok(ops::quantized_matmul(
            x,
            &self.weight,
            &self.scales,
            &self.biases,
            options,
        )?)
    }
}

/// `Linear` : `addMM(bias, x, weightᵀ)`, ou `matmul` sans biais ;
/// `QuantizedLinear` : `quantizedMM(…, transpose: true)`, puis `+ bias` ;
/// `FiveValueLinear` (Phonon-2) : deux plans 2 bits par groupes de 128,
/// chacun par `quantizedMM`, sommés, sans biais.
pub enum Linear {
    Dense { weight: Array, bias: Option<Array> },
    Quantized { packed: Packed, bias: Option<Array> },
    FiveValue { lo: Packed, hi: Packed },
}

impl Linear {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        input: i32,
        output: i32,
        bias: bool,
    ) -> Result<Self> {
        // Comme `update(modules:)` du Swift : la couche à cinq valeurs prend
        // la place de la `Linear`, sans biais ; un biais du fichier resterait
        // en trop (`Weights::finish`).
        let hi = format!("{prefix}.{FIVE_VALUE_HI}");
        if w.has(&format!("{hi}.weight")) {
            let q = Quantization {
                group_size: FIVE_VALUE_GROUP_SIZE,
                bits: 2,
            };
            return Ok(Self::FiveValue {
                lo: Packed::load(w, prefix, output, input, q)?,
                hi: Packed::load(w, &hi, output, input, q)?,
            });
        }
        let quantized = quantization_of(w, prefix)?;
        let bias = if bias {
            Some(w.take(&format!("{prefix}.bias"), &[output])?)
        } else {
            None
        };
        Ok(match quantized {
            Some(q) => Self::Quantized {
                packed: Packed::load(w, prefix, output, input, q)?,
                bias,
            },
            None => Self::Dense {
                weight: w.take(&format!("{prefix}.weight"), &[output, input])?,
                bias,
            },
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        match self {
            Self::Dense {
                weight,
                bias: Some(bias),
            } => Ok(ops::addmm(bias, x, weight.transpose()?, None, None)?),
            Self::Dense { weight, bias: None } => Ok(x.matmul(weight.transpose()?)?),
            Self::Quantized { packed, bias } => {
                let y = packed.matmul(x)?;
                match bias {
                    Some(bias) => Ok(y.add(bias)?),
                    None => Ok(y),
                }
            }
            Self::FiveValue { lo, hi } => Ok(lo.matmul(x)?.add(hi.matmul(x)?)?),
        }
    }
}

/// `Embedding` : `weight[x]` ; `QuantizedEmbedding` : les lignes de `x`
/// déquantifiées, à la forme de `x` plus la dimension.
pub enum Embedding {
    Dense { weight: Array },
    Quantized { packed: Packed },
}

impl Embedding {
    pub fn load(w: &mut Weights, prefix: &str, count: i32, dim: i32) -> Result<Self> {
        Ok(match quantization_of(w, prefix)? {
            Some(q) => Self::Quantized {
                packed: Packed::load(w, prefix, count, dim, q)?,
            },
            None => Self::Dense {
                weight: w.take(&format!("{prefix}.weight"), &[count, dim])?,
            },
        })
    }

    pub fn forward(&self, ids: &Array) -> Result<Array> {
        match self {
            Self::Dense { weight } => Ok(weight.take_axis(ids, 0)?),
            Self::Quantized { packed } => {
                let mut shape = ids.shape().to_vec();
                let flat = ids.flatten(None, None)?;
                let options = DequantizeOptions {
                    group_size: Some(packed.q.group_size),
                    bits: Some(packed.q.bits),
                    ..Default::default()
                };
                let out = ops::dequantize(
                    packed.weight.take_axis(&flat, 0)?,
                    packed.scales.take_axis(&flat, 0)?,
                    &packed.biases.take_axis(&flat, 0)?,
                    options,
                )?;
                shape.push(-1);
                Ok(out.reshape(&shape)?)
            }
        }
    }

    /// `asLinear` : la projection par la matrice liée, `x · weightᵀ`, ou
    /// `quantizedMM(x, …, transpose: true)` (logits de Whisper).
    pub fn as_linear(&self, x: &Array) -> Result<Array> {
        match self {
            Self::Dense { weight } => Ok(x.matmul(weight.transpose()?)?),
            Self::Quantized { packed } => packed.matmul(x),
        }
    }
}

/// `LayerNorm` affine (`MLXFast.layerNorm`), eps 1e-5 par défaut.
pub struct LayerNorm {
    weight: Array,
    bias: Array,
    eps: f32,
}

impl LayerNorm {
    pub const EPS: f32 = 1e-5;

    pub fn load(w: &mut Weights, prefix: &str, dim: i32) -> Result<Self> {
        Ok(Self {
            weight: w.take(&format!("{prefix}.weight"), &[dim])?,
            bias: w.take(&format!("{prefix}.bias"), &[dim])?,
            eps: Self::EPS,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        Ok(fast::layer_norm(x, &self.weight, &self.bias, self.eps)?)
    }
}

/// `RMSNorm` de MLXNN : `MLXFast.rmsNorm(x, weight:, eps:)`.
pub struct RmsNorm {
    weight: Array,
    eps: f32,
}

impl RmsNorm {
    pub fn load(w: &mut Weights, prefix: &str, dim: i32, eps: f32) -> Result<Self> {
        Ok(Self {
            weight: w.take(&format!("{prefix}.weight"), &[dim])?,
            eps,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        Ok(fast::rms_norm(x, Some(&self.weight), self.eps)?)
    }
}

/// `BatchNorm` hors entraînement : les statistiques gardées,
/// `(x − moyenne) · rsqrt(variance + eps) · poids + biais`.
pub struct BatchNorm {
    weight: Array,
    bias: Array,
    running_mean: Array,
    running_var: Array,
    eps: f32,
}

impl BatchNorm {
    pub fn load(w: &mut Weights, prefix: &str, features: i32) -> Result<Self> {
        Ok(Self {
            weight: w.take(&format!("{prefix}.weight"), &[features])?,
            bias: w.take(&format!("{prefix}.bias"), &[features])?,
            running_mean: w.take(&format!("{prefix}.running_mean"), &[features])?,
            running_var: w.take(&format!("{prefix}.running_var"), &[features])?,
            eps: 1e-5,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        let eps = scalar_like(self.eps, &self.running_var)?;
        let normalized = x
            .subtract(&self.running_mean)?
            .multiply(ops::rsqrt(self.running_var.add(eps)?)?)?;
        Ok(self.weight.multiply(normalized)?.add(&self.bias)?)
    }
}

/// `Conv1d` : poids `[sorties, noyau, entrées / groupes]` (disposition de
/// MLX, celle du fichier), puis `+ bias`.
pub struct Conv1d {
    weight: Array,
    bias: Option<Array>,
    stride: i32,
    padding: i32,
    groups: i32,
}

/// Les réglages d'une convolution, comme les initialiseurs de MLXNN.
#[derive(Clone, Copy)]
pub struct ConvShape {
    pub input: i32,
    pub output: i32,
    pub kernel: i32,
    pub stride: i32,
    pub padding: i32,
    pub groups: i32,
    pub bias: bool,
}

impl Conv1d {
    pub fn load(w: &mut Weights, prefix: &str, s: ConvShape) -> Result<Self> {
        ensure!(
            s.groups > 0 && s.input % s.groups == 0,
            "{prefix} : {} groupes pour {} entrées",
            s.groups,
            s.input
        );
        Ok(Self {
            weight: w.take(
                &format!("{prefix}.weight"),
                &[s.output, s.kernel, s.input / s.groups],
            )?,
            bias: if s.bias {
                Some(w.take(&format!("{prefix}.bias"), &[s.output])?)
            } else {
                None
            },
            stride: s.stride,
            padding: s.padding,
            groups: s.groups,
        })
    }

    /// Le type des poids (`conv.weight.dtype` du Swift).
    pub fn dtype(&self) -> mlx_rs::Dtype {
        self.weight.dtype()
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        let y = ops::conv1d(x, &self.weight, self.stride, self.padding, 1, self.groups)?;
        match &self.bias {
            Some(bias) => Ok(y.add(bias)?),
            None => Ok(y),
        }
    }
}

/// `Conv2d` : poids `[sorties, noyau, noyau, entrées / groupes]`, noyau,
/// pas et marge égaux sur les deux axes.
pub struct Conv2d {
    weight: Array,
    bias: Option<Array>,
    stride: i32,
    padding: i32,
    groups: i32,
}

impl Conv2d {
    pub fn load(w: &mut Weights, prefix: &str, s: ConvShape) -> Result<Self> {
        ensure!(
            s.groups > 0 && s.input % s.groups == 0,
            "{prefix} : {} groupes pour {} entrées",
            s.groups,
            s.input
        );
        Ok(Self {
            weight: w.take(
                &format!("{prefix}.weight"),
                &[s.output, s.kernel, s.kernel, s.input / s.groups],
            )?,
            bias: if s.bias {
                Some(w.take(&format!("{prefix}.bias"), &[s.output])?)
            } else {
                None
            },
            stride: s.stride,
            padding: s.padding,
            groups: s.groups,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        let (stride, padding) = ((self.stride, self.stride), (self.padding, self.padding));
        let y = ops::conv2d(x, &self.weight, stride, padding, (1, 1), self.groups)?;
        match &self.bias {
            Some(bias) => Ok(y.add(bias)?),
            None => Ok(y),
        }
    }
}

/// `LSTM` de MLXNN, un sens : `Wx`, `Wh` et `bias` (clés du fichier).
pub struct Lstm {
    wx: Array,
    wh: Array,
    bias: Option<Array>,
    pub hidden_size: i32,
}

impl Lstm {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        input: i32,
        hidden: i32,
        bias: bool,
    ) -> Result<Self> {
        Ok(Self {
            wx: w.take(&format!("{prefix}.Wx"), &[4 * hidden, input])?,
            wh: w.take(&format!("{prefix}.Wh"), &[4 * hidden, hidden])?,
            bias: if bias {
                Some(w.take(&format!("{prefix}.bias"), &[4 * hidden])?)
            } else {
                None
            },
            hidden_size: hidden,
        })
    }

    /// `x` : `[…, temps, entrées]` ; rend tous les états cachés et toutes
    /// les cellules, `[…, temps, caché]`.
    pub fn forward(
        &self,
        x: &Array,
        hidden: Option<Array>,
        cell: Option<Array>,
    ) -> Result<(Array, Array)> {
        let x = match &self.bias {
            Some(bias) => ops::addmm(bias, x, self.wx.transpose()?, None, None)?,
            None => x.matmul(self.wx.transpose()?)?,
        };
        let (mut hidden, mut cell) = (hidden, cell);
        let steps = x
            .shape()
            .get(x.ndim().wrapping_sub(2))
            .copied()
            .unwrap_or(0);
        let (mut all_hidden, mut all_cell) = (Vec::new(), Vec::new());
        for index in 0..steps {
            let mut ifgo = x.try_index((Ellipsis, index, ..))?;
            if let Some(h) = &hidden {
                ifgo = ops::addmm(&ifgo, h, self.wh.transpose()?, None, None)?;
            }
            let pieces = ifgo.split_equal(4, -1)?;
            let [i, f, g, o] = pieces.as_slice() else {
                anyhow::bail!("LSTM : {} portes au lieu de 4", pieces.len());
            };
            let (i, f, g, o) = (
                ops::sigmoid(i)?,
                ops::sigmoid(f)?,
                ops::tanh(g)?,
                ops::sigmoid(o)?,
            );
            let next = match &cell {
                Some(c) => f.multiply(c)?.add(i.multiply(&g)?)?,
                None => i.multiply(&g)?,
            };
            let h = o.multiply(ops::tanh(&next)?)?;
            all_cell.push(next.clone());
            all_hidden.push(h.clone());
            cell = Some(next);
            hidden = Some(h);
        }
        Ok((ops::stack(&all_hidden, -2)?, ops::stack(&all_cell, -2)?))
    }
}

/// Le pas d'agrandissement du cache (`KVCacheSimple.step`).
const CACHE_STEP: i32 = 256;

/// `[B, H, T, D]` d'un tableau de clés ou de valeurs.
fn four_axes(x: &Array) -> Result<[i32; 4]> {
    match x.shape() {
        [b, h, t, d] => Ok([*b, *h, *t, *d]),
        shape => Err(anyhow!(crate::tr!(
            en = "attention cache: shape {shape:?}, four axes expected",
            fr = "cache d'attention : forme {shape:?}, quatre axes attendus",
        ))),
    }
}

/// `KVCacheSimple` (`TTS/KVCache.swift`, celui de mlx-swift-lm) : des
/// tampons agrandis par pas de 256 positions, remplis par tranches ; rend
/// les `offset` premières. Canary et Qwen3-ASR s'en servent.
#[derive(Default)]
pub struct KvCache {
    keys: Option<Array>,
    values: Option<Array>,
    offset: i32,
}

impl KvCache {
    pub fn offset(&self) -> i32 {
        self.offset
    }

    /// `update(keys:values:)`.
    pub fn update(&mut self, keys: &Array, values: &Array) -> Result<(Array, Array)> {
        let previous = self.offset;
        let [b, heads, time, k_dim] = four_axes(keys)?;
        let v_dim = four_axes(values)?[3];
        let reset = match &self.keys {
            Some(current) => previous + time > current.shape()[2],
            None => true,
        };
        if reset {
            let steps = (CACHE_STEP + time - 1) / CACHE_STEP;
            let new_k = ops::zeros_dtype(&[b, heads, steps * CACHE_STEP, k_dim], keys.dtype())?;
            let new_v = ops::zeros_dtype(&[b, heads, steps * CACHE_STEP, v_dim], values.dtype())?;
            match (self.keys.take(), self.values.take()) {
                (Some(mut current_k), Some(mut current_v)) => {
                    if previous % CACHE_STEP != 0 {
                        current_k = current_k.try_index((Ellipsis, ..previous, ..))?;
                        current_v = current_v.try_index((Ellipsis, ..previous, ..))?;
                    }
                    self.keys = Some(ops::concatenate(&[current_k, new_k], 2)?);
                    self.values = Some(ops::concatenate(&[current_v, new_v], 2)?);
                }
                _ => {
                    self.keys = Some(new_k);
                    self.values = Some(new_v);
                }
            }
        }
        self.offset += time;
        let offset = self.offset;
        let (Some(all_k), Some(all_v)) = (self.keys.as_mut(), self.values.as_mut()) else {
            return Err(anyhow!(crate::tr!(
                "attention cache: empty",
                "cache d'attention : vide",
            )));
        };
        all_k.try_index_mut((Ellipsis, previous..offset, ..), keys)?;
        all_v.try_index_mut((Ellipsis, previous..offset, ..), values)?;
        Ok((
            all_k.try_index((Ellipsis, ..offset, ..))?,
            all_v.try_index((Ellipsis, ..offset, ..))?,
        ))
    }
}
