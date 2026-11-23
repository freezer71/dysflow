//! Whisper (tiny, small, large-v3-turbo, en 4 bits, dépôts mlx-community) en
//! MLX, en Rust : la traduction du Whisper Swift
//! (`macos/Vendor/KokoroMLX/STT/Whisper`, chemin de `WhisperSpeech`), à
//! parité avec lui. Le modèle vit sur le fil MLX (`compute::mlx::Held`) et
//! n'en sort que le texte.
//!
//! Propre à Whisper : la langue de la dictée est imposée (`fr`, `en`…),
//! jamais devinée ; décodage glouton ; un audio de plus de 30 s est lu par
//! fenêtres de 30 s, chacune seule, et leurs textes collés. Le cache des
//! fenêtres déjà lues de l'app (`WhisperSpeech.read`) n'est pas porté : les
//! segments de la dictée font 12 s au plus.
//!
//! L'abandon est consulté avant le passage, sous le verrou du GPU, après
//! l'encodeur et l'invite, après chaque jeton (déjà évalué : rien de plus à
//! calculer) et après le passage. Le Swift, lui, va toujours au bout.

mod audio;
mod config;
mod layers;
mod model;
pub(in crate::stt::mlx) mod tokenizer;

#[cfg(test)]
mod stages;
#[cfg(test)]
mod synthetic;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

use self::model::Whisper;
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

/// Whisper en MLX. Un énoncé entier en entrée, son texte en sortie, dans la
/// langue donnée.
///
/// `prepare` et `hold` gardent leur défaut (`Holder` garde le modèle d'une
/// dictée à l'autre) ; `memory` rend la mémoire active de la copie Rust de
/// MLX.
#[derive(Default)]
pub struct WhisperMlx {
    /// Partagé avec le passage en cours : une libération n'attend pas sa fin.
    model: Mutex<Option<Arc<Held<Whisper>>>>,
}

impl WhisperMlx {
    pub fn new() -> Self {
        Self::default()
    }

    fn held(&self) -> Option<Arc<Held<Whisper>>> {
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

impl Recognizer for WhisperMlx {
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
        match Held::load(move || Whisper::load(&dir)) {
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

    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String> {
        cancel.check()?;
        let Some(held) = self.held() else {
            return Err(anyhow!(crate::tr!(
                en = "Whisper model not loaded",
                fr = "modèle Whisper non chargé",
            )));
        };
        if samples.len() < MIN_SAMPLES {
            return Ok(String::new());
        }
        let (samples, language, inner) = (samples.to_vec(), language.to_string(), cancel.clone());
        let text = held.with(move |whisper| {
            inner.check()?;
            let windows = whisper.transcribe(&samples, &language, &|| stop(&inner))?;
            Ok(Whisper::text(&windows))
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
