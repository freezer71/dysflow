//! La dictée : reconnaissance de la parole au terminal.

pub mod detector;
pub mod dictation;
pub mod file;
pub mod holder;
pub mod recognizer;
pub mod rolling;
pub mod segmenter;
pub mod wer;

#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
pub mod mlx;
#[cfg(feature = "stt-sherpa")]
pub mod sherpa;

use crate::models::ModelPackage;
use anyhow::{Context, Result};
use dictation::Engine;
use std::path::{Path, PathBuf};

/// Le modèle de Silero VAD que le paquet du terminal livre à côté du binaire
/// (`scripts/package.sh` ; empreinte épinglée dans
/// `scripts/modeles-epingles.txt`, celle de la porte).
pub const SILERO_FILE: &str = "silero_vad.onnx";
/// Sa taille : un fichier tronqué est refusé avant d'atteindre sherpa-onnx,
/// avec le nombre d'octets trouvés.
pub const SILERO_BYTES: u64 = 643_854;
/// Son empreinte SHA-256, celle de `silero_vad.onnx` dans
/// `scripts/modeles-epingles.txt` (un essai le vérifie) : un fichier de la bonne
/// taille mais d'un autre contenu est refusé aussi, avant sherpa-onnx.
pub const SILERO_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";
/// Vrai pour le binaire d'un paquet : `scripts/package.sh` le construit avec
/// `DYSFLOW_PACKAGE_BUILD=1`. Un paquet doit avoir `SILERO_FILE` ; un binaire
/// de développement (`cargo build`, les essais) peut s'en passer.
#[cfg_attr(not(feature = "stt-sherpa"), allow(dead_code))]
const PACKAGED: bool = option_env!("DYSFLOW_PACKAGE_BUILD").is_some();

/// Le détecteur de parole d'un moteur sherpa.
#[cfg_attr(not(feature = "stt-sherpa"), allow(dead_code))]
#[derive(Debug, PartialEq)]
enum Detector {
    Silero(PathBuf),
    Energy,
}

/// Le détecteur de parole des moteurs sherpa, choisi ici et seulement ici
/// (R46) : Silero VAD quand le paquet l'a (`silero` : `SILERO_FILE` à côté du
/// binaire), sinon le détecteur d'énergie. Un paquet (`packaged`) sans lui,
/// ou avec un fichier d'une autre taille ou d'une autre empreinte SHA-256,
/// est une installation abîmée : erreur, qui dit de réinstaller le paquet
/// entier.
///
/// La porte mesurée de la tâche 10 (D5, au calme) peut encore changer ce
/// choix : il ne tient qu'à cette fonction.
#[cfg_attr(not(feature = "stt-sherpa"), allow(dead_code))]
fn sherpa_detector(silero: Option<&Path>, packaged: bool) -> Result<Detector> {
    sherpa_detector_pinned(silero, packaged, SILERO_BYTES, SILERO_SHA256)
}

