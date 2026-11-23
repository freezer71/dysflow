//! La porte du lot 1 (spec des moteurs, « Une seule copie d'ONNX Runtime ») :
//! Kokoro (`ort`) et sherpa-onnx partagent, dans un seul processus, le même
//! ONNX Runtime 1.31.0.
//!
//!   ./scripts/ort-gate.sh      # compile, assemble target/ort-gate/, lance
//!   target/ort-gate/ort_single_copy
//!
//! Le binaire se lance depuis un dossier qui a la forme du paquet : à côté de
//! lui, ONNX Runtime et la bibliothèque C de sherpa-onnx, et les modèles dans
//! `models/` (`kokoro/model_quantized.onnx`, `kokoro/config.json`,
//! `silero_vad.onnx`), ou là où le disent `DYSFLOW_KOKORO_DIR` et
//! `DYSFLOW_SILERO_VAD`. Dans l'ordre :
//!
//! 1. Kokoro int8 se charge par `OrtBackend` et dit « Bonjour. » ;
//! 2. Silero VAD, par sherpa-onnx (`SileroDetector`, découpé comme une
//!    dictée), trouve exactement un énoncé dans 3 s d'audio : 1 s de silence,
//!    1 s de la sortie de Kokoro, 1 s de silence ;
//! 3. la version de l'ORT que charge Kokoro, celle que sherpa-onnx utilise, et
//!    1.31.0 sont égales (`sherpa::same_runtime`, la vérification que chaque
//!    modèle sherpa fait avant de se charger) ;
//! 4. une seule copie d'ONNX Runtime est chargée dans le processus : une
//!    seule image (bibliothèque ou binaire) exporte `OrtGetApiBase`.
//!
//! Sort 0 si tout passe ; sinon 1, avec la raison de chaque échec.

use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use dysflow::stt::sherpa::{self, VAD_SAMPLE_RATE};
use dysflow::tts::kokoro::onnx::{self, OrtBackend};
use dysflow::tts::kokoro::voices::load_voice;
use dysflow::tts::kokoro::{KokoroBackend, Vocab, SAMPLE_RATE};
use dysflow::tts::Cancel;

/// La version qu'on livre (scripts/package.sh) : les deux côtés doivent
/// l'avoir.
const EXPECTED: &str = "1.31.0";
/// Les phonèmes de « Bonjour. » que rend dysflow-espeak (espeak-ng, fr-fr) :
/// fixés ici, la porte porte sur ONNX Runtime, pas sur la phonétisation.
const BONJOUR: &str = "bɔ̃ʒˈuʁ.";
/// La voix livrée dans le binaire : rien à télécharger de plus.
const VOICE: &str = "p-fenrir60";

