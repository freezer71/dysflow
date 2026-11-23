//! Phonon-2 (Fermion Research, poids sous CC-BY-4.0, dérivés de Parakeet
//! TDT 0.6B v3 de NVIDIA) en MLX, en Rust : la traduction de
//! `PhononLoader.swift` (`ParakeetModel.fromPhonon`), à parité avec lui.
//!
//! Le dossier du paquet tient l'archive telle que téléchargée
//! (`phonon-2.bps.tar.zst` : `config.json` de NeMo et `model.fermion`), lue
//! en mémoire sans être décompressée sur le disque (`archive`). Le graphe
//! est celui de Parakeet v3 (TDT) ; ses `Linear` d'encodeur deviennent des
//! couches à cinq valeurs (`container`, `nn::Linear::FiveValue`). Le
//! passage, l'abandon, les plafonds de MLX (`"dictee"` : l'archive plus
//! 2 Gio) et la libération sont ceux de `ParakeetMlx`.

mod archive;
mod container;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

use super::parakeet::config::Config;
use super::parakeet::model::{Parakeet, COMPUTE};
use super::parakeet::ParakeetMlx;
use super::weights::Weights;
use crate::stt::recognizer::{Hold, Recognizer};
use crate::tts::Cancel;

/// L'archive du paquet (`PhononContainer.archiveName`), celle du catalogue.
pub const ARCHIVE: &str = "phonon-2.bps.tar.zst";
/// Les deux membres lus de l'archive.
const CONFIG_MEMBER: &str = "config.json";
const CONTAINER_MEMBER: &str = "model.fermion";
/// Le nom du modèle dans les messages.
const NAME: &str = "Phonon-2";

/// `fromPhonon` : `config.json` et le conteneur, lus dans l'archive ; le
/// graphe de Parakeet TDT, chaque paramètre pris avec sa forme, flottants
/// en `bfloat16`, mots des plans (`uint32`) tels quels.
pub(in crate::stt::mlx) fn load(dir: &Path) -> Result<Parakeet> {
    let archive = dir.join(ARCHIVE);
    let [config, data] = archive::read_members(&archive, [CONFIG_MEMBER, CONTAINER_MEMBER])?;
    // Comme `String(decoding:as:)` : un octet invalide devient U+FFFD.
    let config = Config::parse(&String::from_utf8_lossy(&config)).with_context(|| {
        crate::tr!(
            en = "unreadable Phonon-2 configuration ({}, {CONFIG_MEMBER})",
            fr = "configuration de Phonon-2 illisible ({}, {CONFIG_MEMBER})",
            archive.display()
        )
    })?;
    let arrays = container::decode(&data, &archive)?;
    drop(data);
    let weights = Weights::from_arrays(NAME, &archive, arrays, COMPUTE)?;
    Parakeet::from_weights(config, weights)
}

/// Phonon-2 en MLX : `ParakeetMlx`, ses poids lus dans l'archive. Comme
/// Parakeet v3, il entend la langue seul : celle de la dictée n'est pas
/// passée.
pub struct PhononMlx(ParakeetMlx);

impl Default for PhononMlx {
    fn default() -> Self {
        Self::new()
    }
}

impl PhononMlx {
    pub fn new() -> Self {
        Self(ParakeetMlx::reading(NAME, load))
    }
}

impl Recognizer for PhononMlx {
    fn load(&self, model_dir: &Path) -> Result<()> {
        self.0.load(model_dir)
    }

    fn warm_up(&self) -> Result<()> {
        self.0.warm_up()
    }

    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String> {
        self.0.transcribe(samples, language, cancel)
    }

    fn release(&self) {
        self.0.release()
    }

    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        self.0.prepare(model_dir, will_load)
    }

    fn hold(&self) -> Hold {
        self.0.hold()
    }

    fn memory(&self) -> Option<u64> {
        self.0.memory()
    }
}
