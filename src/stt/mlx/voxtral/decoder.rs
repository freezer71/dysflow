//! Le décodeur de Voxtral (`VoxtralRealtimeDecoder.swift`) : un Transformer
//! à attention groupée et fenêtre glissante, conditionné par le retard
//! (`AdaRMSNorm` : une échelle par couche, calculée une fois pour le retard),
//! avec un cache de clés et valeurs écrit en place dans des tampons agrandis
//! par 256 lignes, et des logits par la matrice des jetons (liée).

use anyhow::{anyhow, Result};
use mlx_rs::ops::indexing::{TryIndexMutOp, TryIndexOp};
use mlx_rs::{nn, ops, Array, Dtype};
use std::ops::Range;

use super::config::DecoderConfig;
use super::encoder::{apply_interleaved_rope, range, window_mask, Mask};
use super::model::linear;
use crate::stt::mlx::nn::{scalar, scalar_like, Embedding, Linear, RmsNorm};
use crate::stt::mlx::weights::Weights;

/// Le calcul d'index d'un `append` (`VoxtralRealtimeDecoderKVCacheAppendPlan`),
/// sans Metal. Les lignes plus vieilles que la fenêtre restent en place
/// jusqu'à ce qu'un bloc s'en soit accumulé ou que le tampon soit plein ;
/// alors la fenêtre est recopiée à la ligne zéro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendPlan {
    /// Les lignes à recopier à la ligne zéro avant l'ajout, s'il y en a.
    pub compaction: Option<Range<i32>>,
    /// Où vont les lignes nouvelles, après la recopie.
    pub append: Range<i32>,
    pub capacity: i32,
    pub count: i32,
    /// La position absolue de la ligne zéro après l'ajout.
    pub position_offset: i32,
    /// Les lignes que l'attention lit : les `sliding_window` dernières.
    pub window: Range<i32>,
    /// La position absolue de la première de `window`.
    pub window_position_offset: i32,
    pub grows: bool,
}

/// `capacityBlock`.
pub const CAPACITY_BLOCK: i32 = 256;

impl AppendPlan {
    /// `make(count:capacity:positionOffset:appendCount:slidingWindow:)`.
    /// Les `precondition` du Swift deviennent une erreur.
    pub fn make(
        count: i32,
        capacity: i32,
        position_offset: i32,
        append_count: i32,
        sliding_window: i32,
    ) -> Result<Self> {
        if !(0..=capacity).contains(&count) || append_count < 0 || sliding_window <= 0 {
            return Err(anyhow!(crate::tr!(
                en = "Voxtral: impossible decoder cache ({count} rows of {capacity}, {append_count} more, window {sliding_window})",
                fr = "Voxtral : cache du décodeur impossible ({count} lignes sur {capacity}, {append_count} de plus, fenêtre de {sliding_window})",
            )));
        }
        let projected = count + append_count;
        let excess = (projected - sliding_window).max(0);
        let compact = count > sliding_window && (projected > capacity || excess > CAPACITY_BLOCK);
        let compaction = compact.then(|| (count - sliding_window)..count);
        let dropped = compaction.as_ref().map_or(0, |r| r.start);
        let compacted = compaction.as_ref().map_or(count, |r| r.end - r.start);
        let append = compacted..(compacted + append_count);
        let required = append.end;
        let grows = required > capacity;
        let capacity = if grows {
            CAPACITY_BLOCK.max((required + CAPACITY_BLOCK - 1) / CAPACITY_BLOCK * CAPACITY_BLOCK)
        } else {
            capacity
        };
        let count = append.end;
        let start = (count - sliding_window).max(0);
        let position_offset = position_offset + dropped;
        Ok(Self {
            compaction,
            append,
            capacity,
            count,
            position_offset,
            window: start..count,
            window_position_offset: position_offset + start,
            grows,
        })
    }
}

/// `VoxtralRealtimeDecoderKVCache` : les lignes nouvelles sont écrites en
/// place (MLX ne copie pas le tampon tant que rien d'autre ne le tient).
pub struct KvCache {
    keys: Array,
    values: Array,
    count: i32,
    position_offset: i32,
    window: i32,
}

