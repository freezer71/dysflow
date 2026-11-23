//! Le décodeur Transformer de Canary, comme `CanaryModel.swift` : plongement
//! des jetons (quantifié), positions sinusoïdales fixes (1 024, la dernière
//! répétée au-delà), blocs à normes en tête (auto-attention, attention
//! croisée, MLP ReLU), norme finale et projection sur le vocabulaire.
//!
//! Deux passages, ceux du Swift : la suite entière (`forward`, le décodeur
//! de l'amont) et le pas à pas de l'app (`step` : un cache d'auto-attention
//! par bloc, `KVCacheSimple`, et l'attention croisée calculée une fois par
//! lecture). Mêmes opérations dans le même ordre ; les types suivent ceux
//! des poids et de l'entrée, comme dans le Swift (aucune conversion
//! ajoutée).

use anyhow::{anyhow, ensure, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::ops::indexing::{Ellipsis, TryIndexMutOp, TryIndexOp};
use mlx_rs::{nn, ops, Array, Dtype};

use super::config::DecoderConfig;
use crate::stt::mlx::nn::{scalar_like, Embedding, LayerNorm, Linear};
use crate::stt::mlx::weights::Weights;

/// Le modèle dans les messages.
pub const NAME: &str = "Canary";
/// Les positions de la table (`CanaryFixedPositionalEncoding.maxLength`).
pub const MAX_POSITIONS: i32 = 1024;
/// Le pas d'agrandissement du cache (`KVCacheSimple.step`).
const CACHE_STEP: i32 = 256;

/// `[B, T, …]` : les deux premières dimensions.
fn batch_and_time(x: &Array) -> Result<(i32, i32)> {
    match x.shape() {
        [b, t, ..] => Ok((*b, *t)),
        shape => Err(anyhow!(crate::tr!(
            en = "{NAME}: shape {shape:?}, at least two axes expected",
            fr = "{NAME} : forme {shape:?}, au moins deux axes attendus",
        ))),
    }
}

/// `[B, H, T, D]`.
fn four_axes(x: &Array) -> Result<[i32; 4]> {
    match x.shape() {
        [b, h, t, d] => Ok([*b, *h, *t, *d]),
        shape => Err(anyhow!(crate::tr!(
            en = "{NAME}: shape {shape:?}, four axes expected",
            fr = "{NAME} : forme {shape:?}, quatre axes attendus",
        ))),
    }
}

// ---------- positions ----------

/// `CanaryFixedPositionalEncoding` : la table `[1 024, d]`, sinus et cosinus
/// alternés, divisée par √d.
pub struct Positions {
    table: Array,
}

impl Positions {
    pub fn new(d: i32) -> Result<Self> {
        let (max, width) = (MAX_POSITIONS as usize, d as usize);
        let mut values = vec![0f32; max * width];
        let log = -(10_000f32).ln();
        for position in 0..max {
            for channel in (0..width).step_by(2) {
                let div = (log * channel as f32 / d as f32).exp();
                let angle = position as f32 * div;
                values[position * width + channel] = angle.sin();
                if channel + 1 < width {
                    values[position * width + channel + 1] = angle.cos();
                }
            }
        }
        let table = Array::from_slice(&values, &[MAX_POSITIONS, d])
            .divide(Array::from_f32((d as f32).sqrt()))?;
        table.eval()?;
        Ok(Self { table })
    }

    /// `callAsFunction(batch:length:startPosition:dtype:)` : les lignes
    /// `start ..< start + length`, la dernière lue répétée au-delà de la
    /// table ; `[B, length, d]` au type `dtype`.
    pub fn rows(&self, batch: i32, length: i32, start: i32, dtype: Dtype) -> Result<Array> {
        let end = (start + length).min(MAX_POSITIONS);
        let start = start.min(end);
        let count = end - start;
        ensure!(
            count > 0 || length == 0,
            crate::tr!(
                en = "{NAME}: positions from {start}, past the table ({MAX_POSITIONS})",
                fr = "{NAME} : positions depuis {start}, au-delà de la table ({MAX_POSITIONS})",
            )
        );
        let mut positions = self.table.try_index(start..end)?;
        if count < length {
            let last = positions.try_index(count - 1..count)?;
            let pad = Array::repeat_axis::<f32>(last, length - count, 0)?;
            positions = ops::concatenate(&[positions, pad], 0)?;
        }
        positions = positions.expand_dims(0)?;
        if batch > 1 {
            positions = Array::repeat_axis::<f32>(positions, batch, 0)?;
        }
        Ok(positions.as_dtype(dtype)?)
    }

    /// `positions(batch:length:startPosition:dtype:)` du pas à pas : au-delà
    /// de la dernière ligne, la dernière.
    fn rows_from(&self, batch: i32, length: i32, start: i32, dtype: Dtype) -> Result<Array> {
        self.rows(batch, length, start.min(MAX_POSITIONS - 1), dtype)
    }
}

// ---------- attention ----------

/// `CanaryAttention`.
pub struct Attention {
    pub q: Linear,
    pub k: Linear,
    pub v: Linear,
    pub out: Linear,
    heads: i32,
    head_dim: i32,
    scale: f32,
}

impl Attention {
    fn load(w: &mut Weights, prefix: &str, hidden: i32, heads: i32) -> Result<Self> {
        let head_dim = hidden / heads;
        let linear = |w: &mut Weights, name: &str| {
            Linear::load(w, &format!("{prefix}.{name}"), hidden, hidden, true)
        };
        Ok(Self {
            q: linear(w, "q_proj")?,
            k: linear(w, "k_proj")?,
            v: linear(w, "v_proj")?,
            out: linear(w, "out_proj")?,
            heads,
            head_dim,
            // `pow(Float(headDim), -0.5)`.
            scale: (head_dim as f32).powf(-0.5),
        })
    }

    /// `[B, T, E]` → `[B, têtes, T, D]`.
    fn heads_of(&self, x: &Array) -> Result<Array> {
        let (b, t) = batch_and_time(x)?;
        Ok(x.reshape(&[b, t, self.heads, self.head_dim])?
            .transpose_axes(&[0, 2, 1, 3])?)
    }

    /// `callAsFunction(_:keyValue:mask:)` : la suite entière (dans l'app,
    /// seul le pas à pas sert ; ici, l'oracle des essais du cache).
    #[cfg(test)]
    pub fn forward(&self, x: &Array, key_value: &Array, mask: Option<&Array>) -> Result<Array> {
        let (b, t) = batch_and_time(x)?;
        let q = self.heads_of(&self.q.forward(x)?)?;
        let k = self.heads_of(&self.k.forward(key_value)?)?;
        let v = self.heads_of(&self.v.forward(key_value)?)?;
        self.attention(&q, &k, &v, mask, b, t)
    }

    /// `keysValues` : les clés et valeurs de `key_value`, en têtes.
    pub fn keys_values(&self, key_value: &Array) -> Result<(Array, Array)> {
        Ok((
            self.heads_of(&self.k.forward(key_value)?)?,
            self.heads_of(&self.v.forward(key_value)?)?,
        ))
    }

    /// `attend(_:keys:values:mask:)` : `x` sur des clés et valeurs déjà en
    /// têtes.
    pub fn attend(
        &self,
        x: &Array,
        keys: &Array,
        values: &Array,
        mask: Option<&Array>,
    ) -> Result<Array> {
        let (b, t) = batch_and_time(x)?;
        let q = self.heads_of(&self.q.forward(x)?)?;
        self.attention(&q, keys, values, mask, b, t)
    }

    /// `attend(_:cache:mask:)` : l'auto-attention des nouvelles positions
    /// sur tout ce que le cache garde, plus elles.
    fn attend_cached(&self, x: &Array, cache: &mut KvCache, mask: Option<&Array>) -> Result<Array> {
        let (k, v) = self.keys_values(x)?;
        let (keys, values) = cache.update(&k, &v)?;
        self.attend(x, &keys, &values, mask)
    }

    /// `scaledDotProductAttention`, têtes fusionnées (`reshaped(B, T, -1)`),
    /// puis `out_proj`.
    fn attention(
        &self,
        q: &Array,
        k: &Array,
        v: &Array,
        mask: Option<&Array>,
        b: i32,
        t: i32,
    ) -> Result<Array> {
        let attended = fast::scaled_dot_product_attention(
            q,
            k,
            v,
            self.scale,
            mask.map(ScaledDotProductAttentionMask::Array),
            None::<&Array>,
        )?;
        let merged = attended
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[b, t, -1])?;
        self.out.forward(&merged)
    }
}

