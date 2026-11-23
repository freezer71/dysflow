//! Le traitement du signal commun aux modèles de dictée MLX, port de
//! `macos/Vendor/KokoroMLX/STT/DSP.swift` : fenêtres, banc de filtres mel
//! (HTK ou Slaney), STFT (marge en miroir ou à zéro), puissance en décibels
//! et mel « façon Whisper ». Les tables (fenêtres, filtres) se calculent sur
//! le processeur en `f32`, avec les mêmes fonctions de `libm` que le Swift
//! (`cosf`, `logf`, `expf`, `powf`) : les mêmes valeurs à l'ulp près.

use anyhow::{ensure, Result};
use mlx_rs::fft::rfft;
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

/// `Float.pi` du Swift, arrondi vers zéro (`0x1.921fb4p1`, 3,1415925) et
/// non au plus près comme `std::f32::consts::PI` (3,1415927) : les fenêtres
/// du Swift en dépendent, à une ulp près sur la moitié de leurs points, et le
/// mel avec elles (tâche 14).
pub const SWIFT_PI: f32 = f32::from_bits(0x4049_0FDA);

/// Fenêtre de Hann. `periodic` : celle de `torch.hann_window` (N + 1
/// points, le dernier retiré) ; sinon la symétrique, défaut du Swift.
pub fn hanning_window(size: usize, periodic: bool) -> Array {
    Array::from_slice(&hanning_values(size, periodic), &[len_i32(size)])
}

/// Les valeurs de `hanning_window`.
pub fn hanning_values(size: usize, periodic: bool) -> Vec<f32> {
    let effective = if periodic { size + 1 } else { size };
    let denom = effective as f32 - 1.0;
    (0..size)
        .map(|n| 0.5 * (1.0 - (2.0 * SWIFT_PI * n as f32 / denom).cos()))
        .collect()
}

/// Fenêtre de Hamming, périodique par défaut dans le Swift.
pub fn hamming_window(size: usize, periodic: bool) -> Array {
    let values = hamming_values(size, periodic);
    Array::from_slice(&values, &[len_i32(values.len())])
}

/// Les valeurs de `hamming_window` : aucune pour 0, `[1]` pour 1.
pub fn hamming_values(size: usize, periodic: bool) -> Vec<f32> {
    match size {
        0 => Vec::new(),
        1 => vec![1.0],
        _ => {
            let effective = if periodic { size + 1 } else { size };
            let denom = effective as f32 - 1.0;
            let mut values: Vec<f32> = (0..effective)
                .map(|n| 0.54 - 0.46 * (2.0 * SWIFT_PI * n as f32 / denom).cos())
                .collect();
            values.truncate(size);
            values
        }
    }
}

/// L'échelle mel d'un banc de filtres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MelScale {
    /// `mel = 2595 · log10(1 + f / 700)`.
    Htk,
    /// Auditory Toolbox de Slaney : linéaire sous 1 000 Hz, logarithmique
    /// au-dessus.
    Slaney,
}

/// La marge d'une STFT centrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadMode {
    /// En miroir (Whisper).
    Reflect,
    /// À zéro (NeMo : Parakeet, Sortformer).
    Constant,
}

/// Le spectre de puissance en décibels, `10 · log10(max(x, amin))`, borné à
/// `top_db` sous le maximum si donné.
pub fn power_to_db(spectrogram: &Array, amin: f32, top_db: Option<f32>) -> Result<Array> {
    let db = ops::maximum(spectrogram, Array::from_f32(amin))?
        .log10()?
        .multiply(Array::from_f32(10.0))?;
    match top_db {
        Some(top) => Ok(ops::maximum(
            &db,
            db.max(None)?.subtract(Array::from_f32(top))?,
        )?),
        None => Ok(db),
    }
}

/// Le banc de filtres mel `[n_fft / 2 + 1, n_mels]` (`melFilters` du Swift),
/// normalisé à la Slaney si `slaney_norm`.
pub fn mel_filters(
    sample_rate: u32,
    n_fft: usize,
    n_mels: usize,
    f_min: f32,
    f_max: Option<f32>,
    slaney_norm: bool,
    scale: MelScale,
) -> Array {
    let values = mel_filter_values(sample_rate, n_fft, n_mels, f_min, f_max, slaney_norm, scale);
    Array::from_slice(&values, &[len_i32(n_fft / 2 + 1), len_i32(n_mels)])
}

