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
    println!("cargo:rerun-if-env-changed=SILLON_AUDIOCPP_DIR");
    println!("cargo:rerun-if-env-changed=SILLON_TTS_FAMILIES");

    build_sonic();

    #[cfg(feature = "audio-cpp")]
    build_audio_cpp();

    // Le catalogue de modèles ne propose que des familles chargeables.
    #[cfg(not(feature = "audio-cpp"))]
    println!("cargo:rustc-env=SILLON_COMPILED_FAMILIES=");
}

/// Sonic : la vitesse de lecture, appliquée au son sans changer la hauteur.
///
/// Deux fichiers C sans dépendance, compilés à chaque build — quelques
/// dixièmes de seconde, et la vitesse marche avec toutes les voix.
fn build_sonic() {
    println!("cargo:rerun-if-changed=third_party/sonic/sonic.c");
    println!("cargo:rerun-if-changed=third_party/sonic/sonic.h");
    println!("cargo:rerun-if-changed=native/sonic_build.c");
    // sonic.c passe par native/sonic_build.c, qui compte ses allocations.
    cc::Build::new()
        .file("native/sonic_build.c")
        .include("third_party/sonic")
        .opt_level(2)
        .warnings(false)
        .compile("sonic");
}

#[cfg(feature = "audio-cpp")]
fn build_audio_cpp() {
    use std::path::PathBuf;

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = std::env::var("SILLON_AUDIOCPP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("vendor/audio.cpp"));

    if !vendor.join("CMakeLists.txt").exists() {
        panic!(
            "audio.cpp introuvable dans {}.\n\n\
             Récupérez-le puis relancez :\n  \
             git submodule add https://github.com/0xShug0/audio.cpp vendor/audio.cpp\n  \
             git submodule update --init --recursive\n\n\
             Ou indiquez un arbre existant : SILLON_AUDIOCPP_DIR=/chemin/vers/audio.cpp\n\n\
             Sans la feature `audio-cpp`, le lecteur se construit sans rien de tout cela.",
            vendor.display()
        );
    }

    patch_gguf_bulk_read(&vendor);

    // Ne compiler que les familles utiles : « full » construirait 62 familles.
    // Chaque famille ajoutée allonge la compilation et grossit la bibliothèque ;
    // on ne compile que celles que le catalogue propose réellement.
    let families = std::env::var("SILLON_TTS_FAMILIES")
        .unwrap_or_else(|_| "supertonic,magpie_tts".to_string());
    println!("cargo:rustc-env=SILLON_COMPILED_FAMILIES={families}");

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
    //
    // Trois chemins, du plus portable au plus commode : à côté du binaire et
    // dans un `lib/` voisin, pour un binaire distribué ; puis l'arbre de
    // compilation, pour que `cargo run` marche sans rien copier. Sans les deux
    // premiers, déplacer le binaire le casse — et aucune distribution n'est
    // possible.
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../lib");
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:audiocpp_lib={}", lib_dir.display());
}

/// Corrige, dans le ggml vendu, la lecture élément par élément des tableaux.
///
/// Un GGUF d'audio.cpp embarque ses fichiers annexes en un tableau d'octets
/// de plusieurs dizaines de mégaoctets — 54 Mo pour Supertonic — que
/// `gguf_reader::read` lit par un `fread` verrouillé **par octet**. Trois
/// secondes par ouverture, et le modèle s'ouvre quatre fois au chargement. Lu
/// en bloc, le chargement tombe à un quart de seconde.
///
/// Appliqué ici, à la construction, parce que le sous-module n'est pas à
/// nous : le correctif tient en neuf lignes, et se réapplique tout seul après
/// un `git submodule update`. Le fichier garde ses fins de ligne CRLF, d'où
/// le travail sur les octets plutôt que sur du texte.
#[cfg(feature = "audio-cpp")]
fn patch_gguf_bulk_read(vendor: &std::path::Path) {
    let path = vendor.join("external/ggml/src/gguf.cpp");
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let marker = b"sillon: one fread";
    if find(&bytes, marker).is_some() {
        return;
    }
    for newline in ["\r\n", "\n"] {
        let old = [
            "        dst.resize(n);",
            "        for (size_t i = 0; i < dst.size(); ++i) {",
            "            if constexpr (std::is_same<T, bool>::value) {",
            "",
        ]
        .join(newline);
        let new = [
            "        dst.resize(n);",
            "        if constexpr (!std::is_same<T, bool>::value && !std::is_same<T, std::string>::value) {",
            "            // sillon: one fread instead of one per element. Plain arrays are",
            "            // contiguous on disk and in memory; a 54 MB uint8 blob otherwise",
            "            // costs 57 million locked fread calls.",
            "            const size_t size = n * sizeof(T);",
            "            const size_t nread = fread(dst.data(), 1, size, file);",
            "            nbytes_remain -= nread;",
            "            return nread == size;",
            "        }",
            "        for (size_t i = 0; i < dst.size(); ++i) {",
            "            if constexpr (std::is_same<T, bool>::value) {",
            "",
        ]
        .join(newline);
        if let Some(at) = find(&bytes, old.as_bytes()) {
            let mut patched = Vec::with_capacity(bytes.len() + new.len());
            patched.extend_from_slice(&bytes[..at]);
            patched.extend_from_slice(new.as_bytes());
            patched.extend_from_slice(&bytes[at + old.len()..]);
            std::fs::write(&path, patched)
                .unwrap_or_else(|e| panic!("correctif ggml : écriture de {} : {e}", path.display()));
            println!(
                "cargo:warning=correctif ggml appliqué (lecture en bloc des tableaux GGUF) : {}",
                path.display()
            );
            return;
        }
    }
    // Un ggml qui a changé n'est pas une erreur : le lecteur fonctionne, en
    // chargeant lentement. Le dire suffit.
    println!(
        "cargo:warning=gguf.cpp a changé, correctif de lecture en bloc non appliqué : \
         le chargement du modèle prendra quelques secondes de plus"
    );
}

#[cfg(feature = "audio-cpp")]
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
