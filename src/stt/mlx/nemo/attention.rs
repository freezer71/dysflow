//! L'attention multi-têtes de NeMo, avec ou sans positions relatives, et la
//! table des positions relatives. Port de `NemoAttention.swift`.

use anyhow::{ensure, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use crate::stt::mlx::nn::{scalar_like, Linear};
use crate::stt::mlx::weights::Weights;

/// Les projections `linear_q`, `linear_k`, `linear_v` et `linear_out`.
struct Projections {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    heads: i32,
    head_dim: i32,
    features: i32,
    scale: f32,
}

impl Projections {
    fn load(w: &mut Weights, prefix: &str, heads: i32, features: i32, bias: bool) -> Result<Self> {
        ensure!(
            heads > 0 && features % heads == 0,
            "{prefix} : {features} dimensions pour {heads} têtes"
        );
        let head_dim = features / heads;
        let linear = |w: &mut Weights, name: &str| {
            Linear::load(w, &format!("{prefix}.{name}"), features, features, bias)
        };
        Ok(Self {
            q: linear(w, "linear_q")?,
            k: linear(w, "linear_k")?,
            v: linear(w, "linear_v")?,
            out: linear(w, "linear_out")?,
            heads,
            head_dim,
            features,
            scale: (head_dim as f32).powf(-0.5),
        })
    }

    /// `[lot, temps, d]` en `[lot, temps, têtes, d / têtes]`.
    fn heads(&self, x: &Array) -> Result<Array> {
        let s = x.shape();
        Ok(x.reshape(&[s[0], s[1], self.heads, self.head_dim])?)
    }
}

/// `NemoMultiHeadAttention`.
pub struct MultiHeadAttention {
    p: Projections,
}

impl MultiHeadAttention {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        heads: i32,
        features: i32,
        bias: bool,
    ) -> Result<Self> {
        Ok(Self {
            p: Projections::load(w, prefix, heads, features, bias)?,
        })
    }

    pub fn forward(&self, q: &Array, k: &Array, v: &Array, mask: Option<&Array>) -> Result<Array> {
        let p = &self.p;
        let q = p.heads(&p.q.forward(q)?)?.transpose_axes(&[0, 2, 1, 3])?;
        let k = p.heads(&p.k.forward(k)?)?.transpose_axes(&[0, 2, 1, 3])?;
        let v = p.heads(&p.v.forward(v)?)?.transpose_axes(&[0, 2, 1, 3])?;
        let (batch, q_len) = (q.shape()[0], q.shape()[2]);
        let attended = fast::scaled_dot_product_attention(
            &q,
            &k,
            &v,
            p.scale,
            mask.map(ScaledDotProductAttentionMask::Array),
            None,
        )?;
        let out = attended
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[batch, q_len, p.features])?;
        p.out.forward(&out)
    }
}

/// `NemoRelPositionMultiHeadAttention` (Transformer-XL) : `linear_pos`, et
/// les biais `pos_bias_u`, `pos_bias_v` (clés `posBiasU`, `posBiasV`).
pub struct RelPositionMultiHeadAttention {
    p: Projections,
    pos: Linear,
    bias_u: Array,
    bias_v: Array,
}

impl RelPositionMultiHeadAttention {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        heads: i32,
        features: i32,
        bias: bool,
    ) -> Result<Self> {
        let p = Projections::load(w, prefix, heads, features, bias)?;
        let shape = [heads, p.head_dim];
        Ok(Self {
            pos: Linear::load(
                w,
                &format!("{prefix}.linear_pos"),
                features,
                features,
                false,
            )?,
            bias_u: w.take(&format!("{prefix}.posBiasU"), &shape)?,
            bias_v: w.take(&format!("{prefix}.posBiasV"), &shape)?,
            p,
        })
    }

    /// Décale chaque ligne de `[b, h, tq, positions]` d'un cran par ligne :
    /// la position relative devient absolue.
    fn rel_shift(x: &Array) -> Result<Array> {
        let s = x.shape();
        let (b, h, tq, len) = (s[0], s[1], s[2], s[3]);
        let padded = ops::pad(x, &[(0, 0), (0, 0), (0, 0), (1, 0)], None, None)?;
        let shifted = padded
            .reshape(&[b, h, len + 1, tq])?
            .try_index((.., .., 1.., ..))?;
        Ok(shifted.reshape(&[b, h, tq, len])?)
    }

    pub fn forward(
        &self,
        q: &Array,
        k: &Array,
        v: &Array,
        pos_emb: &Array,
        mask: Option<&Array>,
    ) -> Result<Array> {
        let p = &self.p;
        let q = p.heads(&p.q.forward(q)?)?;
        let k = p.heads(&p.k.forward(k)?)?.transpose_axes(&[0, 2, 1, 3])?;
        let v = p.heads(&p.v.forward(v)?)?.transpose_axes(&[0, 2, 1, 3])?;
        let mut pos = self.pos.forward(pos_emb)?;
        let (batch, q_len, k_len) = (q.shape()[0], q.shape()[1], k.shape()[2]);
        let pos_len = pos.shape()[1];
        if pos.shape()[0] == 1 && batch > 1 {
            pos = ops::broadcast_to(&pos, &[batch, pos_len, p.features])?;
        }
        let pos = pos
            .reshape(&[batch, pos_len, p.heads, p.head_dim])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let q_u = q
            .add(self.bias_u.as_dtype(q.dtype())?)?
            .transpose_axes(&[0, 2, 1, 3])?;
        let q_v = q
            .add(self.bias_v.as_dtype(q.dtype())?)?
            .transpose_axes(&[0, 2, 1, 3])?;

        let bd = Self::rel_shift(&q_v.matmul(pos.swap_axes(-2, -1)?)?)?;
        let bd = bd.try_index((.., .., .., ..k_len))?;
        let mut bd = bd.multiply(scalar_like(p.scale, &bd)?)?;
        if let Some(mask) = mask {
            bd = bd.add(mask)?;
        }
        let attended = fast::scaled_dot_product_attention(
            &q_u,
            &k,
            &v,
            p.scale,
            ScaledDotProductAttentionMask::Array(&bd),
            None,
        )?;
        let out = attended
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[batch, q_len, -1])?;
        p.out.forward(&out)
    }
}

