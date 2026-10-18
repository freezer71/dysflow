//! Une phrase jouée à la vitesse choisie, réglable pendant qu'elle sonne.
//!
//! La synthèse se fait toujours à 1× ; la vitesse est appliquée ici, au fil
//! de la lecture, par Sonic. La source lit la vitesse à chaque petit bloc
//! (`CHUNK_FRAMES`) : un changement s'entend au bloc suivant, sans vider la
//! file, sans resynthèse et sans coupure.
//!
//! Deux contraintes du fil audio sont tenues ici :
//!
//! * **Rien ne s'alloue pendant la lecture.** Tous les tampons, ceux de Sonic
//!   compris, sont dimensionnés à la construction, sur le fil qui met la
//!   phrase en file ; la source finie est libérée par un fil à part.
//! * **La source sort à la fréquence de la carte son.** Le mélangeur de rodio
//!   relance sa conversion de fréquence à chaque « trame » de source, et
//!   perd un échantillon à chaque relance : sur une source de longueur
//!   inconnue, toutes les 512 entrées, soit un grain audible ~47 fois par
//!   seconde. Convertie ici, en continu, la source traverse le mélangeur
//!   sans conversion, et ces relances ne coûtent plus rien.

use rodio::Source;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::sonic::Sonic;

/// Trames lues entre deux consultations de la vitesse : 5 ms à 24 kHz.
///
/// Plus petit, Sonic est appelé plus souvent pour rien ; plus grand, un
/// changement de vitesse met plus longtemps à s'entendre.
pub const CHUNK_FRAMES: usize = 128;

/// Bornes de la vitesse de lecture, celles des réglages.
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 3.0;

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
        speed.clamp(MIN_SPEED, MAX_SPEED)
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

/// Conversion de fréquence continue, d'une phrase entière : interpolation
/// cubique (Catmull-Rom) sur quatre trames, sans état perdu entre deux blocs.
struct Resampler {
    channels: usize,
    /// Trames d'entrée par trame de sortie.
    step: f64,
    passthrough: bool,
    /// Trames d'entrée en attente, la première servant d'historique.
    buf: Vec<f32>,
    /// Position de la prochaine trame de sortie dans `buf`, en trames.
    pos: f64,
}

impl Resampler {
    fn new(channels: usize, from: u32, to: u32, capacity_frames: usize) -> Self {
        let mut buf = Vec::with_capacity((capacity_frames + 8) * channels);
        // Une trame de silence avant le son : l'interpolation a besoin d'un
        // voisin à gauche dès la première trame.
        buf.resize(channels, 0.0);
        Self { channels, step: from as f64 / to as f64, passthrough: from == to, buf, pos: 1.0 }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        let ch = self.channels;
        self.buf.extend_from_slice(input);
        let frames = self.buf.len() / ch;
        loop {
            let i = self.pos as usize;
            if i + 2 >= frames {
                break;
            }
            let t = (self.pos - i as f64) as f32;
            for c in 0..ch {
                let p0 = self.buf[(i - 1) * ch + c];
                let p1 = self.buf[i * ch + c];
                let p2 = self.buf[(i + 1) * ch + c];
                let p3 = self.buf[(i + 2) * ch + c];
                out.push(
                    p1 + 0.5
                        * t
                        * (p2 - p0
                            + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3
                                + t * (3.0 * (p1 - p2) + p3 - p0))),
                );
            }
            self.pos += self.step;
        }
        // On ne garde que l'historique utile : la trame avant la position.
        let keep_from = (self.pos as usize).saturating_sub(1).min(frames);
        if keep_from > 0 {
            self.buf.copy_within(keep_from * ch.., 0);
            self.buf.truncate((frames - keep_from) * ch);
            self.pos -= keep_from as f64;
        }
    }
}

enum Stage {
    Speech,
    Silence { left: usize },
    /// Quelques trames de silence pour faire sortir la fin de l'interpolation.
    Tail,
    Done,
}

/// Ce qui pèse : libéré hors du fil audio.
struct Heavy {
    samples: Arc<[f32]>,
    sonic: Sonic,
    /// Sortie de Sonic, à la fréquence de la synthèse.
    stretched: Vec<f32>,
    resampler: Resampler,
    /// Prochain bloc à donner à la carte son.
    out: Vec<f32>,
}

/// Parole à 1×, puis silence de fin, joués à la vitesse de `SpeedControl`.
pub struct Tempo {
    heavy: ManuallyDrop<Heavy>,
    /// Prochain échantillon de parole à donner à Sonic.
    pos: usize,
    /// Fin de la parole dans `samples` ; le reste est le silence de fin.
    speech_end: usize,
    /// Silence de fin à 1×, en échantillons.
    silence: usize,
    channels: u16,
    /// Fréquence de la synthèse, celle de `samples`.
    synthesis_rate: u32,
    output_rate: u32,
    out_pos: usize,
    speed: SpeedControl,
    stage: Stage,
}

