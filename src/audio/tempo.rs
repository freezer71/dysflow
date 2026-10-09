//! Une phrase jouée à la vitesse choisie, réglable pendant qu'elle sonne.
//!
//! La synthèse se fait toujours à 1× ; la vitesse est appliquée ici, au fil
//! de la lecture, par Sonic. La source lit la vitesse à chaque petit bloc
//! (`CHUNK_FRAMES`) : un changement s'entend au bloc suivant, sans vider la
//! file, sans resynthèse et sans coupure.

use rodio::Source;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::sonic::Sonic;

/// Trames lues entre deux consultations de la vitesse : 5 ms à 24 kHz.
///
/// Plus petit, Sonic est appelé plus souvent pour rien ; plus grand, un
/// changement de vitesse met plus longtemps à s'entendre.
pub const CHUNK_FRAMES: usize = 128;

/// La vitesse de lecture, partagée entre le lecteur et les sources en file.
#[derive(Debug, Clone)]
pub struct SpeedControl(Arc<AtomicU32>);

impl Default for SpeedControl {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl SpeedControl {
    pub fn new(speed: f32) -> Self {
        Self(Arc::new(AtomicU32::new(sanitize(speed).to_bits())))
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, speed: f32) {
        self.0.store(sanitize(speed).to_bits(), Ordering::Relaxed);
    }
}

fn sanitize(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed.clamp(super::sonic::MIN_SPEED, super::sonic::MAX_SPEED)
    } else {
        1.0
    }
}

/// Durée réelle d'un silence de fin de bloc à la vitesse `speed`.
///
/// Les pauses portent la hiérarchie (titre, paragraphe, item) : elles
/// raccourcissent moins vite que la parole, d'un facteur √vitesse. À 2×, la
/// parole dure moitié moins, une pause 0,71 fois sa durée — de quoi garder
/// ses repères dans un texte lu vite.
pub fn pause_at(pause: Duration, speed: f32) -> Duration {
    pause.div_f32(sanitize(speed).sqrt())
}

enum Stage {
    Speech,
    Silence { left: usize },
    Done,
}

/// Parole à 1×, puis silence de fin, joués à la vitesse de `SpeedControl`.
pub struct Tempo {
    samples: Arc<[f32]>,
    /// Prochain échantillon de parole à donner à Sonic.
    pos: usize,
    /// Fin de la parole dans `samples` ; le reste est le silence de fin.
    speech_end: usize,
    /// Silence de fin à 1×, en échantillons.
    silence: usize,
    channels: u16,
    sample_rate: u32,
    sonic: Sonic,
    out: Vec<f32>,
    out_pos: usize,
    speed: SpeedControl,
    stage: Stage,
}

impl Tempo {
    /// `start` et `speech_end` sont des indices d'échantillons (trames
    /// entières) dans `samples` ; ce qui suit `speech_end` est du silence.
    pub fn new(
        samples: Arc<[f32]>,
        channels: u16,
        sample_rate: u32,
        start: usize,
        speech_end: usize,
        speed: SpeedControl,
    ) -> Self {
        let channels = channels.max(1);
        let speech_end = speech_end.min(samples.len());
        let start = start.min(speech_end);
        let silence = samples.len() - speech_end;
        let mut sonic = Sonic::new(sample_rate, channels);
        sonic.set_speed(speed.get());
        Self {
            samples,
            pos: start,
            speech_end,
            silence,
            channels,
            sample_rate,
            sonic,
            out: Vec::with_capacity(CHUNK_FRAMES * channels as usize * 32),
            out_pos: 0,
            speed,
            stage: Stage::Speech,
        }
    }

    /// Échantillons de parole à 1× déjà confiés à Sonic.
    #[cfg(test)]
    pub fn consumed(&self) -> usize {
        self.pos
    }

    /// Remplit `out` avec le prochain bloc. Faux quand tout est joué.
    fn refill(&mut self) -> bool {
        let chunk = CHUNK_FRAMES * self.channels as usize;
        self.out.clear();
        self.out_pos = 0;
        loop {
            match self.stage {
                Stage::Speech => {
                    let speed = self.speed.get();
                    self.sonic.set_speed(speed);
                    if self.pos < self.speech_end {
                        let end = (self.pos + chunk).min(self.speech_end);
                        self.sonic.write(&self.samples[self.pos..end]);
                        self.pos = end;
                    } else {
                        self.sonic.flush();
                        let frames = self.silence / self.channels as usize;
                        let seconds = frames as f64 / self.sample_rate as f64;
                        let scaled = pause_at(Duration::from_secs_f64(seconds), speed);
                        let frames = (scaled.as_secs_f64() * self.sample_rate as f64).round() as usize;
                        self.stage = Stage::Silence { left: frames * self.channels as usize };
                    }
                    if self.sonic.read_into(&mut self.out) > 0 {
                        return true;
                    }
                }
                Stage::Silence { left } => {
                    if left == 0 {
                        self.stage = Stage::Done;
                        continue;
                    }
                    let n = left.min(chunk);
                    self.out.resize(n, 0.0);
                    self.stage = Stage::Silence { left: left - n };
                    return true;
                }
                Stage::Done => return false,
            }
        }
    }
}

impl Iterator for Tempo {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.out_pos >= self.out.len() && !self.refill() {
            return None;
        }
        let sample = self.out[self.out_pos];
        self.out_pos += 1;
        Some(sample)
    }
}

