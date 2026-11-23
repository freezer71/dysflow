//! La découpe : de l'audio 16 kHz mono en énoncés, aux silences.
//!
//! Le détecteur juge l'audio trame par trame (`FRAME`). Un énoncé commence à
//! la première trame de parole, avec `MARGIN` de silence devant, et se ferme
//! après `SILENCE_FRAMES` trames calmes, avec `MARGIN` derrière. Moins de
//! `MIN_SPEECH` de parole : un claquement, jeté. Une parole sans fin est
//! coupée avant `MAX_SEGMENT`, sur la trame la plus calme des `CUT_SEARCH`
//! derniers échantillons ; la coupe forcée n'a pas de marge : l'énoncé
//! suivant commence où le précédent finit, ni trou ni doublon.
//!
//! Les marges de deux énoncés voisins ne se touchent jamais : le silence qui
//! ferme un énoncé (`SILENCE_FRAMES` trames) est plus long que leurs deux
//! marges.

use crate::stt::detector::{SpeechDetector, FRAME};
use anyhow::Result;

pub const SILENCE_FRAMES: usize = 13;
pub const MIN_SPEECH: usize = 4_000;
pub const MARGIN: usize = 3_200;
pub const MAX_SEGMENT: usize = 192_000;
pub const CUT_SEARCH: usize = 48_000;

/// Un énoncé : sa position (en échantillons depuis le début de la dictée) et
/// son audio. `forced` : il finit sur une coupe forcée, pas sur un silence.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: u64,
    pub samples: Vec<f32>,
    pub forced: bool,
}

pub struct Segmenter {
    detector: Box<dyn SpeechDetector>,
    /// Échantillons reçus mais pas encore une trame entière (moins de `FRAME`).
    pending: Vec<f32>,
    /// L'audio gardé : hors énoncé, les `MARGIN` derniers échantillons ; dans
    /// un énoncé, tout l'énoncé en cours.
    buf: Vec<f32>,
    /// Position de `buf[0]` depuis le début de la dictée.
    buf_start: u64,
    state: Option<Open>,
}

/// L'énoncé en cours. Les indices sont ceux de `buf`.
struct Open {
    /// Début de la première parole ; 0 pour la suite d'une coupe forcée.
    speech_start: usize,
    /// Fin de la dernière trame de parole.
    last_speech_end: usize,
    silent_run: usize,
    /// Suite d'une coupe forcée : pas de durée minimale de parole.
    continued: bool,
    /// Fin (indice dans `buf`) et énergie moyenne de chaque trame.
    frames: Vec<(usize, f32)>,
}

impl Segmenter {
    pub fn new(detector: Box<dyn SpeechDetector>) -> Self {
        Self {
            detector,
            pending: Vec::new(),
            buf: Vec::new(),
            buf_start: 0,
            state: None,
        }
    }

    /// Donne de l'audio ; rend les énoncés refermés. Une erreur du détecteur
    /// arrête la découpe : les énoncés déjà refermés dans cet appel sont perdus.
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Segment>> {
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / FRAME * FRAME;
        let mut out = Vec::new();
        let mut used = 0;
        let mut result = Ok(());
        while used < whole {
            let frame: Vec<f32> = self.pending[used..used + FRAME].to_vec();
            used += FRAME;
            if let Err(err) = self.frame(&frame, &mut out) {
                result = Err(err);
                break;
            }
        }
        self.pending.drain(..used);
        result.map(|()| out)
    }

    /// L'énoncé en cours : sa marge d'attaque et tout ce qui a été entendu.
    pub fn open(&self) -> Option<&[f32]> {
        self.state.as_ref().map(|_| self.buf.as_slice())
    }

    /// Fin de l'audio : rend l'énoncé resté ouvert, s'il y avait de la parole.
    pub fn finish(&mut self) -> Result<Option<Segment>> {
        let tail = std::mem::take(&mut self.pending);
        let mut segment = None;
        if let Some(open) = self.state.as_mut() {
            self.buf.extend_from_slice(&tail);
            if open.silent_run == 0 {
                open.last_speech_end = self.buf.len();
            }
            segment = self.close(false);
        }
        self.buf.clear();
        self.state = None;
        self.detector.reset();
        Ok(segment)
    }

    /// Échantillons gardés : l'audio retenu et la trame en cours de remplissage.
    pub fn buffered(&self) -> usize {
        self.buf.len() + self.pending.len()
    }