impl Tempo {
    /// `start` et `speech_end` sont des indices d'échantillons (trames
    /// entières) dans `samples`, à `sample_rate` ; ce qui suit `speech_end`
    /// est du silence. La source sort à `output_rate`, celle de la carte son.
    pub fn new(
        samples: Arc<[f32]>,
        channels: u16,
        sample_rate: u32,
        output_rate: u32,
        start: usize,
        speech_end: usize,
        speed: SpeedControl,
    ) -> Self {
        let channels = channels.max(1);
        let ch = channels as usize;
        let sample_rate = sample_rate.max(1);
        let output_rate = output_rate.max(1);
        let speech_end = speech_end.min(samples.len());
        let start = start.min(speech_end);
        let silence = samples.len() - speech_end;

        // Pire cas de Sonic : jusqu'à deux périodes de voix les plus graves
        // (65 Hz) en réserve, plus un bloc ; le vidage de fin y ajoute deux
        // fois cette réserve ; à 0,5× la sortie double.
        let max_required = 2 * (sample_rate as usize / 65 + 1);
        let warm_frames = 3 * max_required + 2 * CHUNK_FRAMES;
        let stretched_frames = 2 * (warm_frames + max_required) + 4 * CHUNK_FRAMES;
        let ratio = (output_rate as f64 / sample_rate as f64).ceil() as usize;

        let mut sonic = Sonic::new(sample_rate, channels);
        let mut stretched = Vec::with_capacity(stretched_frames * ch);
        // Échauffement sur le fil appelant, à la vitesse la plus lente : Sonic
        // agrandit ici, une fois pour toutes, les tampons qu'il agrandirait
        // sinon en pleine lecture.
        sonic.reserve(warm_frames * ch);
        sonic.set_speed(MIN_SPEED);
        stretched.resize(warm_frames * ch, 0.0);
        sonic.write(&stretched);
        stretched.clear();
        sonic.read_into(&mut stretched);
        sonic.flush();
        sonic.read_into(&mut stretched);
        stretched.clear();
        sonic.set_speed(speed.get());

        Self {
            heavy: ManuallyDrop::new(Heavy {
                samples,
                sonic,
                stretched,
                resampler: Resampler::new(ch, sample_rate, output_rate, stretched_frames),
                out: Vec::with_capacity((stretched_frames * ratio + 8) * ch),
            }),
            pos: start,
            speech_end,
            silence,
            channels,
            synthesis_rate: sample_rate,
            output_rate,
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
        let ch = self.channels as usize;
        let chunk = CHUNK_FRAMES * ch;
        let h = &mut *self.heavy;
        h.out.clear();
        self.out_pos = 0;
        loop {
            h.stretched.clear();
            match self.stage {
                Stage::Speech => {
                    let speed = self.speed.get();
                    h.sonic.set_speed(speed);
                    if self.pos < self.speech_end {
                        let end = (self.pos + chunk).min(self.speech_end);
                        h.sonic.write(&h.samples[self.pos..end]);
                        self.pos = end;
                    } else {
                        h.sonic.flush();
                        let frames = self.silence / ch;
                        let seconds = frames as f64 / self.synthesis_rate as f64;
                        let scaled = pause_at(Duration::from_secs_f64(seconds), speed);
                        let frames = (scaled.as_secs_f64() * self.synthesis_rate as f64).round() as usize;
                        self.stage = Stage::Silence { left: frames * ch };
                    }
                    h.sonic.read_into(&mut h.stretched);
                }
                Stage::Silence { left } => {
                    if left == 0 {
                        self.stage = Stage::Tail;
                        continue;
                    }
                    let n = left.min(chunk);
                    h.stretched.resize(n, 0.0);
                    self.stage = Stage::Silence { left: left - n };
                }
                Stage::Tail => {
                    self.stage = Stage::Done;
                    if h.resampler.passthrough {
                        continue;
                    }
                    h.stretched.resize(3 * ch, 0.0);
                }
                Stage::Done => return false,
            }
            h.resampler.push(&h.stretched, &mut h.out);
            if !h.out.is_empty() {
                return true;
            }
        }
    }

    /// Échantillons qui sortiront encore, au moins : la vitesse peut monter
    /// jusqu'à `MAX_SPEED` d'ici là, d'où une estimation prudente.
    fn remaining_lower_bound(&self) -> usize {
        let ch = self.channels as usize;
        let in_buffer = self.heavy.out.len() - self.out_pos;
        let ratio = self.output_rate as f64 / self.synthesis_rate as f64;
        let ahead = match self.stage {
            Stage::Speech => {
                let speech = (self.speech_end - self.pos) / ch;
                let pause = self.silence / ch;
                speech as f64 / MAX_SPEED as f64 + pause as f64 / (MAX_SPEED as f64).sqrt()
            }
            Stage::Silence { left } => (left / ch) as f64,
            Stage::Tail | Stage::Done => 0.0,
        };
        // Marge de 10 % pour l'arrondi de Sonic et de la conversion.
        in_buffer + ((ahead * ratio * 0.9) as usize) * ch
    }
}

impl Iterator for Tempo {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.out_pos >= self.heavy.out.len() && !self.refill() {
            return None;
        }
        let sample = self.heavy.out[self.out_pos];
        self.out_pos += 1;
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_lower_bound(), None)
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
        self.output_rate
    }

    /// Inconnue d'avance : elle dépend de la vitesse, qui peut changer.
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Drop for Tempo {
    /// Une phrase finie meurt dans le fil audio : ses tampons et sa part du
    /// PCM partent vers un fil de ramassage, pour qu'aucune libération ne
    /// s'y fasse. File pleine : libération sur place, plutôt que d'attendre.
    fn drop(&mut self) {
        let heavy = unsafe { ManuallyDrop::take(&mut self.heavy) };
        if let Err(err) = reaper().try_send(heavy) {
            drop(err.into_inner());
        }
    }
}

