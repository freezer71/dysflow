//! Ce que le binaire compile en C : Sonic, et rien d'autre. Avec la dictée
//! par sherpa-onnx, aussi le chemin où chercher ses bibliothèques.
//!
//! audio.cpp (Supertonic, MagpieTTS) en est sorti le 14 oct. 2026 : Kokoro
//! se calcule en MLX dans l'app et par ONNX Runtime au terminal.

fn main() {
    build_sonic();
    if std::env::var_os("CARGO_FEATURE_STT_SHERPA").is_some() {
        sherpa_rpath();
    }
}

/// La bibliothèque C de sherpa-onnx (et l'ONNX Runtime qu'elle charge) est
/// cherchée à côté du binaire, comme ONNX Runtime pour Kokoro. Les
/// `rustc-link-arg` de sherpa-onnx-sys ne valent que pour ses propres
/// cibles : sans cette ligne, le binaire n'a aucun chemin de recherche
/// (« no LC_RPATH's found » sous macOS). Rien sous Windows, où le dossier du
/// binaire passe en premier.
fn sherpa_rpath() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        Ok("linux") => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        _ => {}
    }
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
    let mut build = cc::Build::new();
    build.file("native/sonic_build.c").include("third_party/sonic").warnings(false);
    // Le niveau du profil (3 en release) ; en debug, au moins 2 : Sonic
    // tourne dans le fil audio, et un -O0 y ferait des trous.
    if matches!(std::env::var("OPT_LEVEL").as_deref(), Ok("0" | "1")) {
        build.opt_level(2);
    }
    build.compile("sonic");
}
