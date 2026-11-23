//! Le flux de Nemotron : encodeur à caches tranche par tranche
//! (`streamEncodeChunks`, `NemotronASRStreaming.swift`) et session
//! incrémentale (`NemotronASRStreamSession`). L'audio arrive par morceaux ;
//! seules les tranches entières de trames de mel *gelées* (dont la fenêtre
//! de STFT est couverte par l'audio reçu) sont lues, la dernière tranche
//! partielle attend `finish`. Le texte ne dépend donc pas du découpage des
//! morceaux.

use anyhow::Result;
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use super::encoder::LayerCache;
use super::model::{Nemotron, RnntState, COMPUTE};
use crate::stt::mlx::nemo::alignment::{sentences_to_result, tokens_to_sentences};

/// Les trames de mel gardées devant une tranche pour le sous-échantillonnage
/// (au moins son champ récepteur causal : 8 × par `dw_striding`).
const MEL_CACHE: i32 = 16;

/// L'état de l'encodeur d'un flux (`NemotronASRStreamEncoderState`).
struct EncoderState {
    layers: Vec<LayerCache>,
    mel_cache: Option<Array>,
    /// Les trames sous-échantillonnées déjà données au décodeur (absolu).
    emitted: i32,
    /// Les trames de mel déjà lues par l'encodeur (absolu).
    consumed: i32,
}

/// Le texte et les jetons rendus par un `step` ou un `finish`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    pub text: String,
    pub tokens: Vec<usize>,
}

/// Une session de flux (`NemotronASRStreamSession`) : vit sur le fil MLX,
/// avec le modèle.
pub struct Session {
    prompt: i32,
    /// Les trames de l'encodeur par tranche.
    chunk_frames: i32,
    raw: Vec<f32>,
    encoder: EncoderState,
    rnnt: RnntState,
    emitted_text: String,
    done: bool,
}

impl Session {
    /// `makeStreamSession(language:chunkMs:)` : la tranche en millisecondes
    /// arrondie au nombre de trames de l'encodeur (80 ms), au moins une ;
    /// sans elle, la tranche native (`droite + 1`).
    pub fn new(model: &Nemotron, language: &str, chunk_ms: Option<u32>) -> Self {
        let config = &model.config;
        let chunk_frames = match chunk_ms {
            Some(ms) => {
                let frames = (f64::from(ms) / config.ms_per_frame() as f64).round();
                (frames as i32).max(1)
            }
            None => (config.default_att_context_size[1] + 1).max(1),
        };
        Self {
            prompt: model.prompt_index(language),
            chunk_frames,
            raw: Vec::new(),
            encoder: EncoderState {
                layers: vec![LayerCache::default(); model.encoder.num_layers()],
                mel_cache: None,
                emitted: 0,
                consumed: 0,
            },
            rnnt: RnntState::new(config.blank()),
            emitted_text: String::new(),
            done: false,
        }
    }

    /// Le texte lu jusqu'ici.
    pub fn text(&self) -> &str {
        &self.emitted_text
    }