/// Les valeurs de `mel_filters`, ligne par ligne (fréquence, puis mel).
pub fn mel_filter_values(
    sample_rate: u32,
    n_fft: usize,
    n_mels: usize,
    f_min: f32,
    f_max: Option<f32>,
    slaney_norm: bool,
    scale: MelScale,
) -> Vec<f32> {
    let f_max = f_max.unwrap_or(sample_rate as f32 / 2.0);
    let n_freqs = n_fft / 2 + 1;
    let all_freqs: Vec<f32> = (0..n_freqs)
        .map(|i| i as f32 * sample_rate as f32 / n_fft as f32)
        .collect();

    let f_sp: f32 = 200.0 / 3.0;
    let min_log_hz: f32 = 1000.0;
    let min_log_mel = (min_log_hz - f_min) / f_sp;
    let log_step = 6.4f32.ln() / 27.0;
    let hz_to_mel = |freq: f32| match scale {
        MelScale::Htk => 2595.0 * (1.0 + freq / 700.0).log10(),
        MelScale::Slaney if freq < min_log_hz => (freq - f_min) / f_sp,
        MelScale::Slaney => min_log_mel + (freq / min_log_hz).ln() / log_step,
    };
    let mel_to_hz = |mel: f32| match scale {
        MelScale::Htk => 700.0 * (10f32.powf(mel / 2595.0) - 1.0),
        MelScale::Slaney if mel < min_log_mel => f_min + f_sp * mel,
        MelScale::Slaney => min_log_hz * (log_step * (mel - min_log_mel)).exp(),
    };

    let (m_min, m_max) = (hz_to_mel(f_min), hz_to_mel(f_max));
    let f_pts: Vec<f32> = (0..n_mels + 2)
        .map(|i| mel_to_hz(m_min + i as f32 * (m_max - m_min) / (n_mels + 1) as f32))
        .collect();

    let mut bank = vec![0f32; n_freqs * n_mels];
    for (i, &freq) in all_freqs.iter().enumerate() {
        for j in 0..n_mels {
            let (low, center, high) = (f_pts[j], f_pts[j + 1], f_pts[j + 2]);
            let cell = &mut bank[i * n_mels + j];
            if freq >= low && freq < center {
                *cell = (freq - low) / (center - low);
            } else if freq >= center && freq <= high {
                *cell = (high - freq) / (high - center);
            }
        }
    }
    if slaney_norm {
        for j in 0..n_mels {
            let enorm = 2.0 / (f_pts[j + 2] - f_pts[j]);
            for i in 0..n_freqs {
                bank[i * n_mels + j] *= enorm;
            }
        }
    }
    bank
}

/// `audio` à l'envers sur son premier axe.
fn reversed(audio: &Array) -> Result<Array> {
    let len = audio.shape().first().copied().unwrap_or(0);
    let indices: Vec<i32> = (0..len).rev().collect();
    Ok(audio.take_axis(Array::from_slice(&indices, &[len]), 0)?)
}

/// STFT centrée d'un signal 1-D : marge de `n_fft / 2` de chaque côté,
/// trames de `n_fft` tous les `hop` (vue à pas, sans copie), fenêtre, puis
/// `rfft` : `[trames, n_fft / 2 + 1]`, complexe.
pub fn stft(
    audio: &Array,
    window: &Array,
    n_fft: usize,
    hop: usize,
    mode: PadMode,
) -> Result<Array> {
    ensure!(
        audio.ndim() == 1,
        "STFT : un signal 1-D attendu, pas {:?}",
        audio.shape()
    );
    ensure!(n_fft > 0 && hop > 0, "STFT : n_fft {n_fft} et pas {hop}");
    let padding = len_i32(n_fft / 2);
    let len = audio.shape()[0];
    let padded = match mode {
        PadMode::Reflect => {
            let prefix = reversed(&audio.try_index(1..(padding + 1).min(len))?)?;
            let suffix = reversed(&audio.try_index((len - padding - 1).max(0)..(len - 1).max(1))?)?;
            ops::concatenate(&[prefix, audio.clone(), suffix], 0)?
        }
        PadMode::Constant => {
            let zeros = Array::zeros::<f32>(&[padding])?;
            ops::concatenate(&[zeros.clone(), audio.clone(), zeros], 0)?
        }
    };
    let padded_len = padded.shape()[0];
    let (n_fft, hop) = (len_i32(n_fft), len_i32(hop));
    ensure!(
        padded_len >= n_fft,
        "STFT : {len} échantillons, trop peu pour une trame de {n_fft}"
    );
    let frames = 1 + (padded_len - n_fft) / hop;
    let framed = ops::as_strided(&padded, &[frames, n_fft][..], &[i64::from(hop), 1][..], 0)?;
    Ok(rfft(framed.multiply(window)?, None, 1)?)
}