/// `KVCacheSimple` : des tampons agrandis par pas de 256 positions, remplis
/// par tranches ; rend les `offset` premières.
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
                en = "{NAME}: empty cache",
                fr = "{NAME} : cache vide",
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

// ---------- blocs ----------

/// `CanaryCrossKeysValues` : clés, valeurs et masque (Float32) de
/// l'attention croisée d'un bloc, calculés une fois par lecture.
pub struct Cross {
    keys: Array,
    values: Array,
    mask: Option<Array>,
}

/// Le masque additif de l'encodeur `[B, 1, 1, S]` : −1e9 où il vaut 0, 0
/// ailleurs, en Float32.
fn additive_mask(encoder_mask: &Array) -> Result<Array> {
    let expanded = encoder_mask.expand_dims_axes(&[1, 2])?;
    let blocked = expanded.eq(scalar_like(0.0, &expanded)?)?;
    Ok(ops::select(
        &blocked,
        Array::from_f32(-1e9),
        Array::from_f32(0.0),
    )?)
}

/// `CanaryDecoderBlock`.
pub struct Block {
    pub self_attn_norm: LayerNorm,
    pub self_attn: Attention,
    pub cross_attn_norm: LayerNorm,
    pub cross_attn: Attention,
    pub ff_norm: LayerNorm,
    pub ff1: Linear,
    pub ff2: Linear,
}

