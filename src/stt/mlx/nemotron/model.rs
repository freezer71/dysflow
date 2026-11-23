//! Le modèle Nemotron 3.5 ASR Streaming : chargement (clés renommées comme
//! `sanitize`, quantification lue dans `config.json`, flottants en
//! `bfloat16`), mel, invite de langue et RNN-T glouton d'une tranche. Port
//! de `NemotronASRModel.swift` et `streamRNNTDecode`
//! (`NemotronASRStreamSession.swift`), pour le chemin du flux.

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::{Array, Dtype};
use std::path::Path;

use super::config::Config;
use super::encoder::{Encoder, Prompt};
use super::tokenizer;
use crate::stt::mlx::nemo::alignment::AlignedToken;
use crate::stt::mlx::nemo::decoding::rnnt_step;
use crate::stt::mlx::nemo::rnnt::{JointNetwork, LstmState, PredictNetwork};
use crate::stt::mlx::parakeet::audio::raw_log_mel;
use crate::stt::mlx::parakeet::model::remap_pre_encode_key;
use crate::stt::mlx::weights::Weights;

/// Le type de calcul de l'app (`computeDType` du Swift).
pub const COMPUTE: Dtype = Dtype::Bfloat16;

/// L'état du RNN-T glouton d'un flux, d'une tranche à l'autre
/// (`NemotronASRStreamRNNTState`).
pub struct RnntState {
    pub results: Vec<AlignedToken>,
    last_token: usize,
    decoder: Option<LstmState>,
    /// La trame de l'encodeur (absolue) où commence la tranche suivante.
    global_time: usize,
}

impl RnntState {
    pub fn new(blank: usize) -> Self {
        Self {
            results: Vec::new(),
            last_token: blank,
            decoder: None,
            global_time: 0,
        }
    }
}

pub struct Nemotron {
    pub config: Config,
    pub encoder: Encoder,
    prompt: Option<Prompt>,
    decoder: PredictNetwork,
    joint: JointNetwork,
}

impl Nemotron {
    /// `fromDirectory` : `config.json`, puis les `*.safetensors` du dossier,
    /// chaque clé prise avec sa forme ; une clé absente ou en trop est une
    /// erreur qui la nomme.
    pub fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(dir)?;
        let w = Weights::load(dir, "Nemotron", COMPUTE, config.quantization, remap_key)?;
        Self::from_weights(config, w)
    }

    pub fn from_weights(config: Config, mut w: Weights) -> Result<Self> {
        // Le Swift déquantifie une convolution ponctuelle livrée quantifiée
        // (`sanitize`) avant de passer ses échelles au type de calcul ; les
        // poids du catalogue la livrent en clair. Refusée par son nom.
        for i in 0..config.encoder.n_layers {
            for name in ["pointwise_conv1", "pointwise_conv2"] {
                let key = format!("encoder.layers.{i}.conv.{name}.scales");
                if w.has(&key) {
                    bail!(crate::tr!(
                        en = "Nemotron weights: quantized pointwise convolution ({key}) not ported to Rust",
                        fr = "poids de Nemotron : convolution ponctuelle quantifiée ({key}) non portée en Rust",
                    ));
                }
            }
        }
        let encoder = Encoder::load(&mut w, &config.encoder)?;
        let prompt = match &config.prompt {
            Some(p) => Some(Prompt::load(&mut w, config.encoder.d_model, p)?),
            None => None,
        };
        let decoder = PredictNetwork::load(&mut w, "decoder", &config.decoder)?;
        let joint = JointNetwork::load(&mut w, "joint", &config.joint)?;
        w.finish()?;
        Ok(Self {
            config,
            encoder,
            prompt,
            decoder,
            joint,
        })
    }

    /// `NemotronASRAudio.logMelSpectrogram` de `samples` (16 kHz) :
    /// `[1, trames, bandes]`, `f32`, sans normalisation (le flux l'exige).
    pub fn mel(&self, samples: &[f32]) -> Result<Array> {
        let audio = Array::from_slice(samples, &[samples.len() as i32]);
        let mel = raw_log_mel(&audio, &self.config.preprocessor)?;
        Ok(mel.expand_dims(0)?.as_dtype(audio.dtype())?)
    }

    /// `resolvePromptIndex` : la langue, sinon la langue par défaut, sinon 0.
    pub fn prompt_index(&self, language: &str) -> i32 {
        let Some(prompt) = &self.config.prompt else {
            return 0;
        };
        let index = prompt
            .dictionary
            .get(language)
            .or_else(|| prompt.dictionary.get(&self.config.default_language))
            .copied()
            .unwrap_or(0);
        i32::try_from(index).unwrap_or(0)
    }

    /// `applyPrompt` : sans invite dans le modèle, l'encodeur tel quel.
    pub fn apply_prompt(&self, encoded: &Array, index: i32) -> Result<Array> {
        match &self.prompt {
            Some(prompt) => prompt.apply(encoded, index),
            None => Ok(encoded.clone()),
        }
    }

    /// `streamRNNTDecode` : le RNN-T glouton sur une tranche `[1, c, d]`
    /// après l'invite. Chaque pas lit son jeton (l'`argmax` évalué), ce qui
    /// calcule aussi la tranche.
    pub fn rnnt_decode(&self, prompted: &Array, state: &mut RnntState) -> Result<()> {
        let chunk = usize::try_from(prompted.shape()[1]).unwrap_or(0);
        let blank = self.config.blank();
        let frame_seconds = self.config.frame_seconds();
        let vocabulary = &self.config.vocabulary;
        let (mut time, mut new_symbols) = (0usize, 0usize);
        while time < chunk {
            let t = time as i32;
            let frame = prompted.try_index((.., t..t + 1, ..))?;
            let token = (state.last_token != blank)
                .then(|| Array::from_slice(&[state.last_token as i32], &[1, 1]));
            let (pred, proposed) = self
                .decoder
                .predict(token.as_ref(), state.decoder.as_ref())?;
            let pred = pred.as_dtype(frame.dtype())?;
            let proposed = LstmState {
                hidden: proposed.hidden.as_dtype(frame.dtype())?,
                cell: proposed.cell.as_dtype(frame.dtype())?,
            };
            let joint = self.joint.forward(&frame, &pred)?;
            let token = argmax_axis(&joint, -1, None)?.try_item_exact::<u32>()? as usize;
            let step = rnnt_step(
                token,
                blank,
                time,
                new_symbols,
                Some(self.config.max_symbols),
            );
            if step.emitted_token {
                state.last_token = token;
                state.decoder = Some(proposed);
                if !tokenizer::is_special(token, vocabulary) {
                    state.results.push(AlignedToken {
                        id: token,
                        text: tokenizer::decode(&[token], vocabulary),
                        start: (state.global_time + time) as f64 * frame_seconds,
                        duration: frame_seconds,
                    });
                }
            }
            time = step.next_time;
            new_symbols = step.next_new_symbols;
        }
        state.global_time += chunk;
        Ok(())
    }
}

