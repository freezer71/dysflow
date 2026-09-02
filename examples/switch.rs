//! Mesure un changement de modèle tel que « Utiliser » le fait : le nouveau
//! moteur se charge pendant que l'ancien finit ses phrases en cours.
//!
//! ```bash
//! cargo run --release --features audio-cpp --example switch
//! ```

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sillon::config::Config;
use sillon::doc::DocumentBuilder;
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{Synthesizer, TtsEngine, TtsParams};

const DEMO: &str = include_str!("../docs/demo.md");

fn sentences(config: &Config) -> Vec<(String, u64)> {
    let mut builder = DocumentBuilder::new(config.code_policy());
    for block in sillon::input::split_blocks(DEMO) {
        builder.push_block(&block);
    }
    builder
        .document()
        .sentences
        .iter()
        .map(|s| (s.text.clone(), s.trailing_pause_ms))
        .collect()
}

/// Charge un modèle directement, sans thread : le temps brut.
fn load(config: &Config, path: &str, family: &str) -> Arc<dyn TtsEngine> {
    let mut config = config.clone();
    config.engine.model_path = path.to_string();
    config.engine.family = family.to_string();
    let t = Instant::now();
    let engine = AudioCppEngine::load(&config.engine_settings()).expect("chargement");
    println!("chargement de {family} : {:.2} s", t.elapsed().as_secs_f32());
    Arc::new(engine)
}

fn synth(config: &Config, params: TtsParams, engine: Arc<dyn TtsEngine>, sentences: &[(String, u64)]) -> Synthesizer {
    let synth = Synthesizer::new(engine, params, config.sessions() as usize, config.engine.prefetch as usize);
    for (text, pause) in sentences {
        synth.push(text.clone(), *pause);
    }
    synth.set_cursor(0);
    synth
}

/// Attend `n` phrases prêtes, et dit combien de temps ça a pris.
fn wait_ready(synth: &Synthesizer, n: usize, since: Instant, label: &str) {
    loop {
        let stats = synth.stats();
        if stats.ready >= n || stats.failed > 0 {
            println!(
                "{label} : {} prête(s), {} en cours, {} refusée(s) — {:.2} s",
                stats.ready,
                stats.in_flight,
                stats.failed,
                since.elapsed().as_secs_f32()
            );
            return;
        }
        if since.elapsed() > Duration::from_secs(240) {
            println!("{label} : abandon après 240 s ({stats:?})");
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn main() {
    let (config, _) = Config::load();
    let sentences = sentences(&config);
    let base = dirs::data_local_dir().unwrap().join("sillon/models");
    let supertonic = base.join("supertonic/supertonic-3-orig.gguf");
    let magpie = base.join("magpie_tts/magpie-tts-multilingual-357m-q8_0.gguf");
    let reference = std::env::temp_dir().join("sillon-reference.wav");
    println!("sessions = {}, prefetch = {}, {} phrases", config.sessions(), config.engine.prefetch, sentences.len());

    // 1. Supertonic seul : chargement, première phrase, et un WAV de référence.
    let engine_a = load(&config, supertonic.to_str().unwrap(), "supertonic");
    {
        let mut session = engine_a.open_session().expect("session");
        let t = Instant::now();
        let pcm = session.synthesize("Bonjour, voici une voix de référence pour le clonage.", &config.tts_params()).expect("synthèse");
        println!("supertonic, une phrase dans une session neuve : {:.2} s", t.elapsed().as_secs_f32());
        let spec = hound::WavSpec { channels: pcm.channels, sample_rate: pcm.sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(&reference, spec).expect("wav");
        for s in pcm.samples.iter() {
            writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
        }
        writer.finalize().unwrap();
    }
    let t = Instant::now();
    let a = synth(&config, config.tts_params(), engine_a, &sentences);
    wait_ready(&a, 1, t, "A supertonic, première phrase");
    wait_ready(&a, 6, t, "A supertonic, six phrases");

    // 2. Magpie avec référence, pendant que Supertonic tourne encore.
    let mut params_magpie = config.tts_params();
    params_magpie.voice = match std::env::var("SILLON_MAGPIE_VOICE") {
        Ok(voice) if !voice.is_empty() => voice,
        _ => sillon::models::default_voice("magpie_tts").to_string(),
    };
    println!("voix magpie : {}", params_magpie.voice);
    let _ = &reference;
    let t = Instant::now();
    a.set_cursor(8);
    let engine_b = load(&config, magpie.to_str().unwrap(), "magpie_tts");
    let b = synth(&config, params_magpie.clone(), engine_b, &sentences);
    let stop_a = thread::spawn(move || {
        let t = Instant::now();
        drop(a);
        t.elapsed()
    });
    wait_ready(&b, 1, t, "B magpie, première phrase (chargement compris)");
    println!("arrêt de A : {:.2} s", stop_a.join().unwrap().as_secs_f32());
    wait_ready(&b, 3, t, "B magpie, trois phrases");
    wait_ready(&b, 6, t, "B magpie, six phrases");
    for id in 0..6 {
        if let Some(err) = b.failure(id) {
            println!("  refus {id} : {err}");
        }
    }

    // 3. Retour à Supertonic pendant que Magpie tourne.
    let t = Instant::now();
    b.set_cursor(8);
    let engine_c = load(&config, supertonic.to_str().unwrap(), "supertonic");
    let c = synth(&config, config.tts_params(), engine_c, &sentences);
    let stop_b = thread::spawn(move || {
        let t = Instant::now();
        drop(b);
        t.elapsed()
    });
    wait_ready(&c, 1, t, "C supertonic, première phrase (chargement compris)");
    println!("arrêt de B : {:.2} s", stop_b.join().unwrap().as_secs_f32());
    wait_ready(&c, 6, t, "C supertonic, six phrases");
    drop(c);

    // 4. Magpie seul, à froid dans ce processus déjà chaud : la référence.
    let t = Instant::now();
    let engine_d = load(&config, magpie.to_str().unwrap(), "magpie_tts");
    let d = synth(&config, params_magpie, engine_d, &sentences);
    wait_ready(&d, 1, t, "D magpie seul, première phrase");
    wait_ready(&d, 6, t, "D magpie seul, six phrases");
    for id in 0..6 {
        if let Some(err) = d.failure(id) {
            println!("  refus {id} : {err}");
        }
    }
    drop(d);
    std::process::exit(0);
}
