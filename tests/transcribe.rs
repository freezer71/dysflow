//! `dysflow --transcribe` et `--install-model`, par le vrai binaire : les
//! chemins d'erreur et la promesse de ne rien télécharger.
//!
//! Le chemin qui réussit n'est pas ici : il demande un reconnaisseur, et le
//! binaire n'en reçoit aucun de l'extérieur (pas d'accroche d'essai dans le
//! code livré). Il est essayé dans le processus, avec un reconnaisseur simulé
//! par `Dictation` (`stt::file`), et avec le vrai modèle par la porte
//! (`scripts/ort-gate.sh`).
//!
//! Sous Unix : le dossier des données se déplace avec `HOME` (et `XDG_*`) ;
//! sous Windows il vient du profil, qu'un essai ne déplace pas.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Un dossier de travail propre à l'essai, qui sert aussi de `HOME` : jamais
/// les dossiers de l'utilisateur.
struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("dysflow-transcribe-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Home(dir)
    }

    fn run(&self, lang: &str, args: &[&std::ffi::OsStr]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_dysflow"))
            .args(args)
            .env("HOME", &self.0)
            .env("XDG_DATA_HOME", self.0.join("data"))
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            .env("LC_ALL", lang)
            .env_remove("DYSFLOW_ORT_LIB")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("lancer dysflow")
    }

    /// Tous les fichiers sous ce dossier, hors les WAV que l'essai y a posés.
    fn files_written(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_none_or(|e| e != "wav") {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.0, &mut found);
        found
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_wav(path: &Path, rate: u32, channels: u16) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..3_200 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn os(s: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(s)
}

#[test]
fn a_missing_file_fails_and_names_it() {
    let home = Home::new("absent");
    let wav = home.0.join("rien.wav");
    let out = home.run("en_US.UTF-8", &[os("--transcribe"), wav.as_os_str()]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "stdout : {}", text(&out.stdout));
    let err = text(&out.stderr);
    assert!(
        err.contains("file not found") && err.contains("rien.wav"),
        "{err}"
    );
}

#[test]
fn a_wav_that_is_not_16k_mono_fails_with_the_ffmpeg_command() {
    let home = Home::new("format");
    let wav = home.0.join("Mes notes").join("l'été.wav");
    write_wav(&wav, 44_100, 2);
    let out = home.run("en_US.UTF-8", &[os("--transcribe"), wav.as_os_str()]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = text(&out.stderr);
    let converted = wav.with_file_name("l'été-16k.wav");
    let command = format!(
        "ffmpeg -i '{}' -ar 16000 -ac 1 '{}'",
        wav.display().to_string().replace('\'', r"'\''"),
        converted.display().to_string().replace('\'', r"'\''"),
    );
    assert!(err.contains(&command), "{err}\nattendu : {command}");
    assert!(err.contains("44100 Hz, 2 channel"), "{err}");
    // Rien n'est converti à la place de l'utilisateur.
    assert!(!converted.exists());
}

#[test]
fn the_conversion_message_follows_the_interface_language() {
    let home = Home::new("fr");
    let wav = home.0.join("a.wav");
    write_wav(&wav, 8_000, 1);
    let out = home.run("fr_FR.UTF-8", &[os("--transcribe"), wav.as_os_str()]);
    let err = text(&out.stderr);
    assert!(
        err.contains("pas un WAV mono à 16 kHz") && err.contains("ffmpeg -i"),
        "{err}"
    );
}

#[test]
fn a_missing_model_fails_naming_install_model_and_downloads_nothing() {
    let home = Home::new("modele");
    let wav = home.0.join("ok.wav");
    write_wav(&wav, 16_000, 1);
    let out = home.run(
        "en_US.UTF-8",
        &[
            os("--transcribe"),
            wav.as_os_str(),
            os("--dictation-model"),
            os("parakeet_tdt_v3_int8"),
        ],
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "stdout : {}", text(&out.stdout));
    let err = text(&out.stderr);
    // Sans `stt-sherpa`, le binaire ne sait pas lancer ce modèle : il le dit,
    // sans renvoyer à `--install-model`, qui le refuserait aussi.
    if cfg!(feature = "stt-sherpa") {
        assert!(
            err.contains("--install-model parakeet_tdt_v3_int8"),
            "{err}"
        );
    } else {
        assert!(
            err.contains("cannot run") && !err.contains("--install-model"),
            "{err}"
        );
    }
    // Aucun fichier (modèle, `.part`) n'est apparu nulle part sous HOME.
    assert_eq!(home.files_written(), Vec::<PathBuf>::new());
}

#[test]
fn the_default_model_is_named_too_when_this_build_has_one() {
    let home = Home::new("defaut");
    let wav = home.0.join("ok.wav");
    write_wav(&wav, 16_000, 1);
    let out = home.run("en_US.UTF-8", &[os("--transcribe"), wav.as_os_str()]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    // Avec `stt-sherpa`, le modèle par défaut manque ; sans, le binaire n'a
    // aucun moteur de dictée, et le dit.
    assert!(
        err.contains("--install-model parakeet_tdt_v3_int8") || err.contains("stt-sherpa"),
        "{err}"
    );
    assert_eq!(home.files_written(), Vec::<PathBuf>::new());
}

/// « Élodie Martin » composé (NFC) puis décomposé (NFD) : le fichier est lu
/// (l'erreur est celle du modèle, pas du fichier).
#[test]
fn an_accented_path_is_accepted_in_nfc_and_nfd() {
    for (kind, name) in [
        ("nfc", "\u{c9}lodie Martin"),
        ("nfd", "E\u{301}lodie Martin"),
    ] {
        let home = Home::new(&format!("accents-{kind}"));
        let wav = home.0.join(name).join("dictée.wav");
        write_wav(&wav, 16_000, 1);
        let out = home.run(
            "en_US.UTF-8",
            &[
                os("--transcribe"),
                wav.as_os_str(),
                os("--dictation-model"),
                os("parakeet_tdt_v3_int8"),
            ],
        );
        let err = text(&out.stderr);
        assert!(!out.status.success());
        // L'erreur est celle du modèle (à installer, ou que le binaire ne sait
        // pas lancer), pas celle du fichier.
        assert!(
            err.contains("--install-model parakeet_tdt_v3_int8") || err.contains("cannot run"),
            "{kind} : {err}"
        );
        assert!(
            !err.contains("file not found") && !err.contains("not a 16 kHz"),
            "{kind} : {err}"
        );
    }
}

#[test]
fn install_model_refuses_an_unknown_id_without_downloading() {
    let home = Home::new("install");
    let out = home.run("en_US.UTF-8", &[os("--install-model"), os("pas-un-modele")]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        text(&out.stderr).contains("pas-un-modele"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(home.files_written(), Vec::<PathBuf>::new());
}

#[test]
fn the_dictation_options_need_transcribe() {
    let home = Home::new("seul");
    let out = home.run("en_US.UTF-8", &[os("--dictation-language"), os("fr")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("--transcribe"),
        "{}",
        text(&out.stderr)
    );
}
