//! Le modèle Parakeet TDT : chargement (clés renommées comme `sanitize`,
//! quantification lue dans `config.json`, flottants en `bfloat16`), mel,
//! encodeur, décodeur TDT glouton pas à pas, et découpe d'un long audio.
//! Port de `ParakeetModel.swift` pour la variante TDT, sur le chemin de
//! l'app : encodeur simple, décodeur `serial`.

use anyhow::{ensure, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{Array, Dtype};
use std::path::Path;

use super::audio::log_mel_spectrogram;
use super::config::Config;
use super::conformer::Conformer;
use super::tokenizer;
use crate::stt::mlx::nemo::alignment::{self, AlignedResult, AlignedToken};
use crate::stt::mlx::nemo::decoding::tdt_step;
use crate::stt::mlx::nemo::rnnt::{tdt_decisions, JointNetwork, LstmState, PredictNetwork};
use crate::stt::mlx::weights::Weights;
use crate::tts::Cancel;

/// Le type de calcul de l'app (`computeDType` du Swift).
pub const COMPUTE: Dtype = Dtype::Bfloat16;
/// Au-delà (secondes), l'audio est lu par morceaux de cette durée.
const CHUNK_SECONDS: f64 = 1200.0;
/// Le chevauchement de deux morceaux.
const OVERLAP_SECONDS: f64 = 2.0;

/// Un pas du décodeur TDT, pour les essais de parité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TdtTrace {
    pub time: usize,
    pub token: usize,
    pub decision: usize,
}

/// Un point d'arrêt : avec un test d'abandon, le test est consulté ; sans
/// test, rien.
pub struct Stop<'a>(pub &'a Cancel);

impl Stop<'_> {
    fn check(&self) -> Result<()> {
        if self.0.may_cancel() {
            self.0.check()?;
        }
        Ok(())
    }
}

pub struct Parakeet {
    pub config: Config,
    pub(super) encoder: Conformer,
    decoder: PredictNetwork,
    joint: JointNetwork,
}