impl Block {
    fn load(w: &mut Weights, prefix: &str, c: &DecoderConfig) -> Result<Self> {
        let (d, heads) = (c.hidden_size, c.num_attention_heads);
        let norm = |w: &mut Weights, name: &str| LayerNorm::load(w, &format!("{prefix}.{name}"), d);
        Ok(Self {
            self_attn_norm: norm(w, "self_attn_norm")?,
            self_attn: Attention::load(w, &format!("{prefix}.self_attn"), d, heads)?,
            cross_attn_norm: norm(w, "cross_attn_norm")?,
            cross_attn: Attention::load(w, &format!("{prefix}.cross_attn"), d, heads)?,
            ff_norm: norm(w, "ff_norm")?,
            ff1: Linear::load(w, &format!("{prefix}.ff1"), d, c.inner_size, true)?,
            ff2: Linear::load(w, &format!("{prefix}.ff2"), c.inner_size, d, true)?,
        })
    }

    /// `ff2(relu(ff1(ff_norm(out))))` ajouté à `out`.
    fn feed_forward(&self, out: &Array) -> Result<Array> {
        let y = self.ff_norm.forward(out)?;
        let y = self.ff2.forward(&nn::relu(self.ff1.forward(&y)?)?)?;
        Ok(out.add(&y)?)
    }

    /// `callAsFunction(_:encoderOutput:encoderMask:selfAttentionMask:)`.
    #[cfg(test)]
    pub fn forward(
        &self,
        x: &Array,
        encoder_output: &Array,
        encoder_mask: Option<&Array>,
        self_mask: Option<&Array>,
    ) -> Result<Array> {
        let self_norm = self.self_attn_norm.forward(x)?;
        let out = x.add(self.self_attn.forward(&self_norm, &self_norm, self_mask)?)?;
        let cross_mask = match encoder_mask {
            Some(mask) => Some(additive_mask(mask)?.as_dtype(out.dtype())?),
            None => None,
        };
        let crossed = self.cross_attn.forward(
            &self.cross_attn_norm.forward(&out)?,
            encoder_output,
            cross_mask.as_ref(),
        )?;
        let out = out.add(&crossed)?;
        self.feed_forward(&out)
    }

    /// `callAsFunction(_:cross:cache:selfAttentionMask:)` : le pas à pas.
    fn step(
        &self,
        x: &Array,
        cross: &Cross,
        cache: &mut KvCache,
        self_mask: Option<&Array>,
    ) -> Result<Array> {
        let self_norm = self.self_attn_norm.forward(x)?;
        let out = x.add(self.self_attn.attend_cached(&self_norm, cache, self_mask)?)?;
        let cross_mask = match &cross.mask {
            Some(mask) => Some(mask.as_dtype(out.dtype())?),
            None => None,
        };
        let crossed = self.cross_attn.attend(
            &self.cross_attn_norm.forward(&out)?,
            &cross.keys,
            &cross.values,
            cross_mask.as_ref(),
        )?;
        let out = out.add(&crossed)?;
        self.feed_forward(&out)
    }

    /// `crossKeysValues`.
    fn cross(&self, encoder_output: &Array, encoder_mask: Option<&Array>) -> Result<Cross> {
        let (keys, values) = self.cross_attn.keys_values(encoder_output)?;
        let mask = encoder_mask.map(additive_mask).transpose()?;
        Ok(Cross { keys, values, mask })
    }
}

/// `CanaryDecoderState` : un cache d'auto-attention et l'attention croisée
/// de chaque bloc.
pub struct State {
    caches: Vec<KvCache>,
    cross: Vec<Cross>,
}

impl State {
    /// Les positions déjà décodées.
    pub fn offset(&self) -> i32 {
        self.caches.first().map_or(0, KvCache::offset)
    }
}

