//! PL-BERT (ALBERT) de Kokoro, comme `Albert.swift` : plongements, couches
//! partagées par groupe, sortie de l'encodeur. La sortie regroupée
//! (`pooler`) n'est jamais lue par Kokoro : ses poids sont vérifiés, pas
//! calculés (le Swift la construit sans jamais l'évaluer).

use anyhow::Result;
use mlx_rs::{ops, Array};

use super::config::{Plbert, Weights};
use super::modules::{scalar, Embedding, LayerNorm, Linear};

const GELU: f32 = 0.797_884_6;
const GELU_CUBE: f32 = 0.044_715;

pub struct Albert {
    word: Embedding,
    position: Embedding,
    token_type: Embedding,
    norm: LayerNorm,
    mapping_in: Linear,
    groups: Vec<Vec<Layer>>,
    layers: i32,
    heads: i32,
    head_size: i32,
}

struct Layer {
    query: Linear,
    key: Linear,
    value: Linear,
    dense: Linear,
    attention_norm: LayerNorm,
    full_norm: LayerNorm,
    ffn: Linear,
    ffn_output: Linear,
}

impl Albert {
    pub fn load(w: &mut Weights, config: &Plbert, vocab: i32) -> Result<Self> {
        let (emb, hidden, eps) = (config.embedding_size, config.hidden_size, config.layer_norm_eps);
        let e = |name: &str| format!("bert.embeddings.{name}");
        let word = Embedding::load(w, &e("word_embeddings"), vocab, emb)?;
        let position = Embedding::load(w, &e("position_embeddings"), config.max_position_embeddings, emb)?;
        let token_type = Embedding::load(w, &e("token_type_embeddings"), config.type_vocab_size, emb)?;
        let norm = LayerNorm::load(w, &e("LayerNorm"), emb, eps, ("weight", "bias"))?;
        let mapping_in = Linear::load(w, "bert.encoder.embedding_hidden_mapping_in", emb, hidden)?;
        let mut groups = Vec::new();
        for g in 0..config.num_hidden_groups {
            let mut layers = Vec::new();
            for l in 0..config.inner_group_num {
                let p = format!("bert.encoder.albert_layer_groups.{g}.albert_layers.{l}");
                let a = |name: &str| format!("{p}.attention.{name}");
                layers.push(Layer {
                    query: Linear::load(w, &a("query"), hidden, hidden)?,
                    key: Linear::load(w, &a("key"), hidden, hidden)?,
                    value: Linear::load(w, &a("value"), hidden, hidden)?,
                    dense: Linear::load(w, &a("dense"), hidden, hidden)?,
                    attention_norm: LayerNorm::load(w, &a("LayerNorm"), hidden, eps, ("weight", "bias"))?,
                    full_norm: LayerNorm::load(w, &format!("{p}.full_layer_layer_norm"), hidden, eps, ("weight", "bias"))?,
                    ffn: Linear::load(w, &format!("{p}.ffn"), hidden, config.intermediate_size)?,
                    ffn_output: Linear::load(w, &format!("{p}.ffn_output"), config.intermediate_size, hidden)?,
                });
            }
            groups.push(layers);
        }
        // Vérifiée, puis lâchée : jamais calculée.
        Linear::load(w, "bert.pooler", hidden, hidden)?;
        Ok(Self {
            word,
            position,
            token_type,
            norm,
            mapping_in,
            groups,
            layers: config.num_hidden_layers,
            heads: config.num_attention_heads,
            head_size: hidden / config.num_attention_heads,
        })
    }

    /// `ids` : `[1, T]` ; `attention` : `[1, T]` en int32, 1 pour un jeton
    /// réel. Rend la sortie de l'encodeur, `[1, T, hidden]`.
    pub fn forward(&self, ids: &Array, attention: &Array) -> Result<Array> {
        let length = ids.shape()[1];
        let positions = Array::from_iter(0..length, &[1, length]);
        let types = ops::zeros_like(ids)?;
        let embeddings = self
            .word
            .forward(ids)?
            .add(self.position.forward(&positions)?)?
            .add(self.token_type.forward(&types)?)?;
        let embeddings = self.norm.forward(&embeddings)?;

        let m = attention.shape();
        let expanded = attention.reshape(&[m[0], 1, 1, m[m.len() - 1]])?;
        let mask = scalar(1.0).subtract(expanded)?.multiply(scalar(-10_000.0))?;

        let mut x = self.mapping_in.forward(&embeddings)?;
        let per_group = self.layers / self.groups.len() as i32;
        for i in 0..self.layers {
            let group = &self.groups[(i / per_group) as usize];
            for layer in group {
                x = self.layer(layer, &x, &mask)?;
            }
        }
        Ok(x)
    }

    fn heads(&self, x: Array) -> Result<Array> {
        let mut shape = x.shape().to_vec();
        shape.pop();
        shape.extend([self.heads, self.head_size]);
        Ok(x.reshape(&shape)?.transpose_axes(&[0, 2, 1, 3])?)
    }

    fn layer(&self, layer: &Layer, x: &Array, mask: &Array) -> Result<Array> {
        let q = self.heads(layer.query.forward(x)?)?;
        let k = self.heads(layer.key.forward(x)?)?;
        let v = self.heads(layer.value.forward(x)?)?;
        let mut scores = q.matmul(k.transpose_axes(&[0, 1, 3, 2])?)?;
        scores = scores.divide(scalar((self.head_size as f32).sqrt()))?;
        scores = scores.add(mask)?;
        let probs = ops::softmax_axis(&scores, -1, None)?;
        let context = probs.matmul(&v)?.transpose_axes(&[0, 2, 1, 3])?;
        let mut shape = context.shape().to_vec();
        shape.truncate(shape.len() - 2);
        shape.push(self.heads * self.head_size);
        let context = context.reshape(&shape)?;
        let projected = layer.dense.forward(&context)?;
        let attended = layer.attention_norm.forward(&projected.add(x)?)?;

        let h = layer.ffn.forward(&attended)?;
        let cube = scalar(GELU_CUBE).multiply(h.power(scalar(3.0))?)?;
        let inner = scalar(GELU).multiply(h.add(cube)?)?;
        let activated = scalar(0.5).multiply(&h)?.multiply(scalar(1.0).add(ops::tanh(&inner)?)?)?;
        let ffn = layer.ffn_output.forward(&activated)?;
        layer.full_norm.forward(&ffn.add(&attended)?)
    }
}
