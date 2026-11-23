//! Qwen3-ASR (0.6B en 8 bits, `mlx-community/Qwen3-ASR-0.6B-8bit` ; 1.7B en
//! 4 bits, `mlx-community/Qwen3-ASR-1.7B-4bit`) en MLX, en Rust : la
//! traduction du Qwen3-ASR Swift (`macos/Vendor/KokoroMLX/STT/Qwen3ASR`,
//! chemin de `Qwen3ASRSpeech`), à parité avec lui. Le modèle vit sur le fil
//! MLX (`compute::mlx::Held`) et n'en sort que le texte.
//!
//! Propre à Qwen3-ASR : un encodeur audio Transformer par fenêtres, un
//! décodeur Qwen3 qui lit une invite de conversation où les `<|audio_pad|>`
//! prennent la sortie de l'encodeur ; la langue de la dictée est imposée
//! (l'invite finit par « language French<asr_text> ») ; 8 192 jetons au plus
//! pour toute la lecture ; le vocabulaire est le BPE de Qwen2, commun avec
//! Qwen3-TTS (`compute::mlx::tokenizer::bpe`).
//!
//! L'abandon est consulté avant le passage, sous le verrou du GPU, après
//! chaque encodeur, après chaque jeton (déjà évalué : rien de plus à
//! calculer) et après le passage. Le Swift, lui, va toujours au bout.

mod audio;
mod config;
mod decoder;
mod encoder;
mod model;

#[cfg(test)]
mod synthetic;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

use self::model::{Qwen3Asr, MAX_TOKENS};
use super::{need, BUDGET_KEY};
use crate::compute::mlx::{budget, Held};
use crate::stt::recognizer::Recognizer;
use crate::sync::LockExt;
use crate::tts::Cancel;

/// Moins d'audio que cela (0,1 s) : aucun texte, sans lancer le modèle,
/// comme `MLXDictationEngine.minimumSamples` de l'app.
pub const MIN_SAMPLES: usize = 1_600;
/// La préparation : une demi-seconde de silence, en anglais.
const WARM_UP_SAMPLES: usize = 8_000;
const WARM_UP_LANGUAGE: &str = "en";

/// Qwen3-ASR en MLX (0.6B ou 1.7B, d'après le dossier chargé). Un énoncé
/// entier en entrée, son texte en sortie, dans la langue donnée.
///
/// `prepare` et `hold` gardent leur défaut (`Holder` garde le modèle d'une
/// dictée à l'autre) ; `memory` rend la mémoire active de la copie Rust de
/// MLX.
#[derive(Default)]
pub struct Qwen3AsrMlx {
    /// Partagé avec le passage en cours : une libération n'attend pas sa fin.
    model: Mutex<Option<Arc<Held<Qwen3Asr>>>>,
}

impl Qwen3AsrMlx {
    pub fn new() -> Self {
        Self::default()
    }

    fn held(&self) -> Option<Arc<Held<Qwen3Asr>>> {
        self.model.locked().clone()
    }
}

/// Un point d'arrêt du passage : le test d'abandon, s'il y en a un.
fn stop(cancel: &Cancel) -> Result<()> {
    if cancel.may_cancel() {
        cancel.check()?;
    }
    Ok(())
}

impl Recognizer for Qwen3AsrMlx {
    /// Inscrit le besoin `"dictee"` (poids + 2 Gio, cache 256 Mio), puis
    /// charge ; un échec le retire. L'ancien modèle part avant : jamais deux
    /// en mémoire.
    fn load(&self, model_dir: &Path) -> Result<()> {
        let mut model = self.model.locked();
        if model.take().is_some() {
            budget::remove(BUDGET_KEY);
        }
        budget::set(BUDGET_KEY, need(model_dir));
        let dir = model_dir.to_path_buf();
        match Held::load(move || Qwen3Asr::load(&dir)) {
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

    /// Une demi-seconde de silence, jamais abandonnée : le premier vrai
    /// énoncé ne paie pas la compilation des noyaux Metal.
    fn warm_up(&self) -> Result<()> {
        self.transcribe(&[0.0; WARM_UP_SAMPLES], WARM_UP_LANGUAGE, &Cancel::never())
            .map(drop)
    }

    /// `Qwen3ASRSpeech.transcribe` : la langue imposée, 8 192 jetons au
    /// plus, le texte sans espaces autour.
    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String> {
        cancel.check()?;
        let Some(held) = self.held() else {
            return Err(anyhow!(crate::tr!(
                en = "Qwen3-ASR model not loaded",
                fr = "modèle Qwen3-ASR non chargé",
            )));
        };
        if samples.len() < MIN_SAMPLES {
            return Ok(String::new());
        }
        let (samples, language, inner) = (samples.to_vec(), language.to_string(), cancel.clone());
        let text = held.with(move |asr| {
            inner.check()?;
            let transcript =
                asr.generate(&samples, Some(&language), MAX_TOKENS, &|| stop(&inner))?;
            Ok(transcript.text.trim().to_string())
        })?;
        cancel.check()?;
        Ok(text)
    }

    fn release(&self) {
        if self.model.locked().take().is_some() {
            budget::remove(BUDGET_KEY);
        }
    }

    fn memory(&self) -> Option<u64> {
        self.held()?;
        crate::compute::mlx::memory().ok().map(|m| m.active as u64)
    }
}
