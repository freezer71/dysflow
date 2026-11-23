//! Les contrats des reconnaisseurs. Audio : 16 kHz, mono, `f32`.

use crate::tts::Cancel;
use anyhow::Result;
use std::path::Path;

/// Un reconnaisseur par lot : un énoncé entier en entrée, son texte en sortie.
pub trait Recognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String>;
    fn release(&self);
}

/// Un reconnaisseur en flux : il ouvre un flux par dictée.
pub trait StreamingRecognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn open_stream(&self, language: &str) -> Result<Box<dyn RecognizerStream>>;
    fn release(&self);
}

/// Un flux ouvert par `StreamingRecognizer::open_stream`.
pub trait RecognizerStream: Send {
    /// Donne des échantillons ; rend le texte nouveau depuis l'appel précédent.
    fn feed(&mut self, samples: &[f32]) -> Result<String>;
    /// Termine le flux ; rend le texte entier.
    fn finish(&mut self) -> Result<String>;
}
