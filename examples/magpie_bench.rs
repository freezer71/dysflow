//! Le coût brut de Magpie sur cette machine : une phrase après l'autre dans
//! une session, puis le pool à une et à trois sessions.
//!
//! ```bash
//! cargo run --release --features audio-cpp --example magpie_bench
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use sillon::config::Config;
use sillon::doc::DocumentBuilder;
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{Synthesizer, TtsEngine};

const DEMO: &str = include_str!("../docs/demo.md");

fn main() {
    let (mut config, _) = Config::load();
    let base = dirs::data_local_dir().unwrap().join("sillon/models");
    config.engine.model_path = base.join("magpie_tts/magpie-tts-multilingual-357m-q8_0.gguf").to_string_lossy().into_owned();
    config.engine.family = "magpie_tts".to_string();
    config.voice.voice = sillon::models::default_voice("magpie_tts").to_string();
    let mut builder = DocumentBuilder::new(config.code_policy());
    for block in sillon::input::split_blocks(DEMO) {
        builder.push_block(&block);
    }
    let sentences: Vec<(String, u64)> = builder.document().sentences.iter().map(|s| (s.text.clone(), s.trailing_pause_ms)).collect();
    let engine: Arc<dyn TtsEngine> = Arc::new(AudioCppEngine::load(&config.engine_settings()).expect("chargement"));
    let params = config.tts_params();

    // Une session, une phrase après l'autre : le coût brut.
    let mut session = engine.open_session().expect("session");
    let mut total_audio = 0.0;
    let mut total_time = 0.0;
    for (i, (text, _)) in sentences.iter().take(6).enumerate() {
        let t = Instant::now();
        let pcm = session.synthesize(text, &params).expect("synthèse");
        let took = t.elapsed().as_secs_f32();
        let audio = pcm.duration.as_secs_f32();
        total_audio += audio;
        total_time += took;
        println!("série, phrase {} : {:.1} s de parole en {:.1} s (×{:.1}) — {}", i + 1, audio, took, took / audio.max(0.1), text.chars().take(40).collect::<String>());
    }
    println!("série : {:.1} s de parole en {:.1} s → facteur temps réel ×{:.2}", total_audio, total_time, total_time / total_audio);
    drop(session);

    for sessions in [1usize, 3] {
        let synth = Synthesizer::new(Arc::clone(&engine), params.clone(), sessions, 6);
        for (text, pause) in &sentences {
            synth.push(text.clone(), *pause);
        }
        synth.set_cursor(0);
        let t = Instant::now();
        let mut seen = vec![false; 6];
        let mut firsts = Vec::new();
        while seen.iter().any(|s| !s) && t.elapsed() < Duration::from_secs(120) {
            for id in 0..6 {
                if !seen[id] && synth.get(id).is_some() {
                    seen[id] = true;
                    firsts.push((id, t.elapsed().as_secs_f32()));
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let line: Vec<String> = firsts.iter().map(|(id, at)| format!("p{} à {:.1} s", id + 1, at)).collect();
        println!("pool à {sessions} session(s) : {}", line.join(", "));
        drop(synth);
    }
    std::process::exit(0);
}
