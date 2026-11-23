//! Le décodeur texte de Qwen3-ASR (`Qwen3ASRTextModel`) : plongement des
//! jetons (quantifié), couches Qwen3 (normes RMS en tête ; attention groupée
//! avec normes RMS des requêtes et des clés, RoPE ordinaire décalé de la
//! position du cache, `KVCacheSimple` ; MLP `silu`), norme finale, logits par
//! la matrice des jetons (liée) ou par `lm_head`. Mêmes opérations dans le
//! même ordre que le Swift ; les types suivent ceux des poids (bf16 tels
//! que livrés), aucune conversion ajoutée.

use anyhow::{anyhow, Result};
use mlx_rs::fast::{self, ScaledDotProductAttentionMask};
use mlx_rs::{nn, Array};

use super::config::{TextConfig, NAME};
use crate::stt::mlx::nn::{Embedding, KvCache, Linear, RmsNorm};
use crate::stt::mlx::weights::Weights;

/// `Qwen3ASRTextAttention`.
struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    heads: i32,
    kv_heads: i32,
    head_dim: i32,
    scale: f32,
    rope_theta: f32,
}

impl Attention {
    fn load(w: &mut Weights, prefix: &str, c: &TextConfig) -> Result<Self> {
        let (hidden, heads, kv, d) = (
            c.hidden_size,
            c.num_attention_heads,
            c.num_key_value_heads,
            c.head_dim,
        );
        let width = |n: i32| {
            n.checked_mul(d).ok_or_else(|| {
                anyhow!(crate::tr!(
                    en = "{NAME}: attention width too large",
                    fr = "{NAME} : largeur de l'attention trop grande",
                ))
            })
        };
        Ok(Self {
            q: Linear::load(w, &format!("{prefix}.q_proj"), hidden, width(heads)?, false)?,
            k: Linear::load(w, &format!("{prefix}.k_proj"), hidden, width(kv)?, false)?,
            v: Linear::load(w, &format!("{prefix}.v_proj"), hidden, width(kv)?, false)?,
            o: Linear::load(w, &format!("{prefix}.o_proj"), width(heads)?, hidden, false)?,
            q_norm: RmsNorm::load(w, &format!("{prefix}.q_norm"), d, c.rms_norm_eps)?,
            k_norm: RmsNorm::load(w, &format!("{prefix}.k_norm"), d, c.rms_norm_eps)?,
            heads,
            kv_heads: kv,
            head_dim: d,
            scale: (d as f32).powf(-0.5),
            rope_theta: c.rope_theta,
        })
    }

    /// `RoPE(dimensions: headDim, traditional: false, base: ropeTheta)`.
    fn rope(&self, x: &Array, offset: i32) -> Result<Array> {
        Ok(fast::rope(
            x,
            self.head_dim,
            false,
            self.rope_theta,
            1.0,
            offset,
            None::<&Array>,
        )?)
    }

    fn forward(&self, x: &Array, causal: bool, cache: &mut KvCache) -> Result<Array> {
        let (b, l) = batch_and_time(x)?;
        let q = self
            .q
            .forward(x)?
            .reshape(&[b, l, self.heads, self.head_dim])?;
        let k = self
            .k
            .forward(x)?
            .reshape(&[b, l, self.kv_heads, self.head_dim])?;
        let v = self
            .v
            .forward(x)?
            .reshape(&[b, l, self.kv_heads, self.head_dim])?;
        let q = self.q_norm.forward(&q)?.transpose_axes(&[0, 2, 1, 3])?;
        let k = self.k_norm.forward(&k)?.transpose_axes(&[0, 2, 1, 3])?;
        let v = v.transpose_axes(&[0, 2, 1, 3])?;
        // La position avant l'ajout au cache.
        let offset = cache.offset();
        let q = self.rope(&q, offset)?;
        let k = self.rope(&k, offset)?;
        let (k, v) = cache.update(&k, &v)?;
        let mask = causal.then_some(ScaledDotProductAttentionMask::Causal);
        let attended =
            fast::scaled_dot_product_attention(q, k, v, self.scale, mask, None::<&Array>)?;
        let merged = attended
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[b, l, -1])?;
        self.o.forward(&merged)
    }
}

