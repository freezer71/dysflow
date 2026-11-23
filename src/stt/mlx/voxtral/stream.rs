//! La session de flux de Voxtral (`VoxtralRealtimeStreamSession.swift`) :
//! l'audio arrive par morceaux ; le mel et les convolutions avancent avec
//! leurs retenues (chaque ligne produite est définitive), l'encodeur ne lit
//! que les lignes nouvelles, figées à un jeton entier près, et le décodeur
//! rend chaque jeton au retard du modèle. À `finish`, la marge à droite de
//! la lecture hors ligne est ajoutée : le texte final est celui de
//! `generate`.

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use super::audio::MelStream;
use super::config::SAMPLES_PER_TOKEN;
use super::decoder::Caches;
use super::encoder::{ConvState, StreamState};
use super::model::{num_audio_tokens, num_delay_tokens, Voxtral};
use super::text::Transcript;

/// Seul le dernier jeton partiel (un morceau fini au milieu de ses 1 280
/// échantillons) n'est pas figé.
const FROZEN_GUARD_TOKENS: i32 = 1;
/// Le cache de MLX est vidé tous les 256 jetons, comme `generate`.
const CLEAR_EVERY: usize = 256;

/// Le texte et les jetons rendus par un `step` ou un `finish`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    pub text: String,
    pub tokens: Vec<usize>,
}

