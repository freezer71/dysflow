//! Le modèle Canary : chargement d'un dossier MLX (comme
//! `CanaryModel.fromModelDirectory` : `config.json`, le vocabulaire, les
//! `*.safetensors` renommés comme `sanitizeMLXNative`, la quantification de
//! `config.json`, chaque tenseur à son type du fichier), puis la lecture
//! (`generate` : mel Float32, Conformer de Parakeet, invite avec la langue
//! source et cible, décodage glouton pas à pas).

use anyhow::{anyhow, bail, ensure, Result};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::{Array, Dtype};
use std::cell::Cell;
use std::path::Path;

use super::config::{Config, CONFIG_FILE};
use super::decoder::{Decoder, NAME};
use super::tokenizer::Tokenizer;
use crate::stt::mlx::parakeet::audio::log_mel_spectrogram;
use crate::stt::mlx::parakeet::conformer::Conformer;
use crate::stt::mlx::weights::Weights;

/// Les jetons gardés au plus (`CanarySpeech.maxTokens` : le décodeur a
/// 1 024 positions).
pub const MAX_TOKENS: usize = 1024;

/// Ce qu'une lecture a rendu : l'invite, les jetons gardés (sans la fin),
/// leur texte sans espaces autour (`generate`, avant `CanarySpeech.clean`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub prompt: Vec<i64>,
    pub tokens: Vec<i64>,
    pub text: String,
}

/// La sortie de l'encodeur : états, longueurs encodées, masque Float32.
pub struct Encoded {
    pub hidden: Array,
    #[cfg_attr(not(test), expect(dead_code))]
    pub lengths: Array,
    pub mask: Array,
}

pub struct Canary {
    pub config: Config,
    pub tokenizer: Tokenizer,
    pub encoder: Conformer,
    pub decoder: Decoder,
}

/// Les dispositions de poids que `sanitize` reconnaît.
#[derive(Clone, Copy, Default)]
struct Layout {
    /// `decoder.blocks.*` : déjà aux clés du graphe.
    sanitized: bool,
    /// `head.classifier.weight` ou `transf_decoder.layers.*` : MLX.
    mlx_native: bool,
}

impl Canary {
    /// `fromModelDirectory` : `config.json` et le vocabulaire d'abord (un
    /// fichier absent ou illisible échoue avant les poids), puis les poids,
    /// chaque clé prise avec sa forme ; une clé absente ou en trop est une
    /// erreur.
    pub fn load(dir: &Path) -> Result<Self> {
        if !dir.join(CONFIG_FILE).is_file() {
            bail!(crate::tr!(
                en = "{NAME}: {CONFIG_FILE} is missing from {}",
                fr = "{NAME} : {CONFIG_FILE} absent de {}",
                dir.display()
            ));
        }
        let config = Config::load(dir)?;
        // Sans vocabulaire, le Swift rendrait des ids bruts (« <12><34> ») :
        // refusé ici.
        let tokenizer = Tokenizer::from_dir(dir, &config)?.ok_or_else(|| {
            anyhow!(crate::tr!(
                en = "{NAME}: no vocabulary in {} (tokens.txt, tokenizer.model, tokenizer.json or tokenizer.model_base64 of config.json)",
                fr = "{NAME} : aucun vocabulaire dans {} (tokens.txt, tokenizer.model, tokenizer.json ou tokenizer.model_base64 de config.json)",
                dir.display()
            ))
        })?;

        let layout = Cell::new(Layout::default());
        let mut w = Weights::load_as_stored(dir, NAME, config.quantization, |key| {
            let mut seen = layout.get();
            seen.sanitized |= key.starts_with("decoder.blocks.");
            seen.mlx_native |=
                key == "head.classifier.weight" || key.starts_with("transf_decoder.layers.");
            layout.set(seen);
            remap_key(key)
        })?;
        let layout = layout.get();
        if layout.sanitized || !layout.mlx_native {
            bail!(crate::tr!(
                en = "{NAME}: {} holds no MLX weights (head.classifier.*, transf_decoder.layers.*); the NeMo and pre-renamed layouts are not supported",
                fr = "{NAME} : {} n'a pas de poids MLX (head.classifier.*, transf_decoder.layers.*) ; les dispositions NeMo et déjà renommée ne sont pas gérées",
                dir.display()
            ));
        }
        let encoder = Conformer::load(&mut w, &config.encoder)?;
        let decoder = Decoder::load(&mut w, &config.decoder, config.vocab_size)?;
        w.finish()?;
        Ok(Self {
            config,
            tokenizer,
            encoder,
            decoder,
        })
    }