/// Fil qui libère les phrases finies, hors du fil audio.
fn reaper() -> &'static crossbeam_channel::Sender<Heavy> {
    static REAPER: OnceLock<crossbeam_channel::Sender<Heavy>> = OnceLock::new();
    REAPER.get_or_init(|| {
        // File bornée : son tableau est alloué une fois, l'envoi n'alloue rien.
        let (tx, rx) = crossbeam_channel::bounded::<Heavy>(64);
        std::thread::Builder::new()
            .name("sillage-ramassage-audio".into())
            .spawn(move || for heavy in rx { drop(heavy) })
            .expect("fil de ramassage audio");
        tx
    })
}

/// Prépare le fil de ramassage depuis un fil ordinaire : le créer coûte des
/// allocations, qui n'ont rien à faire dans le fil audio.
pub fn prepare() {
    let _ = reaper();
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::audio::sonic::tests::{speech_like, RATE};

    pub fn tempo(samples: &[f32], silence: usize, speed: &SpeedControl) -> Tempo {
        let mut all = samples.to_vec();
        let speech_end = all.len();
        all.resize(all.len() + silence, 0.0);
        Tempo::new(all.into(), 1, RATE, RATE, 0, speech_end, speed.clone())
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
            Tempo::new(speech.clone().into(), 1, RATE, RATE, start, speech.len(), SpeedControl::new(1.0))
                .collect();
        assert_eq!(out.len(), speech.len() - start);
    }

    #[test]
    fn the_size_hint_never_promises_more_than_what_comes() {
        let speech = speech_like(2.0);
        for speed in [0.5f32, 1.0, 3.0] {
            let control = SpeedControl::new(speed);
            let mut source = tempo(&speech, 6000, &control);
            let mut left = source.by_ref().count();
            let mut source = tempo(&speech, 6000, &control);
            loop {
                let (lower, _) = source.size_hint();
                assert!(lower <= left, "{speed}× : promet {lower}, reste {left}");
                if source.next().is_none() {
                    break;
                }
                left -= 1;
                // On saute en avant pour garder l'essai rapide.
                for _ in 0..97 {
                    if source.next().is_some() {
                        left -= 1;
                    }
                }
            }
            // Au départ, la promesse couvre l'essentiel de la phrase.
            let fresh = tempo(&speech, 6000, &control);
            assert!(fresh.size_hint().0 > speech.len() / 4, "{speed}×");
        }
    }

    #[test]
    fn resampling_keeps_the_duration_and_the_pitch() {
        use crate::audio::sonic::tests::{dominant_frequency, sine};
        let tone = sine(220.0, 2.0, 0.5);
        for device in [44_100u32, 48_000] {
            let out: Vec<f32> =
                Tempo::new(tone.clone().into(), 1, RATE, device, 0, tone.len(), SpeedControl::new(1.0)).collect();
            let expected = tone.len() as f64 * device as f64 / RATE as f64;
            assert!((out.len() as f64 - expected).abs() <= 8.0, "{device} : {} / {expected}", out.len());
            // Comptée à la fréquence de sortie, la note reste à 220 Hz.
            let at_device = dominant_frequency(&out) * device as f32 / RATE as f32;
            assert!((at_device - 220.0).abs() < 2.0, "{device} : {at_device} Hz");
        }
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
        let pause = Duration::from_millis(900);
        assert_eq!(pause_at(pause, 1.0), pause);
        assert_eq!(pause_at(pause, 2.25), Duration::from_millis(600));
        // Hors bornes : ramenée à 0,5×.
        assert_eq!(pause_at(pause, 0.1), pause_at(pause, 0.5));
    }

    #[test]
    fn the_speed_stays_within_the_reader_bounds() {
        let speed = SpeedControl::new(f32::NAN);
        assert_eq!(speed.get(), 1.0);
        speed.set(0.0);
        assert_eq!(speed.get(), 1.0);
        speed.set(2.5);
        assert_eq!(speed.get(), 2.5);
        speed.set(9.0);
        assert_eq!(speed.get(), MAX_SPEED);
        speed.set(0.2);
        assert_eq!(speed.get(), MIN_SPEED);
    }
}