    fn frame(&mut self, frame: &[f32], out: &mut Vec<Segment>) -> Result<()> {
        let speech = self.detector.is_speech(frame)?;
        self.buf.extend_from_slice(frame);
        let end = self.buf.len();
        let energy = frame.iter().map(|x| x * x).sum::<f32>() / FRAME as f32;
        // Une trame illisible n'est jamais la plus calme.
        let energy = if energy.is_finite() { energy } else { f32::MAX };

        match (&mut self.state, speech) {
            (None, false) => {
                self.trim_to_margin();
                return Ok(());
            }
            (None, true) => {
                self.state = Some(Open {
                    speech_start: end - FRAME,
                    last_speech_end: end,
                    silent_run: 0,
                    continued: false,
                    frames: vec![(end, energy)],
                });
            }
            (Some(open), speech) => {
                open.frames.push((end, energy));
                if speech {
                    open.last_speech_end = end;
                    open.silent_run = 0;
                } else {
                    open.silent_run += 1;
                }
            }
        }
        let Some(open) = self.state.as_ref() else {
            return Ok(());
        };
        if open.silent_run >= SILENCE_FRAMES {
            out.extend(self.close(false));
            self.trim_to_margin();
            self.state = None;
        } else if self.buf.len() >= MAX_SEGMENT {
            out.extend(self.cut());
        }
        Ok(())
    }

    /// Hors énoncé, ne garde que la marge d'attaque du prochain.
    fn trim_to_margin(&mut self) {
        if self.buf.len() > MARGIN {
            let drop = self.buf.len() - MARGIN;
            self.buf.drain(..drop);
            self.buf_start += drop as u64;
        }
    }

    /// Referme l'énoncé en cours, sans toucher au tampon.
    fn close(&mut self, forced: bool) -> Option<Segment> {
        let open = self.state.as_ref()?;
        let span = open.last_speech_end.saturating_sub(open.speech_start);
        let worth = span >= MIN_SPEECH || (open.continued && open.last_speech_end > 0);
        if !worth {
            return None;
        }
        let end = (open.last_speech_end + MARGIN).min(self.buf.len());
        Some(Segment {
            start: self.buf_start,
            samples: self.buf[..end].to_vec(),
            forced,
        })
    }

    /// Coupe forcée : l'énoncé finit sur la trame la plus calme des
    /// `CUT_SEARCH` derniers échantillons (la plus tardive à égalité), le
    /// reste devient le début du suivant.
    fn cut(&mut self) -> Option<Segment> {
        let open = self.state.as_mut()?;
        let latest = MAX_SEGMENT.min(self.buf.len());
        let earliest = latest.saturating_sub(CUT_SEARCH);
        let at = open
            .frames
            .iter()
            .filter(|(end, _)| *end > earliest && *end <= latest)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
            .map_or(latest, |(end, _)| *end);
        let segment = Segment {
            start: self.buf_start,
            samples: self.buf[..at].to_vec(),
            forced: true,
        };
        self.buf.drain(..at);
        self.buf_start += at as u64;
        open.frames.retain(|(end, _)| *end > at);
        open.frames.iter_mut().for_each(|(end, _)| *end -= at);
        open.speech_start = 0;
        open.last_speech_end = open.last_speech_end.saturating_sub(at);
        open.continued = true;
        Some(segment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::detector::FRAME;

    /// Détecteur scripté : une trame est de la parole si elle n'est pas nulle.
    struct Scripted;
    impl SpeechDetector for Scripted {
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
            Ok(frame.iter().any(|x| *x != 0.0))
        }
        fn reset(&mut self) {}
    }

    fn segmenter() -> Segmenter {
        Segmenter::new(Box::new(Scripted))
    }

    fn silence(n: usize) -> Vec<f32> {
        vec![0.0; n]
    }

    /// De la « parole » reconnaissable échantillon par échantillon, jamais nulle.
    fn speech(n: usize, offset: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.1 + ((i + offset) % 1000) as f32 * 1e-4)
            .collect()
    }

    fn all_at_once(audio: &[f32]) -> Vec<Segment> {
        let mut seg = segmenter();
        let mut out = seg.push(audio).unwrap();
        out.extend(seg.finish().unwrap());
        out
    }