impl KvCache {
    fn new(width: i32, dtype: Dtype, window: i32) -> Result<Self> {
        Ok(Self {
            keys: ops::zeros_dtype(&[CAPACITY_BLOCK, width], dtype)?,
            values: ops::zeros_dtype(&[CAPACITY_BLOCK, width], dtype)?,
            count: 0,
            position_offset: 0,
            window,
        })
    }

    /// `append(keys:values:)` : rend les lignes que l'attention lit et la
    /// position absolue de la première.
    fn append(&mut self, keys: &Array, values: &Array) -> Result<(Array, Array, i32)> {
        let plan = AppendPlan::make(
            self.count,
            self.keys.shape()[0],
            self.position_offset,
            keys.shape()[0],
            self.window,
        )?;
        if let Some(kept) = plan.compaction.clone() {
            let n = kept.end - kept.start;
            // Les plages peuvent se chevaucher : l'écriture est fonctionnelle,
            // la droite est lue avant.
            let k = self.keys.try_index(kept.clone())?;
            let v = self.values.try_index(kept)?;
            self.keys.try_index_mut(0..n, &k)?;
            self.values.try_index_mut(0..n, &v)?;
            self.count = n;
        }
        if plan.grows {
            let width = self.keys.shape()[1];
            let mut keys = ops::zeros_dtype(&[plan.capacity, width], self.keys.dtype())?;
            let mut values = ops::zeros_dtype(&[plan.capacity, width], self.values.dtype())?;
            if self.count > 0 {
                keys.try_index_mut(0..self.count, self.keys.try_index(0..self.count)?)?;
                values.try_index_mut(0..self.count, self.values.try_index(0..self.count)?)?;
            }
            self.keys = keys;
            self.values = values;
        }
        self.keys.try_index_mut(plan.append.clone(), keys)?;
        self.values.try_index_mut(plan.append, values)?;
        self.count = plan.count;
        self.position_offset = plan.position_offset;
        Ok((
            self.keys.try_index(plan.window.clone())?,
            self.values.try_index(plan.window)?,
            plan.window_position_offset,
        ))
    }
}

/// `voxtralComputeTimeEmbedding` : `[cos(t·f), sin(t·f)]`, θ = 10 000.
pub fn time_embedding(t: f32, dim: i32) -> Result<Array> {
    let half = dim / 2;
    let idx = range(0, half).as_dtype(Dtype::Float32)?;
    let inv = ops::exp(
        scalar(-(10_000f32).ln())
            .multiply(&idx)?
            .divide(scalar(half as f32))?,
    )?;
    let emb = scalar(t).multiply(&inv)?;
    Ok(ops::concatenate(&[ops::cos(&emb)?, ops::sin(&emb)?], 0)?)
}

/// `VoxtralRealtimeAdaRMSNorm`.
struct AdaNorm {
    down: Linear,
    up: Linear,
}

impl AdaNorm {
    fn scale(&self, t_cond: &Array) -> Result<Array> {
        self.up.forward(&nn::gelu(self.down.forward(t_cond)?)?)
    }

    /// `x · (1 + échelle)`, l'échelle au type de `x`.
    fn forward(x: &Array, scale: &Array) -> Result<Array> {
        let scale = scale.as_dtype(x.dtype())?;
        Ok(x.multiply(scalar_like(1.0, &scale)?.add(&scale)?)?)
    }
}

struct Attention {
    wq: Linear,
    wk: Linear,
    wv: Linear,
    wo: Linear,
    heads: i32,
    kv_heads: i32,
    head_dim: i32,
    window: i32,
    scale: f32,
}

