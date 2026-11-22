//! Kokoro en MLX, en Rust (`mlx-rs`) : la traduction du port Swift
//! (`macos/Vendor/KokoroMLX`, repris de mlx-audio-swift), à parité avec lui.
//! Le modèle vit sur le fil MLX (`compute::mlx::Held`) et n'en sort que
//! l'audio et les durées, copiés.
//!
//! L'abandon est consulté avant le passage, après les durées, puis dans le
//! décodeur (après `encode`, chaque bloc de `decode`, chaque étage du
//! générateur), comme en Swift ; une synthèse qui ne peut pas être
//! abandonnée (`Cancel::never`) n'a ni ces points ni leurs évaluations.

mod albert;
mod config;
mod decoder;
mod modules;

use anyhow::{anyhow, ensure, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::random::RandomState;
use mlx_rs::{ops, Array};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use self::albert::Albert;
use self::config::{Config, Weights};
use self::decoder::Decoder;
use self::modules::{scalar, Linear, Predictor, TextEncoder};
use super::{KokoroBackend, KokoroRaw};
use crate::compute::mlx::{budget, Held};
use crate::sync::LockExt;
use crate::tts::Cancel;

/// Le besoin de Kokoro dans les plafonds de MLX, celui du Swift
/// (`KokoroMLXCompute.memoryLimit`, `cacheLimit`).
const BUDGET_KEY: &str = "kokoro";
const NEED: budget::Need = budget::Need { memory: 1 << 30, cache: 64 << 20 };
/// Au plus 100 trames par phonème, comme le Swift.
const MAX_FRAMES_PER_PHONEME: i32 = 100;

/// La phrase de préparation, en phonèmes : courte mais réaliste (36
/// symboles, espaces, une virgule), pour que les noyaux préparés soient
/// ceux d'une lecture. Celle du Swift (`KokoroMLXCompute.warmUpPhonemes`).
const WARM_UP_PHONEMES: &str = "la vwa se prepar, avek yn fraz kurt.";

/// Un passage complet a eu lieu dans ce processus : les noyaux Metal sont
/// compilés, la préparation n'a plus rien à faire.
static KERNELS_COMPILED: AtomicBool = AtomicBool::new(false);

/// Les identifiants de la phrase de préparation, entourés de 0 : lettres
/// ASCII (a → 43 … z → 68), espace 16, virgule 3, point 4.
pub fn warm_up_ids() -> Vec<i64> {
    let ids = WARM_UP_PHONEMES.chars().map(|c| match c {
        ' ' => 16,
        ',' => 3,
        '.' => 4,
        c => 43 + (c as i64 - 'a' as i64),
    });
    std::iter::once(0).chain(ids).chain(std::iter::once(0)).collect()
}

/// Le calcul Kokoro sur le GPU du Mac.
pub struct MlxBackend {
    /// Partagé avec le passage en cours : une libération n'attend pas sa fin.
    model: Mutex<Option<Arc<Held<Kokoro>>>>,
    passes: Arc<AtomicUsize>,
}

impl Default for MlxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MlxBackend {
    pub fn new() -> Self {
        Self { model: Mutex::new(None), passes: Arc::new(AtomicUsize::new(0)) }
    }

    /// Passages avant commencés (préparation comprise), abandonnés en route
    /// compris ; un abandon avant le passage n'y compte pas.
    pub fn passes(&self) -> usize {
        self.passes.load(Ordering::SeqCst)
    }

    fn held(&self) -> Option<Arc<Held<Kokoro>>> {
        self.model.locked().clone()
    }

    /// Un passage sous le verrou du GPU : l'abandon d'abord, puis le calcul ;
    /// complet, il marque les noyaux compilés. `warm_up` : rien à faire (et
    /// `None`) si un passage complet a eu lieu, lu sous ce même verrou,
    /// donc après tout passage rangé avant dans la file.
    fn run(
        &self,
        held: &Held<Kokoro>,
        ids: &[i64],
        style: &[f32],
        cancel: &Cancel,
        warm_up: bool,
    ) -> Result<Option<KokoroRaw>> {
        let (ids, style, cancel) = (ids.to_vec(), style.to_vec(), cancel.clone());
        let passes = Arc::clone(&self.passes);
        held.with(move |kokoro| {
            cancel.check()?;
            if warm_up && KERNELS_COMPILED.load(Ordering::SeqCst) {
                return Ok(None);
            }
            passes.fetch_add(1, Ordering::SeqCst);
            let raw = kokoro.forward(&ids, &style, &cancel)?;
            KERNELS_COMPILED.store(true, Ordering::SeqCst);
            Ok(Some(raw))
        })
    }

    /// Un passage complet aux tirages de graine `seed`, jamais abandonné :
    /// celui des essais de charge (`mlx_rust_kokoro_check`, porte du lot 3).
    /// `inside` est appelé sur le fil MLX, sous le verrou du GPU, avant le
    /// calcul ; ce qu'il rend est lâché après, verrou encore tenu. Sans
    /// modèle chargé : une erreur, `inside` jamais appelé.
    pub fn infer_seeded<G: 'static>(
        &self,
        ids: &[i64],
        style: &[f32],
        seed: u64,
        inside: impl FnOnce() -> G + Send + 'static,
    ) -> Result<KokoroRaw> {
        let held = self.held().ok_or_else(not_loaded)?;
        let (ids, style) = (ids.to_vec(), style.to_vec());
        let passes = Arc::clone(&self.passes);
        held.with(move |kokoro| {
            let _inside = inside();
            kokoro.rng = RandomState::with_seed(seed)?;
            passes.fetch_add(1, Ordering::SeqCst);
            let raw = kokoro.forward(&ids, &style, &Cancel::never())?;
            KERNELS_COMPILED.store(true, Ordering::SeqCst);
            Ok(raw)
        })
    }
}