    /// Recolle les segments par leurs positions ; panique sur un trou ou un doublon.
    fn rebuilt_from(segments: &[Segment]) -> Vec<f32> {
        let mut out: Vec<f32> = Vec::new();
        let Some(first) = segments.first() else {
            return out;
        };
        let base = first.start;
        for s in segments {
            assert_eq!(
                s.start - base,
                out.len() as u64,
                "trou ou doublon à {}",
                s.start
            );
            out.extend_from_slice(&s.samples);
        }
        out
    }

    #[test]
    fn silence_alone_gives_no_segment() {
        assert!(all_at_once(&silence(16_000 * 5)).is_empty());
    }

    #[test]
    fn two_sentences_give_two_segments_with_their_margins() {
        let one = speech(FRAME * 20, 0);
        let two = speech(FRAME * 30, 7);
        let mut audio = silence(FRAME * 40);
        audio.extend(&one);
        audio.extend(silence(FRAME * 40));
        audio.extend(&two);
        audio.extend(silence(FRAME * 40));
        let segments = all_at_once(&audio);
        assert_eq!(segments.len(), 2);
        let first_at = (FRAME * 40) as u64;
        assert_eq!(segments[0].start, first_at - MARGIN as u64);
        assert_eq!(segments[0].samples.len(), MARGIN + one.len() + MARGIN);
        assert_eq!(&segments[0].samples[MARGIN..MARGIN + one.len()], &one[..]);
        let second_at = first_at + one.len() as u64 + (FRAME * 40) as u64;
        assert_eq!(segments[1].start, second_at - MARGIN as u64);
        assert_eq!(segments[1].samples.len(), MARGIN + two.len() + MARGIN);
        assert!(segments.iter().all(|s| !s.forced));
        let end_one = segments[0].start + segments[0].samples.len() as u64;
        assert!(
            end_one <= segments[1].start,
            "les marges ne se chevauchent pas ici"
        );
    }

    #[test]
    fn a_pause_shorter_than_thirteen_frames_does_not_cut() {
        let mut audio = speech(FRAME * 20, 0);
        audio.extend(silence(FRAME * (SILENCE_FRAMES - 1)));
        audio.extend(speech(FRAME * 20, 3));
        audio.extend(silence(FRAME * 40));
        assert_eq!(all_at_once(&audio).len(), 1);
        // À treize trames, elle coupe.
        let mut audio = speech(FRAME * 20, 0);
        audio.extend(silence(FRAME * SILENCE_FRAMES));
        audio.extend(speech(FRAME * 20, 3));
        audio.extend(silence(FRAME * 40));
        assert_eq!(all_at_once(&audio).len(), 2);
    }

    #[test]
    fn a_click_shorter_than_a_quarter_second_is_not_speech() {
        let mut audio = silence(FRAME * 40);
        audio.extend(speech(FRAME * 3, 0)); // 1 536 < 4 000
        audio.extend(silence(FRAME * 40));
        assert!(all_at_once(&audio).is_empty());
        let mut audio = silence(FRAME * 40);
        audio.extend(speech(FRAME * 8, 0)); // 4 096 >= 4 000
        audio.extend(silence(FRAME * 40));
        assert_eq!(all_at_once(&audio).len(), 1);
    }

    #[test]
    fn finish_returns_speech_still_open() {
        let mut seg = segmenter();
        let mut audio = silence(FRAME * 10);
        audio.extend(speech(FRAME * 20, 0));
        assert!(seg.push(&audio).unwrap().is_empty());
        assert!(seg.open().is_some_and(|o| o.len() >= FRAME * 20));
        let last = seg.finish().unwrap().expect("la parole ouverte sort");
        assert_eq!(last.start, (FRAME * 10 - MARGIN) as u64);
        assert_eq!(last.samples.len(), MARGIN + FRAME * 20);
        assert!(!last.forced);
        assert!(seg.open().is_none());
        assert!(seg.finish().unwrap().is_none());
    }