/// `sherpa_detector`, avec la taille et l'empreinte attendues en argument
/// (les essais en donnent d'autres que celles du vrai Silero).
#[cfg_attr(not(feature = "stt-sherpa"), allow(dead_code))]
fn sherpa_detector_pinned(
    silero: Option<&Path>,
    packaged: bool,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<Detector> {
    let reinstall = crate::tr!(
        "reinstall the whole Dysflow package",
        "réinstallez le paquet de Dysflow en entier"
    );
    let size = silero.and_then(|path| std::fs::metadata(path).ok().map(|m| (path, m.len())));
    match size {
        Some((path, bytes)) if bytes == expected_bytes => {
            let found = sha256_of(path)?;
            anyhow::ensure!(
                found == expected_sha256,
                crate::tr!(
                    en = "damaged installation: {} has SHA-256 {found} instead of {expected_sha256}; {reinstall}",
                    fr = "installation abîmée : {} a l'empreinte SHA-256 {found} au lieu de {expected_sha256} ; {reinstall}",
                    path.display()
                )
            );
            Ok(Detector::Silero(path.to_path_buf()))
        }
        Some((path, bytes)) => anyhow::bail!(crate::tr!(
            en = "damaged installation: {} is {bytes} bytes instead of {expected_bytes}; {reinstall}",
            fr = "installation abîmée : {} fait {bytes} octets au lieu de {expected_bytes} ; {reinstall}",
            path.display()
        )),
        None if packaged => anyhow::bail!(crate::tr!(
            en = "damaged installation: the speech detector {} is missing; {reinstall}",
            fr = "installation abîmée : le détecteur de parole {} manque ; {reinstall}",
            silero.unwrap_or(Path::new(SILERO_FILE)).display()
        )),
        None => Ok(Detector::Energy),
    }
}

/// L'empreinte SHA-256 de `path`, en hexadécimal minuscule.
#[cfg_attr(not(feature = "stt-sherpa"), allow(dead_code))]
fn sha256_of(path: &Path) -> Result<String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).with_context(|| {
        crate::tr!(en = "cannot read {}", fr = "lecture impossible : {}", path.display())
    })?;
    Ok(sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

/// Le reconnaisseur d'un paquet, pas encore chargé : **la seule liste** des
/// moteurs de dictée que le cœur sait calculer. Le terminal l'enveloppe de
/// son détecteur (`engine_for`), l'app d'un `Holder` gardé d'une dictée à
/// l'autre (`ffi/src/dictation.rs`). Un port de modèle de dictée y ajoute
/// son moteur.
pub fn recognizer_for(
    package: &ModelPackage,
    threads: u32,
) -> Result<std::sync::Arc<dyn recognizer::Recognizer>> {
    match package.runtime {
        #[cfg(feature = "stt-sherpa")]
        crate::models::Runtime::ParakeetSherpa => Ok(std::sync::Arc::new(
            sherpa::parakeet::ParakeetSherpa::new(threads),
        )),
        #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
        crate::models::Runtime::ParakeetMlx => {
            let _ = threads;
            Ok(std::sync::Arc::new(mlx::parakeet::ParakeetMlx::new()))
        }
        #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
        crate::models::Runtime::PhononMlx => {
            let _ = threads;
            Ok(std::sync::Arc::new(mlx::phonon::PhononMlx::new()))
        }
        #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
        crate::models::Runtime::WhisperMlx => {
            let _ = threads;
            Ok(std::sync::Arc::new(mlx::whisper::WhisperMlx::new()))
        }
        #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
        crate::models::Runtime::CanaryMlx => {
            let _ = threads;
            Ok(std::sync::Arc::new(mlx::canary::CanaryMlx::new()))
        }
        // Un moteur que ce binaire ne sait pas calculer : seul l'app, ou un
        // binaire construit avec la feature, le fait.
        runtime => {
            let _ = threads;
            anyhow::bail!(crate::tr!(
                en = "this build cannot run {} (engine {runtime:?})",
                fr = "ce binaire ne sait pas lancer {} (moteur {runtime:?})",
                package.display_name.get()
            ))
        }
    }
}

/// Le détecteur de parole d'un paquet : Silero ou l'énergie pour sherpa
/// (`sherpa_detector`, R46), l'énergie pour MLX (D5).
fn detector_for(package: &ModelPackage) -> Result<Box<dyn detector::SpeechDetector>> {
    match package.runtime {
        #[cfg(feature = "stt-sherpa")]
        crate::models::Runtime::ParakeetSherpa => {
            let silero = crate::tts::kokoro::phonemes::exe_dir().map(|dir| dir.join(SILERO_FILE));
            Ok(match sherpa_detector(silero.as_deref(), PACKAGED)? {
                Detector::Silero(model) => Box::new(sherpa::vad::SileroDetector::new(&model)?),
                Detector::Energy => Box::new(detector::EnergyDetector::new()),
            })
        }
        _ => Ok(Box::new(detector::EnergyDetector::new())),
    }
}

/// Le moteur de dictée d'un paquet installé, prêt à passer à
/// `Dictation::start` : son reconnaisseur (`recognizer_for`) et son
/// détecteur. Le reconnaisseur n'est pas chargé ici (`Dictation` le
/// charge) ; Silero VAD, petit, l'est (`sherpa_detector`), et son erreur
/// vient avant toute lecture.
pub fn engine_for(package: &ModelPackage, threads: u32) -> Result<Engine> {
    let recognizer = recognizer_for(package, threads)?;
    Ok(Engine::Segmented(recognizer, detector_for(package)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CATALOG;

    fn package(id: &str) -> &'static ModelPackage {
        CATALOG
            .iter()
            .find(|p| p.id == id)
            .expect("paquet du catalogue")
    }

    #[test]
    fn a_sherpa_package_gets_a_segmented_engine_where_the_build_can_run_it() {
        let engine = engine_for(package("parakeet_tdt_v3_int8"), 4);
        if cfg!(feature = "stt-sherpa") {
            assert!(matches!(engine, Ok(Engine::Segmented(..))));
        } else {
            let err = engine.err().expect("refusé sans stt-sherpa").to_string();
            assert!(err.contains("Parakeet"), "{err}");
        }
    }

    /// Un fichier de `bytes` octets, pour les essais du choix du détecteur.
    fn file_of(name: &str, bytes: u64) -> PathBuf {
        let path = crate::paths::test_scratch(name).join(SILERO_FILE);
        std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        path
    }

    /// L'empreinte de `file_of(_, SILERO_BYTES)` : 643 854 octets nuls.
    const ZEROS_SHA256: &str = "a634a455f2b22a67fefbdb1e442257ddef8a558ce3a512c426fbb02738494517";

    #[test]
    fn with_its_silero_model_a_sherpa_engine_detects_speech_with_silero() {
        let model = file_of("silero-present", SILERO_BYTES);
        for packaged in [true, false] {
            assert_eq!(
                sherpa_detector_pinned(Some(&model), packaged, SILERO_BYTES, ZEROS_SHA256).unwrap(),
                Detector::Silero(model.clone())
            );
        }
    }

    #[test]
    fn the_silero_hash_is_the_one_the_package_pins() {
        let pins = include_str!("../../scripts/modeles-epingles.txt");
        assert!(
            pins.lines().any(|line| line.starts_with(&format!("{SILERO_FILE} {SILERO_SHA256} "))),
            "scripts/modeles-epingles.txt"
        );
    }

    #[test]
    fn without_silero_a_development_build_detects_speech_by_energy() {
        let absent = crate::paths::test_scratch("silero-absent").join(SILERO_FILE);
        assert_eq!(sherpa_detector(Some(&absent), false).unwrap(), Detector::Energy);
        assert_eq!(sherpa_detector(None, false).unwrap(), Detector::Energy);
    }

    #[test]
    fn a_package_without_silero_is_a_damaged_installation_named_in_the_error() {
        let absent = crate::paths::test_scratch("silero-manquant").join(SILERO_FILE);
        let err = sherpa_detector(Some(&absent), true).unwrap_err().to_string();
        assert!(err.contains(&absent.display().to_string()), "{err}");
        assert!(err.contains("réinstallez le paquet") || err.contains("reinstall the whole"), "{err}");
        let err = sherpa_detector(None, true).unwrap_err().to_string();
        assert!(err.contains(SILERO_FILE), "{err}");
    }

    #[test]
    fn a_truncated_silero_model_is_refused_with_its_size() {
        let truncated = file_of("silero-tronque", SILERO_BYTES / 2);
        for packaged in [true, false] {
            let err = sherpa_detector(Some(&truncated), packaged).unwrap_err().to_string();
            assert!(err.contains(&(SILERO_BYTES / 2).to_string()), "{err}");
            assert!(err.contains(&SILERO_BYTES.to_string()), "{err}");
        }
    }

    /// Un faux Silero de la bonne taille, mais d'un autre contenu : refusé
    /// par son empreinte, avant sherpa-onnx (qui, lui, rendrait « Protobuf
    /// parsing failed », mesuré par
    /// `sherpa::vad::tests::a_garbage_silero_model_of_the_right_size_is_measured_in_a_child`).
    #[test]
    fn a_silero_model_of_the_right_size_but_another_content_is_refused_by_its_hash() {
        let garbage = file_of("silero-faux-contenu", SILERO_BYTES);
        for packaged in [true, false] {
            let err = sherpa_detector(Some(&garbage), packaged).unwrap_err().to_string();
            assert!(err.contains(&garbage.display().to_string()), "{err}");
            assert!(err.contains("SHA-256"), "{err}");
            assert!(err.contains("réinstallez le paquet") || err.contains("reinstall the whole"), "{err}");
        }
    }

    /// Parakeet MLX (v3 et v2), Phonon-2 et les trois Whisper : un
    /// reconnaisseur et un moteur là où le binaire a MLX ; refusés par leur
    /// nom ailleurs.
    #[test]
    fn parakeet_mlx_is_recognized_where_the_build_has_mlx_and_refused_by_name_elsewhere() {
        let mlx = cfg!(all(
            feature = "mlx",
            target_os = "macos",
            target_arch = "aarch64"
        ));
        for (id, engine_name) in [
            ("parakeet_tdt_v3", "ParakeetMlx"),
            ("parakeet_tdt_v2", "ParakeetMlx"),
            ("phonon_2", "PhononMlx"),
            ("whisper_tiny_4bit", "WhisperMlx"),
            ("whisper_small_4bit", "WhisperMlx"),
            ("whisper_large_v3_turbo_4bit", "WhisperMlx"),
            ("canary_1b_v2_q8", "CanaryMlx"),
        ] {
            let recognizer = recognizer_for(package(id), 4);
            let engine = engine_for(package(id), 4);
            if mlx {
                assert!(recognizer.is_ok(), "{id}");
                assert!(matches!(engine, Ok(Engine::Segmented(..))), "{id}");
            } else {
                for err in [
                    recognizer.err().map(|e| e.to_string()),
                    engine.err().map(|e| e.to_string()),
                ] {
                    let err = err.expect("MLX : pas ici");
                    assert!(err.contains(engine_name), "{err}");
                }
            }
        }
    }

    #[test]
    fn an_engine_this_binary_cannot_compute_is_refused_by_name() {
        let err = engine_for(package("nemotron_35_asr_streaming_8bit"), 4)
            .err()
            .expect("Nemotron MLX : pas porté")
            .to_string();
        assert!(err.contains("NemotronMlx"), "{err}");
    }
}
