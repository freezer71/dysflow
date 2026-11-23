//! Le détecteur de parole : trame par trame, sans modèle pour `EnergyDetector`.

use anyhow::{bail, Result};
use std::collections::VecDeque;

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
/// Plancher supposé tant que la fenêtre n'est pas pleine : -50 dBFS, une
/// pièce calme. Une dictée qui commence par de la parole est entendue tout de
/// suite ; le bruit d'une pièce plus calme que cela est appris tout de suite.
const PRIOR_FLOOR: f32 = 3.162_277_7e-3;
/// Longueur de la fenêtre du minimum : 375 trames, 12 s.
///
/// Le plancher est le plus petit RMS de la fenêtre. La parole a des creux
/// jusqu'au bruit (entre deux mots, deux syllabes) : en 12 s il y en a, le
/// minimum est le bruit, et ni une parole continue ni ses creux un peu
/// au-dessus du bruit ne le font monter. Un bruit constant n'en a pas : quand
/// il change de niveau, la fenêtre l'oublie en 12 s et le nouveau niveau
/// devient le plancher. Un fond qui monte lentement est suivi, avec ce retard.
const WINDOW: usize = 375;

/// Détecteur à énergie : une trame est de la parole quand son RMS dépasse
/// le plancher de bruit, le minimum du RMS des `WINDOW` dernières trames
/// (au plus `PRIOR_FLOOR` tant que la fenêtre se remplit). Une trame non
/// finie (NaN, infini) n'est pas de la parole et n'entre pas dans la fenêtre.
///
/// Limite : une parole continue de plus de 12 s, sans aucun creux, finit par
/// devenir le plancher.
#[derive(Default)]
pub struct EnergyDetector {
    window: VecDeque<f32>,
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
        if self.window.len() == WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(rms);
        let lowest = self.window.iter().copied().fold(f32::MAX, f32::min);
        let floor = if self.window.len() < WINDOW {
            lowest.min(PRIOR_FLOOR)
        } else {
            lowest
        };
        Ok(rms > (floor.max(MIN_FLOOR) * RATIO).max(MIN_SPEECH_RMS))
    }

    fn reset(&mut self) {
        self.window.clear();
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

    /// Compte les trames entendues comme parole.
    fn heard(det: &mut EnergyDetector, audio: &[f32]) -> usize {
        audio
            .chunks_exact(FRAME)
            .filter(|f| det.is_speech(f).unwrap())
            .count()
    }

    #[test]
    fn speech_over_noise_is_detected() {
        // Bruit à -45 dBFS, puis une parole à -35 dBFS (10 dB au-dessus),
        // puis le bruit seul.
        let mut det = EnergyDetector::new();
        let background = noise(dbfs(-45.0), 2 * 16_000, 2);
        let voice = noise(dbfs(-35.0), 16_000, 3);
        let after = noise(dbfs(-45.0), 2 * 16_000, 4);
        heard(&mut det, &background);
        assert_eq!(
            heard(&mut det, &voice),
            voice.len() / FRAME,
            "toute la parole est entendue"
        );
        let leftover = after
            .chunks_exact(FRAME)
            .skip(5)
            .filter(|f| det.is_speech(f).unwrap())
            .count();
        assert_eq!(leftover, 0, "le bruit qui suit n'en est pas");
    }

    #[test]
    fn ten_seconds_of_speech_with_dips_above_the_floor_are_all_heard() {
        // 2 s de bruit à -45 dBFS, puis 10 s de parole à -35 dBFS dont une
        // trame sur 4, 8 ou 16 retombe à -45, -42, -40 ou -38 dBFS (creux
        // entre deux syllabes, plus ou moins au-dessus du bruit).
        for dip in [-45.0f32, -42.0, -40.0, -38.0] {
            for every in [4usize, 8, 16] {
                let mut det = EnergyDetector::new();
                heard(&mut det, &noise(dbfs(-45.0), 2 * 16_000, 2));
                let mut audio = noise(dbfs(-35.0), 10 * 16_000, 3);
                let dips = noise(dbfs(dip), 10 * 16_000, 9);
                for k in (0..audio.len() / FRAME).step_by(every) {
                    audio[k * FRAME..(k + 1) * FRAME]
                        .copy_from_slice(&dips[k * FRAME..(k + 1) * FRAME]);
                }
                let loud: Vec<bool> = audio
                    .chunks_exact(FRAME)
                    .enumerate()
                    .map(|(i, f)| (det.is_speech(f).unwrap(), i % every != 0))
                    .filter(|(_, loud)| *loud)
                    .map(|(heard, _)| heard)
                    .collect();
                let missed = loud.iter().filter(|h| !**h).count();
                assert_eq!(
                    missed,
                    0,
                    "creux à {dip} dBFS toutes les {every} trames : {missed}/{} manquées",
                    loud.len()
                );
            }
        }
    }

    #[test]
    fn continuous_speech_over_noise_is_heard_for_ten_seconds() {
        for gain in [10.0f32, 15.0, 20.0] {
            let mut det = EnergyDetector::new();
            heard(&mut det, &noise(dbfs(-45.0), 2 * 16_000, 2));
            let voice = noise(dbfs(-45.0 + gain), 10 * 16_000, 3);
            assert_eq!(heard(&mut det, &voice), voice.len() / FRAME, "+{gain} dB");
        }
    }

    #[test]
    fn a_dictation_that_starts_with_speech_is_heard_from_the_first_frame() {
        // Parole à -35 dBFS dès la première trame, sans aucun bruit avant :
        // 10 s d'affilée, puis 1 s de bruit, puis 2 s de parole.
        let mut det = EnergyDetector::new();
        let first = noise(dbfs(-35.0), 10 * 16_000, 7);
        assert_eq!(heard(&mut det, &first), first.len() / FRAME);
        heard(&mut det, &noise(dbfs(-45.0), 16_000, 4));
        let again = noise(dbfs(-35.0), 2 * 16_000, 5);
        assert_eq!(heard(&mut det, &again), again.len() / FRAME);
        // Et après un reset, comme à chaque dictée.
        det.reset();
        let fresh = noise(dbfs(-35.0), 2 * 16_000, 8);
        assert_eq!(heard(&mut det, &fresh), fresh.len() / FRAME);
    }

    #[test]
    fn a_noise_step_stops_being_speech_within_thirteen_seconds() {
        // Le bruit passe de -45 à -35 dBFS et y reste : au plus 13 s plus
        // tard (la fenêtre de 12 s, plus une marge), ce n'est plus de la parole.
        let bound = 13 * 16_000 / FRAME;
        let mut det = EnergyDetector::new();
        heard(&mut det, &noise(dbfs(-45.0), 2 * 16_000, 2));
        let loud = noise(dbfs(-35.0), 40 * 16_000, 3);
        let verdicts: Vec<bool> = loud
            .chunks_exact(FRAME)
            .map(|f| det.is_speech(f).unwrap())
            .collect();
        assert!(verdicts[0], "le saut est d'abord entendu");
        assert!(
            verdicts[bound..].iter().all(|h| !h),
            "encore de la parole après 13 s"
        );
    }

    #[test]
    fn a_slowly_rising_background_is_followed() {
        // Le fond monte de -50 à -35 dBFS en 5 minutes (3 dB par minute) :
        // jamais de la parole, hors la prise du plancher des premières secondes.
        let mut det = EnergyDetector::new();
        let unit = noise(1.0, 300 * 16_000, 11);
        let frames = unit.len() / FRAME;
        let heard_frames = (0..frames)
            .filter(|&i| {
                let level = dbfs(-50.0 + 15.0 * i as f32 / frames as f32);
                let frame: Vec<f32> = unit[i * FRAME..(i + 1) * FRAME]
                    .iter()
                    .map(|x| x * level)
                    .collect();
                i >= 10 && det.is_speech(&frame).unwrap()
            })
            .count();
        assert_eq!(heard_frames, 0);
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
