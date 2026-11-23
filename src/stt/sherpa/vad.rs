//! Silero VAD par sherpa-onnx : le détecteur de parole de la dictée sous
//! Windows et Linux.

use crate::stt::detector::{SpeechDetector, FRAME};
use crate::stt::sherpa::{model_file, same_runtime, VAD_SAMPLE_RATE};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Seuil de Silero : une trame est de la parole au-delà de cette probabilité.
const THRESHOLD: f32 = 0.5;
/// Parole et silence minimaux de Silero, en secondes. Courts : la découpe
/// (`Segmenter`) fait le reste (silence de 13 trames, parole de 0,25 s) ;
/// une parole de plus de 0,1 s est entendue avant la fin de la marge de
/// 0,2 s que la découpe garde devant.
const MIN_SPEECH: f32 = 0.1;
const MIN_SILENCE: f32 = 0.1;
/// Au-delà, Silero coupe de lui-même ; la découpe coupe avant, à 12 s.
const MAX_SPEECH: f32 = 20.0;
/// Le tampon des segments de Silero, en secondes : chaque trame les vide.
const BUFFER_SECONDS: f32 = MAX_SPEECH + 10.0;

/// Silero VAD, trame par trame (`FRAME`, sa fenêtre à 16 kHz) : une trame est
/// de la parole quand Silero dit en entendre (`detected`).
pub struct SileroDetector {
    vad: sherpa_onnx::VoiceActivityDetector,
}

impl SileroDetector {
    /// Charge `model` (`silero_vad.onnx`), après la vérification d'ONNX
    /// Runtime (`same_runtime`).
    pub fn new(model: &Path) -> Result<Self> {
        Self::with_runtime(model, same_runtime)
    }

    /// `new`, la vérification d'ONNX Runtime passée en argument : les essais
    /// ne touchent pas à celle, gardée, du processus.
    fn with_runtime(model: &Path, runtime: fn() -> Result<String>) -> Result<Self> {
        runtime()?;
        let model = model_file(model)?;
        let config = sherpa_onnx::VadModelConfig {
            silero_vad: sherpa_onnx::SileroVadModelConfig {
                model: Some(model),
                threshold: THRESHOLD,
                min_silence_duration: MIN_SILENCE,
                min_speech_duration: MIN_SPEECH,
                window_size: FRAME as i32,
                max_speech_duration: MAX_SPEECH,
            },
            sample_rate: VAD_SAMPLE_RATE as i32,
            num_threads: 1,
            provider: Some("cpu".to_string()),
            ..Default::default()
        };
        let vad = sherpa_onnx::VoiceActivityDetector::create(&config, BUFFER_SECONDS).context(
            crate::tr!(
                "sherpa-onnx cannot create Silero VAD",
                "sherpa-onnx ne crée pas Silero VAD"
            ),
        )?;
        Ok(Self { vad })
    }
}

impl SpeechDetector for SileroDetector {
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
        self.vad.accept_waveform(frame);
        // Les segments que Silero garde ne servent pas : la découpe garde
        // l'audio elle-même.
        self.vad.clear();
        Ok(self.vad.detected())
    }

    fn reset(&mut self) {
        self.vad.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same() -> Result<String> {
        Ok("1.31.0".to_string())
    }

    #[test]
    fn a_missing_silero_model_is_named() {
        let model = crate::paths::test_scratch("sherpa-silero-absent").join("silero_vad.onnx");
        let err = SileroDetector::with_runtime(&model, same)
            .err()
            .expect("modèle absent")
            .to_string();
        assert!(err.contains("silero_vad.onnx"), "{err}");
    }

    #[test]
    fn two_different_runtimes_refuse_silero_before_loading() {
        let two =
            || crate::stt::sherpa::check_same_runtime("1.31.0", "1.28.2").map(|()| String::new());
        let model = crate::paths::test_scratch("sherpa-silero-jamais-cree").join("silero_vad.onnx");
        let err = SileroDetector::with_runtime(&model, two)
            .err()
            .expect("refusé")
            .to_string();
        assert!(
            err.contains("1.28.2") && !err.contains("silero_vad.onnx"),
            "{err}"
        );
    }
}
