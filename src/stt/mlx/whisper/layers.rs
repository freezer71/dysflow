//! Le graphe de Whisper, comme `WhisperLayers.swift` : l'attention à
//! plusieurs têtes (sans biais sur `k_proj`), l'encodeur (deux convolutions,
//! positions sinusoïdales, blocs), le décodeur (blocs à attention propre et
//! croisée, caches K/V) et la projection par la matrice des jetons. Mêmes
//! opérations, dans le même ordre ; les types suivent ceux des poids et de
//! l'entrée, comme dans le Swift (aucune conversion ajoutée).

use anyhow::{anyhow, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{nn, ops, Array, Dtype};

use super::config::Config;
use crate::stt::mlx::nn::{Conv1d, ConvShape, Embedding, LayerNorm, Linear};
use crate::stt::mlx::weights::Weights;

/// Le modèle dans les messages.
pub const NAME: &str = "Whisper";

/// La clé `key` manque : la même erreur que `Weights::take`.
fn missing(key: &str) -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "{NAME} weights: key {key} is missing",
        fr = "poids de {NAME} : la clé {key} manque",
    ))
}

/// Une `Linear` : quantifiée si `config.json` l'est (`quantize` du Swift les
/// quantifie toutes, et `verify: .all` exige alors leurs `scales`), en clair
/// sinon.
fn linear(w: &mut Weights, prefix: &str, input: i32, output: i32, bias: bool) -> Result<Linear> {
    let scales = format!("{prefix}.scales");
    if w.quantization.is_some() && !w.has(&scales) {
        return Err(missing(&scales));
    }
    Linear::load(w, prefix, input, output, bias)
}

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

/// `WhisperAttention`.
pub struct Attention {
    pub q: Linear,
    pub k: Linear,
    pub v: Linear,
    pub out: Linear,
    heads: i32,
    head_dim: i32,
    embed: i32,
    scale: f32,
}

