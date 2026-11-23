//! Les contrats des reconnaisseurs. Audio : 16 kHz, mono, `f32`.

use crate::tts::Cancel;
use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

/// Un reconnaisseur par lot : un énoncé entier en entrée, son texte en sortie.
pub trait Recognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String>;
    fn release(&self);

    /// Charge `model_dir` s'il ne l'est pas déjà. `will_load` est appelé juste
    /// avant un vrai chargement. Rend sa durée ; `None` : le modèle était déjà
    /// là. Par défaut, chaque appel est un vrai chargement ; `Holder` garde le
    /// modèle d'une dictée à l'autre.
    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        will_load();
        let start = Instant::now();
        self.load(model_dir)?;
        Ok(Some(start.elapsed()))
    }

    /// Le modèle reste chargé tant que la garde vit : une dictée la prend, si
    /// longues que soient ses pauses. Par défaut, rien à garder.
    fn hold(&self) -> Hold {
        Hold::none()
    }

    /// La mémoire que le modèle occupe, en octets, s'il la connaît.
    fn memory(&self) -> Option<u64> {
        None
    }
}

/// Un reconnaisseur en flux : il ouvre un flux par dictée.
pub trait StreamingRecognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn open_stream(&self, language: &str) -> Result<Box<dyn RecognizerStream>>;
    fn release(&self);

    /// Comme `Recognizer::prepare`.
    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        will_load();
        let start = Instant::now();
        self.load(model_dir)?;
        Ok(Some(start.elapsed()))
    }

    /// Comme `Recognizer::hold`.
    fn hold(&self) -> Hold {
        Hold::none()
    }

    /// Comme `Recognizer::memory`.
    fn memory(&self) -> Option<u64> {
        None
    }
}

/// Un flux ouvert par `StreamingRecognizer::open_stream`.
pub trait RecognizerStream: Send {
    /// Donne des échantillons ; rend le texte nouveau depuis l'appel précédent.
    fn feed(&mut self, samples: &[f32]) -> Result<String>;
    /// Termine le flux ; rend le texte entier.
    fn finish(&mut self) -> Result<String>;
}

/// Une garde rendue par `hold` : la lâcher (la laisser tomber) rend le modèle
/// libre d'être libéré.
#[must_use = "la garde lâchée tout de suite ne garde rien"]
pub struct Hold(Option<Box<dyn FnOnce() + Send>>);

impl Hold {
    /// Rien à garder.
    pub fn none() -> Self {
        Self(None)
    }

    /// `let_go` est appelé une fois, quand la garde tombe.
    pub fn new(let_go: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(let_go)))
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(let_go) = self.0.take() {
            let_go();
        }
    }
}