fn not_loaded() -> anyhow::Error {
    anyhow!(crate::tr!("Kokoro model not loaded", "modèle Kokoro non chargé"))
}

impl KokoroBackend for MlxBackend {
    fn label(&self) -> &'static str {
        "MLX"
    }

    fn load(&self, model_dir: &Path) -> Result<()> {
        let mut model = self.model.locked();
        // L'ancien modèle part avant : jamais deux en mémoire.
        if model.take().is_some() {
            budget::remove(BUDGET_KEY);
        }
        budget::set(BUDGET_KEY, NEED);
        let dir = model_dir.to_path_buf();
        match Held::load(move || Kokoro::load(&dir)) {
            Ok(held) => {
                *model = Some(Arc::new(held));
                Ok(())
            }
            Err(err) => {
                budget::remove(BUDGET_KEY);
                Err(err)
            }
        }
    }

    /// Une courte synthèse réaliste, style nul, jamais abandonnée. Rien à
    /// faire si un passage complet a déjà eu lieu dans ce processus.
    fn warm_up(&self) -> Result<()> {
        let held = self.held().ok_or_else(not_loaded)?;
        // Sans attendre le verrou quand c'est déjà fait ; sinon, relu sous
        // lui (une synthèse peut finir avant le tour de la préparation).
        if KERNELS_COMPILED.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.run(&held, &warm_up_ids(), &[0.0; 256], &Cancel::never(), true).map(drop)
    }

    fn infer(&self, ids: &[i64], style: &[f32], cancel: &Cancel) -> Result<KokoroRaw> {
        let Some(held) = self.held() else {
            // Une synthèse inutile pendant une brève libération des poids
            // est un abandon, pas un échec.
            cancel.check()?;
            return Err(not_loaded());
        };
        self.run(&held, ids, style, cancel, false)?
            .ok_or_else(|| anyhow!("Kokoro : passage sauté hors de la préparation"))
    }

    fn release(&self) {
        if self.model.locked().take().is_some() {
            budget::remove(BUDGET_KEY);
        }
    }
}

/// Un point d'arrêt : avec un test d'abandon, `x` est évalué (sans quoi rien
/// n'aurait encore tourné) puis le test consulté ; sans test, rien.
pub(super) struct Stop<'a>(&'a Cancel);

impl Stop<'_> {
    fn here(&self, x: &Array) -> Result<()> {
        if self.0.may_cancel() {
            x.eval()?;
            self.0.check()?;
        }
        Ok(())
    }
}

/// Kokoro-82M (StyleTTS2) : le passage avant seul, comme `KokoroModel`.
struct Kokoro {
    config: Config,
    bert: Albert,
    bert_encoder: Linear,
    predictor: Predictor,
    text_encoder: TextEncoder,
    decoder: Decoder,
    /// Les tirages du générateur. La graine du temps, comme le MLX par
    /// défaut ; les essais la fixent.
    rng: RandomState,
}