/// Ce qu'une session laisse voir pendant un pas (la référence du Swift ;
/// rien ne le lit hors des essais).
#[cfg_attr(not(test), allow(dead_code))]
pub enum Trace<'a> {
    /// Les colonnes de mel nouvelles, `[bandes, n]`.
    Mel(&'a Array),
    /// Les lignes nouvelles de l'adaptateur, `[n, dim]`.
    Adapter(&'a Array),
}

/// Une session de flux : vit sur le fil MLX, avec le modèle.
pub struct Session {
    delay_ms: i32,
    max_tokens: usize,

    pending: Vec<f32>,
    real_samples: usize,
    mel: Option<MelStream>,
    conv: ConvState,
    conv_rows: Option<Array>,
    conv_rows_dropped: i32,
    adapter_rows_dropped: i32,
    delay_tokens: i32,
    flushed: bool,

    encoder: StreamState,
    adapter: Option<Array>,
    caches: Caches,
    last_logits: Option<Array>,
    position: i32,
    prompt_length: i32,
    prefilled: bool,
    done: bool,

    generated: Vec<usize>,
    transcript: Transcript,
}

impl Session {
    /// `makeStreamSession(temperature: 0, maxTokens:, transcriptionDelayMs:)`.
    pub fn new(model: &Voxtral, delay_ms: i32, max_tokens: usize) -> Self {
        Self {
            delay_ms,
            max_tokens,
            pending: Vec::new(),
            real_samples: 0,
            mel: None,
            conv: ConvState::default(),
            conv_rows: None,
            conv_rows_dropped: 0,
            adapter_rows_dropped: 0,
            delay_tokens: 0,
            flushed: false,
            encoder: StreamState {
                caches: (0..model.encoder.num_layers()).map(|_| None).collect(),
                block_base: 0,
                consumed: 0,
            },
            adapter: None,
            caches: Vec::new(),
            last_logits: None,
            position: 0,
            prompt_length: 0,
            prefilled: false,
            done: false,
            generated: Vec::new(),
            transcript: Transcript::default(),
        }
    }

    /// Tout le texte décodé jusqu'ici.
    pub fn text(&self) -> String {
        self.transcript.text()
    }

    /// Les jetons décodés (sans la fin de texte).
    #[cfg(test)]
    pub fn tokens(&self) -> &[usize] {
        &self.generated
    }

    /// Le flux a rendu la fin de texte ou atteint `max_tokens`.
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.done
    }

    /// `step` : des échantillons à 16 kHz ; le texte décodé par cet appel.
    /// Rien n'est lu après `finish`, ni après la fin du flux.
    pub fn step(
        &mut self,
        model: &mut Voxtral,
        samples: &[f32],
        trace: &mut dyn FnMut(Trace) -> Result<()>,
    ) -> Result<Delta> {
        if self.done {
            return Ok(Delta::default());
        }
        self.pending.extend_from_slice(samples);
        self.advance(model, false, trace)
    }

    /// `finish` : la marge de la lecture hors ligne, puis le reste ; à
    /// appeler une fois après le dernier `step`.
    pub fn finish(
        &mut self,
        model: &mut Voxtral,
        trace: &mut dyn FnMut(Trace) -> Result<()>,
    ) -> Result<Delta> {
        self.advance(model, true, trace)
    }

    fn advance(
        &mut self,
        model: &mut Voxtral,
        last: bool,
        trace: &mut dyn FnMut(Trace) -> Result<()>,
    ) -> Result<Delta> {
        if self.done {
            return Ok(Delta::default());
        }
        if self.flushed {
            // La marge finale a scellé le flux.
            self.pending.clear();
            if !last {
                return Ok(Delta::default());
            }
        }
        // Sans audio, rien n'avance, sauf à `finish` : hors ligne, le flux
        // vide et sa marge sont lus aussi.
        if !last && self.real_samples + self.pending.len() == 0 {
            return Ok(Delta::default());
        }
        let delta = self.advance_inner(model, last, trace)?;
        // Hors `generate`, le cache n'est vidé que tous les 256 jetons et à
        // `finish`, après les tampons du dernier pas.
        if last {
            mlx_rs::memory::clear_cache()?;
        }
        Ok(delta)
    }

    fn advance_inner(
        &mut self,
        model: &mut Voxtral,
        last: bool,
        trace: &mut dyn FnMut(Trace) -> Result<()>,
    ) -> Result<Delta> {
        let config = model.config.clone();
        let ds = config.encoder.downsample_factor;
        let per_token = SAMPLES_PER_TOKEN as usize;
        if self.mel.is_none() {
            model.ensure_ada_scales(self.delay_ms)?;
            self.delay_tokens = num_delay_tokens(self.delay_ms);
            self.prompt_length = 1 + config.n_left_pad_tokens + self.delay_tokens;
            self.mel = Some(MelStream::new(
                config.n_left_pad_tokens as usize * per_token,
                model.mel_filters(),
                &config.audio,
            ));
        }
        let Some(mel) = self.mel.as_mut() else {
            bail!("Voxtral: mel stream");
        };

        // L'audio nouveau ; à `finish`, la marge à droite exacte de la
        // lecture hors ligne (alignement sur un jeton, puis `(retard + 1) +
        // 10` jetons de zéros) et les zéros de la marge en miroir.
        let mut new = std::mem::take(&mut self.pending);
        self.real_samples += new.len();
        if last && !self.flushed {
            let align = (per_token - self.real_samples % per_token) % per_token;
            let right = (self.delay_tokens as usize + 1 + 10) * per_token;
            new.resize(new.len() + align + right + mel.finish_tail_pad(), 0.0);
            self.flushed = true;
        }

        if let Some(columns) = mel.append(&new)? {
            trace(Trace::Mel(&columns))?;
            if let Some(rows) = model.encoder.conv_stem_step(&columns, &mut self.conv)? {
                self.conv_rows = Some(match self.conv_rows.take() {
                    Some(old) => ops::concatenate(&[&old, &rows], 0)?,
                    None => rows,
                });
            }
        }
        let conv_count =
            self.conv_rows_dropped + self.conv_rows.as_ref().map_or(0, |r| r.shape()[0]);

        // Le plafond : les jetons entiers couverts par l'audio reçu, moins le
        // dernier, partiel ; après la marge finale, tout ce qui est produit.
        let real_region = config.n_left_pad_tokens + num_audio_tokens(self.real_samples);
        let emit_limit = if last {
            conv_count / ds
        } else {
            (real_region - FROZEN_GUARD_TOKENS).max(0)
        };
        let freeze = (conv_count / ds).min(emit_limit) * ds;

        if let (true, Some(rows)) = (freeze > self.encoder.consumed, self.conv_rows.as_ref()) {
            let encoded = model.encoder.feed_incremental(
                rows,
                self.conv_rows_dropped,
                freeze,
                &mut self.encoder,
            )?;
            if let Some(encoded) = encoded {
                let rows = model.encoder.downsample_and_project(&encoded)?;
                trace(Trace::Adapter(&rows))?;
                self.adapter = Some(match self.adapter.take() {
                    Some(old) => ops::concatenate(&[&old, &rows], 0)?,
                    None => rows,
                });
            }
        }
        self.trim_retained_tails()?;
        self.freeze_encoder_state()?;

        let Some(adapter) = self.adapter.clone() else {
            return Ok(Delta::default());
        };
        self.prefill_if_needed(model, &adapter)?;
        let limit = emit_limit.min(self.adapter_rows_dropped + adapter.shape()[0]);
        self.decode(model, &adapter, limit)
    }

    /// Retire ce qu'aucun pas ne relira : les lignes de convolution déjà
    /// passées à l'encodeur, et celles de l'adaptateur sous la position du
    /// décodeur (0 avant le préremplissage : l'invite reste). Une tranche
    /// garde son tampon parent : `contiguous` copie ce qui reste.
    fn trim_retained_tails(&mut self) -> Result<()> {
        if let (Some(rows), true) = (
            self.conv_rows.as_ref(),
            self.encoder.consumed > self.conv_rows_dropped,
        ) {
            let drop = self.encoder.consumed - self.conv_rows_dropped;
            self.conv_rows = if drop < rows.shape()[0] {
                Some(rows.try_index(drop..)?.contiguous()?)
            } else {
                None
            };
            self.conv_rows_dropped = self.encoder.consumed;
        }
        if let (Some(adapter), true) = (
            self.adapter.as_ref(),
            self.position > self.adapter_rows_dropped,
        ) {
            let drop = self.position - self.adapter_rows_dropped;
            self.adapter = if drop < adapter.shape()[0] {
                Some(adapter.try_index(drop..)?.contiguous()?)
            } else {
                None
            };
            self.adapter_rows_dropped = self.position;
        }
        Ok(())
    }

    /// Calcule ce qui est retenu d'un pas à l'autre (lignes, retenues des
    /// convolutions, adaptateur, caches de l'encodeur) : le graphe reste
    /// borné.
    fn freeze_encoder_state(&self) -> Result<()> {
        let mut arrays: Vec<&Array> = Vec::new();
        arrays.extend(self.conv_rows.iter());
        arrays.extend(self.conv.conv1.iter());
        arrays.extend(self.conv.conv2.iter());
        arrays.extend(self.adapter.iter());
        for cache in self.encoder.caches.iter().flatten() {
            arrays.push(&cache.keys);
            arrays.push(&cache.values);
        }
        if !arrays.is_empty() {
            mlx_rs::transforms::eval(arrays)?;
        }
        Ok(())
    }

    fn prefill_if_needed(&mut self, model: &Voxtral, adapter: &Array) -> Result<()> {
        if self.prefilled || adapter.shape()[0] < self.prompt_length {
            return Ok(());
        }
        if self.adapter_rows_dropped != 0 {
            bail!(crate::tr!(
                "Voxtral: adapter rows dropped before the prompt",
                "Voxtral : lignes de l'adaptateur retirées avant l'invite"
            ));
        }
        let config = &model.config;
        let pads = (self.prompt_length - 1) as usize;
        let mut ids = vec![config.bos_token_id];
        ids.extend(std::iter::repeat_n(config.streaming_pad_token_id, pads));
        let text = model.decoder.embed_tokens(&ids)?;
        let prefix = adapter.try_index(0..self.prompt_length)?.add(&text)?;
        let mut caches = Caches::new();
        let h = model.decoder.forward(&prefix, 0, &mut caches)?;
        let logits = model.decoder.logits(&h.try_index(h.shape()[0] - 1)?)?;
        mlx_rs::transforms::eval([&logits])?;
        self.last_logits = Some(logits);
        self.caches = caches;
        self.position = self.prompt_length;
        self.prefilled = true;
        Ok(())
    }

    /// Le décodage jusqu'à `limit`, jeton par jeton, comme `generate` (ajout,
    /// test de fin, fin de texte retirée).
    fn decode(&mut self, model: &Voxtral, adapter: &Array, limit: i32) -> Result<Delta> {
        if !self.prefilled {
            return Ok(Delta::default());
        }
        let start = self.transcript.mark();
        let eos = model.config.eos_token_id as usize;
        let mut tokens = Vec::new();
        while self.position < limit {
            let Some(logits) = self.last_logits.as_ref() else {
                break;
            };
            let token = Voxtral::sample(logits)?;
            self.generated.push(token);
            if token != eos {
                self.transcript.append(model.token_bytes(token));
            }
            if token == eos || self.generated.len() > self.max_tokens {
                self.done = true;
                if self.generated.last() == Some(&eos) {
                    self.generated.pop();
                }
                break;
            }
            tokens.push(token);

            let embed = model.decoder.embed_token(token)?;
            let row = self.position - self.adapter_rows_dropped;
            let input = if row < adapter.shape()[0] {
                adapter.try_index(row)?.add(&embed)?
            } else {
                embed
            };
            let h =
                model
                    .decoder
                    .forward(&input.expand_dims(0)?, self.position, &mut self.caches)?;
            let logits = model.decoder.logits(&h.try_index(0)?)?;
            mlx_rs::transforms::eval([&logits])?;
            self.last_logits = Some(logits);
            self.position += 1;
            if self.generated.len().is_multiple_of(CLEAR_EVERY) {
                mlx_rs::memory::clear_cache()?;
            }
        }
        Ok(Delta {
            text: self.transcript.delta(&start),
            tokens,
        })
    }
}