impl Attention {
    fn load(w: &mut Weights, prefix: &str, embed: i32, heads: i32) -> Result<Self> {
        let head_dim = embed / heads;
        Ok(Self {
            q: linear(w, &format!("{prefix}.q_proj"), embed, embed, true)?,
            k: linear(w, &format!("{prefix}.k_proj"), embed, embed, false)?,
            v: linear(w, &format!("{prefix}.v_proj"), embed, embed, true)?,
            out: linear(w, &format!("{prefix}.out_proj"), embed, embed, true)?,
            heads,
            head_dim,
            embed,
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

    /// L'attention de `q` sur `keys` et `values`, têtes fusionnées, puis
    /// `out_proj`.
    fn attend(
        &self,
        q: &Array,
        keys: &Array,
        values: &Array,
        mask: Option<&Array>,
    ) -> Result<Array> {
        let (b, _, tq) = match q.shape() {
            [b, h, t, ..] => (*b, *h, *t),
            shape => {
                return Err(anyhow!(crate::tr!(
                    en = "{NAME}: queries of shape {shape:?}, four axes expected",
                    fr = "{NAME} : requêtes de forme {shape:?}, quatre axes attendus",
                )))
            }
        };
        let attn = fast::scaled_dot_product_attention(
            q,
            keys,
            values,
            self.scale,
            mask.map(ScaledDotProductAttentionMask::Array),
            None::<&Array>,
        )?;
        let merged = attn
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[b, tq, self.embed])?;
        self.out.forward(&merged)
    }

    /// `callAsFunction` : rend la sortie et les clés et valeurs (les
    /// anciennes de `cached`, suivies des nouvelles).
    pub fn forward(
        &self,
        hidden: &Array,
        kv_input: &Array,
        cached: Option<(&Array, &Array)>,
        mask: Option<&Array>,
    ) -> Result<(Array, Array, Array)> {
        let q = self.heads_of(&self.q.forward(hidden)?)?;
        let new_k = self.heads_of(&self.k.forward(kv_input)?)?;
        let new_v = self.heads_of(&self.v.forward(kv_input)?)?;
        let (keys, values) = match cached {
            Some((k, v)) => (
                ops::concatenate(&[k.clone(), new_k], 2)?,
                ops::concatenate(&[v.clone(), new_v], 2)?,
            ),
            None => (new_k, new_v),
        };
        Ok((self.attend(&q, &keys, &values, mask)?, keys, values))
    }
}

/// `WhisperEncoderLayer`.
pub struct EncoderLayer {
    pub self_attn: Attention,
    pub self_attn_layer_norm: LayerNorm,
    pub fc1: Linear,
    pub fc2: Linear,
    pub final_layer_norm: LayerNorm,
}

impl EncoderLayer {
    fn load(w: &mut Weights, prefix: &str, c: &Config) -> Result<Self> {
        let d = c.d_model;
        Ok(Self {
            self_attn: Attention::load(
                w,
                &format!("{prefix}.self_attn"),
                d,
                c.encoder_attention_heads,
            )?,
            self_attn_layer_norm: LayerNorm::load(w, &format!("{prefix}.self_attn_layer_norm"), d)?,
            fc1: linear(w, &format!("{prefix}.fc1"), d, c.encoder_ffn_dim, true)?,
            fc2: linear(w, &format!("{prefix}.fc2"), c.encoder_ffn_dim, d, true)?,
            final_layer_norm: LayerNorm::load(w, &format!("{prefix}.final_layer_norm"), d)?,
        })
    }

    pub fn forward(&self, x: &Array) -> Result<Array> {
        let h = self.self_attn_layer_norm.forward(x)?;
        let (attn, _, _) = self.self_attn.forward(&h, &h, None, None)?;
        let h = x.add(&attn)?;
        let y = self.final_layer_norm.forward(&h)?;
        let y = self.fc2.forward(&nn::gelu(self.fc1.forward(&y)?)?)?;
        Ok(h.add(&y)?)
    }
}

/// `WhisperEncoder`.
pub struct Encoder {
    pub conv1: Conv1d,
    pub conv2: Conv1d,
    /// `embed_positions.weight` : `[positions, d]`.
    pub embed_positions: Array,
    pub layers: Vec<EncoderLayer>,
    pub layer_norm: LayerNorm,
}

impl Encoder {
    pub fn load(w: &mut Weights, c: &Config) -> Result<Self> {
        let d = c.d_model;
        let conv = |input, stride| ConvShape {
            input,
            output: d,
            kernel: 3,
            stride,
            padding: 1,
            groups: 1,
            bias: true,
        };
        let p = "model.encoder";
        Ok(Self {
            conv1: Conv1d::load(w, &format!("{p}.conv1"), conv(c.num_mel_bins, 1))?,
            conv2: Conv1d::load(w, &format!("{p}.conv2"), conv(d, 2))?,
            embed_positions: w.take(
                &format!("{p}.embed_positions.weight"),
                &[c.max_source_positions, d],
            )?,
            layers: (0..c.encoder_layers)
                .map(|i| EncoderLayer::load(w, &format!("{p}.layers.{i}"), c))
                .collect::<Result<_>>()?,
            layer_norm: LayerNorm::load(w, &format!("{p}.layer_norm"), d)?,
        })
    }

    /// `gelu(conv1)`, `gelu(conv2)`, plus les positions, les blocs, la norme.
    pub fn forward(&self, features: &Array) -> Result<Array> {
        let h = nn::gelu(self.conv1.forward(features)?)?;
        let h = nn::gelu(self.conv2.forward(&h)?)?;
        let mut h = self.with_positions(&h)?;
        for layer in &self.layers {
            h = layer.forward(&h)?;
        }
        self.layer_norm.forward(&h)
    }

    /// `h + embedPositions.weight[0..<seqLen]`.
    pub fn with_positions(&self, h: &Array) -> Result<Array> {
        let (_, len) = batch_and_time(h)?;
        Ok(h.add(self.embed_positions.try_index(..len)?)?)
    }
}

/// Les caches d'un bloc du décodeur : clés et valeurs de l'attention propre
/// (elles grandissent d'un jeton à chaque pas) et de l'attention croisée
/// (calculées une fois par fenêtre).
#[derive(Default)]
pub struct LayerCache {
    self_keys: Option<Array>,
    self_values: Option<Array>,
    cross_keys: Option<Array>,
    cross_values: Option<Array>,
}

/// `WhisperDecoderLayer`.
pub struct DecoderLayer {
    pub self_attn: Attention,
    pub self_attn_layer_norm: LayerNorm,
    pub encoder_attn: Attention,
    pub encoder_attn_layer_norm: LayerNorm,
    pub fc1: Linear,
    pub fc2: Linear,
    pub final_layer_norm: LayerNorm,
}

impl DecoderLayer {
    fn load(w: &mut Weights, prefix: &str, c: &Config) -> Result<Self> {
        let d = c.d_model;
        let heads = c.decoder_attention_heads;
        Ok(Self {
            self_attn: Attention::load(w, &format!("{prefix}.self_attn"), d, heads)?,
            self_attn_layer_norm: LayerNorm::load(w, &format!("{prefix}.self_attn_layer_norm"), d)?,
            encoder_attn: Attention::load(w, &format!("{prefix}.encoder_attn"), d, heads)?,
            encoder_attn_layer_norm: LayerNorm::load(
                w,
                &format!("{prefix}.encoder_attn_layer_norm"),
                d,
            )?,
            fc1: linear(w, &format!("{prefix}.fc1"), d, c.decoder_ffn_dim, true)?,
            fc2: linear(w, &format!("{prefix}.fc2"), c.decoder_ffn_dim, d, true)?,
            final_layer_norm: LayerNorm::load(w, &format!("{prefix}.final_layer_norm"), d)?,
        })
    }

    fn forward(
        &self,
        x: &Array,
        encoder_hidden: &Array,
        self_mask: Option<&Array>,
        cache: &mut LayerCache,
    ) -> Result<Array> {
        let h = self.self_attn_layer_norm.forward(x)?;
        let cached = cache.self_keys.as_ref().zip(cache.self_values.as_ref());
        let (attn, keys, values) = self.self_attn.forward(&h, &h, cached, self_mask)?;
        cache.self_keys = Some(keys);
        cache.self_values = Some(values);
        let h = x.add(&attn)?;

        let y = self.encoder_attn_layer_norm.forward(&h)?;
        let cross = match (&cache.cross_keys, &cache.cross_values) {
            // Clés et valeurs croisées déjà projetées : l'attention seule.
            (Some(keys), Some(values)) => {
                let q = self
                    .encoder_attn
                    .heads_of(&self.encoder_attn.q.forward(&y)?)?;
                self.encoder_attn.attend(&q, keys, values, None)?
            }
            _ => {
                let (out, keys, values) =
                    self.encoder_attn.forward(&y, encoder_hidden, None, None)?;
                cache.cross_keys = Some(keys);
                cache.cross_values = Some(values);
                out
            }
        };
        let h = h.add(&cross)?;

        let y = self.final_layer_norm.forward(&h)?;
        let y = self.fc2.forward(&nn::gelu(self.fc1.forward(&y)?)?)?;
        Ok(h.add(&y)?)
    }
}

/// `WhisperDecoder`.
pub struct Decoder {
    pub embed_tokens: Embedding,
    /// `embed_positions.weight` : `[positions, d]`.
    pub embed_positions: Array,
    pub layers: Vec<DecoderLayer>,
    pub layer_norm: LayerNorm,
}

impl Decoder {
    pub fn load(w: &mut Weights, c: &Config) -> Result<Self> {
        let p = "model.decoder";
        let tokens = format!("{p}.embed_tokens");
        if w.quantization.is_some() && !w.has(&format!("{tokens}.scales")) {
            return Err(missing(&format!("{tokens}.scales")));
        }
        Ok(Self {
            embed_tokens: Embedding::load(w, &tokens, c.vocab_size, c.d_model)?,
            embed_positions: w.take(
                &format!("{p}.embed_positions.weight"),
                &[c.max_target_positions, c.d_model],
            )?,
            layers: (0..c.decoder_layers)
                .map(|i| DecoderLayer::load(w, &format!("{p}.layers.{i}"), c))
                .collect::<Result<_>>()?,
            layer_norm: LayerNorm::load(w, &format!("{p}.layer_norm"), c.d_model)?,
        })
    }

    /// `callAsFunction` : l'invite entière (masque causal) ou un jeton, à
    /// partir de la position `start` ; rend les états `[1, T, d]`.
    pub fn forward(
        &self,
        tokens: &Array,
        start: i32,
        encoder_hidden: &Array,
        caches: &mut [LayerCache],
    ) -> Result<Array> {
        let (_, count) = batch_and_time(tokens)?;
        let indices: Vec<i32> = (start..start + count).collect();
        let positions = self
            .embed_positions
            .take_axis(Array::from_slice(&indices, &[count]), 0)?
            .expand_dims(0)?;
        let mut h = self.embed_tokens.forward(tokens)?.add(&positions)?;
        let mask = if count > 1 {
            Some(causal_mask(count, start + count, h.dtype())?)
        } else {
            None
        };
        for (layer, cache) in self.layers.iter().zip(caches.iter_mut()) {
            h = layer.forward(&h, encoder_hidden, mask.as_ref(), cache)?;
        }
        self.layer_norm.forward(&h)
    }

    /// `projectToVocab` : les logits par la matrice des jetons.
    pub fn project(&self, hidden: &Array) -> Result<Array> {
        self.embed_tokens.as_linear(hidden)
    }
}

/// Le masque causal `[nouveaux, total]` : 0 où la colonne précède ou égale
/// la ligne, −1e9 ailleurs, au type des états.
fn causal_mask(count: i32, total: i32, dtype: Dtype) -> Result<Array> {
    let start = total - count;
    let rows: Vec<i32> = (start..total).collect();
    let cols: Vec<i32> = (0..total).collect();
    let rows = Array::from_slice(&rows, &[count]).expand_dims(1)?;
    let cols = Array::from_slice(&cols, &[total]).expand_dims(0)?;
    let allowed = cols.le(&rows)?;
    let zero = ops::zeros_dtype(&[count, total], dtype)?;
    let blocked = ops::broadcast_to(Array::from_f32(-1e9).as_dtype(dtype)?, &[count, total])?;
    Ok(ops::select(&allowed, &zero, &blocked)?)
}
