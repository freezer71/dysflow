//! Moteur muet : du silence, à la durée d'une vraie phrase.
//!
//! Sert aux essais et au mode de test de l'app : la lecture avance, le
//! surlignage bouge, sans modèle ni carte son.

use anyhow::Result;

use super::{PcmBuffer, TtsEngine, TtsParams, TtsSession};

pub struct SilentEngine;
struct SilentSession;

const RATE: u32 = 16_000;
/// Durée d'un caractère : de l'ordre d'une lecture à voix haute.
const PER_CHAR_MS: usize = 50;

impl TtsEngine for SilentEngine {
    fn name(&self) -> String {
        crate::tr!("silent", "silencieux").into()
    }
    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(SilentSession))
    }
    fn max_sessions(&self) -> usize {
        1
    }
}

impl TtsSession for SilentSession {
    fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
        let samples = text.chars().count().max(1) * PER_CHAR_MS * RATE as usize / 1000;
        Ok(PcmBuffer::new(vec![0.0; samples], RATE, 1))
    }
}
