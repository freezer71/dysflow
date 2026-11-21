//! sherpa-onnx au terminal, sur le même ONNX Runtime que Kokoro.
//!
//! La bibliothèque C de sherpa-onnx est liée dynamiquement (feature `shared`)
//! et charge `libonnxruntime` par son nom, à côté d'elle : le fichier livré à
//! côté du binaire, celui que `tts::kokoro::onnx` charge aussi. Une seule copie
//! d'ONNX Runtime dans le processus ; `examples/ort_single_copy.rs` le vérifie
//! (lot 1 de la spec des moteurs).

use crate::tts::kokoro::onnx;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Fréquence attendue par Silero VAD.
pub const VAD_SAMPLE_RATE: u32 = 16_000;
/// Pas d'analyse de Silero VAD, en échantillons à 16 kHz.
const VAD_WINDOW: usize = 512;

/// La version d'ONNX Runtime que sherpa-onnx utilise réellement : sa
/// bibliothèque C la demande à l'`OrtGetApiBase` auquel elle est liée (la
/// copie statique qu'elle embarque, ou la bibliothèque partagée qu'elle
/// charge), pas une constante de compilation.
pub fn onnxruntime_version() -> Result<String> {
    let version = sherpa_onnx::onnxruntime_version().trim().to_string();
    if version.is_empty() {
        bail!(crate::tr!("sherpa-onnx gives no ONNX Runtime version", "sherpa-onnx ne donne aucune version d'ONNX Runtime"));
    }
    Ok(version)
}

/// Refuse deux copies d'ONNX Runtime de versions différentes : celle que
/// charge Kokoro et celle que sherpa-onnx utilise. Appelée avant le premier
/// chargement d'un modèle sherpa (`vad_smoke`).
pub fn check_same_runtime(kokoro: &str, sherpa: &str) -> Result<()> {
    anyhow::ensure!(
        kokoro == sherpa,
        crate::tr!(
            en = "two ONNX Runtime copies: {kokoro} (voice) and {sherpa} (dictation)",
            fr = "deux copies d'ONNX Runtime : {kokoro} (voix) et {sherpa} (dictée)"
        )
    );
    Ok(())
}

/// Le nombre de segments de parole que Silero VAD trouve dans `samples`
/// (mono, 16 kHz, entre -1 et 1). Réglages de la dictée par défaut : seuil
/// 0,5, un quart de seconde au moins de parole et de silence.
pub fn vad_smoke(model: &Path, samples: &[f32]) -> Result<usize> {
    check_same_runtime(&onnx::version_of(&onnx::library_path())?, &onnxruntime_version()?)?;
    anyhow::ensure!(
        model.is_file(),
        crate::tr!(en = "Silero VAD model missing: {}", fr = "modèle Silero VAD absent : {}", model.display())
    );
    let model = model.to_str().context(crate::tr!("Silero VAD model path is not UTF-8", "chemin du modèle Silero VAD hors UTF-8"))?;
    let config = sherpa_onnx::VadModelConfig {
        silero_vad: sherpa_onnx::SileroVadModelConfig {
            model: Some(model.to_string()),
            threshold: 0.5,
            min_silence_duration: 0.25,
            min_speech_duration: 0.25,
            window_size: VAD_WINDOW as i32,
            max_speech_duration: 20.0,
        },
        sample_rate: VAD_SAMPLE_RATE as i32,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        ..Default::default()
    };
    let buffer_seconds = (samples.len() as f32 / VAD_SAMPLE_RATE as f32).max(1.0) + 1.0;
    let vad = sherpa_onnx::VoiceActivityDetector::create(&config, buffer_seconds)
        .context(crate::tr!("sherpa-onnx cannot create Silero VAD", "sherpa-onnx ne crée pas Silero VAD"))?;
    let mut segments = 0;
    let mut drain = |vad: &sherpa_onnx::VoiceActivityDetector| {
        while !vad.is_empty() {
            segments += 1;
            vad.pop();
        }
    };
    for window in samples.chunks(VAD_WINDOW) {
        vad.accept_waveform(window);
        drain(&vad);
    }
    vad.flush();
    drain(&vad);
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_runtime_passes() {
        assert!(check_same_runtime("1.31.0", "1.31.0").is_ok());
    }

    #[test]
    fn two_runtimes_are_refused_with_both_versions_named() {
        let err = check_same_runtime("1.31.0", "1.28.2").unwrap_err().to_string();
        assert!(err.contains("1.31.0") && err.contains("1.28.2"), "{err}");
    }
}
