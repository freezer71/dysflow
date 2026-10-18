//! Le son synthétisé, tel qu'il passe de la synthèse à la sortie.

use std::sync::Arc;
use std::time::Duration;

/// Audio synthétisé, prêt pour la sortie.
#[derive(Debug, Clone)]
pub struct PcmBuffer {
    pub samples: Arc<[f32]>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Durée réelle du signal, mesurée sur les échantillons.
    ///
    /// C'est elle qui pilote le curseur de lecture, et non une estimation en
    /// mots par seconde.
    pub duration: Duration,
}

impl PcmBuffer {
    /// Le même son, privé de ses `offset` premières secondes.
    ///
    /// La coupe tombe sur une trame entière : couper entre deux canaux
    /// inverserait gauche et droite pour toute la suite.
    pub fn skip(&self, offset: Duration) -> PcmBuffer {
        let start = self.sample_index(offset);
        PcmBuffer::new(self.samples[start..].to_vec(), self.sample_rate, self.channels)
    }

    /// Indice du premier échantillon de la trame qui tombe à `offset`.
    pub fn sample_index(&self, offset: Duration) -> usize {
        let frames = (offset.as_secs_f64() * self.sample_rate as f64) as usize;
        (frames * self.channels as usize).min(self.samples.len())
    }

    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self::from_shared(samples.into(), sample_rate, channels)
    }

    /// Comme `new`, sur un tampon déjà partagé : rien n'est recopié.
    pub fn from_shared(samples: Arc<[f32]>, sample_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1);
        let sample_rate = sample_rate.max(1);
        let frames = samples.len() / channels as usize;
        let duration = Duration::from_secs_f64(frames as f64 / sample_rate as f64);
        Self {
            samples,
            sample_rate,
            channels,
            duration,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Bord d'un son à adoucir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

/// Durée du fondu à la coupe : assez court pour ne pas s'entendre comme
/// tel, assez long pour ne pas claquer.
const CUT_FADE: Duration = Duration::from_millis(5);

/// Fondu en cosinus sur `CUT_FADE` au bord `edge`, jusqu'au silence.
///
/// Tête et suite passent chacune par leur propre conversion de fréquence,
/// qui part d'un silence et y retourne : une tête coupée en pleine onde
/// claquait à la jonction dès que la carte son ne tourne pas à la fréquence
/// de la synthèse (24 kHz contre 48 kHz, le cas courant sur Mac).
///
/// Sur place quand le son n'est pas partagé (sortie de synthèse), sinon sur
/// une copie.
pub fn fade_edge(mut pcm: PcmBuffer, edge: Edge) -> PcmBuffer {
    let ch = pcm.channels as usize;
    let frames = pcm.samples.len() / ch;
    let n = ((pcm.sample_rate as f64 * CUT_FADE.as_secs_f64()) as usize).min(frames);
    if n == 0 {
        return pcm;
    }
    if Arc::get_mut(&mut pcm.samples).is_none() {
        pcm.samples = Arc::from(&pcm.samples[..]);
    }
    let samples = Arc::get_mut(&mut pcm.samples).expect("tampon à soi");
    for k in 0..n {
        // `k` : distance au bord, en trames ; gain nul au bord même.
        let gain = 0.5 * (1.0 - (std::f32::consts::PI * k as f32 / n as f32).cos());
        let frame = match edge {
            Edge::Start => k,
            Edge::End => frames - 1 - k,
        };
        for sample in &mut samples[frame * ch..(frame + 1) * ch] {
            *sample *= gain;
        }
    }
    pcm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipping_drops_whole_frames_from_the_start() {
        // Stéréo, 10 trames par seconde : 0,3 s = 3 trames = 6 échantillons.
        let pcm = PcmBuffer::new((0..20).map(|i| i as f32).collect(), 10, 2);
        let rest = pcm.skip(Duration::from_millis(300));
        assert_eq!(rest.samples.len(), 14);
        assert_eq!(rest.samples[0], 6.0);
        assert_eq!(rest.channels, 2);
    }

    #[test]
    fn a_fade_leaves_a_shared_sound_untouched() {
        let pcm = PcmBuffer::new(vec![1.0; 1000], 1000, 1);
        let kept = pcm.clone();
        let faded = fade_edge(pcm, Edge::Start);
        assert_eq!(faded.samples[0], 0.0);
        assert_eq!(kept.samples[0], 1.0, "le son partagé a été modifié");
        assert_eq!(faded.samples[999], 1.0);
        assert_eq!(faded.duration, kept.duration);
    }

    #[test]
    fn skipping_past_the_end_leaves_silence() {
        let pcm = PcmBuffer::new(vec![0.5; 10], 10, 1);
        assert!(pcm.skip(Duration::from_secs(5)).is_empty());
    }
}
