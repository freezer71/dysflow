//! Dix cycles chargement → une phrase → déchargement, mémoire relevée à
//! chaque cycle. Répond à une question : la voix rend-elle sa mémoire ?
//!
//! ```bash
//! cargo run --release --features audio-cpp --example voice_cycles -- <modele.gguf> <famille>
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use sillon::config::Config;
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{Synthesizer, TtsEngine};

/// Mémoire résidente du processus, en Mo.
fn rss_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / 1024
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut config, _) = Config::load();
    if let (Some(path), Some(family)) = (args.next(), args.next()) {
        config.engine.model_path = path;
        config.engine.family = family;
    }
    println!("modèle : {} ({})", config.engine.model_path, config.engine.family);
    println!("départ : {} Mo", rss_mb());

    for cycle in 1..=10 {
        let t = Instant::now();
        let engine: Arc<dyn TtsEngine> =
            Arc::new(AudioCppEngine::load(&config.engine_settings()).expect("chargement"));
        let load = t.elapsed();
        let synth = Synthesizer::new(
            Arc::clone(&engine),
            config.tts_params(),
            config.sessions() as usize,
            config.engine.prefetch as usize,
        );
        synth.push("Les monades enchaînent des calculs qui portent un contexte.".into(), 0);
        synth.set_cursor(0);
        while synth.get(0).is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let first = t.elapsed();
        let loaded = rss_mb();
        drop(synth);
        drop(engine);
        std::thread::sleep(Duration::from_millis(500));
        println!(
            "cycle {cycle:2} : chargement {:.2} s · première phrase {:.2} s · chargé {loaded} Mo · relâché {} Mo",
            load.as_secs_f32(),
            first.as_secs_f32(),
            rss_mb()
        );
    }
}
