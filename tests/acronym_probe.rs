//! Donne à entendre ce que le moteur fait d'un sigle, avant et après.
//!
//! Diagnostic, pas régression : la mesure d'un sigle est une oreille.
//!
//! ```sh
//! OUT=/tmp/sigles.wav cargo test --release --features audio-cpp \
//!     --test acronym_probe -- --ignored --nocapture
//! ```
#![cfg(feature = "audio-cpp")]

use sillon::text::{sanitize, verbalize};
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{EngineSettings, TtsEngine, TtsParams};

#[test]
#[ignore]
fn listen_to_the_acronyms() {
    let home = std::env::var("HOME").unwrap();
    let settings = EngineSettings {
        model_path: format!(
            "{home}/Library/Application Support/sillon/models/supertonic/supertonic-3-orig.gguf"
        ),
        family: "supertonic".to_string(),
        backend: String::new(),
        threads: 4,
        sessions: 1,
    };
    let engine = AudioCppEngine::load(&settings).expect("chargement");
    let mut session = engine.open_session().expect("session");
    let params = TtsParams { voice: "M5".to_string(), ..TtsParams::default() };

    let lines = [
        "Le montant HT est de 1 200 €, soit 1 440 € TTC après TVA.",
        "SIRET : 984 733 204 00015. SIREN : 984 733 204.",
        "Une réclamation peut être adressée à la CNIL, au titre du RGPD.",
        "Notre SAV répond par SMS. La SARL est inscrite au RCS de PARIS.",
    ];

    let mut all: Vec<f32> = Vec::new();
    let mut rate = 24_000;
    for line in lines {
        let clean = sanitize(line);
        for (label, text) in [("avant", clean.clone()), ("après", verbalize(&clean))] {
            println!("{label} : {text}");
            let pcm = session.synthesize(&text, &params).expect("synthèse");
            rate = pcm.sample_rate;
            all.extend(pcm.samples.iter().copied());
            all.extend(std::iter::repeat_n(0.0, (rate as usize) / 2));
        }
    }

    let out = std::env::var("OUT").expect("OUT=chemin.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&out, spec).expect("wav");
    for s in all {
        writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
    println!("\nécrit : {out}");
}