fn main() -> ExitCode {
    // Avant le moindre fil : ONNX Runtime n'envoie rien à Microsoft.
    onnx::disable_telemetry();
    let mut failures = Vec::new();
    match run(&mut failures) {
        Ok(()) if failures.is_empty() => {
            println!("PORTE : une seule copie d'ONNX Runtime {EXPECTED}, partagée par Kokoro et sherpa-onnx");
            ExitCode::SUCCESS
        }
        Ok(()) => {
            for failure in &failures {
                eprintln!("ÉCHEC : {failure}");
            }
            ExitCode::FAILURE
        }
        Err(err) => {
            for failure in &failures {
                eprintln!("ÉCHEC : {failure}");
            }
            eprintln!("ÉCHEC : {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// `name`, sinon `models/<beside>` à côté du binaire.
fn model_path(name: &str, beside: &str) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(name) {
        return Ok(PathBuf::from(path));
    }
    let dir = dysflow::tts::kokoro::phonemes::exe_dir().context("dossier du binaire introuvable")?;
    Ok(dir.join("models").join(beside))
}

fn run(failures: &mut Vec<String>) -> Result<()> {
    let kokoro_dir = model_path("DYSFLOW_KOKORO_DIR", "kokoro")?;
    let vad_model = model_path("DYSFLOW_SILERO_VAD", "silero_vad.onnx")?;

    // 1. Kokoro int8 dit « Bonjour. ».
    let backend = OrtBackend::new(2);
    backend.load(&kokoro_dir).context("Kokoro int8")?;
    let vocab = Vocab::from_config(&kokoro_dir.join("config.json"))?;
    let (_, ids) = vocab.encode(BONJOUR);
    let tokens: Vec<i64> = std::iter::once(0).chain(ids).chain(std::iter::once(0)).collect();
    let voice = load_voice(&kokoro_dir, VOICE)?;
    let raw = backend.infer(&tokens, voice.row(BONJOUR.chars().count()), &Cancel::never()).context("Kokoro int8")?;
    let peak = raw.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    println!(
        "1. Kokoro int8 : « Bonjour. », {:.2} s d'audio, crête {peak:.2}",
        raw.samples.len() as f32 / SAMPLE_RATE as f32
    );
    if peak < 0.05 {
        failures.push(format!("Kokoro rend du silence (crête {peak:.3})"));
    }

    // 2. Silero VAD : un segment, et un seul.
    let second = VAD_SAMPLE_RATE as usize;
    let mut speech = resample(&raw.samples, SAMPLE_RATE, VAD_SAMPLE_RATE);
    speech.resize(second, 0.0);
    let audio: Vec<f32> = std::iter::repeat_n(0.0, second).chain(speech).chain(std::iter::repeat_n(0.0, second)).collect();
    let segments = sherpa::vad_smoke(&vad_model, &audio)?;
    println!("2. Silero VAD (sherpa-onnx) : {segments} énoncé(s) dans 3 s");
    if segments != 1 {
        failures.push(format!("Silero VAD trouve {segments} énoncé(s), 1 attendu"));
    }

    // 3. Les deux côtés ont le même ORT, celui qu'on livre : la vérification
    // que les modèles sherpa font avant de se charger, gardée pour le
    // processus (`sherpa::same_runtime`).
    let library = onnx::library_path();
    let sherpa_version = sherpa::onnxruntime_version()?;
    println!("3. ONNX Runtime : Kokoro {} ({}), sherpa-onnx {sherpa_version}", onnx::version_of(&library)?, library.display());
    match sherpa::same_runtime() {
        Ok(version) if version == EXPECTED => {}
        Ok(version) => failures.push(format!("ONNX Runtime {version} partagé, {EXPECTED} attendue")),
        Err(err) => failures.push(format!("{err:#} ; {EXPECTED} attendue")),
    }

    // 4. Une seule copie d'ONNX Runtime chargée.
    let loaded = loaded_onnxruntime()?;
    println!("4. Copies d'ONNX Runtime chargées (images qui exportent OrtGetApiBase) : {}", loaded.len());
    for path in &loaded {
        println!("   {}", path.display());
    }
    if loaded.len() != 1 {
        failures.push(format!("{} copies d'ONNX Runtime chargées, 1 attendue", loaded.len()));
    }
    backend.release();
    Ok(())
}

/// Rééchantillonnage linéaire : assez pour un détecteur de parole.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    let len = (samples.len() as u64 * to as u64 / from as u64) as usize;
    let step = from as f64 / to as f64;
    (0..len)
        .map(|i| {
            let at = i as f64 * step;
            let left = at.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let t = (at - left as f64) as f32;
            samples[left] * (1.0 - t) + samples[right] * t
        })
        .collect()
}

/// Linux : chaque fichier projeté en mémoire (`/proc/self/maps`), ouvert
/// sans le charger de nouveau ; une copie d'ONNX Runtime est l'objet qui
/// définit l'`OrtGetApiBase` trouvé (`dladdr`), compté une fois. Le nom du
/// fichier n'y est pour rien : une copie renommée compte aussi.
///
/// Limite : un ORT lié statiquement dans un exécutable ELF sans `-rdynamic`
/// n'exporte pas `OrtGetApiBase` et n'est pas compté ; une telle copie n'est
/// attrapée que par la comparaison des versions, si la sienne diffère.
#[cfg(target_os = "linux")]
fn loaded_onnxruntime() -> Result<Vec<PathBuf>> {
    use std::collections::BTreeMap;
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    let maps = std::fs::read_to_string("/proc/self/maps").context("/proc/self/maps")?;
    let mut files: Vec<String> = Vec::new();
    for line in maps.lines() {
        // adresses droits décalage périphérique inœud chemin
        let Some(path) = line.splitn(6, ' ').nth(5).map(str::trim) else { continue };
        if path.starts_with('/') && !files.iter().any(|f| f == path) {
            files.push(path.to_string());
        }
    }
    let mut copies = BTreeMap::new();
    for file in files {
        let Ok(name) = CString::new(Path::new(&file).as_os_str().as_bytes()) else { continue };
        // SAFETY : RTLD_NOLOAD ne charge rien, il rend seulement un objet
        // déjà là (refermé aussitôt) ; dladdr lit une adresse de cet objet.
        unsafe {
            let handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY);
            if handle.is_null() {
                continue;
            }
            let symbol = libc::dlsym(handle, c"OrtGetApiBase".as_ptr());
            let mut info: libc::Dl_info = std::mem::zeroed();
            if !symbol.is_null() && libc::dladdr(symbol, &mut info) != 0 && !info.dli_fname.is_null() {
                let defining = PathBuf::from(CStr::from_ptr(info.dli_fname).to_string_lossy().into_owned());
                copies.entry(info.dli_fbase as usize).or_insert(defining);
            }
            libc::dlclose(handle);
        }
    }
    Ok(copies.into_values().collect())
}

/// macOS : chaque image que dyld a chargée, binaire compris ; une copie
/// d'ONNX Runtime est l'image qui exporte elle-même `OrtGetApiBase`
/// (RTLD_FIRST : l'image seule, pas ses dépendances). Une copie statique
/// dans le binaire compte aussi, un binaire macOS exportant ses symboles.
#[cfg(target_os = "macos")]
fn loaded_onnxruntime() -> Result<Vec<PathBuf>> {
    use std::ffi::{c_char, CStr};
    extern "C" {
        fn _dyld_image_count() -> u32;
        fn _dyld_get_image_name(index: u32) -> *const c_char;
    }
    let mut found = Vec::new();
    // SAFETY : fonctions de dyld sans précondition ; un nom nul (image
    // déchargée entre-temps) est sauté. RTLD_NOLOAD ne charge rien.
    unsafe {
        for index in 0.._dyld_image_count() {
            let name = _dyld_get_image_name(index);
            if name.is_null() {
                continue;
            }
            let handle = libc::dlopen(name, libc::RTLD_NOLOAD | libc::RTLD_FIRST | libc::RTLD_LAZY);
            if handle.is_null() {
                continue;
            }
            if !libc::dlsym(handle, c"OrtGetApiBase".as_ptr()).is_null() {
                found.push(PathBuf::from(CStr::from_ptr(name).to_string_lossy().into_owned()));
            }
            libc::dlclose(handle);
        }
    }
    Ok(found)
}

/// Windows : les modules du processus (`EnumProcessModules`) qui exportent
/// `OrtGetApiBase`, quel que soit leur nom.
///
/// Limite : un ORT lié statiquement dans une DLL n'exporte pas
/// `OrtGetApiBase` et n'est pas compté ; une telle copie n'est attrapée que
/// par la comparaison des versions, si la sienne diffère.
#[cfg(windows)]
fn loaded_onnxruntime() -> Result<Vec<PathBuf>> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32EnumProcessModules(process: *mut c_void, modules: *mut *mut c_void, bytes: u32, needed: *mut u32) -> i32;
        fn K32GetModuleFileNameExW(process: *mut c_void, module: *mut c_void, name: *mut u16, size: u32) -> u32;
        fn GetProcAddress(module: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void;
    }
    // SAFETY : tampons dimensionnés par l'appel précédent ; le pseudo-handle
    // du processus courant n'a pas à être fermé ; les modules restent chargés
    // (rien ne les décharge pendant la porte).
    unsafe {
        let process = GetCurrentProcess();
        let mut modules: Vec<*mut c_void> = vec![std::ptr::null_mut(); 1024];
        loop {
            let bytes = (modules.len() * std::mem::size_of::<*mut c_void>()) as u32;
            let mut needed = 0u32;
            anyhow::ensure!(
                K32EnumProcessModules(process, modules.as_mut_ptr(), bytes, &mut needed) != 0,
                "EnumProcessModules : {}",
                std::io::Error::last_os_error()
            );
            let count = needed as usize / std::mem::size_of::<*mut c_void>();
            if needed <= bytes {
                modules.truncate(count);
                break;
            }
            modules.resize(count, std::ptr::null_mut());
        }
        let mut found = Vec::new();
        for module in modules {
            if GetProcAddress(module, c"OrtGetApiBase".as_ptr()).is_null() {
                continue;
            }
            let mut name = vec![0u16; 32_768];
            let len = K32GetModuleFileNameExW(process, module, name.as_mut_ptr(), name.len() as u32) as usize;
            found.push(PathBuf::from(std::ffi::OsString::from_wide(&name[..len])));
        }
        Ok(found)
    }
}