/// `NemoRelPositionalEncoding` : la table des positions relatives, de
/// `max_len − 1` à `−(max_len − 1)`, sinus aux colonnes paires et cosinus
/// aux impaires, en `f32` sur le processeur. Agrandie si une entrée la
/// dépasse.
pub struct RelPositionalEncoding {
    d_model: i32,
    max_len: i32,
    scale_input: f32,
    pe: Array,
}

impl RelPositionalEncoding {
    pub fn new(d_model: i32, max_len: i32, scale_input: bool) -> Result<Self> {
        ensure!(
            d_model > 0 && max_len > 0,
            "positions relatives : d_model {d_model}, max_len {max_len}"
        );
        let scale_input = if scale_input {
            (d_model as f32).sqrt()
        } else {
            1.0
        };
        let mut this = Self {
            d_model,
            max_len,
            scale_input,
            pe: Array::from_f32(0.0),
        };
        this.calculate()?;
        Ok(this)
    }

    fn calculate(&mut self) -> Result<()> {
        self.pe = Array::from_slice(
            &rel_positional_values(self.d_model as usize, self.max_len as usize),
            &[1, 2 * self.max_len - 1, self.d_model],
        );
        self.pe.eval()?;
        Ok(())
    }

    /// `x` mis à l'échelle et les positions de `[1, 2T − 1, d]`, au type de
    /// `x`.
    pub fn forward(&mut self, x: &Array) -> Result<(Array, Array)> {
        let len = x.shape()[1];
        if len > self.max_len {
            self.max_len = len + 1;
            self.calculate()?;
        }
        let scaled = x.multiply(scalar_like(self.scale_input, x)?)?;
        let buffer = self.pe.shape()[1];
        let (start, end) = (buffer / 2 - (len - 1), buffer / 2 + (len - 1) + 1);
        let pos = self
            .pe
            .try_index((.., start..end, ..))?
            .as_dtype(x.dtype())?;
        Ok((scaled, pos))
    }
}

/// Les valeurs de la table, ligne par ligne : `pos = max_len − 1 − ligne`,
/// `div = exp(−c · ln(10 000) / d)` (le logarithme en `f64` puis `f32`,
/// comme `Float(log(10000.0))` du Swift).
pub fn rel_positional_values(d_model: usize, max_len: usize) -> Vec<f32> {
    let rows = 2 * max_len - 1;
    let mut values = vec![0f32; rows * d_model];
    let log_div = 10_000f64.ln() as f32 / d_model as f32;
    for r in 0..rows {
        let pos = (max_len as f32 - 1.0) - r as f32;
        for c in (0..d_model).step_by(2) {
            let angle = pos * (-(c as f32) * log_div).exp();
            values[r * d_model + c] = angle.sin();
            if c + 1 < d_model {
                values[r * d_model + c + 1] = angle.cos();
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_des_positions() {
        let v = rel_positional_values(4, 3);
        assert_eq!(v.len(), 5 * 4);
        // Ligne du milieu : position 0, sin 0 et cos 0.
        assert_eq!(&v[2 * 4..3 * 4], &[0.0, 1.0, 0.0, 1.0]);
        // Première ligne : position 2.
        assert!((v[0] - 2f32.sin()).abs() < 1e-6 && (v[1] - 2f32.cos()).abs() < 1e-6);
    }
}