impl Attention {
    fn forward(
        &self,
        x: &Array,
        positions: &Array,
        cos: &Array,
        sin: &Array,
        cache: &mut Option<KvCache>,
    ) -> Result<Array> {
        let t = x.shape()[0];
        let d = self.head_dim;
        let q = apply_interleaved_rope(&self.wq.forward(x)?, cos, sin, self.heads, d)?;
        let k = apply_interleaved_rope(&self.wk.forward(x)?, cos, sin, self.kv_heads, d)?;
        let v = self.wv.forward(x)?;
        let fresh = cache.is_none();
        if fresh {
            *cache = Some(KvCache::new(k.shape()[1], k.dtype(), self.window)?);
        }
        let Some(cache) = cache.as_mut() else {
            return Err(anyhow!("Voxtral: decoder cache"));
        };
        let (k, v, offset) = cache.append(&k, &v)?;
        let kv_len = k.shape()[0];
        let q4 = q
            .reshape(&[1, t, self.heads, d])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let k4 = k
            .reshape(&[1, kv_len, self.kv_heads, d])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let v4 = v
            .reshape(&[1, kv_len, self.kv_heads, d])?
            .transpose_axes(&[0, 2, 1, 3])?;
        let mask = if t == 1 {
            Mask::None
        } else if t <= self.window && fresh {
            Mask::Causal
        } else {
            Mask::Array(window_mask(
                positions,
                offset,
                kv_len,
                self.window,
                q.dtype(),
            )?)
        };
        let attn = mlx_rs::fast::scaled_dot_product_attention(
            q4,
            k4,
            v4,
            self.scale,
            mask.mode(),
            None::<&Array>,
        )?;
        let out = attn
            .transpose_axes(&[0, 2, 1, 3])?
            .reshape(&[t, self.heads * d])?;
        self.wo.forward(&out)
    }
}

struct Layer {
    attention_norm: RmsNorm,
    attention: Attention,
    ffn_norm: RmsNorm,
    ada: Option<AdaNorm>,
    w1: Linear,
    w3: Linear,
    w2: Linear,
}

impl Layer {
    fn load(w: &mut Weights, prefix: &str, c: &DecoderConfig) -> Result<Self> {
        let (q_dim, kv_dim) = (c.n_heads * c.head_dim, c.n_kv_heads * c.head_dim);
        let attention = Attention {
            wq: linear(w, &format!("{prefix}.attention.wq"), c.dim, q_dim, false)?,
            wk: linear(w, &format!("{prefix}.attention.wk"), c.dim, kv_dim, false)?,
            wv: linear(w, &format!("{prefix}.attention.wv"), c.dim, kv_dim, false)?,
            wo: linear(w, &format!("{prefix}.attention.wo"), q_dim, c.dim, false)?,
            heads: c.n_heads,
            kv_heads: c.n_kv_heads,
            head_dim: c.head_dim,
            window: c.sliding_window,
            scale: (c.head_dim as f32).powf(-0.5),
        };
        // Jamais quantifiées (`shouldQuantize`) : des `Linear` en clair.
        let ada = match c.ada_dim {
            Some(bottleneck) => {
                let p = format!("{prefix}.ada_rms_norm_t_cond");
                Some(AdaNorm {
                    down: Linear::Dense {
                        weight: w.take(&format!("{p}.ada_down.weight"), &[bottleneck, c.dim])?,
                        bias: None,
                    },
                    up: Linear::Dense {
                        weight: w.take(&format!("{p}.ada_up.weight"), &[c.dim, bottleneck])?,
                        bias: None,
                    },
                })
            }
            None => None,
        };
        Ok(Self {
            attention_norm: RmsNorm::load(
                w,
                &format!("{prefix}.attention_norm"),
                c.dim,
                c.norm_eps,
            )?,
            attention,
            ffn_norm: RmsNorm::load(w, &format!("{prefix}.ffn_norm"), c.dim, c.norm_eps)?,
            ada,
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
                false,
            )?,
        })
    }

    fn forward(
        &self,
        x: &Array,
        positions: &Array,
        cos: &Array,
        sin: &Array,
        ada_scale: Option<&Array>,
        cache: &mut Option<KvCache>,
    ) -> Result<Array> {
        let h = self.attention_norm.forward(x)?;
        let h = self.attention.forward(&h, positions, cos, sin, cache)?;
        let out = x.add(&h)?;
        let mut h = self.ffn_norm.forward(&out)?;
        if let (Some(scale), Some(_)) = (ada_scale, &self.ada) {
            h = AdaNorm::forward(&h, scale)?;
        }
        let gate = nn::silu(self.w1.forward(&h)?)?;
        let up = self.w3.forward(&h)?;
        Ok(out.add(self.w2.forward(&gate.multiply(&up)?)?)?)
    }
}

