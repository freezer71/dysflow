//! Kokoro par ONNX Runtime, au terminal (Linux, et `sillon` sous macOS).
//!
//! `ort` en `load-dynamic` : la bibliothèque est choisie ici, et sa version
//! vérifiée avant le moindre appel. Le modèle `model_quantized.onnx` (int8)
//! prend `input_ids`, `style` [1, 256] et `speed`, et rend `waveform` et
//! `durations`.

use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::{KokoroBackend, KokoroRaw};
use crate::tts::Cancel;

pub const ONNX_FILE: &str = "model_quantized.onnx";
const MINIMUM: (u32, u32) = (1, 31);

#[cfg(target_os = "macos")]
const LIBRARY: &str = "libonnxruntime.dylib";
#[cfg(not(target_os = "macos"))]
const LIBRARY: &str = "libonnxruntime.so";

/// La bibliothèque ORT : `SILLON_ORT_LIB`, à côté du binaire (liens
/// symboliques résolus), dans un `lib/` voisin, sinon le nom nu (chemins de
/// recherche du système).
pub fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SILLON_ORT_LIB") {
        return explicit_library(path);
    }
    if let Some(dir) = super::phonemes::exe_dir() {
        for candidate in [dir.join(LIBRARY), dir.join("lib").join(LIBRARY), dir.join("../lib").join(LIBRARY)] {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(LIBRARY)
}

/// `SILLON_ORT_LIB` : un chemin relatif devient absolu (depuis le dossier
/// courant), sans quoi `version_of` et `ort`, qui le résout depuis le
/// binaire, pourraient charger deux fichiers différents ; un nom nu reste nu.
fn explicit_library(path: std::ffi::OsString) -> PathBuf {
    let path = PathBuf::from(path);
    if path.components().count() > 1 && !path.is_absolute() {
        if let Ok(absolute) = std::path::absolute(&path) {
            return absolute;
        }
    }
    path
}

#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "C" fn(u32) -> *const c_void,
    get_version_string: unsafe extern "C" fn() -> *const c_char,
}

/// La version d'une bibliothèque ORT, lue par `OrtGetApiBase`.
pub fn version_of(library: &Path) -> Result<String> {
    // SAFETY : on charge la bibliothèque et on appelle la seule fonction
    // d'entrée d'ORT, sans argument ; la chaîne rendue est statique.
    unsafe {
        let lib = libloading::Library::new(library)
            .map_err(|e| anyhow!("ONNX Runtime introuvable ({}) : {e}", library.display()))?;
        let base: libloading::Symbol<unsafe extern "C" fn() -> *const OrtApiBase> =
            lib.get(b"OrtGetApiBase").context("bibliothèque qui n'est pas ONNX Runtime")?;
        let base = base();
        anyhow::ensure!(!base.is_null(), "ONNX Runtime sans point d'entrée");
        let version = CStr::from_ptr(((*base).get_version_string)()).to_string_lossy().into_owned();
        Ok(version)
    }
}

/// Refuse un ORT plus ancien que 1.31, avec la version trouvée et la raison.
pub fn check_version(found: &str) -> Result<()> {
    let mut parts = found.trim().split('.').map(|p| p.parse::<u32>());
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        bail!("version d'ONNX Runtime illisible : {found}");
    };
    if (major, minor) < MINIMUM {
        bail!(
            "ONNX Runtime {found} refusé : il faut la {}.{} au moins (les versions antérieures sont 4 fois plus lentes en int8)",
            MINIMUM.0,
            MINIMUM.1
        );
    }
    Ok(())
}

/// Vérifie que Kokoro peut se charger : ONNX Runtime trouvé, assez récent et
/// chargé par `ort`, puis `sillon-espeak` lancé avec ses données sur un mot
/// de chaque langue. Le modèle, téléchargé à part, n'est pas demandé. Rend
/// une ligne par élément vérifié.
pub fn check_install() -> Result<String> {
    use super::phonemes::{data_dir, EspeakProcess, Phonemizer};

    let library = library_path();
    let version = version_of(&library)?;
    check_version(&version)?;
    let _ = ort::init_from(&library).map_err(|e| anyhow!("ONNX Runtime : {e}"))?.commit();
    let mut report = format!("ONNX Runtime {version} : {}\n", library.display());

    let mut espeak = EspeakProcess::locate()?;
    for (language, word) in [("fr", "bonjour"), ("en", "hello")] {
        let phonemes =
            espeak.phonemize(language, &[word.to_string()]).with_context(|| format!("sillon-espeak ({language})"))?;
        let Some(first) = phonemes.first().filter(|p| !p.trim().is_empty()) else {
            bail!("sillon-espeak ne rend aucun phonème pour « {word} » ({language})");
        };
        report.push_str(&format!("sillon-espeak ({language}) : {word} → {first}\n"));
    }
    espeak.stop();
    let data = match data_dir() {
        Some(dir) => dir.join("espeak-ng-data").display().to_string(),
        None => "celles du système".to_string(),
    };
    report.push_str(&format!("Données espeak-ng : {data}\n"));
    Ok(report)
}

/// Le calcul Kokoro sur processeur.
pub struct OrtBackend {
    pub(crate) threads: usize,
    session: Mutex<Option<Session>>,
}