impl Parakeet {
    /// `fromDirectory` : `config.json`, puis les `*.safetensors` du dossier,
    /// chaque clé prise avec sa forme ; une clé en trop est une erreur.
    pub fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(dir)?;
        let w = Weights::load(dir, "Parakeet", COMPUTE, config.quantization, remap_key)?;
        Self::from_weights(config, w)
    }

    /// Le graphe de `config`, chaque paramètre pris dans `w` avec sa forme ;
    /// une clé en trop est une erreur (`verify: .all` du Swift). Phonon-2 y
    /// passe aussi, ses poids lus dans son conteneur.
    pub fn from_weights(config: Config, mut w: Weights) -> Result<Self> {
        let encoder = Conformer::load(&mut w, &config.encoder)?;
        let decoder = PredictNetwork::load(&mut w, "decoder", &config.decoder)?;
        let joint = JointNetwork::load(&mut w, "joint", &config.joint)?;
        w.finish()?;
        ensure!(
            config.decoder.vocab_size as usize == config.blank(),
            crate::tr!(
                en = "Parakeet: {} prediction tokens for a vocabulary of {}",
                fr = "Parakeet : {} jetons de prédiction pour un vocabulaire de {}",
                config.decoder.vocab_size,
                config.blank(),
            )
        );
        Ok(Self {
            config,
            encoder,
            decoder,
            joint,
        })
    }

    /// Le mel de `samples` (16 kHz) : `[1, trames, bandes]`, `f32`.
    pub fn mel(&self, samples: &[f32]) -> Result<Array> {
        let audio = Array::from_slice(samples, &[samples.len() as i32]);
        log_mel_spectrogram(&audio, &self.config.preprocessor)
    }

    /// L'encodeur sur un mel `[1, trames, bandes]` mis au type de calcul :
    /// `[1, trames / 8, d]` et la longueur encodée, évalués.
    pub fn encode(&mut self, mel: &Array) -> Result<(Array, Array)> {
        let features = mel.as_dtype(COMPUTE)?;
        let s = features.shape();
        ensure!(
            s.len() == 3 && s[2] == self.config.preprocessor.features,
            "Parakeet : entrée {s:?} au lieu de [B, T, {}]",
            self.config.preprocessor.features
        );
        let lengths = Array::from_slice(&vec![s[1]; s[0] as usize], &[s[0]]);
        let (encoded, lengths) = self.encoder.forward(&features, &lengths)?;
        mlx_rs::transforms::eval([&encoded, &lengths])?;
        Ok((encoded, lengths))
    }

    /// `decode(mel:)[0]` : encodeur puis décodeur TDT, pour la première
    /// rangée. `trace` reçoit chaque pas.
    pub fn decode(
        &mut self,
        mel: &Array,
        stop: &Stop,
        trace: &mut dyn FnMut(TdtTrace),
    ) -> Result<AlignedResult> {
        let (encoded, lengths) = self.encode(mel)?;
        stop.check()?;
        let mut results = self.decode_tdt(&encoded, &lengths, stop, trace)?;
        ensure!(!results.is_empty(), "Parakeet : aucune rangée décodée");
        Ok(results.swap_remove(0))
    }

    /// `decodeTDTSerial` : pour chaque rangée, trame après trame, un pas de
    /// prédiction et du réseau joint ; l'état n'avance que sur un jeton.
    fn decode_tdt(
        &self,
        encoded: &Array,
        lengths: &Array,
        stop: &Stop,
        trace: &mut dyn FnMut(TdtTrace),
    ) -> Result<Vec<AlignedResult>> {
        let blank = self.config.blank();
        let blank_i32 = blank as i32;
        let blank_array = Array::from_slice(&[blank_i32], &[1, 1]);
        let lengths = lengths.as_dtype(Dtype::Int32)?;
        let lengths: Vec<i32> = lengths.try_as_slice::<i32>()?.to_vec();
        let (layers, hidden) = (
            self.decoder.dec_rnn.num_layers(),
            self.decoder.dec_rnn.hidden_size(),
        );
        let vocabulary = &self.config.joint.vocabulary;
        let mut results = Vec::with_capacity(lengths.len());

        for (b, &max_length) in lengths.iter().enumerate() {
            let rows = encoded.try_index(b as i32..b as i32 + 1)?;
            let zeros = Array::zeros::<f32>(&[layers, 1, hidden])?.as_dtype(COMPUTE)?;
            let mut state = LstmState {
                hidden: zeros.clone(),
                cell: zeros,
            };
            let mut current = blank_array.clone();
            let mut hypothesis = Vec::new();
            let (mut t, mut new_symbols) = (0usize, 0usize);

            while t < max_length.max(0) as usize {
                let frame = rows.try_index((.., t as i32..t as i32 + 1, ..))?;
                let (pred, next) = self.decoder.step(&current, &blank_array, &state)?;
                let dtype = frame.dtype();
                let pred = pred.as_dtype(dtype)?;
                let next = LstmState {
                    hidden: next.hidden.as_dtype(dtype)?,
                    cell: next.cell.as_dtype(dtype)?,
                };
                let decisions = tdt_decisions(&self.joint.forward(&frame, &pred)?, blank_i32)?;
                mlx_rs::transforms::eval([&decisions, &next.hidden, &next.cell])?;
                let pair = decisions.try_as_slice::<i32>()?;
                let (token, decision) = (pair[0].max(0) as usize, pair[1].max(0) as usize);
                let step = tdt_step(
                    token,
                    blank,
                    decision,
                    &self.config.durations,
                    t,
                    new_symbols,
                    self.config.max_symbols,
                );
                trace(TdtTrace {
                    time: t,
                    token,
                    decision,
                });

                if token != blank {
                    state = next;
                    current = Array::from_slice(&[token as i32], &[1, 1]);
                    if !tokenizer::is_special(token, vocabulary) {
                        hypothesis.push(AlignedToken {
                            id: token,
                            text: tokenizer::decode(&[token], vocabulary),
                            start: self.frame_seconds(t),
                            duration: self.frame_seconds(step.jump),
                        });
                    }
                }
                t = step.next_time;
                new_symbols = step.next_new_symbols;
                stop.check()?;
            }
            results.push(alignment::sentences_to_result(
                alignment::tokens_to_sentences(&hypothesis),
            ));
        }
        Ok(results)
    }

    /// La date d'une trame encodée, en secondes.
    fn frame_seconds(&self, frames: usize) -> f64 {
        let factor = self.config.encoder.subsampling_factor as usize;
        let hop = self.config.preprocessor.hop_length();
        (frames * factor * hop) as f64 / f64::from(self.config.preprocessor.sample_rate)
    }

    /// `generate(samples:)` : l'audio entier, ou par morceaux de 20 min
    /// recollés au-delà ; le texte, sans espaces autour.
    pub fn generate(&mut self, samples: &[f32], stop: &Stop) -> Result<String> {
        let rate = f64::from(self.config.preprocessor.sample_rate);
        let duration = samples.len() as f64 / rate;
        let result = if duration <= CHUNK_SECONDS {
            let mel = self.mel(samples)?;
            self.decode(&mel, stop, &mut |_| {})?
        } else {
            let chunk = ((CHUNK_SECONDS * rate) as usize).max(1);
            let overlap = ((OVERLAP_SECONDS * rate) as usize).min(chunk - 1);
            let step = (chunk - overlap).max(1);
            let mut all: Vec<AlignedToken> = Vec::new();
            let mut start = 0;
            while start < samples.len() {
                let end = (start + chunk).min(samples.len());
                let mel = self.mel(&samples[start..end])?;
                let part = self.decode(&mel, stop, &mut |_| {})?;
                let offset = start as f64 / rate;
                let incoming: Vec<AlignedToken> = part
                    .tokens()
                    .map(|t| AlignedToken {
                        start: t.start + offset,
                        ..t.clone()
                    })
                    .collect();
                all = merge(all, incoming);
                start += step;
            }
            alignment::sentences_to_result(alignment::tokens_to_sentences(&all))
        };
        Ok(result.text.trim().to_string())
    }
}