/// Le mel « façon Whisper » (`computeMelSpectrogram`) d'un signal 1-D :
/// Hann, STFT en miroir, puissance, filtres de Slaney normalisés, `log10`
/// borné à 8 sous le maximum, puis `(x + 4) / 4`. Un tableau 2-D est
/// supposé déjà un spectrogramme et rendu tel quel.
pub fn compute_mel_spectrogram(
    audio: &Array,
    sample_rate: u32,
    n_fft: usize,
    hop: usize,
    n_mels: usize,
    scale: MelScale,
    hann_periodic: bool,
) -> Result<Array> {
    if audio.ndim() != 1 {
        return Ok(audio.clone());
    }
    let window = hanning_window(n_fft, hann_periodic);
    let magnitudes = stft(audio, &window, n_fft, hop, PadMode::Reflect)?
        .abs()?
        .square()?;
    mlx_rs::transforms::eval([&magnitudes])?;
    let filters = mel_filters(sample_rate, n_fft, n_mels, 0.0, None, true, scale);
    let mel = magnitudes.matmul(&filters)?;
    mlx_rs::transforms::eval([&mel])?;
    let mel = ops::maximum(&mel, Array::from_f32(1e-10))?.log10()?;
    let mel = ops::maximum(&mel, mel.max(None)?.subtract(Array::from_f32(8.0))?)?;
    Ok(mel
        .add(Array::from_f32(4.0))?
        .divide(Array::from_f32(4.0))?)
}

/// Une longueur de table en dimension MLX. Les tables d'ici (fenêtres,
/// filtres) ont quelques milliers d'éléments au plus.
fn len_i32(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_du_swift() {
        // `Float.pi` du Swift : `0x1.921fb4p1`, une ulp sous le `PI` de Rust.
        assert_eq!(SWIFT_PI, 3.1415925);
        assert_eq!(SWIFT_PI.to_bits() + 1, std::f32::consts::PI.to_bits());
    }

    #[test]
    fn hann_symetrique_et_periodique() {
        let sym = hanning_values(5, false);
        assert_eq!(sym[0], 0.0);
        assert!((sym[2] - 1.0).abs() < 1e-6);
        assert!(sym[4].abs() < 1e-6);
        let per = hanning_values(4, true);
        assert!((per[2] - 1.0).abs() < 1e-6, "{per:?}");
    }

    #[test]
    fn hamming_bornes() {
        assert!(hamming_values(0, true).is_empty());
        assert_eq!(hamming_values(1, true), vec![1.0]);
        let sym = hamming_values(3, false);
        assert!((sym[0] - 0.08).abs() < 1e-6 && (sym[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn filtres_de_slaney() {
        let (n_fft, n_mels) = (512, 128);
        let bank = mel_filter_values(16_000, n_fft, n_mels, 0.0, None, true, MelScale::Slaney);
        assert_eq!(bank.len(), (n_fft / 2 + 1) * n_mels);
        assert!(bank.iter().all(|v| v.is_finite() && *v >= 0.0));
        // Chaque filtre a au moins une fréquence non nulle.
        for j in 0..n_mels {
            assert!(
                (0..=n_fft / 2).any(|i| bank[i * n_mels + j] > 0.0),
                "filtre {j} vide"
            );
        }
        let htk = mel_filter_values(16_000, n_fft, 80, 0.0, None, false, MelScale::Htk);
        assert!(htk.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