impl OrtBackend {
    /// `threads` : `engine.threads`, borné à 4 (au-delà, rien à gagner et des
    /// pointes de charge).
    pub fn new(threads: u32) -> Self {
        Self { threads: threads.clamp(1, 4) as usize, session: Mutex::new(None) }
    }
}

impl KokoroBackend for OrtBackend {
    fn label(&self) -> &'static str {
        "int8"
    }

    fn load(&self, model_dir: &Path) -> Result<()> {
        let library = library_path();
        check_version(&version_of(&library)?)?;
        // `ort` charge la bibliothèque vérifiée, une fois par processus (un
        // second appel garde la première) ; pas de variable d'environnement
        // modifiée dans un processus à plusieurs fils.
        let _ = ort::init_from(&library).map_err(|e| anyhow!("ONNX Runtime : {e}"))?.commit();
        let session = Session::builder()
            .map_err(|e| anyhow!("ONNX Runtime : {e}"))?
            .with_intra_threads(self.threads)
            .map_err(|e| anyhow!("ONNX Runtime : {e}"))?
            .with_inter_threads(1)
            .map_err(|e| anyhow!("ONNX Runtime : {e}"))?
            .with_memory_pattern(false)
            .map_err(|e| anyhow!("ONNX Runtime : {e}"))?
            .commit_from_file(model_dir.join(ONNX_FILE))
            .map_err(|e| anyhow!("modèle Kokoro int8 ({}) : {e}", ONNX_FILE))?;
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
        Ok(())
    }

    fn warm_up(&self) -> Result<()> {
        self.infer(&[0, 43, 0], &[0.0; 256], &Cancel::never()).map(|_| ())
    }

    /// L'abandon n'est vérifié qu'avant l'appel (avant et après l'attente
    /// de la session) : un passage ORT ne s'interrompt pas (pas de point
    /// d'arrêt interne).
    fn infer(&self, ids: &[i64], style: &[f32], cancel: &Cancel) -> Result<KokoroRaw> {
        cancel.check()?;
        let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
        cancel.check()?;
        let session = guard.as_mut().context("modèle Kokoro non chargé")?;
        let outputs = session.run(ort::inputs![
            "input_ids" => Tensor::from_array(([1usize, ids.len()], ids.to_vec()))?,
            "style" => Tensor::from_array(([1usize, style.len()], style.to_vec()))?,
            "speed" => Tensor::from_array(([1usize], vec![1.0f32]))?,
        ])?;
        let (_, waveform) = outputs["waveform"].try_extract_tensor::<f32>()?;
        let (_, durations) = outputs["durations"].try_extract_tensor::<f32>()?;
        Ok(KokoroRaw {
            samples: waveform.to_vec(),
            durations: durations.iter().map(|d| d.round().max(0.0) as u32).collect(),
        })
    }

    fn release(&self) {
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_recent_onnx_runtime_is_accepted() {
        assert!(check_version("1.31.0").is_ok());
        assert!(check_version("1.32.1").is_ok());
        assert!(check_version("2.0.0").is_ok());
        let err = check_version("1.28.0").unwrap_err().to_string();
        assert!(err.contains("1.28.0") && err.contains("4 fois plus lent"), "{err}");
        assert!(check_version("n'importe quoi").is_err());
    }

    #[test]
    fn a_relative_library_path_is_made_absolute() {
        // `version_of` et `ort` (qui résout un chemin relatif depuis le
        // binaire) doivent charger le même fichier.
        let relative = explicit_library("lib/libonnxruntime.so".into());
        assert!(relative.is_absolute(), "{}", relative.display());
        assert!(relative.ends_with("lib/libonnxruntime.so"));
        assert_eq!(explicit_library("/opt/ort/libonnxruntime.so".into()), PathBuf::from("/opt/ort/libonnxruntime.so"));
        // Un nom nu reste nu : les chemins de recherche du système.
        assert_eq!(explicit_library("libonnxruntime.so".into()), PathBuf::from("libonnxruntime.so"));
    }

    #[test]
    fn at_most_four_threads() {
        assert_eq!(OrtBackend::new(8).threads, 4);
        assert_eq!(OrtBackend::new(2).threads, 2);
        assert_eq!(OrtBackend::new(0).threads, 1);
    }

    #[test]
    fn an_absent_library_is_named_in_the_error() {
        let err = version_of(Path::new("/nulle/part/libonnxruntime.so")).unwrap_err().to_string();
        assert!(err.contains("ONNX Runtime introuvable"), "{err}");
    }

    /// Avec ORT ≥ 1.31 et le paquet int8 :
    /// `SILLON_ORT_LIB=… SILLON_KOKORO_DIR=… cargo test --features kokoro-onnx --lib onnx -- --ignored`
    #[test]
    #[ignore]
    fn the_int8_model_speaks_with_durations() {
        let dir = std::path::PathBuf::from(std::env::var("SILLON_KOKORO_DIR").expect("SILLON_KOKORO_DIR"));
        let backend = OrtBackend::new(4);
        backend.load(&dir).unwrap();
        let raw = backend.infer(&[0, 44, 43, 54, 43, 4, 0], &[0.0; 256], &Cancel::never()).unwrap();
        crate::tts::kokoro::check(&raw, 7).unwrap();
        backend.release();
    }
}