impl Source for Tempo {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Inconnue d'avance : elle dépend de la vitesse, qui peut changer.
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sonic::tests::{speech_like, RATE};

    fn tempo(samples: &[f32], silence: usize, speed: &SpeedControl) -> Tempo {
        let mut all = samples.to_vec();
        let speech_end = all.len();
        all.resize(all.len() + silence, 0.0);
        Tempo::new(all.into(), 1, RATE, 0, speech_end, speed.clone())
    }

    #[test]
    fn at_1x_the_sentence_plays_as_synthesized() {
        let speech = speech_like(1.0);
        let out: Vec<f32> = tempo(&speech, 4800, &SpeedControl::new(1.0)).collect();
        assert_eq!(out.len(), speech.len() + 4800);
        let worst = speech.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-3, "{worst}");
    }

    #[test]
    fn at_2x_speech_halves_and_the_pause_shrinks_less() {
        let speech = speech_like(4.0);
        let pause = RATE as usize; // 1 s
        let out: Vec<f32> = tempo(&speech, pause, &SpeedControl::new(2.0)).collect();
        let expected = speech.len() as f32 / 2.0 + pause as f32 / 2f32.sqrt();
        let ratio = out.len() as f32 / expected;
        assert!((ratio - 1.0).abs() < 0.02, "{} au lieu de {expected}", out.len());
    }

    #[test]
    fn a_word_jump_starts_inside_the_speech() {
        let speech = speech_like(1.0);
        let start = speech.len() / 2;
        let out: Vec<f32> =
            Tempo::new(speech.clone().into(), 1, RATE, start, speech.len(), SpeedControl::new(1.0)).collect();
        assert_eq!(out.len(), speech.len() - start);
    }

    /// Le lecteur tire les échantillons comme le ferait la carte son ; la
    /// vitesse passe de 1× à 2× au milieu. On mesure, en temps audio, combien
    /// de son sort encore à l'ancienne vitesse : à 1× la sortie recopie
    /// l'entrée, le premier échantillon qui s'en écarte est le changement.
    #[test]
    fn a_speed_change_is_heard_within_a_few_milliseconds() {
        let speech = speech_like(3.0);
        let speed = SpeedControl::new(1.0);
        let mut source = tempo(&speech, 0, &speed);
        let switch_at = RATE as usize; // après 1 s de son
        let mut out: Vec<f32> = source.by_ref().take(switch_at).collect();
        let consumed_before = source.consumed();
        let command = std::time::Instant::now();
        speed.set(2.0);
        let command_cost = command.elapsed();
        out.extend(source.by_ref());

        let diverges = (switch_at..out.len())
            .find(|&i| i >= speech.len() || (out[i] - speech[i]).abs() > 2e-3)
            .expect("la vitesse n'a jamais changé");
        let latency_ms = (diverges - switch_at) as f64 * 1000.0 / RATE as f64;
        eprintln!(
            "latence du changement de vitesse : {latency_ms:.2} ms de son (+ tampon de la carte), \
             commande {command_cost:?}, entrée déjà lue {} ms",
            (consumed_before - switch_at) as f64 * 1000.0 / RATE as f64
        );
        assert!(latency_ms <= 10.0, "{latency_ms} ms");

        // Le reste (2 s à 1×) dure 1 s à 2×.
        let rest = (out.len() - switch_at) as f32 / RATE as f32;
        assert!((rest - 1.0).abs() < 0.03, "reste {rest} s");
    }

    /// Même mesure de 2× à 3× : on compare à une lecture restée à 2×, le
    /// premier échantillon qui diffère marque le changement. Sonic garde
    /// alors en réserve jusqu'à deux périodes de voix : c'est le pire cas.
    #[test]
    fn a_speed_change_between_two_fast_speeds_is_heard_quickly_too() {
        let speech = speech_like(3.0);
        let reference: Vec<f32> = tempo(&speech, 0, &SpeedControl::new(2.0)).collect();
        let speed = SpeedControl::new(2.0);
        let mut source = tempo(&speech, 0, &speed);
        let switch_at = RATE as usize / 2;
        let mut out: Vec<f32> = source.by_ref().take(switch_at).collect();
        speed.set(3.0);
        out.extend(source);
        let diverges = (switch_at..out.len().min(reference.len()))
            .find(|&i| (out[i] - reference[i]).abs() > 1e-4)
            .expect("la vitesse n'a jamais changé");
        let latency_ms = (diverges - switch_at) as f64 * 1000.0 / RATE as f64;
        eprintln!("latence 2× → 3× : {latency_ms:.2} ms de son");
        assert!(latency_ms <= 40.0, "{latency_ms} ms");
    }

    #[test]
    fn the_pause_follows_the_square_root_of_the_speed() {
        let pause = Duration::from_millis(800);
        assert_eq!(pause_at(pause, 1.0), pause);
        assert_eq!(pause_at(pause, 4.0), Duration::from_millis(400));
        assert_eq!(pause_at(pause, 0.25), Duration::from_millis(1600));
    }

    #[test]
    fn a_nonsensical_speed_falls_back_to_1x() {
        let speed = SpeedControl::new(f32::NAN);
        assert_eq!(speed.get(), 1.0);
        speed.set(0.0);
        assert_eq!(speed.get(), 1.0);
        speed.set(2.5);
        assert_eq!(speed.get(), 2.5);
    }
}