    #[test]
    fn one_sample_at_a_time_equals_all_at_once() {
        let mut audio = silence(FRAME * 7 + 100);
        audio.extend(speech(FRAME * 25 + 13, 0));
        audio.extend(silence(FRAME * 30));
        audio.extend(speech(FRAME * 9, 5));
        audio.extend(silence(FRAME * 2 + 1));
        audio.extend(speech(FRAME * 12 + 5, 9));
        let whole = all_at_once(&audio);
        let mut seg = segmenter();
        let mut single = Vec::new();
        for s in &audio {
            single.extend(seg.push(std::slice::from_ref(s)).unwrap());
        }
        single.extend(seg.finish().unwrap());
        assert!(!whole.is_empty());
        assert_eq!(whole.len(), single.len());
        for (a, b) in whole.iter().zip(&single) {
            assert_eq!(
                (a.start, a.forced, &a.samples),
                (b.start, b.forced, &b.samples)
            );
        }
    }

    #[test]
    fn endless_speech_is_cut_by_twelve_seconds() {
        let audio = speech(120 * 16_000, 0);
        let mut seg = segmenter();
        let mut segments = Vec::new();
        for chunk in audio.chunks(1_600) {
            segments.extend(seg.push(chunk).unwrap());
        }
        assert!(segments.len() >= 10);
        assert!(segments
            .iter()
            .all(|s| s.forced && s.samples.len() <= MAX_SEGMENT));
        segments.extend(seg.finish().unwrap());
        assert_eq!(rebuilt_from(&segments), audio, "ni trou ni doublon");
    }

    #[test]
    fn a_forced_cut_lands_on_the_quietest_frame_of_the_last_three_seconds() {
        // Parole sans pause ; une trame plus faible (mais non nulle) à 10 s,
        // une encore plus faible à 3 s (hors fenêtre : 12 s - 3 s = 9 s).
        let mut audio = vec![0.5f32; MAX_SEGMENT + FRAME * 20];
        let quiet_frame = 10 * 16_000 / FRAME; // la trame de 10 s
        let outside = 2 * 16_000 / FRAME;
        audio[quiet_frame * FRAME..(quiet_frame + 1) * FRAME].fill(0.02);
        audio[outside * FRAME..(outside + 1) * FRAME].fill(0.001);
        let mut seg = segmenter();
        let segments = seg.push(&audio).unwrap();
        assert_eq!(segments.len(), 1);
        assert!(segments[0].forced);
        let end = segments[0].start as usize + segments[0].samples.len();
        assert_eq!(
            end,
            (quiet_frame + 1) * FRAME,
            "la coupe suit la trame la plus calme"
        );
        // Le reste continue sans trou.
        let rest = seg.finish().unwrap().unwrap();
        assert_eq!(rest.start as usize, end);
        assert_eq!(rest.start as usize + rest.samples.len(), audio.len());
    }

    #[test]
    fn the_buffer_stays_bounded_over_an_hour() {
        let mut seg = segmenter();
        let mut max_buffered = 0;
        let sentence = speech(16_000 * 20, 0);
        let gap = silence(16_000 * 3 + 77);
        let mut fed = 0usize;
        let mut turn = 0;
        while fed < 3_600 * 16_000 {
            let block = if turn % 2 == 0 { &sentence } else { &gap };
            for chunk in block.chunks(4_000) {
                seg.push(chunk).unwrap();
                max_buffered = max_buffered.max(seg.buffered());
            }
            fed += block.len();
            turn += 1;
        }
        assert!(
            max_buffered <= MAX_SEGMENT + 2 * MARGIN + FRAME,
            "{max_buffered}"
        );
        // Le silence seul garde aussi peu.
        let mut seg = segmenter();
        for _ in 0..1_000 {
            seg.push(&silence(16_000)).unwrap();
            assert!(seg.buffered() <= MARGIN + FRAME);
        }
    }

    #[test]
    fn empty_input_and_a_detector_error_do_not_panic() {
        struct Failing;
        impl SpeechDetector for Failing {
            fn is_speech(&mut self, _: &[f32]) -> Result<bool> {
                anyhow::bail!("détecteur en panne")
            }
            fn reset(&mut self) {}
        }
        let mut seg = segmenter();
        assert!(seg.push(&[]).unwrap().is_empty());
        assert!(seg.finish().unwrap().is_none());
        let mut seg = Segmenter::new(Box::new(Failing));
        assert!(seg.push(&[0.0; FRAME]).is_err());
    }

    #[test]
    fn nan_samples_do_not_panic() {
        let mut audio = speech(FRAME * 20, 0);
        audio[100] = f32::NAN;
        audio.extend(silence(FRAME * 20));
        let mut seg = segmenter();
        let _ = seg.push(&audio).unwrap();
        let _ = seg.finish().unwrap();
    }
}