/// `VoxtralRealtimeDecoder`.
pub struct Decoder {
    pub config: DecoderConfig,
    embeddings: Embedding,
    layers: Vec<Layer>,
    norm: RmsNorm,
    /// L'échelle de chaque couche pour le retard (`precomputeAdaScales`).
    ada_scales: Vec<Option<Array>>,
}

/// Les caches d'un flux, une couche chacun.
pub type Caches = Vec<Option<KvCache>>;

impl Decoder {
    pub fn load(w: &mut Weights, c: &DecoderConfig) -> Result<Self> {
        let layers = (0..c.n_layers)
            .map(|i| Layer::load(w, &format!("decoder.layers.{i}"), c))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            config: c.clone(),
            embeddings: Embedding::load(w, "decoder.tok_embeddings", c.vocab_size, c.dim)?,
            ada_scales: Vec::new(),
            layers,
            norm: RmsNorm::load(w, "decoder.norm", c.dim, c.norm_eps)?,
        })
    }

    /// `precomputeAdaScales` puis l'`eval` de chaque échelle.
    pub fn precompute_ada_scales(&mut self, t_cond: &Array) -> Result<()> {
        let scales = self
            .layers
            .iter()
            .map(|layer| layer.ada.as_ref().map(|ada| ada.scale(t_cond)).transpose())
            .collect::<Result<Vec<_>>>()?;
        for scale in scales.iter().flatten() {
            mlx_rs::transforms::eval([scale])?;
        }
        self.ada_scales = scales;
        Ok(())
    }

    /// `embedToken` : la ligne d'un jeton, `[dim]`.
    pub fn embed_token(&self, token: usize) -> Result<Array> {
        let id = i32::try_from(token).map_err(|_| anyhow!("Voxtral: token {token}"))?;
        Ok(self
            .embeddings
            .forward(&Array::from_slice(&[id], &[1]))?
            .squeeze_axes(&[0])?)
    }

    /// `embedTokens`.
    pub fn embed_tokens(&self, ids: &[i32]) -> Result<Array> {
        self.embeddings
            .forward(&Array::from_slice(ids, &[ids.len() as i32]))
    }

    /// `callAsFunction(_:startPos:cache:)` : `embeds` `[t, dim]` aux
    /// positions `[start, start + t)` ; rend l'état normé.
    pub fn forward(&self, embeds: &Array, start: i32, caches: &mut Caches) -> Result<Array> {
        let t = embeds.shape()[0];
        let positions = range(start, start + t);
        let (cos, sin) = self.rope_frequencies(&positions)?;
        caches.resize_with(self.layers.len(), || None);
        let mut h = embeds.clone();
        for (i, (layer, cache)) in self.layers.iter().zip(caches.iter_mut()).enumerate() {
            let scale = self.ada_scales.get(i).and_then(Option::as_ref);
            h = layer.forward(&h, &positions, &cos, &sin, scale, cache)?;
        }
        self.norm.forward(&h)
    }

    /// `ropeFrequencies` (décodeur) : `1 / θ^(i / d)`.
    fn rope_frequencies(&self, positions: &Array) -> Result<(Array, Array)> {
        let d = self.config.head_dim;
        let idx: Vec<i32> = (0..d).step_by(2).collect();
        let idx = Array::from_slice(&idx, &[idx.len() as i32]).as_dtype(Dtype::Float32)?;
        let inv = scalar(1.0).divide(ops::power(
            scalar(self.config.rope_theta),
            idx.divide(scalar(d as f32))?,
        )?)?;
        let angles = positions
            .as_dtype(Dtype::Float32)?
            .expand_dims(1)?
            .multiply(inv.expand_dims(0)?)?;
        Ok((ops::cos(&angles)?, ops::sin(&angles)?))
    }

    /// `logits` : une ligne `[dim]` → `[vocabulaire]`, par la matrice liée.
    pub fn logits(&self, h: &Array) -> Result<Array> {
        Ok(self
            .embeddings
            .as_linear(&h.expand_dims(0)?)?
            .squeeze_axes(&[0])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(count: i32, capacity: i32, offset: i32, append: i32, window: i32) -> AppendPlan {
        AppendPlan::make(count, capacity, offset, append, window).unwrap()
    }

    /// `VoxtralRealtimeDecoderKVCacheAppendPlan` : le premier ajout prend un
    /// bloc de 256 lignes ; tout tient tant que la fenêtre n'est pas
    /// dépassée.
    #[test]
    fn the_first_rows_take_one_block() {
        let p = plan(0, 256, 0, 39, 8192);
        assert_eq!(p.compaction, None);
        assert_eq!((p.append.clone(), p.capacity, p.count), (0..39, 256, 39));
        assert_eq!((p.window.clone(), p.window_position_offset), (0..39, 0));
        assert!(!p.grows);
    }

    #[test]
    fn a_full_buffer_grows_by_whole_blocks() {
        let p = plan(256, 256, 0, 1, 8192);
        assert!(p.grows);
        assert_eq!(
            (p.capacity, p.count, p.append.clone()),
            (512, 257, 256..257)
        );
        let p = plan(0, 256, 0, 600, 8192);
        assert_eq!(p.capacity, 768, "arrondi au bloc");
    }

    /// Les lignes hors de la fenêtre restent tant qu'un bloc ne s'en est pas
    /// accumulé ; puis la fenêtre est recopiée à zéro et la position
    /// absolue avance d'autant.
    #[test]
    fn old_rows_are_compacted_once_a_block_has_piled_up() {
        // Fenêtre de 4 : 10 lignes dans 256, rien ne bouge encore.
        let p = plan(10, 256, 0, 1, 4);
        assert_eq!(p.compaction, None);
        assert_eq!((p.window.clone(), p.window_position_offset), (7..11, 7));
        // 260 lignes : 257 de trop après l'ajout, plus d'un bloc.
        let p = plan(260, 512, 0, 1, 4);
        assert_eq!(p.compaction, Some(256..260));
        assert_eq!(
            (p.append.clone(), p.count, p.position_offset),
            (4..5, 5, 256)
        );
        assert_eq!((p.window.clone(), p.window_position_offset), (1..5, 257));
        assert!(!p.grows);
        // Le tampon plein : recopié plutôt qu'agrandi.
        let p = plan(256, 256, 100, 1, 4);
        assert_eq!(p.compaction, Some(252..256));
        assert_eq!((p.capacity, p.count, p.position_offset), (256, 5, 352));
    }

    #[test]
    fn an_impossible_cache_is_an_error() {
        assert!(AppendPlan::make(3, 2, 0, 1, 4).is_err());
        assert!(AppendPlan::make(-1, 2, 0, 1, 4).is_err());
        assert!(AppendPlan::make(0, 2, 0, -1, 4).is_err());
        assert!(AppendPlan::make(0, 2, 0, 1, 0).is_err());
    }

    /// Le cache rend, après chaque ajout, les mêmes lignes qu'une simple
    /// concaténation coupée à la fenêtre, recopies et agrandissements
    /// compris.
    #[test]
    fn the_cache_reads_like_a_windowed_concatenation() {
        let _serial = crate::stt::mlx::parakeet::tests::serial();
        super::super::synthetic::on_the_cpu_thread(|| {
            let mut cache = KvCache::new(2, Dtype::Float32, 4)?;
            let mut all: Vec<f32> = Vec::new();
            for step in 0..700 {
                let rows = if step % 50 == 0 { 3 } else { 1 };
                let values: Vec<f32> = (0..rows * 2)
                    .map(|i| (all.len() + i as usize) as f32)
                    .collect();
                let new = Array::from_slice(&values, &[rows, 2]);
                let (k, _, offset) = cache.append(&new, &new)?;
                all.extend_from_slice(&values);
                let total = all.len() as i32 / 2;
                let start = (total - 4).max(0);
                k.eval()?;
                anyhow::ensure!(
                    k.as_slice::<f32>() == &all[(start * 2) as usize..],
                    "pas {step}"
                );
                anyhow::ensure!(offset == start, "pas {step} : {offset}");
                anyhow::ensure!(cache.keys.shape()[0] <= 512, "le tampon reste borné");
            }
            Ok(())
        })
        .unwrap();
    }
}
