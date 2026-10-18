//! Ce que le binaire compile en C : Sonic, et rien d'autre.
//!
//! audio.cpp (Supertonic, MagpieTTS) en est sorti le 14 oct. 2026 : Kokoro
//! se calcule en MLX dans l'app et par ONNX Runtime au terminal.

fn main() {
    build_sonic();
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
