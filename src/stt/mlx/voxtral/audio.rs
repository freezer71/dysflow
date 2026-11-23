//! Le mel de Voxtral en flux (`VoxtralRealtimeAudio.swift`) : le banc de
//! filtres de Slaney (0 à 8 000 Hz), et `VoxtralRealtimeMelStream`, qui ne
//! calcule une trame qu'une fois sa fenêtre entière reçue. La fenêtre de
//! Hann (périodique) et le reste sont calculés par MLX, comme le Swift, avec
//! son `Float.pi` (`dsp::SWIFT_PI`).

use anyhow::Result;
use mlx_rs::fft::rfft;
use mlx_rs::{ops, Array};

use super::config::AudioConfig;
use crate::stt::mlx::dsp::{self, MelScale, SWIFT_PI};
use crate::stt::mlx::nn::scalar;

/// `computeMelFilters` : `[window / 2 + 1, bandes]`, Slaney normalisé,
/// 0 à 8 000 Hz.
pub fn mel_filters(audio: &AudioConfig) -> Array {
    dsp::mel_filters(
        audio.sampling_rate as u32,
        audio.window_size as usize,
        audio.num_mel_bins as usize,
        0.0,
        Some(8000.0),
        true,
        MelScale::Slaney,
    )
}

/// La fenêtre de Hann périodique, en MLX : `0.5 · (1 − cos(2π n / N))`.
fn hann(window_size: i32) -> Result<Array> {
    let n = super::encoder::range(0, window_size).as_dtype(mlx_rs::Dtype::Float32)?;
    let angle = scalar(2.0 * SWIFT_PI)
        .multiply(&n)?
        .divide(scalar(window_size as f32))?;
    Ok(scalar(0.5).multiply(scalar(1.0).subtract(ops::cos(&angle)?)?)?)
}

/// `melColumns` : des trames `[n, window]` déjà découpées → `[bandes, n]`.
pub fn mel_columns(
    frames: &Array,
    filters: &Array,
    window_size: i32,
    global_log_mel_max: f32,
) -> Result<Array> {
    let window = hann(window_size)?;
    let windowed = frames.multiply(window.expand_dims(0)?)?;
    let spectrum = rfft(&windowed, None, -1)?;
    let magnitudes = ops::abs(&spectrum)?.square()?.transpose_axes(&[1, 0])?;
    let mel = filters.transpose_axes(&[1, 0])?.matmul(&magnitudes)?;
    let mel = ops::maximum(&mel, scalar(1e-10))?;
    let log = mel.log10()?;
    let log = ops::maximum(&log, scalar(global_log_mel_max - 8.0))?;
    Ok(log.add(scalar(4.0))?.divide(scalar(4.0))?)
}

/// `VoxtralRealtimeMelStream` : la contrepartie incrémentale du mel hors
/// ligne. Hors ligne, la trame `t` couvre les échantillons `[t·hop −
/// window/2, t·hop + window/2)` ; les deux marges en miroir se réduisent à
/// des zéros sous la marge à zéro de la session : la retenue part de
/// `window/2` zéros, et une trame ne sort qu'une fois sa fenêtre entière
/// reçue — un audio plus tardif ne tombe jamais dans une fenêtre déjà
/// sortie. `finish_tail_pad` zéros de plus à la fin donnent exactement le
/// nombre de trames hors ligne.
pub struct MelStream {
    filters: Array,
    window_size: usize,
    hop: usize,
    global_log_mel_max: f32,
    carry: Vec<f32>,
}

impl MelStream {
    /// `left_pad` doit dépasser `window / 2` (vérifié par `Config::check`).
    pub fn new(left_pad: usize, filters: Array, audio: &AudioConfig) -> Self {
        let window_size = audio.window_size as usize;
        Self {
            filters,
            window_size,
            hop: audio.hop_length as usize,
            global_log_mel_max: audio.global_log_mel_max,
            // `window/2` zéros de la marge en miroir + ceux de la marge à
            // gauche du flux.
            carry: vec![0.0; window_size / 2 + left_pad],
        }
    }

    /// Les zéros que la lecture hors ligne lit après la fin du flux ; à
    /// ajouter à `finish` seulement.
    pub fn finish_tail_pad(&self) -> usize {
        self.window_size - self.hop - self.window_size / 2
    }

    /// Ajoute des échantillons ; les colonnes `[bandes, n]` de chaque
    /// fenêtre nouvellement complète, ou rien tant qu'il en manque une.
    pub fn append(&mut self, samples: &[f32]) -> Result<Option<Array>> {
        self.carry.extend_from_slice(samples);
        if self.carry.len() < self.window_size {
            return Ok(None);
        }
        let frames = 1 + (self.carry.len() - self.window_size) / self.hop;
        let used = (frames - 1) * self.hop + self.window_size;
        let framed = Array::from_slice(&self.carry[..used], &[used as i32]);
        self.carry.drain(..frames * self.hop);
        let strided = ops::as_strided(
            &framed,
            &[frames as i32, self.window_size as i32][..],
            &[self.hop as i64, 1][..],
            0,
        )?;
        Ok(Some(mel_columns(
            &strided,
            &self.filters,
            self.window_size as i32,
            self.global_log_mel_max,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::mlx::voxtral::config::Config;

    /// Une trame sort dès que sa fenêtre est complète : 200 zéros de marge
    /// en miroir et 32 · 1 280 de marge à gauche, puis une trame tous les
    /// 160 échantillons ; le découpage des ajouts ne change pas le nombre de
    /// colonnes, et leurs valeurs à un arrondi près.
    #[test]
    fn a_frame_comes_out_once_its_window_is_complete_whatever_the_split() {
        let _serial = crate::stt::mlx::parakeet::tests::serial();
        let (shape, worst) = super::super::synthetic::on_the_cpu_thread(|| {
            let config = Config::from_json(&serde_json::json!({}))?;
            let audio = &config.audio;
            let left = 32 * 1_280;
            let filters = mel_filters(audio);
            let mut whole = MelStream::new(left, filters.clone(), audio);
            anyhow::ensure!(whole.finish_tail_pad() == 40);
            let signal: Vec<f32> = (0..3_000)
                .map(|i| ((i * 37 % 101) as f32 - 50.0) / 60.0)
                .collect();
            let all = whole
                .append(&signal)?
                .ok_or_else(|| anyhow::anyhow!("rien"))?;
            let mut pieces = MelStream::new(left, filters, audio);
            let mut columns = Vec::new();
            for part in signal.chunks(7) {
                columns.extend(pieces.append(part)?);
            }
            let joined = ops::concatenate(&columns, 1)?;
            mlx_rs::transforms::eval([&all, &joined])?;
            anyhow::ensure!(joined.shape() == all.shape(), "{:?}", joined.shape());
            let worst = all
                .as_slice::<f32>()
                .iter()
                .zip(joined.as_slice::<f32>())
                .map(|(x, y)| (x - y).abs())
                .fold(0f32, f32::max);
            Ok((all.shape().to_vec(), worst))
        })
        .unwrap();
        // ⌊(200 + 40 960 + 3 000 − 400) / 160⌋ + 1 = 274 trames.
        assert_eq!(shape, [128, 274]);
        // Le produit par les filtres dépend du nombre de colonnes, même sur
        // le processeur : un arrondi de Float32 au plus (mesuré : 57 valeurs
        // sur 35 072, de 1,2e-7). Le Swift découpe pareil : la parité compare
        // au même découpage, celui du pas de la dictée.
        assert!(worst <= f32::EPSILON, "écart {worst:e}");
    }
}
