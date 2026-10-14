//! Accélération de la parole sans changement de hauteur, par Sonic.
//!
//! Sonic (Bill Cox, Apache-2.0, `third_party/sonic`) retire ou répète des
//! périodes entières de la voix, repérées par leur hauteur : la parole va plus
//! vite, la voix ne monte pas dans les aigus. Il est conçu pour les lecteurs
//! d'écran au-delà de 2×, là où les autres méthodes brouillent les consonnes.
//!
//! L'enveloppe tient en un flux : on écrit des échantillons, on relit ce qui
//! est prêt, et la vitesse peut changer entre deux écritures.

use std::os::raw::{c_float, c_int, c_void};
use std::ptr::NonNull;

#[allow(non_camel_case_types)]
type sonicStream = *mut c_void;

extern "C" {
    fn sonicCreateStream(sample_rate: c_int, num_channels: c_int) -> sonicStream;
    fn sonicDestroyStream(stream: sonicStream);
    fn sonicWriteFloatToStream(stream: sonicStream, samples: *const c_float, num_samples: c_int) -> c_int;
    fn sonicReadFloatFromStream(stream: sonicStream, samples: *mut c_float, max_samples: c_int) -> c_int;
    fn sonicFlushStream(stream: sonicStream) -> c_int;
    fn sonicSamplesAvailable(stream: sonicStream) -> c_int;
    fn sonicSetSpeed(stream: sonicStream, speed: c_float);
    fn sonicSetQuality(stream: sonicStream, quality: c_int);
    fn sillon_sonic_heap_calls_on_this_thread() -> u64;
}

/// Appels de Sonic au tas (calloc, realloc, free) faits par le fil courant.
///
/// Sert à vérifier que le fil audio n'alloue rien une fois la phrase prête.
pub fn heap_calls_on_this_thread() -> u64 {
    unsafe { sillon_sonic_heap_calls_on_this_thread() }
}

/// Marge avant Sonic, qui stocke en 16 bits et déborderait au-delà de ±1 :
/// le son y entre à moitié et en ressort doublé. Les crêtes d'un modèle
/// jusqu'à ±2 passent intactes, et l'écrêtage n'a lieu qu'à la sortie, après
/// le volume. Coût : un bit de résolution, bruit de fond vers −84 dB.
const HEADROOM: f32 = 0.5;

/// Vitesses que Sonic accepte ici. Les réglages du lecteur (0,5–3×) y tiennent
/// largement ; au-delà, la parole devient inintelligible bien avant Sonic.
pub const MIN_SPEED: f32 = 0.25;
pub const MAX_SPEED: f32 = 6.0;

/// Un flux Sonic : un seul propriétaire, un seul fil à la fois.
pub struct Sonic {
    raw: NonNull<c_void>,
    channels: usize,
    speed: f32,
    /// Copie de l'entrée mise à l'échelle et bornée : Sonic stocke en 16 bits
    /// et multiplie par 32767 sans borner — au-delà, le signe s'inverserait.
    scratch: Vec<f32>,
}

// Le flux n'est qu'un bloc de mémoire tas, sans état partagé ni lien à un
// fil : le déplacer d'un fil à l'autre est sûr tant qu'un seul s'en sert, ce
// que `&mut self` garantit.
unsafe impl Send for Sonic {}

impl Sonic {
    /// Ouvre un flux à la vitesse 1×.
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1);
        let raw = unsafe { sonicCreateStream(sample_rate.max(1) as c_int, channels as c_int) };
        let raw = NonNull::new(raw).expect("Sonic : mémoire insuffisante pour ouvrir un flux");
        Self { raw, channels: channels as usize, speed: 1.0, scratch: Vec::new() }
    }

    /// Réserve la copie d'entrée pour des écritures de `samples` échantillons,
    /// afin qu'elles n'allouent plus rien ensuite.
    pub fn reserve(&mut self, samples: usize) {
        self.scratch.reserve(samples.saturating_sub(self.scratch.len()));
    }

    /// 0 (défaut) : hauteur cherchée sur un signal sous-échantillonné ; 1 :
    /// à pleine résolution, plus précis, plus coûteux.
    pub fn set_quality(&mut self, quality: i32) {
        unsafe { sonicSetQuality(self.raw.as_ptr(), quality as c_int) };
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Change la vitesse pour ce qui reste à traiter ; ce qui est déjà sorti
    /// garde l'ancienne.
    pub fn set_speed(&mut self, speed: f32) {
        let speed = if speed.is_finite() { speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };
        if speed == self.speed {
            return;
        }
        self.speed = speed;
        unsafe { sonicSetSpeed(self.raw.as_ptr(), speed) };
    }

    /// Écrit des échantillons entrelacés (trames entières).
    pub fn write(&mut self, samples: &[f32]) {
        let frames = samples.len() / self.channels;
        if frames == 0 {
            return;
        }
        self.scratch.clear();
        self.scratch
            .extend(samples[..frames * self.channels].iter().map(|s| (s * HEADROOM).clamp(-1.0, 1.0)));
        let ok = unsafe {
            sonicWriteFloatToStream(self.raw.as_ptr(), self.scratch.as_ptr(), frames as c_int)
        };
        debug_assert!(ok != 0, "Sonic : écriture refusée (mémoire)");
    }

    /// Ajoute à `out` tout ce que Sonic a déjà produit. Rend le nombre
    /// d'échantillons ajoutés.
    pub fn read_into(&mut self, out: &mut Vec<f32>) -> usize {
        let frames = unsafe { sonicSamplesAvailable(self.raw.as_ptr()) }.max(0) as usize;
        if frames == 0 {
            return 0;
        }
        let start = out.len();
        out.resize(start + frames * self.channels, 0.0);
        let read = unsafe {
            sonicReadFloatFromStream(self.raw.as_ptr(), out[start..].as_mut_ptr(), frames as c_int)
        }
        .max(0) as usize;
        out.truncate(start + read * self.channels);
        for s in &mut out[start..] {
            *s /= HEADROOM;
        }
        read * self.channels
    }

    /// Pousse ce que Sonic garde encore en réserve : à appeler en fin de
    /// texte, sans quoi les dernières dizaines de millisecondes se perdent.
    pub fn flush(&mut self) {
        unsafe { sonicFlushStream(self.raw.as_ptr()) };
    }
}

