//! Banc de Kokoro MLX en Rust (P5 de la procédure de port) : chargement,
//! premier appel, médiane à chaud de 10 passages, facteur temps réel, pic
//! MLX et mémoire résidente maximale, pour les phrases de 34 et 162 jetons
//! de la fixture. Écrit une ligne JSON sur la sortie ; rien n'est joué.
//!
//!   . scripts/mlx-sources.sh
//!   DYSFLOW_MODELS=… cargo run --release --locked --features mlx --example voice_bench
//!
//! Le pendant Swift : `scripts/voice-bench-swift.sh`, mêmes entrées, même
//! sortie. À lancer sur une machine au calme (charge < 2, pas de swap).

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod bench {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use anyhow::{Context, Result};
    use dysflow::compute::mlx;
    use dysflow::tts::kokoro::mlx::MlxBackend;
    use dysflow::tts::kokoro::{KokoroBackend, FRAME, SAMPLE_RATE};
    use dysflow::tts::Cancel;
    use serde_json::json;

    const WARM_RUNS: usize = 10;
    /// La phrase longue : le corps de la fixture cinq fois (162 jetons).
    const LONG_REPEATS: usize = 5;

    pub fn main() -> Result<()> {
        let models = std::env::var_os("DYSFLOW_MODELS").context("DYSFLOW_MODELS : le dossier des modèles")?;
        let dir = PathBuf::from(models).join("kokoro");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("macos/Tests/Fixtures/kokoro-parite.json");
        let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&fixture)?)?;
        let ids: Vec<i64> = serde_json::from_value(fixture["ids"].clone())?;
        let style: Vec<f32> = serde_json::from_value(fixture["style"].clone())?;
        let body = &ids[1..ids.len() - 1];
        let long: Vec<i64> = std::iter::once(0)
            .chain(std::iter::repeat_n(body, LONG_REPEATS).flatten().copied())
            .chain(std::iter::once(0))
            .collect();
        let load_before = load_average();

        let backend = MlxBackend::new();
        let started = Instant::now();
        backend.load(&dir)?;
        let load_s = started.elapsed().as_secs_f64();

        let mut sentences = Vec::new();
        for input in [&ids, &long] {
            mlx::reset_peak()?;
            let started = Instant::now();
            let raw = backend.infer(input, &style, &Cancel::never())?;
            let first_s = started.elapsed().as_secs_f64();
            let mut runs = Vec::with_capacity(WARM_RUNS);
            for _ in 0..WARM_RUNS {
                let started = Instant::now();
                backend.infer(input, &style, &Cancel::never())?;
                runs.push(started.elapsed().as_secs_f64());
            }
            runs.sort_by(f64::total_cmp);
            let median_s = (runs[WARM_RUNS / 2 - 1] + runs[WARM_RUNS / 2]) / 2.0;
            let audio_s = raw.samples.len() as f64 / SAMPLE_RATE as f64;
            let frames: u32 = raw.durations.iter().sum();
            assert_eq!(raw.samples.len(), frames as usize * FRAME);
            sentences.push(json!({
                "tokens": input.len(),
                "audio_s": audio_s,
                "first_s": first_s,
                "median_s": median_s,
                "min_s": runs[0],
                "max_s": runs[WARM_RUNS - 1],
                "real_time_factor": audio_s / median_s,
                "mlx_peak_bytes": mlx::memory()?.peak,
            }));
        }
        backend.release();
        println!(
            "{}",
            json!({
                "engine": "rust mlx-rs",
                "load_s": load_s,
                "sentences": sentences,
                "rss_max_bytes": max_rss(),
                "load_average_before": load_before,
                "load_average_after": load_average(),
            })
        );
        Ok(())
    }

    /// Mémoire résidente maximale du processus (`ru_maxrss` est en octets sous
    /// macOS).
    fn max_rss() -> i64 {
        // SAFETY : `getrusage` remplit la structure passée, rien d'autre.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        usage.ru_maxrss
    }

    /// La charge sur 1, 5 et 15 minutes.
    fn load_average() -> Vec<f64> {
        let mut loads = [0f64; 3];
        // SAFETY : `getloadavg` écrit au plus 3 valeurs dans le tableau.
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        loads[..n.clamp(0, 3) as usize].to_vec()
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> anyhow::Result<()> {
    bench::main()
}

/// MLX n'existe que sur les Mac Apple Silicon.
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("voice_bench : Mac Apple Silicon seulement");
}
