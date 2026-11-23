//! Voxtral Realtime (4 bits, `mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit`)
//! en MLX, en Rust : la traduction du Voxtral Swift
//! (`macos/Vendor/KokoroMLX/STT/VoxtralRealtime`, chemin de `VoxtralSpeech`
//! et `VoxtralSession`), à parité avec lui. Un vrai flux : mel et
//! convolutions à retenues, encodeur à caches par blocs de fenêtre, décodeur
//! à caches ; chaque bout d'audio n'est lu qu'une fois.
//!
//! Propre à Voxtral : le texte suit la voix de 480 ms (`DELAY_MS`, le
//! réglage du modèle), un jeton tous les 1 280 échantillons (80 ms, le pas
//! de la dictée) ; aucune langue : celle de la dictée est ignorée
//! (multilingue sans invite) ; 4 096 jetons au plus par flux, comme le
//! Swift (environ 5 min 28 s d'audio : au-delà, le flux est fini et ne lit
//! plus rien) ; le cache de MLX est vidé à la fin de chaque flux.
//!
//! Le modèle et ses flux vivent sur le fil MLX (`compute::mlx::Held`) : un
//! flux ouvert n'est, hors de ce fil, qu'un numéro. Rien d'autre que du
//! texte n'en sort.

mod audio;
mod config;
mod decoder;
mod encoder;
mod model;
mod stream;
mod text;
mod tokenizer;

#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod synthetic;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use self::model::Voxtral;
use self::stream::Session;
use super::{need, BUDGET_KEY};
use crate::compute::mlx::{budget, Held};
use crate::stt::recognizer::{RecognizerStream, StreamingRecognizer};
use crate::sync::LockExt;

/// Le retard du texte sur la voix (`VoxtralSpeech.delayMs`, le défaut du
/// modèle).
pub const DELAY_MS: i32 = 480;
/// Les jetons d'un flux au plus (`makeStreamSession`, `maxTokens` par
/// défaut).
pub const MAX_TOKENS: usize = 4096;
/// La préparation : une demi-seconde de silence (`VoxtralSmokeTests`).
const WARM_UP_SAMPLES: usize = 8_000;

/// Le modèle chargé et ses flux ouverts, par numéro.
struct Loaded {
    model: Voxtral,
    sessions: HashMap<u64, Session>,
}

/// Voxtral Realtime en MLX : un flux par dictée.
///
/// `prepare` et `hold` gardent leur défaut ; `memory` rend la mémoire
/// active de la copie Rust de MLX.
#[derive(Default)]
pub struct VoxtralMlx {
    model: Mutex<Option<Arc<Held<Loaded>>>>,
}

impl VoxtralMlx {
    pub fn new() -> Self {
        Self::default()
    }
}

fn not_loaded() -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "Voxtral model not loaded",
        fr = "modèle Voxtral non chargé",
    ))
}

impl StreamingRecognizer for VoxtralMlx {
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
        let loaded = Held::load(move || {
            Ok(Loaded {
                model: Voxtral::load(&dir)?,
                sessions: HashMap::new(),
            })
        });
        match loaded {
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

    /// Une demi-seconde de silence par un flux, jusqu'au bout : le premier
    /// vrai flux ne paie pas la compilation des noyaux Metal.
    fn warm_up(&self) -> Result<()> {
        let mut stream = self.open_stream("")?;
        stream.feed(&[0.0; WARM_UP_SAMPLES])?;
        stream.finish().map(drop)
    }

    /// `VoxtralSpeech.makeSession` : une session neuve, retard de 480 ms.
    /// La langue est ignorée.
    fn open_stream(&self, _language: &str) -> Result<Box<dyn RecognizerStream>> {
        let held = self.model.locked().clone().ok_or_else(not_loaded)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        held.with(move |loaded| {
            let session = Session::new(&loaded.model, DELAY_MS, MAX_TOKENS);
            loaded.sessions.insert(id, session);
            Ok(())
        })?;
        Ok(Box::new(VoxtralStream {
            held: Arc::downgrade(&held),
            id,
            ended: None,
        }))
    }

    fn release(&self) {
        if self.model.locked().take().is_some() {
            budget::remove(BUDGET_KEY);
        }
    }

    fn memory(&self) -> Option<u64> {
        self.model.locked().as_ref()?;
        crate::compute::mlx::memory().ok().map(|m| m.active as u64)
    }
}

/// Un flux ouvert (`VoxtralSession` de l'app) : sa session vit sur le fil
/// MLX, sous son numéro. Le modèle libéré ou rechargé, il échoue.
struct VoxtralStream {
    held: Weak<Held<Loaded>>,
    id: u64,
    /// Le texte de `finish`, une fois le flux fermé.
    ended: Option<String>,
}

impl RecognizerStream for VoxtralStream {
    /// `VoxtralSession.step` : le texte que la session a décodé pendant ce
    /// pas (`Delta.text`).
    fn feed(&mut self, samples: &[f32]) -> Result<String> {
        if self.ended.is_some() {
            return Ok(String::new());
        }
        let held = self.held.upgrade().ok_or_else(not_loaded)?;
        let (id, samples) = (self.id, samples.to_vec());
        held.with(move |loaded| {
            let session = loaded.sessions.get_mut(&id).ok_or_else(not_loaded)?;
            Ok(session
                .step(&mut loaded.model, &samples, &mut |_| Ok(()))?
                .text)
        })
    }

    /// `VoxtralSession.finish` : le texte entier, sans espaces autour. La
    /// session est rendue, puis le cache de MLX vidé, une fois Metal sorti de
    /// ses tampons (`releaseCache` de l'app) : rien ne reste en réserve. Un second appel rend
    /// le même texte.
    fn finish(&mut self) -> Result<String> {
        if let Some(text) = &self.ended {
            return Ok(text.clone());
        }
        let held = self.held.upgrade().ok_or_else(not_loaded)?;
        let id = self.id;
        let text = held.with(move |loaded| {
            let session = loaded.sessions.get_mut(&id).ok_or_else(not_loaded)?;
            session.finish(&mut loaded.model, &mut |_| Ok(()))?;
            let text = session.text().trim().to_string();
            loaded.sessions.remove(&id);
            crate::compute::mlx::clear_cache()?;
            Ok(text)
        })?;
        self.ended = Some(text.clone());
        Ok(text)
    }
}

impl Drop for VoxtralStream {
    /// Rend la session (ses caches) : sur le fil MLX, sans erreur si le
    /// modèle est déjà parti ou si l'appel vient du fil MLX lui-même.
    fn drop(&mut self) {
        if self.ended.is_some() {
            return;
        }
        if let Some(held) = self.held.upgrade() {
            let id = self.id;
            let _ = held.with(move |loaded| {
                loaded.sessions.remove(&id);
                crate::compute::mlx::clear_cache()
            });
        }
    }
}