impl Kokoro {
    fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(dir)?;
        let mut w = Weights::load(dir)?;
        let bert = Albert::load(&mut w, &config.plbert, config.n_token)?;
        let bert_encoder = Linear::load(&mut w, "bert_encoder", config.plbert.hidden_size, config.hidden_dim)?;
        let predictor = Predictor::load(&mut w, &config)?;
        let text_encoder = TextEncoder::load(&mut w, &config)?;
        let decoder = Decoder::load(&mut w, &config)?;
        Ok(Self { config, bert, bert_encoder, predictor, text_encoder, decoder, rng: RandomState::new()? })
    }

    fn check_input(&self, ids: &[i64], style: &[f32]) -> Result<Vec<i32>> {
        let longest = self.config.plbert.max_position_embeddings as usize;
        ensure!(
            !ids.is_empty() && ids.len() <= longest,
            crate::tr!(
                en = "Kokoro takes 1 to {longest} tokens, not {}",
                fr = "Kokoro prend de 1 à {longest} jetons, pas {}",
                ids.len(),
            )
        );
        let style_len = 2 * self.config.style_dim as usize;
        ensure!(
            style.len() == style_len,
            crate::tr!(
                en = "Kokoro style of {} values instead of {style_len}",
                fr = "style Kokoro de {} valeurs au lieu de {style_len}",
                style.len(),
            )
        );
        let vocab = self.config.n_token as i64;
        ids.iter()
            .map(|&id| {
                ensure!(
                    (0..vocab).contains(&id),
                    crate::tr!(en = "unknown Kokoro token {id}", fr = "jeton Kokoro inconnu {id}")
                );
                Ok(id as i32)
            })
            .collect()
    }

    /// Le passage avant à vitesse 1 : l'audio à 24 kHz et une durée par
    /// jeton, évalués et copiés.
    fn forward(&mut self, ids: &[i64], style: &[f32], cancel: &Cancel) -> Result<KokoroRaw> {
        let ids = self.check_input(ids, style)?;
        let stop = Stop(cancel);
        let tokens = ids.len() as i32;
        let input = Array::from_slice(&ids, &[1, tokens]);
        let style_dim = self.config.style_dim;
        let refs = Array::from_slice(style, &[1, 2 * style_dim]);
        let lengths = Array::from_slice(&[tokens], &[1]);
        let positions = Array::from_iter(0..tokens, &[1, tokens]);
        let text_mask = positions.add(Array::from_int(1))?.gt(lengths.reshape(&[-1, 1])?)?;

        let attention = Array::from_int(1).subtract(text_mask.as_type::<i32>()?)?;
        let bert = self.bert.forward(&input, &attention)?;
        let d_en = self.bert_encoder.forward(&bert)?.transpose_axes(&[0, 2, 1])?;
        let global = refs.try_index((.., style_dim..))?;
        let acoustic = refs.try_index((.., ..style_dim))?;

        let d = self.predictor.encode_durations(&d_en, &global, &text_mask)?;
        let x = self.predictor.lstm.forward(&d)?;
        let duration = self.predictor.duration_proj.forward(&x)?;
        let raw = ops::sigmoid(&duration)?.sum_axis(-1, None)?.divide(scalar(1.0))?;
        let safe = raw.nan_to_num(1.0f32, None, None)?;
        let predicted = ops::clip(safe.round(None)?, (1.0f32, MAX_FRAMES_PER_PHONEME as f32))?
            .as_type::<i32>()?
            .try_index(0)?;
        predicted.eval()?;
        let durations: Vec<i32> = predicted.try_as_slice::<i32>()?.to_vec();
        stop.0.check()?;

        let alignment = alignment(&durations);
        let frames = alignment.len() as i32 / tokens;
        let alignment = Array::from_slice(&alignment, &[1, tokens, frames]);
        let en = d.transpose_axes(&[0, 2, 1])?.matmul(&alignment)?;
        let (f0, n) = self.predictor.predict(&en, &global)?;
        let t_en = self.text_encoder.forward(&input, &text_mask)?;
        let asr = t_en.matmul(&alignment)?;
        let audio = self.decoder.forward(&asr, &f0, &n, &acoustic, &mut self.rng, &stop)?;
        let audio = audio.try_index(0)?.reshape(&[-1])?;
        audio.eval()?;
        Ok(KokoroRaw {
            samples: audio.try_as_slice::<f32>()?.to_vec(),
            durations: durations.iter().map(|&d| d.max(0) as u32).collect(),
        })
    }
}

/// L'alignement jetons → trames, `[jetons × trames]` : chaque jeton occupe
/// ses trames, à la suite (les mêmes valeurs que la dispersion du Swift).
fn alignment(durations: &[i32]) -> Vec<f32> {
    let counts: Vec<usize> = durations.iter().map(|&n| n.clamp(0, MAX_FRAMES_PER_PHONEME) as usize).collect();
    let frames: usize = counts.iter().sum();
    let mut matrix = vec![0f32; durations.len() * frames];
    let mut column = 0;
    for (token, &count) in counts.iter().enumerate() {
        for _ in 0..count {
            matrix[token * frames + column] = 1.0;
            column += 1;
        }
    }
    matrix
}

#[cfg(test)]
mod tests;