impl Drop for Sonic {
    fn drop(&mut self) {
        unsafe { sonicDestroyStream(self.raw.as_ptr()) };
    }
}

/// Le même son, `speed` fois plus rapide, d'un seul tenant.
pub fn stretch(samples: &[f32], sample_rate: u32, channels: u16, speed: f32) -> Vec<f32> {
    let mut sonic = Sonic::new(sample_rate, channels);
    sonic.set_speed(speed);
    let mut out = Vec::with_capacity((samples.len() as f32 / speed.max(MIN_SPEED)) as usize + 1024);
    for chunk in samples.chunks(4096 * channels.max(1) as usize) {
        sonic.write(chunk);
        sonic.read_into(&mut out);
    }
    sonic.flush();
    sonic.read_into(&mut out);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const RATE: u32 = 24_000;

    pub fn sine(freq: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        (0..n)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32).sin())
            .collect()
    }

    /// Une voix de synthèse grossière : fondamentale qui ondule autour de
    /// 140 Hz, cinq harmoniques, syllabes de 200 ms séparées de courts
    /// silences. Assez pour que Sonic trouve une hauteur, et assez irrégulier
    /// pour qu'un décalage se voie.
    pub fn speech_like(seconds: f32) -> Vec<f32> {
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

    /// Fréquence dominante d'un signal presque sinusoïdal, par comptage des
    /// passages à zéro montants.
    pub fn dominant_frequency(samples: &[f32]) -> f32 {
        // On écarte les bords, où Sonic amorce et vide son tampon.
        let body = &samples[samples.len() / 10..samples.len() * 9 / 10];
        let rising = body.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        rising as f32 * RATE as f32 / body.len() as f32
    }

    #[test]
    fn at_1x_the_sound_passes_through_untouched() {
        let input = speech_like(2.0);
        let out = stretch(&input, RATE, 1, 1.0);
        assert_eq!(out.len(), input.len(), "1× ne doit ni ajouter ni retirer");
        let worst = input.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        // Seule perte : le passage par 16 bits (15 utiles, marge comprise).
        assert!(worst < 1e-3, "écart maximal {worst}");
    }

    #[test]
    fn at_2x_the_sound_lasts_half_as_long() {
        let input = speech_like(4.0);
        let out = stretch(&input, RATE, 1, 2.0);
        let ratio = out.len() as f32 / input.len() as f32;
        assert!((ratio - 0.5).abs() < 0.02, "rapport {ratio}");
    }

    #[test]
    fn every_speed_of_the_reader_keeps_its_duration() {
        let input = speech_like(4.0);
        for speed in [0.5f32, 0.75, 1.25, 1.5, 2.5, 3.0] {
            let out = stretch(&input, RATE, 1, speed);
            let ratio = out.len() as f32 * speed / input.len() as f32;
            assert!((ratio - 1.0).abs() < 0.03, "{speed}× : rapport {ratio}");
        }
    }

    #[test]
    fn the_pitch_stays_where_it_was() {
        let input = sine(220.0, 3.0, 0.5);
        let before = dominant_frequency(&input);
        for speed in [0.5f32, 2.0, 3.0] {
            let after = dominant_frequency(&stretch(&input, RATE, 1, speed));
            let drift = (after - before).abs() / before;
            assert!(drift < 0.03, "{speed}× : {before} Hz devenu {after} Hz");
        }
    }

    #[test]
    fn a_peak_above_full_scale_passes_intact() {
        // Sans marge, 1,2 × 32767 déborde un `short` et ressort négatif ; avec
        // un simple écrêtage, la crête serait aplatie avant même le volume.
        let input = sine(220.0, 1.0, 1.2);
        let out = stretch(&input, RATE, 1, 1.0);
        let worst = input.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-3, "écart maximal {worst}");
    }

    #[test]
    fn beyond_twice_full_scale_it_saturates_instead_of_wrapping() {
        let input = sine(220.0, 1.0, 3.0);
        let out = stretch(&input, RATE, 1, 1.0);
        for (a, b) in input.iter().zip(&out) {
            if a.abs() > 0.01 {
                assert_eq!(a.signum(), b.signum(), "{a} ressorti à {b}");
            }
            assert!(b.abs() <= 2.0 + 1e-3);
        }
    }

    #[test]
    fn stereo_frames_stay_paired() {
        // Gauche et droite opposées : un décalage d'un échantillon les
        // mélangerait.
        let mono = sine(200.0, 2.0, 0.5);
        let stereo: Vec<f32> = mono.iter().flat_map(|s| [*s, -*s]).collect();
        let out = stretch(&stereo, RATE, 2, 2.0);
        assert_eq!(out.len() % 2, 0);
        for frame in out.chunks(2) {
            assert!((frame[0] + frame[1]).abs() < 2e-3, "{frame:?}");
        }
    }
}
