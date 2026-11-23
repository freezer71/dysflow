//! Le mel logarithmique de Parakeet (préprocesseur NeMo), comme
//! `ParakeetAudio.logMelSpectrogram` : préaccentuation, Hann centrée dans
//! `n_fft`, STFT à marge nulle, filtres de Slaney, `log(x + garde)`, puis
//! normalisation par bande. Calculé au type de l'audio (`f32`).

use anyhow::Result;
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use super::config::PreprocessConfig;
use crate::stt::mlx::dsp::{self, MelScale, PadMode};
use crate::stt::mlx::nn::scalar_like;

/// `audio` : `[échantillons]` ; rend `[1, trames, bandes]`.
pub fn log_mel_spectrogram(audio: &Array, config: &PreprocessConfig) -> Result<Array> {
    let original = audio.dtype();
    let mut x = audio.clone();
    let len = x.shape()[0] as usize;

    if config.pad_to > 0 && len < config.pad_to {
        let tail = vec![config.pad_value; config.pad_to - len];
        let tail = Array::from_slice(&tail, &[tail.len() as i32]);
        x = ops::concatenate(&[x, tail], 0)?;
    }

    let len = x.shape()[0];
    if config.preemph > 0.0 && len > 1 {
        let first = x.try_index(0..1)?;
        let previous = x
            .try_index(..len - 1)?
            .multiply(Array::from_f32(config.preemph))?;
        let rest = x.try_index(1..)?.subtract(previous)?;
        x = ops::concatenate(&[first, rest], 0)?;
    }

    let window = window(&config.window, config.win_length(), config.n_fft);
    let spectrum = dsp::stft(
        &x,
        &window,
        config.n_fft,
        config.hop_length(),
        PadMode::Constant,
    )?;
    let power = spectrum.abs()?.square()?.as_dtype(original)?;
    let filters = dsp::mel_filters(
        config.sample_rate,
        config.n_fft,
        config.features as usize,
        0.0,
        None,
        true,
        MelScale::Slaney,
    );
    let mel = power.matmul(filters.as_dtype(power.dtype())?)?;
    let mel = mel
        .add(Array::from_f32(config.log_zero_guard_value).as_dtype(mel.dtype())?)?
        .log()?;

    let eps = Array::from_f32(1e-5).as_dtype(mel.dtype())?;
    let normalized = if config.normalize == "per_feature" {
        let mean = mel.mean_axis(0, true)?;
        let denominator = (mel.shape()[0] - 1).max(1) as f32;
        let centered = mel.subtract(&mean)?;
        let variance = centered
            .square()?
            .sum_axis(0, true)?
            .divide(scalar_like(denominator, &mel)?)?;
        centered.divide(variance.sqrt()?.add(&eps)?)?
    } else {
        let mean = mel.mean(None)?;
        let std = ops::std(&mel, None, None)?;
        mel.subtract(&mean)?.divide(std.add(&eps)?)?
    };
    Ok(normalized.expand_dims(0)?.as_dtype(original)?)
}

/// La fenêtre nommée, de `win_length` points, centrée dans `n_fft` (zéros
/// de part et d'autre), ou coupée à `n_fft` si plus longue. Hann pour un
/// nom inconnu, comme le Swift.
fn window(name: &str, win_length: usize, n_fft: usize) -> Array {
    let base = window_values(name, win_length);
    let values: Vec<f32> = if win_length >= n_fft {
        base[..n_fft].to_vec()
    } else {
        let left = (n_fft - win_length) / 2;
        let right = n_fft - win_length - left;
        std::iter::repeat_n(0.0, left)
            .chain(base)
            .chain(std::iter::repeat_n(0.0, right))
            .collect()
    };
    Array::from_slice(&values, &[values.len() as i32])
}

fn window_values(name: &str, size: usize) -> Vec<f32> {
    let lower = name.to_lowercase();
    if lower != "hann" && lower != "hanning" && size <= 1 {
        return vec![1.0; size.max(1)];
    }
    let denom = size as f32 - 1.0;
    let tau = 2.0 * dsp::SWIFT_PI;
    match lower.as_str() {
        "hamming" => (0..size)
            .map(|n| 0.54 - 0.46 * (tau * n as f32 / denom).cos())
            .collect(),
        "blackman" => (0..size)
            .map(|n| {
                let k = tau * n as f32 / denom;
                0.42 - 0.5 * k.cos() + 0.08 * (2.0 * k).cos()
            })
            .collect(),
        "bartlett" => {
            let mid = (size as f32 - 1.0) / 2.0;
            (0..size)
                .map(|n| 1.0 - ((n as f32 - mid) / mid).abs())
                .collect()
        }
        _ => dsp::hanning_values(size, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_centree() {
        let values = window_values("hann", 400);
        assert_eq!(values.len(), 400);
        let w = window("hann", 400, 512);
        assert_eq!(w.shape(), &[512]);
        assert_eq!(window_values("bartlett", 1), vec![1.0]);
        assert!((window_values("hamming", 3)[1] - 1.0).abs() < 1e-6);
    }
}