/// `mergeTokenSequences` : la plus longue suite contiguë, sinon la plus
/// longue sous-suite commune.
fn merge(existing: Vec<AlignedToken>, incoming: Vec<AlignedToken>) -> Vec<AlignedToken> {
    if existing.is_empty() {
        return incoming;
    }
    if incoming.is_empty() {
        return existing;
    }
    alignment::merge_longest_contiguous(&existing, &incoming, OVERLAP_SECONDS).unwrap_or_else(
        |_| alignment::merge_longest_common_subsequence(&existing, &incoming, OVERLAP_SECONDS),
    )
}

/// `remapKey` du Swift (variante TDT) : la clé du graphe, ou `None` pour
/// une clé sans paramètre du sous-échantillonnage (les ReLU de la liste).
pub fn remap_key(key: &str) -> Option<String> {
    let key = key
        .replace(".decoder_layers.0.", ".decoder_layers.")
        .replace("joint.joint_net.2.", "joint.joint_net.")
        .replace(".pos_bias_u", ".posBiasU")
        .replace(".pos_bias_v", ".posBiasV");
    remap_pre_encode_key(key)
}

/// La liste `encoder.pre_encode.conv.N` de NeMo en `conv0`, puis
/// `depthwise_layers.i` et `pointwise_layers.i` (les ReLU de la liste,
/// sans poids, sont écartées) ; `conv.1` et toute autre clé restent telles
/// quelles. Commun à Parakeet et Nemotron (`remapPreEncodeConvListKey`).
pub(in crate::stt::mlx) fn remap_pre_encode_key(key: String) -> Option<String> {
    let pieces: Vec<&str> = key.split('.').collect();
    if pieces.len() >= 5 && pieces[..3] == ["encoder", "pre_encode", "conv"] {
        if let Ok(index) = pieces[3].parse::<usize>() {
            let suffix = pieces[4..].join(".");
            if index == 0 {
                return Some(format!("encoder.pre_encode.conv0.{suffix}"));
            }
            if index < 2 {
                return Some(key);
            }
            let (block, slot) = ((index - 2) / 3, (index - 2) % 3);
            return match slot {
                0 => Some(format!(
                    "encoder.pre_encode.depthwise_layers.{block}.{suffix}"
                )),
                1 => Some(format!(
                    "encoder.pre_encode.pointwise_layers.{block}.{suffix}"
                )),
                _ => None,
            };
        }
    }
    Some(key)
}

/// La forme, la moyenne et la moyenne des valeurs absolues de `x`, en
/// `f32` : les statistiques des fixtures.
#[cfg(test)]
pub fn stats(x: &Array) -> Result<(Vec<i32>, f32, f32)> {
    let f = x.as_dtype(Dtype::Float32)?;
    let mean = f.mean(None)?;
    let mean_abs = mlx_rs::ops::abs(&f)?.mean(None)?;
    mlx_rs::transforms::eval([&mean, &mean_abs])?;
    Ok((
        x.shape().to_vec(),
        mean.try_item_exact::<f32>()?,
        mean_abs.try_item_exact::<f32>()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::remap_key;

    #[test]
    fn cles_renommees_comme_le_swift() {
        let cases = [
            (
                "encoder.pre_encode.conv.0.weight",
                Some("encoder.pre_encode.conv0.weight"),
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
                "encoder.pre_encode.conv.1.weight",
                Some("encoder.pre_encode.conv.1.weight"),
            ),
            (
                "encoder.layers.3.self_attn.pos_bias_u",
                Some("encoder.layers.3.self_attn.posBiasU"),
            ),
            ("joint.joint_net.2.weight", Some("joint.joint_net.weight")),
            (
                "decoder.prediction.embed.weight",
                Some("decoder.prediction.embed.weight"),
            ),
        ];
        for (key, expected) in cases {
            assert_eq!(remap_key(key).as_deref(), expected, "{key}");
        }
    }
}
