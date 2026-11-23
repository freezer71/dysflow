//! sherpa-onnx au terminal, sur le même ONNX Runtime que Kokoro.
//!
//! La bibliothèque C de sherpa-onnx est liée dynamiquement (feature `shared`)
//! et charge `libonnxruntime` par son nom, à côté d'elle : le fichier livré à
//! côté du binaire, celui que `tts::kokoro::onnx` charge aussi. Une seule copie
//! d'ONNX Runtime dans le processus ; `examples/ort_single_copy.rs` le vérifie
//! (lot 1 de la spec des moteurs).

pub mod parakeet;
pub mod vad;

use crate::stt::segmenter::Segmenter;
use crate::tts::kokoro::onnx;
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::sync::OnceLock;

/// Fréquence attendue par Silero VAD et Parakeet.
pub const VAD_SAMPLE_RATE: u32 = 16_000;

/// La version d'ONNX Runtime que sherpa-onnx utilise réellement : sa
/// bibliothèque C la demande à l'`OrtGetApiBase` auquel elle est liée (la
/// copie statique qu'elle embarque, ou la bibliothèque partagée qu'elle
/// charge), pas une constante de compilation.
pub fn onnxruntime_version() -> Result<String> {
    let version = sherpa_onnx::onnxruntime_version().trim().to_string();
    if version.is_empty() {
        bail!(crate::tr!(
            "sherpa-onnx gives no ONNX Runtime version",
            "sherpa-onnx ne donne aucune version d'ONNX Runtime"
        ));
    }
    Ok(version)
}

/// Refuse deux copies d'ONNX Runtime de versions différentes : celle que
/// charge Kokoro et celle que sherpa-onnx utilise.
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

/// La version d'ONNX Runtime que Kokoro et sherpa-onnx partagent, vérifiée
/// une fois par processus (`check_same_runtime`) : la réponse, bonne ou non,
/// est gardée. Appelée avant le premier chargement de chaque modèle sherpa
/// (`ParakeetSherpa`, `SileroDetector`) ; la porte (`ort_single_copy`) la
/// réutilise pour son point 3.
pub fn same_runtime() -> Result<String> {
    static CHECKED: OnceLock<Result<String, String>> = OnceLock::new();
    checked_once(&CHECKED, || {
        let kokoro = onnx::version_of(&onnx::library_path())?;
        check_same_runtime(&kokoro, &onnxruntime_version()?)?;
        Ok(kokoro)
    })
}

/// `probe` lancée une fois pour `cell`, sa réponse rendue ensuite telle
/// quelle.
fn checked_once(
    cell: &OnceLock<Result<String, String>>,
    probe: impl FnOnce() -> Result<String>,
) -> Result<String> {
    cell.get_or_init(|| probe().map_err(|e| format!("{e:#}")))
        .clone()
        .map_err(|e| anyhow!(e))
}

/// Le chemin d'un fichier de modèle, en UTF-8 pour la bibliothèque C (qui le
/// reçoit ainsi sur tous les systèmes). Absent : l'erreur nomme le fichier.
pub(crate) fn model_file(path: &Path) -> Result<String> {
    anyhow::ensure!(
        path.is_file(),
        crate::tr!(
            en = "model file missing: {}",
            fr = "fichier du modèle absent : {}",
            path.display()
        )
    );
    path.to_str().map(str::to_string).with_context(|| {
        crate::tr!(
            en = "model path is not UTF-8: {}",
            fr = "chemin du modèle hors UTF-8 : {}",
            path.display()
        )
    })
}

/// Le nombre d'énoncés que la dictée trouve dans `samples` (mono, 16 kHz,
/// entre -1 et 1) : Silero VAD (`SileroDetector`) trame par trame, découpé
/// par `Segmenter`, comme une dictée.
pub fn vad_smoke(model: &Path, samples: &[f32]) -> Result<usize> {
    let mut segmenter = Segmenter::new(Box::new(vad::SileroDetector::new(model)?));
    let mut segments = segmenter.push(samples)?.len();
    segments += usize::from(segmenter.finish()?.is_some());
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
    fn the_runtime_is_checked_once_and_the_answer_kept() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cell = OnceLock::new();
        let calls = AtomicUsize::new(0);
        let probe = || {
            calls.fetch_add(1, Ordering::SeqCst);
            check_same_runtime("1.31.0", "1.28.2").map(|()| String::new())
        };
        let first = checked_once(&cell, probe).unwrap_err().to_string();
        let again = checked_once(&cell, probe).unwrap_err().to_string();
        assert_eq!(first, again);
        assert!(first.contains("1.28.2"), "{first}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "une seule vérification par processus"
        );
        let fine = OnceLock::new();
        assert_eq!(
            checked_once(&fine, || Ok("1.31.0".to_string())).unwrap(),
            "1.31.0"
        );
    }

    #[test]
    fn two_runtimes_are_refused_with_both_versions_named() {
        let err = check_same_runtime("1.31.0", "1.28.2")
            .unwrap_err()
            .to_string();
        assert!(err.contains("1.31.0") && err.contains("1.28.2"), "{err}");
    }
}
