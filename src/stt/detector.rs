//! Le détecteur de parole : trame par trame, sans modèle pour `EnergyDetector`.

use anyhow::{bail, Result};

/// Une trame : 512 échantillons à 16 kHz, soit 32 ms.
pub const FRAME: usize = 512;

pub trait SpeechDetector: Send {
    /// `Err` si la trame n'a pas exactement `FRAME` échantillons.
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool>;
    fn reset(&mut self);
}

/// Rapport entre le RMS d'une trame et le plancher de bruit à partir duquel
/// la trame est de la parole : 2,5 (environ 8 dB), sous les 10 dB d'une voix
/// ordinaire au-dessus du bruit, au-dessus des écarts d'un bruit blanc.
const RATIO: f32 = 2.5;
/// Plancher de bruit minimal : -80 dBFS, pour ne pas diviser par un silence
/// numérique.
const MIN_FLOOR: f32 = 1e-4;
/// Seuil absolu : -60 dBFS. Une trame plus faible n'est jamais de la parole.
const MIN_SPEECH_RMS: f32 = 1e-3;
/// Le plancher monte de 2 % par trame de bruit plus fort que lui.
const RISE_NOISE: f32 = 1.02;
/// Il ne monte que de 0,3 % par trame de parole : un ventilateur qui démarre
/// finit par être du bruit, une phrase ne devient pas du bruit.
const RISE_SPEECH: f32 = 1.003;
/// Il descend de moitié vers une trame plus faible.
const FALL: f32 = 0.5;

/// Détecteur à énergie : une trame est de la parole quand son RMS dépasse
/// le plancher de bruit suivi. Le plancher apprend de la première trame ; si
/// la dictée commence par de la parole, il redescend dès les trames calmes.
/// Une trame non finie (NaN, infini) n'est pas de la parole et ne touche pas
/// au plancher.
#[derive(Default)]
pub struct EnergyDetector {
    floor: Option<f32>,
}

impl EnergyDetector {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SpeechDetector for EnergyDetector {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
        if frame.len() != FRAME {
            bail!(
                "{}",
                crate::tr!(
                    en = "a detector frame is {FRAME} samples, got {}",
                    fr = "une trame du détecteur fait {FRAME} échantillons, reçu {}",
                    frame.len()
                )
            );
        }
        let rms = (frame.iter().map(|x| x * x).sum::<f32>() / FRAME as f32).sqrt();
        if !rms.is_finite() {
            return Ok(false);
        }
        let floor = self.floor.get_or_insert(rms.max(MIN_FLOOR));
        let speech = rms > (*floor * RATIO).max(MIN_SPEECH_RMS);
        *floor = if rms < *floor {
            *floor + (rms - *floor) * FALL
        } else if speech {
            *floor * RISE_SPEECH
        } else {
            (*floor * RISE_NOISE).min(rms)
        }
        .max(MIN_FLOOR);
        Ok(speech)
    }

    fn reset(&mut self) {
        self.floor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bruit blanc déterministe (xorshift64), ramené à un RMS exact.
    pub(crate) fn noise(rms: f32, len: usize, seed: u64) -> Vec<f32> {
        let mut state = seed.max(1);
        let mut out: Vec<f32> = (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
            })
            .collect();
        let measured = (out.iter().map(|x| x * x).sum::<f32>() / len as f32).sqrt();
        out.iter_mut().for_each(|x| *x *= rms / measured);
        out
    }

    fn dbfs(db: f32) -> f32 {
        10f32.powf(db / 20.0)
    }

    #[test]
    fn constant_noise_is_not_endless_speech() {
        // Bruit blanc à -45 dBFS pendant 60 s : jamais de la parole (hors
        // les toutes premières trames, le temps d'apprendre le plancher).
        let mut det = EnergyDetector::new();
        let audio = noise(dbfs(-45.0), 60 * 16_000, 1);
        let speech = audio
            .chunks_exact(FRAME)
            .skip(10)
            .filter(|f| det.is_speech(f).unwrap())
            .count();
        assert_eq!(speech, 0);
    }

    #[test]
    fn speech_over_noise_is_detected() {
        // Bruit à -45 dBFS, puis une parole à -35 dBFS (10 dB au-dessus),
        // puis le bruit seul.
        let mut det = EnergyDetector::new();
        let background = noise(dbfs(-45.0), 2 * 16_000, 2);
        let voice = noise(dbfs(-35.0), 16_000, 3);
        let after = noise(dbfs(-45.0), 2 * 16_000, 4);
        for f in background.chunks_exact(FRAME) {
            det.is_speech(f).unwrap();
        }
        let heard = voice
            .chunks_exact(FRAME)
            .filter(|f| det.is_speech(f).unwrap())
            .count();
        assert_eq!(heard, voice.len() / FRAME, "toute la parole est entendue");
        let leftover = after
            .chunks_exact(FRAME)
            .skip(5)
            .filter(|f| det.is_speech(f).unwrap())
            .count();
        assert_eq!(leftover, 0, "le bruit qui suit n'en est pas");
    }

    #[test]
    fn a_frame_of_the_wrong_length_is_an_error_not_a_panic() {
        let mut det = EnergyDetector::new();
        assert!(det.is_speech(&[]).is_err());
        assert!(det.is_speech(&[0.1; FRAME - 1]).is_err());
        assert!(det.is_speech(&[0.1; FRAME + 1]).is_err());
    }

    #[test]
    fn nan_samples_are_silence_and_do_not_poison_the_floor() {
        let mut det = EnergyDetector::new();
        let background = noise(dbfs(-45.0), 16_000, 5);
        for f in background.chunks_exact(FRAME) {
            det.is_speech(f).unwrap();
        }
        assert!(!det.is_speech(&[f32::NAN; FRAME]).unwrap());
        assert!(!det.is_speech(&[f32::INFINITY; FRAME]).unwrap());
        let voice = noise(dbfs(-35.0), FRAME, 6);
        assert!(det.is_speech(&voice).unwrap(), "le plancher est resté sain");
    }
}
