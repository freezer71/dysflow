//! Le mel logarithmique de Whisper, comme `WhisperAudio.swift` : la fenêtre
//! de 30 s (coupée ou complétée de zéros), Hann périodique calculée par MLX,
//! STFT à marge en miroir, dernière trame retirée, filtres de Slaney,
//! `log10` borné à 8 sous le maximum, puis `(x + 4) / 4`. Tout en `f32`.

use anyhow::{ensure, Result};
use mlx_rs::fft::rfft;
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use crate::stt::mlx::dsp::{self, MelScale, SWIFT_PI};

/// `WhisperAudioConfig`.
pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 400;
pub const HOP_LENGTH: usize = 160;
/// Une fenêtre de 30 s : 480 000 échantillons.
pub const CHUNK_SAMPLES: usize = 30 * SAMPLE_RATE as usize;

/// `padOrTrimToWindow` : `audio` coupé ou complété de zéros à 30 s.
fn pad_or_trim(audio: &[f32]) -> Array {
    let mut window = audio[..audio.len().min(CHUNK_SAMPLES)].to_vec();
    window.resize(CHUNK_SAMPLES, 0.0);
    Array::from_slice(&window, &[CHUNK_SAMPLES as i32])
}

/// La fenêtre de Hann de `logMelSpectrogram`, calculée par MLX comme le
/// Swift : `0.5 · (1 − cos((2π · n) / N))`, `π` étant `Float.pi`.
pub fn hann_window() -> Result<Array> {
    let n: Vec<f32> = (0..N_FFT).map(|i| i as f32).collect();
    let n = Array::from_slice(&n, &[N_FFT as i32]);
    let angle = Array::from_f32(2.0 * SWIFT_PI)
        .multiply(&n)?
        .divide(Array::from_f32(N_FFT as f32))?;
    Ok(Array::from_f32(0.5).multiply(Array::from_f32(1.0).subtract(ops::cos(&angle)?)?)?)
}

/// `reflectPad` d'un signal de plus d'un échantillon : `pad` points en miroir
/// de chaque côté (sans répéter le bord), complétés de zéros si le signal est
/// trop court.
fn reflect_pad(audio: &Array, pad: i32) -> Result<Array> {
    let n = audio.shape().first().copied().unwrap_or(0);
    ensure!(
        n > 1,
        "Whisper : {n} échantillon(s), trop peu pour la marge en miroir"
    );
    let count = pad.min(n - 1);
    let left = dsp::reversed(&audio.try_index(1..count + 1)?)?;
    let right = dsp::reversed(&audio.try_index(n - 1 - count..n - 1)?)?;
    let mut pieces = Vec::new();
    if count < pad {
        pieces.push(Array::zeros::<f32>(&[pad - count])?);
    }
    pieces.extend([left, audio.clone(), right]);
    if count < pad {
        pieces.push(Array::zeros::<f32>(&[pad - count])?);
    }
    Ok(ops::concatenate(&pieces, 0)?)
}

/// `logMelSpectrogram` d'un signal de 30 s : `[bandes, 3000]`.
fn log_mel_spectrogram(audio: &Array, n_mels: i32) -> Result<Array> {
    let window = hann_window()?;
    let padded = reflect_pad(audio, (N_FFT / 2) as i32)?;
    let samples = padded.shape()[0];
    let (n_fft, hop) = (N_FFT as i32, HOP_LENGTH as i32);
    let frames = 1 + (samples - n_fft) / hop;
    let framed = ops::as_strided(&padded, &[frames, n_fft][..], &[i64::from(hop), 1][..], 0)?;
    let windowed = framed.multiply(window.expand_dims(0)?)?;
    let spectrum = rfft(&windowed, None, -1)?;
    let magnitudes = ops::abs(&spectrum)?.square()?;
    // La dernière trame retirée, comme `torch.stft(center=True)`.
    let magnitudes = magnitudes
        .try_index((..frames - 1, ..))?
        .transpose_axes(&[1, 0])?;
    let filters = dsp::mel_filters(
        SAMPLE_RATE,
        N_FFT,
        n_mels as usize,
        0.0,
        Some(SAMPLE_RATE as f32 / 2.0),
        true,
        MelScale::Slaney,
    );
    let mel = filters.transpose_axes(&[1, 0])?.matmul(&magnitudes)?;
    let mel = ops::maximum(&mel, Array::from_f32(1e-10))?;
    let log = mel.log10()?;
    let floor = log.max(None)?.subtract(Array::from_f32(8.0))?;
    let log = ops::maximum(&log, &floor)?;
    Ok(log
        .add(Array::from_f32(4.0))?
        .divide(Array::from_f32(4.0))?)
}

/// `encoderFeatures` : la fenêtre de 30 s de `audio`, son mel, transposé :
/// `[1, 3000, bandes]`.
pub fn features(audio: &[f32], n_mels: i32) -> Result<Array> {
    let mel = log_mel_spectrogram(&pad_or_trim(audio), n_mels)?;
    Ok(mel.transpose_axes(&[1, 0])?.expand_dims(0)?)
}

/// Les fenêtres de 30 s de `generate` (`chunkAudioFor30sWindows`) : l'audio
/// entier s'il tient dans une, sinon des tranches de 30 s à la suite (la
/// dernière plus courte).
pub fn windows(audio: &[f32]) -> Vec<&[f32]> {
    if audio.len() <= CHUNK_SAMPLES {
        return vec![audio];
    }
    audio.chunks(CHUNK_SAMPLES).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_is_cut_in_30_second_windows() {
        let long = vec![0.0f32; CHUNK_SAMPLES * 2 + 5];
        let lengths: Vec<usize> = windows(&long).iter().map(|w| w.len()).collect();
        assert_eq!(lengths, vec![CHUNK_SAMPLES, CHUNK_SAMPLES, 5]);
        assert_eq!(windows(&long[..CHUNK_SAMPLES]).len(), 1);
        assert_eq!(windows(&[]).len(), 1, "un seul passage, même vide");
    }

    /// Sur le fil MLX, comme tout calcul : les tableaux n'en sortent pas.
    #[test]
    fn features_have_3000_frames() {
        let held = crate::compute::mlx::Held::load(|| Ok(())).unwrap();
        let shapes = held
            .with(|_| {
                let mut shapes = Vec::new();
                for audio in [&[0.25f32; 16_000][..], &[0.0; 1]] {
                    let x = features(audio, 80)?;
                    x.eval()?;
                    shapes.push(x.shape().to_vec());
                }
                Ok(shapes)
            })
            .unwrap();
        assert_eq!(
            shapes,
            vec![vec![1, 3000, 80]; 2],
            "un seul échantillon : complété à 30 s"
        );
    }
}
