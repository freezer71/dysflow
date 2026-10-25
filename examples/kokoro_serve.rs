//! Kokoro int8 derrière le protocole du banc (`.superpowers/research/eval/`) :
//! une requête JSON par ligne, l'audio en f32 brut dans un fichier.
//!
//!   DYSFLOW_KOKORO_DIR=<models/kokoro> DYSFLOW_ORT_LIB=<libonnxruntime> \
//!     cargo run --release --features kokoro-onnx --example kokoro_serve
//!
//! stdin : {"text": "…", "lang": "fr", "out": "/tmp/x.f32"}
//! stdout : {"n": échantillons, "ms": durée de synthèse, "dur": [durées des mots]}
//!
//! Outil de mesure : stdin et stdout seulement, aucune écoute réseau.

use std::io::{BufRead, Write};
use std::sync::Arc;

use dysflow::tts::kokoro::onnx::OrtBackend;
use dysflow::tts::kokoro::phonemes::EspeakProcess;
use dysflow::tts::kokoro::KokoroEngine;
use dysflow::tts::{silent::SilentEngine, TtsEngine, TtsParams};

fn main() -> anyhow::Result<()> {
    let dir = std::path::PathBuf::from(std::env::var("DYSFLOW_KOKORO_DIR")?);
    let threads: u32 = std::env::var("THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(4);
    let engine = KokoroEngine::load(
        Arc::new(OrtBackend::new(threads)),
        Box::new(EspeakProcess::locate()?),
        &dir,
        Arc::new(SilentEngine),
    )?;
    let mut session = engine.open_session()?;
    println!("{}", serde_json::json!({"ready": true, "threads": threads}));
    std::io::stdout().flush()?;
    for line in std::io::stdin().lock().lines() {
        let request: serde_json::Value = serde_json::from_str(&line?)?;
        let text = request["text"].as_str().unwrap_or_default();
        let params = TtsParams { language: request["lang"].as_str().unwrap_or("fr").to_string(), ..TtsParams::default() };
        let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        let started = std::time::Instant::now();
        let out = session.synthesize_timed(text, &words, &params)?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let bytes: Vec<u8> = out.pcm.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(request["out"].as_str().unwrap_or("/dev/null"), bytes)?;
        let durations: Vec<f32> = out.words.unwrap_or_default().iter().map(|w| w.end - w.start).collect();
        println!("{}", serde_json::json!({"n": out.pcm.samples.len(), "ms": ms, "dur": durations}));
        std::io::stdout().flush()?;
    }
    Ok(())
}