/// `[B, T, …]`.
fn batch_and_time(x: &Array) -> Result<(i32, i32)> {
    match x.shape() {
        [b, t, ..] => Ok((*b, *t)),
        shape => Err(anyhow!(crate::tr!(
            en = "{NAME}: shape {shape:?}, at least two axes expected",
            fr = "{NAME} : forme {shape:?}, au moins deux axes attendus",
        ))),
    }
}

/// `Qwen3ASRTextMLP` : `down(silu(gate(x)) · up(x))`.
struct Mlp {
    gate: Linear,
    up: Linear,
    down: Linear,
}

/// `Qwen3ASRTextDecoderLayer`.
struct Layer {
    attn: Attention,
    mlp: Mlp,
    input_norm: RmsNorm,
    post_attention_norm: RmsNorm,
}

impl Layer {
    fn load(w: &mut Weights, prefix: &str, c: &TextConfig) -> Result<Self> {
        let (hidden, inner) = (c.hidden_size, c.intermediate_size);
        Ok(Self {
            attn: Attention::load(w, &format!("{prefix}.self_attn"), c)?,
            mlp: Mlp {
                gate: Linear::load(w, &format!("{prefix}.mlp.gate_proj"), hidden, inner, false)?,
                up: Linear::load(w, &format!("{prefix}.mlp.up_proj"), hidden, inner, false)?,
                down: Linear::load(w, &format!("{prefix}.mlp.down_proj"), inner, hidden, false)?,
            },
            input_norm: RmsNorm::load(
                w,
                &format!("{prefix}.input_layernorm"),
                hidden,
                c.rms_norm_eps,
            )?,
            post_attention_norm: RmsNorm::load(
                w,
                &format!("{prefix}.post_attention_layernorm"),
                hidden,
                c.rms_norm_eps,
            )?,
        })
    }

    fn forward(&self, x: &Array, causal: bool, cache: &mut KvCache) -> Result<Array> {
        let h = x.add(
            self.attn
                .forward(&self.input_norm.forward(x)?, causal, cache)?,
        )?;
        let y = self.post_attention_norm.forward(&h)?;
        let gated = nn::silu(self.mlp.gate.forward(&y)?)?.multiply(self.mlp.up.forward(&y)?)?;
        Ok(h.add(self.mlp.down.forward(&gated)?)?)
    }
}

pub struct Decoder {
    pub embed_tokens: Embedding,
    layers: Vec<Layer>,
    norm: RmsNorm,
    /// `lm_head`, si les plongements ne sont pas liés.
    lm_head: Option<Linear>,
}

impl Decoder {
    /// Les poids sous `model.` (et `lm_head`), chaque clé avec sa forme.
    pub fn load(w: &mut Weights, c: &TextConfig) -> Result<Self> {
        let layers = (0..c.num_hidden_layers)
            .map(|i| Layer::load(w, &format!("model.layers.{i}"), c))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            embed_tokens: Embedding::load(w, "model.embed_tokens", c.vocab_size, c.hidden_size)?,
            layers,
            norm: RmsNorm::load(w, "model.norm", c.hidden_size, c.rms_norm_eps)?,
            lm_head: if c.tie_word_embeddings {
                None
            } else {
                Some(Linear::load(
                    w,
                    "lm_head",
                    c.hidden_size,
                    c.vocab_size,
                    false,
                )?)
            },
        })
    }

    /// Un cache par couche (`makeCache`).
    pub fn make_cache(&self) -> Vec<KvCache> {
        self.layers.iter().map(|_| KvCache::default()).collect()
    }

    /// `callAsFunction(inputIds:inputEmbeddings:cache:)` sans audio : les
    /// plongements `[1, T, hidden]` passés par toutes les couches (masque
    /// causal dès deux positions, aucun sinon), la norme, puis les logits
    /// `[1, T, vocabulaire]`.
    pub fn forward(&self, embeds: &Array, caches: &mut [KvCache]) -> Result<Array> {
        let (_, time) = batch_and_time(embeds)?;
        let causal = time > 1;
        let mut h = embeds.clone();
        for (layer, cache) in self.layers.iter().zip(caches.iter_mut()) {
            h = layer.forward(&h, causal, cache)?;
        }
        let h = self.norm.forward(&h)?;
        match &self.lm_head {
            Some(head) => head.forward(&h),
            None => self.embed_tokens.as_linear(&h),
        }
    }
}
