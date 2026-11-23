//! Nemotron 3.5 ASR Streaming (8 bits,
//! `mlx-community/nemotron-3.5-asr-streaming-0.6b-8bit`) en MLX, en Rust : la
//! traduction du Nemotron Swift (`macos/Vendor/KokoroMLX/STT/NemotronASR`,
//! chemin de `NemotronSpeech` et `NemotronSession`), à parité avec lui. Un
//! vrai flux : encodeur à caches et RNN-T glouton, chaque bout d'audio lu
//! une seule fois.
//!
//! Propre à Nemotron : tranches de 560 ms (`CHUNK_MS`, 7 trames de 80 ms) ;
//! la langue de la dictée passe en invite (« fr », « en » : le dictionnaire
//! d'invites du modèle les a) ; le texte rendu est réduit (`tidy` : une
//! seule espace entre deux phrases) et ne fait que s'allonger ; le même
//! audio donne le même texte quel que soit le découpage des `feed`.
//!
//! Le modèle et ses flux vivent sur le fil MLX (`compute::mlx::Held`) : un
//! flux ouvert n'est, hors de ce fil, qu'un numéro. Rien d'autre que du
//! texte n'en sort.

mod config;
mod encoder;
mod model;
mod stream;
mod tokenizer;

#[cfg(test)]
mod synthetic;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use self::model::Nemotron;
use self::stream::Session;
use super::{need, BUDGET_KEY};
use crate::compute::mlx::{budget, Held};
use crate::stt::recognizer::{RecognizerStream, StreamingRecognizer};
use crate::sync::LockExt;

/// La tranche de la dictée (`NemotronSpeech.chunkMs`) : le texte suit la
/// voix d'environ 0,6 s.
pub const CHUNK_MS: u32 = 560;
/// La préparation : une demi-seconde de silence, en anglais.
const WARM_UP_SAMPLES: usize = 8_000;
const WARM_UP_LANGUAGE: &str = "en";

/// Le modèle chargé et ses flux ouverts, par numéro.
struct Loaded {
    model: Nemotron,
    sessions: HashMap<u64, Session>,
}

/// Nemotron en MLX : un flux par dictée.
///
/// `prepare` et `hold` gardent leur défaut (`Holder` garde le modèle d'une
/// dictée à l'autre) ; `memory` rend la mémoire active de la copie Rust de
/// MLX.
#[derive(Default)]
pub struct NemotronMlx {
    model: Mutex<Option<Arc<Held<Loaded>>>>,
}

impl NemotronMlx {
    pub fn new() -> Self {
        Self::default()
    }
}

fn not_loaded() -> anyhow::Error {
    anyhow!(crate::tr!(
        en = "Nemotron model not loaded",
        fr = "modèle Nemotron non chargé",
    ))
}

impl StreamingRecognizer for NemotronMlx {
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
                model: Nemotron::load(&dir)?,
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
        let mut stream = self.open_stream(WARM_UP_LANGUAGE)?;
        stream.feed(&[0.0; WARM_UP_SAMPLES])?;
        stream.finish().map(drop)
    }

    /// `NemotronSpeech.makeSession` : une session neuve, tranches de 560 ms,
    /// la langue en invite.
    fn open_stream(&self, language: &str) -> Result<Box<dyn RecognizerStream>> {
        let held = self.model.locked().clone().ok_or_else(not_loaded)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let language = language.to_string();
        held.with(move |loaded| {
            let session = Session::new(&loaded.model, &language, Some(CHUNK_MS));
            loaded.sessions.insert(id, session);
            Ok(())
        })?;
        Ok(Box::new(NemotronStream {
            held: Arc::downgrade(&held),
            id,
            shown: String::new(),
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

/// Un flux ouvert (`NemotronSession` de l'app) : sa session vit sur le fil
/// MLX, sous son numéro. Le modèle libéré ou rechargé, il échoue.
struct NemotronStream {
    held: Weak<Held<Loaded>>,
    id: u64,
    /// Le texte réduit déjà rendu.
    shown: String,
    /// Le texte de `finish`, une fois le flux fermé.
    ended: Option<String>,
}

impl NemotronStream {
    /// `body` sur la session, sur le fil MLX ; rend le texte entier de la
    /// session après lui.
    fn with_session(
        &self,
        body: impl FnOnce(&mut Nemotron, &mut Session) -> Result<()> + Send + 'static,
    ) -> Result<String> {
        let held = self.held.upgrade().ok_or_else(not_loaded)?;
        let id = self.id;
        held.with(move |loaded| {
            let session = loaded.sessions.get_mut(&id).ok_or_else(not_loaded)?;
            body(&mut loaded.model, session)?;
            Ok(session.text().to_string())
        })
    }
}

impl RecognizerStream for NemotronStream {
    /// `NemotronSession.step` : le texte réduit nouveau, s'il allonge celui
    /// déjà rendu ; sinon rien.
    fn feed(&mut self, samples: &[f32]) -> Result<String> {
        if self.ended.is_some() {
            return Ok(String::new());
        }
        let samples = samples.to_vec();
        let text = self.with_session(move |model, session| {
            session.step(model, &samples, &mut |_| Ok(())).map(drop)
        })?;
        Ok(grown(&mut self.shown, &tokenizer::tidy(&text)))
    }

    /// `NemotronSession.finish` : le texte entier, réduit, sans espaces
    /// autour. La session est rendue ; un second appel rend le même texte.
    fn finish(&mut self) -> Result<String> {
        if let Some(text) = &self.ended {
            return Ok(text.clone());
        }
        let id = self.id;
        let text = self
            .with_session(move |model, session| session.finish(model, &mut |_| Ok(())).map(drop))?;
        let text = tokenizer::tidy(&text).trim().to_string();
        self.forget(id);
        self.ended = Some(text.clone());
        Ok(text)
    }
}

impl NemotronStream {
    /// Rend la session (ses caches) : sur le fil MLX, sans erreur si le
    /// modèle est déjà parti ou si l'appel vient du fil MLX lui-même.
    fn forget(&self, id: u64) {
        if let Some(held) = self.held.upgrade() {
            let _ = held.with(move |loaded| {
                loaded.sessions.remove(&id);
                Ok(())
            });
        }
    }
}

impl Drop for NemotronStream {
    fn drop(&mut self) {
        if self.ended.is_none() {
            self.forget(self.id);
        }
    }
}

/// Le morceau qui allonge `shown` jusqu'à `text`, et `shown` mis à jour ;
/// rien si `text` n'est pas plus long ou n'en part pas.
fn grown(shown: &mut String, text: &str) -> String {
    if text.len() <= shown.len() || !text.starts_with(shown.as_str()) {
        return String::new();
    }
    let delta = text[shown.len()..].to_string();
    *shown = text.to_string();
    delta
}

#[cfg(test)]
mod grown_tests {
    use super::grown;

    #[test]
    fn only_a_longer_text_with_the_same_start_is_shown() {
        let mut shown = String::new();
        assert_eq!(grown(&mut shown, "Bon"), "Bon");
        assert_eq!(grown(&mut shown, "Bonjour"), "jour");
        assert_eq!(grown(&mut shown, "Bonjour"), "", "pas plus long");
        assert_eq!(grown(&mut shown, "Bonsoir à tous"), "", "un autre début");
        assert_eq!(shown, "Bonjour", "rien de changé");
        assert_eq!(grown(&mut shown, "Bonjour à"), " à");
    }
}
