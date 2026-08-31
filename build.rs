//! Construction optionnelle d'audio.cpp et de son shim.
//!
//! Sans la feature `audio-cpp`, ce script ne fait rien : `cargo build` reste
//! une affaire de secondes et n'exige ni CMake ni arbre source vendorisé. Avec
//! la feature, audio.cpp est compilé — mais seulement les familles de modèles
//! demandées, ce qui rend le build tenable face aux 62 familles du projet.

fn main() {
    println!("cargo:rerun-if-changed=native/audiocpp_shim.cpp");
    println!("cargo:rerun-if-changed=native/audiocpp_shim.h");
    println!("cargo:rerun-if-changed=native/CMakeLists.txt");
    println!("cargo:rerun-if-env-changed=RS_READER_AUDIOCPP_DIR");
    println!("cargo:rerun-if-env-changed=RS_READER_TTS_FAMILIES");

    #[cfg(feature = "audio-cpp")]
    build_audio_cpp();

    // Le catalogue de modèles ne propose que des familles chargeables.
    #[cfg(not(feature = "audio-cpp"))]
    println!("cargo:rustc-env=RS_READER_COMPILED_FAMILIES=");
}

#[cfg(feature = "audio-cpp")]
fn build_audio_cpp() {
    use std::path::PathBuf;

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = std::env::var("RS_READER_AUDIOCPP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("vendor/audio.cpp"));

    if !vendor.join("CMakeLists.txt").exists() {
        panic!(
            "audio.cpp introuvable dans {}.\n\n\
             Récupérez-le puis relancez :\n  \
             git submodule add https://github.com/0xShug0/audio.cpp vendor/audio.cpp\n  \
             git submodule update --init --recursive\n\n\
             Ou indiquez un arbre existant : RS_READER_AUDIOCPP_DIR=/chemin/vers/audio.cpp\n\n\
             Sans la feature `audio-cpp`, le lecteur se construit sans rien de tout cela.",
            vendor.display()
        );
    }

    // Ne compiler que les familles utiles : « full » construirait 62 familles.
    // Chaque famille ajoutée allonge la compilation et grossit la bibliothèque ;
    // on ne compile que celles que le catalogue propose réellement.
    let families = std::env::var("RS_READER_TTS_FAMILIES")
        .unwrap_or_else(|_| "supertonic,magpie_tts".to_string());
    println!("cargo:rustc-env=RS_READER_COMPILED_FAMILIES={families}");

    let mut config = cmake::Config::new(manifest.join("native"));
    config
        .define("AUDIOCPP_DIR", vendor.to_str().unwrap())
        .define("AUDIOCPP_MODEL_SET", "custom")
        .define("AUDIOCPP_MODELS", &families)
        .define("ENGINE_BUILD_TESTS", "OFF")
        .define("ENGINE_BUILD_MODEL_TESTS", "OFF")
        .define("ENGINE_BUILD_EXAMPLES", "OFF")
        .define("ENGINE_BUILD_WARMBENCH", "OFF")
        .define("AUDIOCPP_BUILD_NATIVE_MODEL_MANAGER", "OFF")
        // CMake 4 refuse les `cmake_minimum_required` antérieurs à 3.5 que
        // portent encore certaines dépendances vendorisées.
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .profile("Release");

    if cfg!(target_os = "macos") {
        config.define("ENGINE_ENABLE_METAL", "ON");
        // OpenMP n'est pas fourni par AppleClang sans dépendance externe.
        config.define("ENGINE_ENABLE_OPENMP", "OFF");
    }

    let destination = config.build();

    let lib_dir = destination.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=audiocpp_shim");
    // Le binaire doit retrouver le shim sans variable d'environnement.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:audiocpp_lib={}", lib_dir.display());
}