    /// `preprocessAudio` d'un audio `[échantillons]` (16 kHz) : le mel de
    /// Parakeet `[1, trames, bandes]`, `asType(.float32)`.
    pub fn mel(&self, samples: &[f32]) -> Result<Array> {
        let audio = Array::from_slice(samples, &[samples.len() as i32]);
        Ok(log_mel_spectrogram(&audio, &self.config.preprocessor)?.as_dtype(Dtype::Float32)?)
    }

    /// `encode(mel:)` : le Conformer sur le mel tel quel, évalué, puis le
    /// masque des trames encodées.
    pub fn encode(&mut self, mel: &Array) -> Result<Encoded> {
        let s = mel.shape().to_vec();
        ensure!(
            s.len() == 3 && s[2] == self.config.preprocessor.features,
            crate::tr!(
                en = "{NAME}: input {s:?} instead of [B, T, {}]",
                fr = "{NAME} : entrée {s:?} au lieu de [B, T, {}]",
                self.config.preprocessor.features
            )
        );
        let lengths = Array::from_slice(&vec![s[1]; s[0] as usize], &[s[0]]);
        let (hidden, lengths) = self.encoder.forward(mel, &lengths)?;
        mlx_rs::transforms::eval([&hidden, &lengths])?;
        let frames = hidden.shape().get(1).copied().unwrap_or(0);
        let positions = Array::arange::<_, i32>(None, frames, None)?.expand_dims(0)?;
        let mask = positions
            .lt(lengths.expand_dims(1)?)?
            .as_dtype(Dtype::Float32)?;
        Ok(Encoded {
            hidden,
            lengths,
            mask,
        })
    }