/// `remapKey` : les clés NeMo vers celles du graphe (réseau joint, biais
/// des positions, noyau de l'invite, liste du sous-échantillonnage).
pub fn remap_key(key: &str) -> Option<String> {
    let key = key
        .replace("joint.joint_net.2.", "joint.joint_net.")
        .replace(".pos_bias_u", ".posBiasU")
        .replace(".pos_bias_v", ".posBiasV")
        .replace("prompt_kernel.0.", "prompt_kernel.linear0.")
        .replace("prompt_kernel.2.", "prompt_kernel.linear2.");
    remap_pre_encode_key(key)
}

#[cfg(test)]
mod tests {
    use super::remap_key;

    #[test]
    fn keys_are_renamed_like_the_swift() {
        let cases = [
            (
                "encoder.pre_encode.conv.0.weight",
                Some("encoder.pre_encode.conv0.weight"),
            ),
            (
                "encoder.pre_encode.conv.1.weight",
                Some("encoder.pre_encode.conv.1.weight"),
            ),
            (
                "encoder.pre_encode.conv.2.bias",
                Some("encoder.pre_encode.depthwise_layers.0.bias"),
            ),
            (
                "encoder.pre_encode.conv.3.weight",
                Some("encoder.pre_encode.pointwise_layers.0.weight"),
            ),
            ("encoder.pre_encode.conv.4.weight", None),
            (
                "encoder.pre_encode.conv.5.weight",
                Some("encoder.pre_encode.depthwise_layers.1.weight"),
            ),
            (
                "encoder.pre_encode.conv.6.bias",
                Some("encoder.pre_encode.pointwise_layers.1.bias"),
            ),
            (
                "encoder.layers.3.self_attn.pos_bias_u",
                Some("encoder.layers.3.self_attn.posBiasU"),
            ),
            (
                "encoder.layers.3.self_attn.pos_bias_v",
                Some("encoder.layers.3.self_attn.posBiasV"),
            ),
            ("joint.joint_net.2.weight", Some("joint.joint_net.weight")),
            (
                "prompt_kernel.0.scales",
                Some("prompt_kernel.linear0.scales"),
            ),
            ("prompt_kernel.2.bias", Some("prompt_kernel.linear2.bias")),
            // Parakeet seul retire ce `0` : Nemotron garde la clé.
            ("x.decoder_layers.0.y", Some("x.decoder_layers.0.y")),
        ];
        for (key, expected) in cases {
            assert_eq!(remap_key(key).as_deref(), expected, "{key}");
        }
    }
}