#[cfg(test)]
mod mixer_tests {
    //! Ce que la carte son reçoit vraiment : file du `Sink`, puis
    //! conversion de fréquence du mélangeur de rodio (`UniformSourceIterator`).
    use super::*;
    use crate::audio::sonic::tests::{speech_like, RATE};
    use rodio::buffer::SamplesBuffer;
    use rodio::source::UniformSourceIterator;
    use rodio::Sink;

    /// Échantillons sortis du mélangeur jusqu'à ce que la file soit vide.
    ///
    /// Une phrase muette passe d'abord : c'est l'enchaînement de deux phrases
    /// qu'on mesure, le cas courant. (Au tout début d'une file, rodio
    /// convertit les ≤ 512 premiers échantillons avec la fréquence de son
    /// silence d'attente, 44,1 kHz : défaut de rodio, hors de portée d'une
    /// source.) Le silence, exactement nul, est retiré du début.
    fn through_mixer(append: impl FnOnce(&Sink), device_rate: u32) -> Vec<f32> {
        let (sink, queue) = Sink::new_idle();
        append(&sink);
        let mut mixer = UniformSourceIterator::<_, f32>::new(queue, 1, device_rate);
        let mut out = Vec::new();
        while !sink.empty() {
            out.push(mixer.next().unwrap());
        }
        let lead = out.iter().position(|s| *s != 0.0).unwrap_or(out.len());
        // La parole d'essai commence elle-même par un zéro exact.
        out.drain(..lead.saturating_sub(1));
        out
    }

    fn tempo_for_device(speech: &[f32], device_rate: u32) -> Tempo {
        Tempo::new(speech.to_vec().into(), 1, RATE, device_rate, 0, speech.len(), SpeedControl::new(1.0))
    }

    /// La perte mesurée par la relecture : 137 échantillons de plus que
    /// `SamplesBuffer` en 3 s à 48 kHz, une relance de conversion toutes les
    /// 512 entrées. On exige désormais la longueur exacte.
    #[test]
    fn the_mixer_loses_no_sample_of_a_sentence() {
        let speech = speech_like(3.0);
        for device_rate in [48_000u32, 44_100] {
            let expected = speech.len() as f64 * device_rate as f64 / RATE as f64;
            let reference = through_mixer(
                |sink| {
                    sink.append(SamplesBuffer::new(1, RATE, vec![0.0; 2400]));
                    sink.append(SamplesBuffer::new(1, RATE, speech.clone()));
                },
                device_rate,
            );
            let ours = through_mixer(
                |sink| {
                    sink.append(tempo_for_device(&[0.0; 2400], device_rate));
                    sink.append(tempo_for_device(&speech, device_rate));
                },
                device_rate,
            );
            eprintln!(
                "{device_rate} Hz : attendu {expected:.0}, SamplesBuffer {}, Tempo {}",
                reference.len(),
                ours.len()
            );
            assert!(
                (ours.len() as f64 - expected).abs() <= 8.0,
                "{device_rate} Hz : {} échantillons au lieu de {expected:.0}",
                ours.len()
            );
            // Le contenu : comparé à la source, interpolée linéairement à la
            // fréquence de la carte. (La référence `SamplesBuffer` ne sert pas
            // d'étalon : ses relances de conversion la décalent elle aussi.)
            let ideal = |j: usize| {
                let x = j as f64 * RATE as f64 / device_rate as f64;
                let i = x as usize;
                let t = (x - i as f64) as f32;
                let next = speech.get(i + 1).copied().unwrap_or(0.0);
                speech[i] * (1.0 - t) + next * t
            };
            let deviation = |out: &[f32]| {
                (0..out.len().min(speech.len() * device_rate as usize / RATE as usize - 2))
                    .map(|j| (out[j] - ideal(j)).abs())
                    .fold(0.0, f32::max)
            };
            let (ours_dev, reference_dev) = (deviation(&ours), deviation(&reference));
            eprintln!("{device_rate} Hz : écart à la source, Tempo {ours_dev:.4}, SamplesBuffer {reference_dev:.4}");
            assert!(ours_dev < 0.01, "{device_rate} Hz : écart {ours_dev}");
        }
    }
}
