//! Dix minutes de flux sur le vrai modèle (ignoré par défaut :
//! `DYSFLOW_MODELS`), la mémoire relevée en route.

use super::tests::{inputs, model_dir, STEP};
use super::*;
use crate::stt::mlx::parakeet::tests::{cycled, serial};

// ---------- dix minutes ----------

/// Dix minutes de flux (les deux dictées et une seconde de silence, en
/// boucle), par pas de 80 ms : la mémoire est relevée toutes les 30 s
/// d'audio. Bornes, tirées des structures du Swift (les mêmes ici) :
/// - le cache de MLX ne dépasse jamais son plafond (256 Mio) ;
/// - la mémoire active ne dépasse le plancher que des caches du flux : le
///   cache de clés et valeurs du décodeur (Float32, 26 couches × 2 × 1 024
///   par ligne, au plus 4 097 jetons + l'invite de 39, arrondis au bloc de
///   256 lignes, soit 4 352 lignes : 884 Mio), ceux de l'encodeur (bornés à
///   un bloc de 750 lignes : 32 couches × 2 × 2 048 × 750 × 4 o, 375 Mio),
///   et 256 Mio de marge pour les tampons d'un pas ;
/// - le flux fini (4 096 jetons, environ 5 min 28 s, `maxTokens` du Swift),
///   la mémoire ne bouge plus (64 Mio au plus) ;
/// - après `finish`, mémoire au plancher et cache vide.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS), environ dix minutes d'audio"]
fn ten_minutes_of_stream_keep_the_memory_bounded() {
    const MINUTES: usize = 10;
    const EVERY: usize = 375; // pas de 80 ms : 30 s
    let _serial = serial();
    let all = inputs();
    let (fr, en) = (&all[0].1, &all[1].1);
    let silence = vec![0.0f32; 16_000];
    let audio = cycled(&[fr, &silence, en, &silence], MINUTES * 60 * 16_000);
    let recognizer = VoxtralMlx::new();
    recognizer.load(&model_dir()).unwrap();
    recognizer.warm_up().unwrap();
    let floor = crate::compute::mlx::memory().unwrap();
    let mib = |b: usize| b as f64 / (1 << 20) as f64;
    let mut stream = recognizer.open_stream("fr").unwrap();
    let started = std::time::Instant::now();
    let mut samples: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut text_len = 0;
    for (index, part) in audio.chunks(STEP).enumerate() {
        text_len += stream.feed(part).unwrap().chars().count();
        if (index + 1) % EVERY == 0 {
            let m = crate::compute::mlx::memory().unwrap();
            let seconds = (index + 1) * STEP / 16_000;
            eprintln!(
                "{seconds:>4} s d'audio, {:>5.0} s : active {:>7.1} Mio (+{:.1}), cache {:>6.1} Mio, pic {:>7.1} Mio, texte {text_len} car.",
                started.elapsed().as_secs_f64(),
                mib(m.active),
                mib(m.active.saturating_sub(floor.active)),
                mib(m.cache),
                mib(m.peak),
            );
            samples.push((seconds, m.active, m.cache, text_len));
        }
    }
    let text = stream.finish().unwrap();
    let after = crate::compute::mlx::memory().unwrap();
    eprintln!(
        "plancher {:.1} Mio ; après finish {:.1} Mio (cache {:.1}) ; texte {} car. ; {:.0} s",
        mib(floor.active),
        mib(after.active),
        mib(after.cache),
        text.chars().count(),
        started.elapsed().as_secs_f64()
    );
    recognizer.release();

    let kv_rows = (MAX_TOKENS + 1 + 39).div_ceil(256) * 256;
    let kv = kv_rows * 26 * 2 * 1_024 * 4;
    let encoder = 32 * 2 * 2_048 * 750 * 4;
    let bound = kv + encoder + (256 << 20);
    for &(seconds, active, cache, _) in &samples {
        assert!(
            cache <= crate::stt::mlx::CACHE_LIMIT,
            "{seconds} s : cache {cache}"
        );
        let grown = active.saturating_sub(floor.active);
        assert!(
            grown <= bound,
            "{seconds} s : +{grown} o au-delà de {bound}"
        );
    }
    // Le flux fini : plus rien ne bouge (le texte non plus).
    let done: Vec<_> = samples.iter().filter(|s| s.0 >= 360).collect();
    assert!(done.len() >= 8, "{} relevés après 6 min", done.len());
    let first = done[0];
    for s in &done {
        assert!(
            s.1.abs_diff(first.1) <= 64 << 20,
            "{} s : {} contre {}",
            s.0,
            s.1,
            first.1
        );
        assert_eq!(s.3, first.3, "{} s : le texte a bougé", s.0);
    }
    assert_eq!(after.cache, 0, "cache après finish");
    assert_eq!(after.active, floor.active, "mémoire active après finish");
}
