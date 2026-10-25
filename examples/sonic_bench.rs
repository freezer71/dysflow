//! Coût de la vitesse de lecture (Sonic) sur de la parole de synthèse à 24 kHz.
//!
//!   cargo run --release --example sonic_bench
//!
//! Mesure ce que fait la lecture : la source `Tempo` (Sonic puis conversion
//! vers la carte son à 48 kHz), tirée échantillon par échantillon comme le
//! ferait la carte, par blocs de 5 ms. Puis Sonic seul, qualité 0 (défaut)
//! contre qualité 1. Aucun son ne sort : rien n'est joué.

use std::sync::Arc;
use std::time::Instant;

use dysflow::audio::sonic::Sonic;
use dysflow::audio::tempo::{SpeedControl, Tempo};

const RATE: u32 = 24_000;
const DEVICE: u32 = 48_000;
const SECONDS: f32 = 10.0;
const ROUNDS: usize = 20;

/// Fondamentale ondulant autour de 140 Hz, cinq harmoniques, syllabes de
/// 200 ms et courts silences.
fn speech_like(seconds: f32) -> Vec<f32> {
    let n = (RATE as f32 * seconds) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f0 = 140.0 + 25.0 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
            phase += 2.0 * std::f32::consts::PI * f0 / RATE as f32;
            let syllable = (t * 4.0).fract();
            let envelope = if syllable < 0.8 { (std::f32::consts::PI * syllable / 0.8).sin() } else { 0.0 };
            let voice: f32 = (1..=5).map(|h| (h as f32 * phase).sin() / h as f32).sum();
            0.3 * envelope * voice
        })
        .collect()
}

fn main() {
    let samples: Arc<[f32]> = speech_like(SECONDS).into();
    println!("{SECONDS} s de parole de synthèse, 24 kHz mono, {ROUNDS} passes par vitesse");
    println!("{:>7} {:>14} {:>16} {:>14}", "vitesse", "ms / passe", "ms / s de son", "× temps réel");
    for speed in [0.5f32, 1.0, 1.5, 2.0, 3.0] {
        let mut out_len = 0;
        let start = Instant::now();
        for _ in 0..ROUNDS {
            let source = Tempo::new(
                Arc::clone(&samples),
                1,
                RATE,
                DEVICE,
                0,
                samples.len(),
                SpeedControl::new(speed),
            );
            let mut sum = 0.0f32;
            let mut n = 0usize;
            for s in source {
                sum += s;
                n += 1;
            }
            std::hint::black_box(sum);
            out_len = n;
        }
        let per_pass = start.elapsed().as_secs_f64() * 1000.0 / ROUNDS as f64;
        // Ce qu'on entend dure SECONDS / vitesse.
        let heard = out_len as f64 / DEVICE as f64;
        println!(
            "{speed:>6.1}× {per_pass:>14.2} {:>16.3} {:>14.0}",
            per_pass / heard,
            heard * 1000.0 / per_pass
        );
    }

    println!();
    println!("Sonic seul, 24 kHz : ms CPU par seconde de son produit");
    println!("{:>7} {:>12} {:>12}", "vitesse", "qualité 0", "qualité 1");
    for speed in [0.5f32, 2.0, 3.0] {
        let mut row = Vec::new();
        for quality in [0, 1] {
            let start = Instant::now();
            let mut produced = 0usize;
            for _ in 0..ROUNDS {
                let mut sonic = Sonic::new(RATE, 1);
                sonic.set_quality(quality);
                sonic.set_speed(speed);
                let mut out = Vec::new();
                for chunk in samples.chunks(128) {
                    sonic.write(chunk);
                    sonic.read_into(&mut out);
                }
                sonic.flush();
                sonic.read_into(&mut out);
                produced = out.len();
                std::hint::black_box(&out);
            }
            let per_pass = start.elapsed().as_secs_f64() * 1000.0 / ROUNDS as f64;
            row.push(per_pass / (produced as f64 / RATE as f64));
        }
        println!("{speed:>6.1}× {:>12.3} {:>12.3}", row[0], row[1]);
    }
}
