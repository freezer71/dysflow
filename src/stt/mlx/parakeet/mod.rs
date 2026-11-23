//! Parakeet TDT (v3 multilingue 8 bits, v2 anglais) en MLX, en Rust : la
//! traduction du Parakeet Swift (`macos/Vendor/KokoroMLX/STT/Parakeet`), à
//! parité avec lui. Le modèle vit sur le fil MLX (`compute::mlx::Held`) et
//! n'en sort que le texte.
//!
//! L'abandon est consulté avant le passage (sous le verrou du GPU), après
//! l'encodeur, après chaque pas du décodeur (déjà évalué : rien de plus à
//! calculer) et après le passage. Le Swift, lui, va toujours au bout.

pub(in crate::stt::mlx) mod audio;
pub(in crate::stt::mlx) mod config;
pub(in crate::stt::mlx) mod conformer;
pub(in crate::stt::mlx) mod model;
mod tokenizer;

#[cfg(test)]
pub(in crate::stt::mlx) mod tests;

use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

use self::model::{Parakeet, Stop};
use super::{need, BUDGET_KEY};
use crate::compute::mlx::{budget, Held};
use crate::stt::recognizer::Recognizer;
use crate::sync::LockExt;
use crate::tts::Cancel;

/// Moins d'audio que cela (0,1 s) : aucun texte, sans lancer le modèle,
/// comme `MLXDictationEngine.minimumSamples` de l'app.
pub const MIN_SAMPLES: usize = 1_600;
/// La préparation : une demi-seconde de silence.
const WARM_UP_SAMPLES: usize = 8_000;

/// Parakeet en MLX. Un énoncé entier en entrée, son texte en sortie ;
/// Parakeet entend la langue seul : celle de la dictée n'est pas passée.
///
/// `prepare` et `hold` gardent leur défaut (`Holder` garde le modèle d'une
/// dictée à l'autre) ; `memory` rend la mémoire active de la copie Rust de
/// MLX.
pub struct ParakeetMlx {
    /// Partagé avec le passage en cours : une libération n'attend pas sa fin.
    model: Mutex<Option<Arc<Held<Parakeet>>>>,
    /// Le nom du modèle dans les messages.
    name: &'static str,
    /// La lecture du dossier du paquet, sur le fil MLX.
    open: fn(&Path) -> Result<Parakeet>,
}

impl Default for ParakeetMlx {
    fn default() -> Self {
        Self::new()
    }
}

impl ParakeetMlx {
    pub fn new() -> Self {
        Self::reading("Parakeet", Parakeet::load)
    }

    /// Le même graphe et le même passage, ses poids lus par `open` : ceux
    /// d'un dérivé de Parakeet (Phonon-2).
    pub(in crate::stt::mlx) fn reading(
        name: &'static str,
        open: fn(&Path) -> Result<Parakeet>,
    ) -> Self {
        Self {
            model: Mutex::new(None),
            name,
            open,
        }
    }

    fn held(&self) -> Option<Arc<Held<Parakeet>>> {
        self.model.locked().clone()
    }
}

fn not_loaded(name: &str) -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "{name} model not loaded",
        fr = "modèle {name} non chargé",
    ))
}

impl Recognizer for ParakeetMlx {
    /// Inscrit le besoin `"dictee"` (poids + 2 Gio, cache 256 Mio), puis
    /// charge ; un échec le retire. L'ancien modèle part avant : jamais deux
    /// en mémoire.
    fn load(&self, model_dir: &Path) -> Result<()> {
        let mut model = self.model.locked();
        if model.take().is_some() {
            budget::remove(BUDGET_KEY);
        }
        budget::set(BUDGET_KEY, need(model_dir));
        let (dir, open) = (model_dir.to_path_buf(), self.open);
        match Held::load(move || open(&dir)) {
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
        self.transcribe(&[0.0; WARM_UP_SAMPLES], "", &Cancel::never())
            .map(drop)
    }

    fn transcribe(&self, samples: &[f32], _language: &str, cancel: &Cancel) -> Result<String> {
        cancel.check()?;
        let Some(held) = self.held() else {
            return Err(not_loaded(self.name));
        };
        if samples.len() < MIN_SAMPLES {
            return Ok(String::new());
        }
        let (samples, inner) = (samples.to_vec(), cancel.clone());
        let text = held.with(move |parakeet| {
            inner.check()?;
            parakeet.generate(&samples, &Stop(&inner))
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