/// `MultiHeadAttention.createAdditiveCausalMask(n)` au type `dtype` : −1e9
/// au-dessus de la diagonale (calculé en Float32).
fn causal_mask(n: i32, dtype: Dtype) -> Result<Array> {
    let indices = Array::arange::<_, i32>(None, n, None)?;
    let mask = indices.expand_dims(1)?.lt(indices.expand_dims(0)?)?;
    Ok(mask
        .as_dtype(Dtype::Float32)?
        .multiply(Array::from_f32(-1e9))?
        .as_dtype(dtype)?)
}

/// `CanaryDecoder` (clés `decoder.…`).
pub struct Decoder {
    pub embedding: Embedding,
    pub positions: Positions,
    pub embedding_layer_norm: LayerNorm,
    pub blocks: Vec<Block>,
    pub final_norm: LayerNorm,
    pub output_proj: Linear,
}

impl Decoder {
    pub fn load(w: &mut Weights, c: &DecoderConfig, vocab: i32) -> Result<Self> {
        let d = c.hidden_size;
        Ok(Self {
            embedding: Embedding::load(w, "decoder.embedding", vocab, d)?,
            positions: Positions::new(d)?,
            embedding_layer_norm: LayerNorm::load(w, "decoder.embedding_layer_norm", d)?,
            blocks: (0..c.num_layers)
                .map(|i| Block::load(w, &format!("decoder.blocks.{i}"), c))
                .collect::<Result<_>>()?,
            final_norm: LayerNorm::load(w, "decoder.final_norm", d)?,
            output_proj: Linear::load(w, "decoder.output_proj", d, vocab, true)?,
        })
    }

    /// Plongement, positions (`clamped` : celles du pas à pas), norme :
    /// l'entrée des blocs, et son masque causal (plus d'une position).
    fn embed(&self, tokens: &Array, start: i32, clamped: bool) -> Result<(Array, Option<Array>)> {
        let (batch, time) = batch_and_time(tokens)?;
        let x = self.embedding.forward(tokens)?;
        let positions = if clamped {
            self.positions.rows_from(batch, time, start, x.dtype())?
        } else {
            self.positions.rows(batch, time, start, x.dtype())?
        };
        let x = x.add(&positions)?;
        let x = self.embedding_layer_norm.forward(&x)?;
        let mask = if time > 1 {
            Some(causal_mask(time, x.dtype())?)
        } else {
            None
        };
        Ok((x, mask))
    }

    /// `callAsFunction(_:encoderOutput:encoderMask:startPosition:)` : les
    /// logits de toute la suite `tokens` `[B, T]`.
    #[cfg(test)]
    pub fn forward(
        &self,
        tokens: &Array,
        encoder_output: &Array,
        encoder_mask: Option<&Array>,
        start: i32,
    ) -> Result<Array> {
        let (mut x, mask) = self.embed(tokens, start, false)?;
        for block in &self.blocks {
            x = block.forward(&x, encoder_output, encoder_mask, mask.as_ref())?;
        }
        self.output_proj.forward(&self.final_norm.forward(&x)?)
    }

    /// `makeState` : un état neuf pour décoder pas à pas sur
    /// `encoder_output`.
    pub fn make_state(
        &self,
        encoder_output: &Array,
        encoder_mask: Option<&Array>,
    ) -> Result<State> {
        Ok(State {
            caches: self.blocks.iter().map(|_| KvCache::default()).collect(),
            cross: self
                .blocks
                .iter()
                .map(|b| b.cross(encoder_output, encoder_mask))
                .collect::<Result<_>>()?,
        })
    }

    /// `callAsFunction(_:state:)` : les logits des seules nouvelles
    /// positions `tokens`, qui suivent les `state.offset()` déjà décodées.
    /// Plusieurs positions : seulement la première fois (l'invite).
    pub fn step(&self, tokens: &Array, state: &mut State) -> Result<Array> {
        let (_, time) = batch_and_time(tokens)?;
        let start = state.offset();
        ensure!(
            time == 1 || start == 0,
            crate::tr!(
                en = "{NAME}: several positions only at the start ({time} after {start})",
                fr = "{NAME} : plusieurs positions seulement au début ({time} après {start})",
            )
        );
        let (mut x, mask) = self.embed(tokens, start, true)?;
        for ((block, cross), cache) in self
            .blocks
            .iter()
            .zip(&state.cross)
            .zip(state.caches.iter_mut())
        {
            x = block.step(&x, cross, cache, mask.as_ref())?;
        }
        self.output_proj.forward(&self.final_norm.forward(&x)?)
    }
}
