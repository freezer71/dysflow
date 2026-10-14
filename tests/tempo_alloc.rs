//! Le fil audio n'alloue rien : une phrase, une fois en file, se joue et se
//! libère sans un appel au tas sur le fil qui la tire (Rust comme Sonic).
//!
//! Binaire d'essai à part : il pose son propre allocateur global, qui compte
//! les appels du fil courant.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use sillon::audio::sonic::heap_calls_on_this_thread;
use sillon::audio::tempo::{prepare, SpeedControl, Tempo};

struct Counting;

thread_local! {
    static CALLS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    let _ = CALLS.try_with(|c| c.set(c.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count();
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn heap_calls() -> u64 {
    CALLS.with(|c| c.get()) + heap_calls_on_this_thread()
}

/// Parole de synthèse grossière, comme dans les essais du module.
fn speech_like(rate: u32, seconds: f32) -> Vec<f32> {
    let n = (rate as f32 * seconds) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let f0 = 140.0 + 25.0 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
            phase += 2.0 * std::f32::consts::PI * f0 / rate as f32;
            let syllable = (t * 4.0).fract();
            let envelope = if syllable < 0.8 { (std::f32::consts::PI * syllable / 0.8).sin() } else { 0.0 };
            let voice: f32 = (1..=5).map(|h| (h as f32 * phase).sin() / h as f32).sum();
            0.3 * envelope * voice
        })
        .collect()
}

#[test]
fn playing_and_dropping_a_sentence_never_touches_the_heap() {
    prepare();
    // Fréquences de synthèse et de carte du catalogue et des voix système.
    for (rate, device) in [(24_000u32, 48_000u32), (44_100, 44_100), (16_000, 44_100), (22_050, 48_000)] {
        for start_speed in [0.5f32, 1.0, 3.0] {
            let mut samples = speech_like(rate, 2.0);
            let speech_end = samples.len();
            samples.resize(speech_end + rate as usize / 2, 0.0); // pause de fin
            let samples: Arc<[f32]> = samples.into();
            let speed = SpeedControl::new(start_speed);
            let mut tempo = Tempo::new(samples, 1, rate, device, 0, speech_end, speed.clone());

            let before = heap_calls();
            let mut n = 0usize;
            let mut sum = 0.0f32;
            for s in tempo.by_ref() {
                sum += s;
                n += 1;
                // La vitesse change en route, dans tous les sens.
                match n {
                    5_000 => speed.set(3.0),
                    15_000 => speed.set(0.5),
                    30_000 => speed.set(1.0),
                    40_000 => speed.set(2.0),
                    _ => {}
                }
            }
            drop(tempo);
            let calls = heap_calls() - before;
            std::hint::black_box(sum);
            assert_eq!(
                calls, 0,
                "{rate} Hz → {device} Hz, départ {start_speed}× : {calls} appels au tas sur le fil audio"
            );
            assert!(n > 0);
        }
    }
}