    /// Les jetons gardés jusqu'ici.
    #[cfg(test)]
    pub fn tokens(&self) -> Vec<usize> {
        self.rnnt.results.iter().map(|t| t.id).collect()
    }

    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.done
    }

    /// `step` : ajoute des échantillons ; le texte et les jetons nouveaux.
    /// `trace` reçoit chaque tranche de l'encodeur après l'invite.
    pub fn step(
        &mut self,
        model: &mut Nemotron,
        samples: &[f32],
        trace: &mut dyn FnMut(&Array) -> Result<()>,
    ) -> Result<Delta> {
        // Comme le Swift : rien n'est ajouté après `finish`.
        if self.done {
            return Ok(Delta::default());
        }
        self.raw.extend_from_slice(samples);
        self.advance(model, false, trace)
    }

    /// `finish` : lit la dernière tranche, partielle, comme la lecture d'un
    /// coup ; après lui, plus rien n'est lu.
    pub fn finish(
        &mut self,
        model: &mut Nemotron,
        trace: &mut dyn FnMut(&Array) -> Result<()>,
    ) -> Result<Delta> {
        self.advance(model, true, trace)
    }

    fn advance(
        &mut self,
        model: &mut Nemotron,
        last: bool,
        trace: &mut dyn FnMut(&Array) -> Result<()>,
    ) -> Result<Delta> {
        if self.done {
            return Ok(Delta::default());
        }
        if self.raw.is_empty() {
            if last {
                self.done = true;
            }
            return Ok(Delta::default());
        }
        let (hop, half) = frame_geometry(model);
        let chunk_mel = self.chunk_frames * model.config.encoder.subsampling_factor;
        // Pas de tranche entière gelée de plus : le mel n'est pas recalculé.
        let frozen = frozen_frames(self.raw.len(), hop, half);
        if !last && frozen - i64::from(self.encoder.consumed) < i64::from(chunk_mel) {
            return Ok(Delta::default());
        }

        let mel = model.mel(&self.raw)?;
        let total = mel.shape()[1];
        let limit = if last {
            total
        } else {
            i32::try_from(frozen.min(i64::from(total))).unwrap_or(total)
        };
        let first_new = self.rnnt.results.len();
        self.encode_chunks(model, &mel, limit, last, trace)?;

        // Le graphe borné d'un pas à l'autre : les caches dont repart le
        // suivant sont calculés (le `item` de chaque pas du RNN-T a déjà
        // calculé la tranche).
        let mut live: Vec<&Array> = self.encoder.mel_cache.iter().collect();
        for cache in &self.encoder.layers {
            live.extend(cache.attention.iter());
            live.extend(cache.conv.iter());
        }
        if !live.is_empty() {
            mlx_rs::transforms::async_eval(live)?;
        }

        let full = sentences_to_result(tokens_to_sentences(&self.rnnt.results)).text;
        let text = match full.strip_prefix(self.emitted_text.as_str()) {
            Some(rest) => rest.to_string(),
            None => full.clone(),
        };
        self.emitted_text = full;
        let tokens = self.rnnt.results[first_new..]
            .iter()
            .map(|t| t.id)
            .collect();
        if last {
            self.done = true;
        }
        mlx_rs::memory::clear_cache()?;
        Ok(Delta { text, tokens })
    }

    /// `streamEncodeChunks` : les trames de mel `[consumed, limit)` par
    /// tranches de `chunk_frames × facteur`, chacune avec le cache de mel
    /// devant, sous-échantillonnée, passée aux blocs à caches, conditionnée
    /// par l'invite, puis au RNN-T. En cours de flux (`flush` faux), une
    /// tranche partielle attend ; à la fin, elle est lue et toutes ses
    /// trames sont rendues.
    fn encode_chunks(
        &mut self,
        model: &mut Nemotron,
        mel: &Array,
        limit: i32,
        flush: bool,
        trace: &mut dyn FnMut(&Array) -> Result<()>,
    ) -> Result<()> {
        let features = mel.as_dtype(COMPUTE)?;
        let sf = model.config.encoder.subsampling_factor;
        let chunk_mel = self.chunk_frames * sf;
        let left_cache = model.config.default_att_context_size[0];
        let conv_left = model.config.encoder.conv_kernel_size - 1;
        let state = &mut self.encoder;

        while state.consumed < limit {
            let end = (state.consumed + chunk_mel).min(limit);
            if !flush && end - state.consumed < chunk_mel {
                break;
            }
            let m = features.try_index((.., state.consumed..end, ..))?;
            let (window, cache_len) = match &state.mel_cache {
                Some(cache) => (ops::concatenate(&[cache, &m], 1)?, cache.shape()[1]),
                None => (m, 0),
            };
            let window_len = window.shape()[1];
            let sub = model.encoder.pre_encode(&window)?;

            let is_final = flush && end >= limit;
            let base = (state.consumed - cache_len) / sf;
            let lo = state.emitted - base;
            let hi = if is_final {
                sub.shape()[1]
            } else {
                end / sf - base
            };
            state.consumed = end;
            state.mel_cache =
                Some(window.try_index((.., (window_len - MEL_CACHE).max(0)..window_len, ..))?);

            if hi <= lo {
                state.emitted = base + lo.max(hi);
                continue;
            }
            state.emitted = base + hi;
            let h = sub.try_index((.., lo..hi, ..))?;
            let h = model
                .encoder
                .stream_layers(&h, &mut state.layers, left_cache, conv_left)?;
            let prompted = model.apply_prompt(&h, self.prompt)?;
            trace(&prompted)?;
            model.rnnt_decode(&prompted, &mut self.rnnt)?;
        }
        Ok(())
    }
}

/// Le pas de la STFT et sa demi-fenêtre (`n_fft / 2`).
fn frame_geometry(model: &Nemotron) -> (usize, usize) {
    let p = &model.config.preprocessor;
    (p.hop_length(), p.n_fft / 2)
}

/// Les trames de mel gelées pour `samples` échantillons : la STFT centrée
/// (`n_fft / 2` de zéros de chaque côté) couvre `[m·hop − n_fft/2, m·hop +
/// n_fft/2)` pour la trame `m`, gelée dès que `m·hop + n_fft/2 ≤ samples`.
/// Sans le mel : le STFT en rend `1 + samples / hop`, jamais moins (le
/// `frozenMelFrames(sampleCount:)` de Dysflow).
pub fn frozen_frames(samples: usize, hop: usize, half: usize) -> i64 {
    if samples < half || hop == 0 {
        return 0;
    }
    ((samples - half) / hop + 1) as i64
}

#[cfg(test)]
mod tests {
    use super::frozen_frames;

    /// 160 échantillons par trame, demi-fenêtre de 256 : la trame m est
    /// gelée dès `160 m + 256` échantillons.
    #[test]
    fn a_mel_frame_is_frozen_once_its_window_is_covered() {
        assert_eq!(frozen_frames(0, 160, 256), 0);
        assert_eq!(frozen_frames(255, 160, 256), 0);
        assert_eq!(frozen_frames(256, 160, 256), 1);
        assert_eq!(frozen_frames(415, 160, 256), 1);
        assert_eq!(frozen_frames(416, 160, 256), 2);
        // Une tranche de 560 ms (56 trames) : 9 056 échantillons.
        assert_eq!(frozen_frames(256 + 55 * 160, 160, 256), 56);
    }
}