    /// `generate` : mel, encodeur, puis au plus `max_tokens` jetons gloutons
    /// pas à pas (l'invite d'abord, puis un jeton à la fois), jusqu'à
    /// `<|endoftext|>`. La langue est source et cible. `stop` est consulté
    /// après l'encodeur et après chaque jeton (déjà évalué).
    pub fn generate(
        &mut self,
        samples: &[f32],
        language: &str,
        max_tokens: usize,
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<Transcript> {
        let mel = self.mel(samples)?;
        let encoded = self.encode(&mel)?;
        stop()?;
        self.decode(&encoded, language, max_tokens, stop)
    }

    /// Le décodage glouton de `generate`, sur une sortie de l'encodeur.
    pub fn decode(
        &self,
        encoded: &Encoded,
        language: &str,
        max_tokens: usize,
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<Transcript> {
        let prompt = self.tokenizer.prompt(&self.config, language, language);
        let eos = self.tokenizer.eos(&self.config);
        let vocab = i64::from(self.config.vocab_size);
        if let Some(id) = prompt.iter().find(|&&id| !(0..vocab).contains(&id)) {
            bail!(crate::tr!(
                en = "{NAME}: prompt token {id} outside the vocabulary of {vocab}",
                fr = "{NAME} : jeton d'invite {id} hors du vocabulaire de {vocab}",
            ));
        }

        let mut state = self
            .decoder
            .make_state(&encoded.hidden, Some(&encoded.mask))?;
        let mut pending: Vec<i32> = prompt.iter().map(|&id| id as i32).collect();
        let mut generated: Vec<i64> = Vec::new();
        for _ in 0..max_tokens {
            let ids = Array::from_slice(&pending, &[1, pending.len() as i32]);
            let logits = self.decoder.step(&ids, &mut state)?;
            let time = logits.shape().get(1).copied().unwrap_or(1);
            let next_logits = logits.try_index((.., time - 1.., ..))?.squeeze_axes(&[1])?;
            next_logits.eval()?;
            let next = i64::from(argmax_axis(&next_logits, -1, None)?.try_item_exact::<u32>()?);
            if next == eos {
                break;
            }
            generated.push(next);
            pending = vec![next as i32];
            stop()?;
        }
        let text = self.tokenizer.decode(&generated).trim().to_string();
        Ok(Transcript {
            prompt,
            tokens: generated,
            text,
        })
    }
}

/// `sanitizeMLXNative` : la clé du graphe, ou `None` pour une clé ignorée
/// (le Swift écarte ce qu'il ne reconnaît pas). `encoder.*` va au Conformer
/// de Parakeet sous ses propres noms (`remap_key` de Parakeet : la liste
/// `pre_encode.conv.N`, `pos_bias_u`).
pub fn remap_key(key: &str) -> Option<String> {
    let rest = |prefix: &str| key.strip_prefix(prefix);
    let mapped = if key.starts_with("encoder.") {
        return crate::stt::mlx::parakeet::model::remap_key(key);
    } else if let Some(r) = rest("transf_decoder.token_embedding.") {
        format!("decoder.embedding.{r}")
    } else if let Some(r) = rest("transf_decoder.embedding_layer_norm.") {
        format!("decoder.embedding_layer_norm.{r}")
    } else if let Some(r) = rest("transf_decoder.final_layer_norm.") {
        format!("decoder.final_norm.{r}")
    } else if let Some(r) = rest("transf_decoder.layers.") {
        let (layer, sub) = r.split_once('.')?;
        format!("decoder.blocks.{layer}.{}", decoder_sub_layer(sub))
    } else {
        format!("decoder.output_proj.{}", rest("head.classifier.")?)
    };
    Some(
        mapped
            .replace(".pos_bias_u", ".posBiasU")
            .replace(".pos_bias_v", ".posBiasV"),
    )
}

/// `remapMLXNativeDecoderSubLayer`.
fn decoder_sub_layer(sub: &str) -> String {
    let pairs = [
        ("third_sub_layer.linear1.", "ff1."),
        ("third_sub_layer.linear2.", "ff2."),
        ("layer_norm_1.", "self_attn_norm."),
        ("layer_norm_2.", "cross_attn_norm."),
        ("layer_norm_3.", "ff_norm."),
    ];
    if let Some(r) = sub.strip_prefix("first_sub_layer.") {
        return format!("self_attn.{}", attention(r));
    }
    if let Some(r) = sub.strip_prefix("second_sub_layer.") {
        return format!("cross_attn.{}", attention(r));
    }
    for (from, to) in pairs {
        if let Some(r) = sub.strip_prefix(from) {
            return format!("{to}{r}");
        }
    }
    sub.to_string()
}

/// `remapMLXNativeAttention`.
fn attention(sub: &str) -> String {
    for (from, to) in [
        ("linear_q.", "q_proj."),
        ("linear_k.", "k_proj."),
        ("linear_v.", "v_proj."),
        ("linear_out.", "out_proj."),
    ] {
        if let Some(r) = sub.strip_prefix(from) {
            return format!("{to}{r}");
        }
    }
    sub.to_string()
}

#[cfg(test)]
mod tests {
    use super::remap_key;

    #[test]
    fn keys_are_renamed_like_the_swift() {
        for (key, expected) in [
            (
                "encoder.pre_encode.conv.0.weight",
                Some("encoder.pre_encode.conv0.weight"),
            ),
            (
                "encoder.pre_encode.conv.3.bias",
                Some("encoder.pre_encode.pointwise_layers.0.bias"),
            ),
            ("encoder.pre_encode.conv.4.weight", None),
            (
                "encoder.layers.0.self_attn.pos_bias_u",
                Some("encoder.layers.0.self_attn.posBiasU"),
            ),
            (
                "transf_decoder.token_embedding.scales",
                Some("decoder.embedding.scales"),
            ),
            (
                "transf_decoder.embedding_layer_norm.bias",
                Some("decoder.embedding_layer_norm.bias"),
            ),
            (
                "transf_decoder.final_layer_norm.weight",
                Some("decoder.final_norm.weight"),
            ),
            (
                "transf_decoder.layers.3.first_sub_layer.linear_q.weight",
                Some("decoder.blocks.3.self_attn.q_proj.weight"),
            ),
            (
                "transf_decoder.layers.3.second_sub_layer.linear_out.biases",
                Some("decoder.blocks.3.cross_attn.out_proj.biases"),
            ),
            (
                "transf_decoder.layers.0.third_sub_layer.linear2.bias",
                Some("decoder.blocks.0.ff2.bias"),
            ),
            (
                "transf_decoder.layers.7.layer_norm_2.weight",
                Some("decoder.blocks.7.cross_attn_norm.weight"),
            ),
            (
                "transf_decoder.layers.7.autre.weight",
                Some("decoder.blocks.7.autre.weight"),
            ),
            ("transf_decoder.layers.7", None),
            ("head.classifier.bias", Some("decoder.output_proj.bias")),
            ("log_softmax.mlp.layer0.weight", None),
            ("inconnu", None),
        ] {
            assert_eq!(remap_key(key).as_deref(), expected, "{key}");
        }
    }
}
