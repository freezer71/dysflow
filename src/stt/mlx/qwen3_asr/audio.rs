//! L'audio de Qwen3-ASR, comme `Qwen3ASR.swift` : la découpe des longs
//! audios aux silences (`splitAudioIntoChunks`), le mel « façon Whisper »
//! (`preprocessAudio` : Slaney, Hann périodique), le nombre de jetons audio
//! (`getFeatExtractOutputLengths`) et le plan des fenêtres de l'encodeur
//! (`computeChunkedEncoderWindowLengths`). Tout ici, hors du mel, se calcule
//! sur le processeur, avec les mêmes types que le Swift.

use anyhow::Result;
use mlx_rs::Array;

use crate::stt::mlx::dsp::{compute_mel_spectrogram, MelScale};

/// Les échantillons par seconde (`Qwen3ASRModel.sampleRate`).
pub const SAMPLE_RATE: usize = 16_000;
const N_FFT: usize = 400;
const HOP: usize = 160;

/// `featureLengthAfterConvStem` : trois divisions par deux, arrondies vers
/// le haut.
pub fn length_after_conv_stem(length: i32) -> i32 {
    (0..3).fold(length, |n, _| (n + 1) / 2)
}

/// `getFeatExtractOutputLengths` d'une longueur (en trames) : les jetons
/// audio de l'invite. Le Swift calcule `floor(a / b)` en flottants ; ici la
/// division entière vers le bas, qui rend la même chose (les longueurs
/// tiennent largement dans la mantisse d'un `f32`).
pub fn feat_extract_output_length(frames: i64) -> i64 {
    let floor_div = |a: i64, b: i64| a.div_euclid(b);
    let leave = frames % 100;
    let feat = floor_div(leave - 1, 2) + 1;
    floor_div(floor_div(feat - 1, 2) + 1 - 1, 2) + 1 + floor_div(frames, 100) * 13
}

/// `computeChunkedEncoderWindowLengths` : les longueurs (après la pile de
/// convolutions) de chaque tranche, regroupées par `chunks_per_window`
/// tranches d'une même entrée ; une fenêtre vide n'est pas rendue.
pub fn window_lengths(
    after_cnn: &[i32],
    chunks_per_input: &[usize],
    chunks_per_window: usize,
) -> Vec<i32> {
    let per_window = chunks_per_window.max(1);
    let mut lengths = Vec::new();
    let mut offset = 0usize;
    for &count in chunks_per_input {
        let mut remaining = count;
        while remaining > 0 {
            let take = per_window.min(remaining);
            let end = (offset + take).min(after_cnn.len());
            if offset >= end {
                break;
            }
            let length: i32 = after_cnn[offset..end].iter().sum();
            if length > 0 {
                lengths.push(length);
            }
            offset = end;
            remaining -= take;
        }
    }
    if offset < after_cnn.len() {
        lengths.push(after_cnn[offset..].iter().sum());
    }
    lengths
}

/// Une tranche d'audio et son début, en secondes.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub samples: Vec<f32>,
    pub offset: f32,
}

/// `samples`, complété de zéros jusqu'à `min_samples` s'il est plus court.
fn padded(samples: &[f32], min_samples: usize) -> Vec<f32> {
    let mut out = samples.to_vec();
    if out.len() < min_samples {
        out.resize(min_samples, 0.0);
    }
    out
}

/// `splitAudioIntoChunks` d'un audio 1-D : tel quel s'il dure au plus
/// `chunk_duration` secondes (complété de zéros jusqu'à `min_chunk_duration`
/// s'il est plus court), sinon coupé au point de plus faible énergie (par
/// fenêtres de `min_window_ms`) à ± `search_expand_sec` de chaque limite,
/// une seconde au moins par tranche. Mêmes calculs en `f32` que le Swift,
/// dans le même ordre.
pub fn split_into_chunks(
    wav: &[f32],
    sample_rate: usize,
    chunk_duration: f32,
    min_chunk_duration: f32,
) -> Vec<Chunk> {
    const SEARCH_EXPAND_SEC: f32 = 5.0;
    const MIN_WINDOW_MS: f32 = 100.0;
    let rate = sample_rate as f32;
    let total = wav.len();
    let total_sec = total as f32 / rate;
    let min_samples = (min_chunk_duration * rate) as usize;
    if total_sec <= chunk_duration {
        let samples = if total_sec < min_chunk_duration {
            padded(wav, min_samples)
        } else {
            wav.to_vec()
        };
        return vec![Chunk {
            samples,
            offset: 0.0,
        }];
    }
    let max_chunk = (chunk_duration * rate) as usize;
    let search = (SEARCH_EXPAND_SEC * rate) as usize;
    let min_window = (MIN_WINDOW_MS * rate / 1000.0) as usize;
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < total {
        let end = (start + max_chunk).min(total);
        if end >= total {
            chunks.push(Chunk {
                samples: padded(&wav[start..total], min_samples),
                offset: start as f32 / rate,
            });
            break;
        }
        let search_start = start.max(end.saturating_sub(search));
        let search_end = total.min(end + search);
        let mut cut = if search_end - search_start > min_window {
            search_start + quietest(&wav[search_start..search_end], min_window)
        } else {
            end
        };
        cut = cut.max(start + sample_rate);
        let actual_end = cut.min(total);
        chunks.push(Chunk {
            samples: padded(&wav[start..actual_end], min_samples),
            offset: start as f32 / rate,
        });
        start = cut;
    }
    chunks
}

/// Le milieu de la fenêtre de `window` échantillons de plus faible énergie
/// moyenne (somme glissante en `f32`, comme le Swift ; la première à
/// égalité).
fn quietest(region: &[f32], window: usize) -> usize {
    let count = region.len() + 1 - window;
    let inv = 1.0 / window as f32;
    let mut sum = 0.0f32;
    for &v in &region[..window] {
        sum += v * v;
    }
    let (mut min_index, mut min_energy) = (0usize, sum * inv);
    for i in 1..count {
        let (old, new) = (region[i - 1], region[i + window - 1]);
        sum += new * new - old * old;
        let energy = sum * inv;
        if energy < min_energy {
            min_energy = energy;
            min_index = i;
        }
    }
    min_index + window / 2
}

/// `preprocessAudio` sans le masque : le mel `[1, bandes, trames]` (Float32)
/// et le nombre de jetons audio de l'invite.
pub fn preprocess(samples: &[f32], n_mels: usize) -> Result<(Array, i64)> {
    let audio = Array::from_slice(samples, &[samples.len() as i32]);
    let mel = compute_mel_spectrogram(
        &audio,
        SAMPLE_RATE as u32,
        N_FFT,
        HOP,
        n_mels,
        MelScale::Slaney,
        true,
    )?;
    let frames = mel.shape().first().copied().unwrap_or(0);
    let features = mel.transpose_axes(&[1, 0])?.expand_dims(0)?;
    Ok((features, feat_extract_output_length(i64::from(frames))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les jetons audio des longueurs de la fixture (55 pour 423 trames, 14
    /// pour 101…), et les bords (0, 1, 99, 100, 101 trames).
    #[test]
    fn the_audio_tokens_follow_the_swift_formula() {
        for (frames, tokens) in [
            (423, 55),
            (391, 51),
            (1201, 157),
            (101, 14),
            (3501, 456),
            (0, 0),
            (1, 1),
            (99, 13),
            (100, 13),
            (200, 26),
        ] {
            assert_eq!(feat_extract_output_length(frames), tokens, "{frames}");
        }
        assert_eq!(length_after_conv_stem(100), 13);
        assert_eq!(length_after_conv_stem(1), 1);
        assert_eq!(length_after_conv_stem(128), 16);
    }

    /// Huit tranches par fenêtre ; la dernière, partielle, rendue ; une
    /// fenêtre vide sautée ; le reste d'une liste plus longue que les
    /// comptes, ajouté.
    #[test]
    fn the_windows_group_eight_chunks() {
        let mut after = vec![13; 12];
        after.push(1);
        assert_eq!(window_lengths(&after, &[13], 8), vec![104, 53]);
        assert_eq!(window_lengths(&[13, 0, 0], &[1, 2], 8), vec![13]);
        assert_eq!(window_lengths(&[13, 13, 5], &[1], 8), vec![13, 18]);
        assert_eq!(window_lengths(&[13, 13], &[2], 0), vec![13, 13]);
        assert_eq!(window_lengths(&[], &[3], 8), Vec::<i32>::new());
    }

    /// Au plus une tranche entière : telle quelle, ou complétée à une
    /// seconde de zéros.
    #[test]
    fn a_short_audio_is_one_chunk_padded_to_one_second() {
        let tenth = vec![0.5f32; 1_600];
        let chunks = split_into_chunks(&tenth, SAMPLE_RATE, 1200.0, 1.0);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), 16_000);
        assert_eq!(&chunks[0].samples[..1_600], &tenth[..]);
        assert!(chunks[0].samples[1_600..].iter().all(|&s| s == 0.0));
        let second = vec![0.5f32; 16_000];
        assert_eq!(
            split_into_chunks(&second, SAMPLE_RATE, 1200.0, 1.0)[0].samples,
            second
        );
        let empty = split_into_chunks(&[], SAMPLE_RATE, 1200.0, 1.0);
        assert_eq!(empty[0].samples.len(), 16_000);
    }

    /// Plus long qu'une tranche : coupé au silence le plus proche de la
    /// limite, dans la fenêtre de recherche.
    #[test]
    fn a_long_audio_is_cut_at_the_quietest_point() {
        // 3 s de bruit avec un silence de 0,2 s centré sur 1,5 s ; tranches
        // de 2 s, recherche à ± 5 s : la coupe tombe dans le silence.
        let rate = 1_000usize;
        let mut wav: Vec<f32> = (0..3_000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        for s in &mut wav[1_400..1_600] {
            *s = 0.0;
        }
        let chunks = split_into_chunks(&wav, rate, 2.0, 1.0);
        assert_eq!(chunks.len(), 2);
        let first = chunks[0].samples.len();
        assert!((1_400..=1_600).contains(&first), "{first}");
        assert_eq!(chunks[1].offset, first as f32 / rate as f32);
        assert_eq!(first + chunks[1].samples.len(), 3_000);
    }

    /// Une seconde au moins par tranche, même si le silence est plus tôt.
    #[test]
    fn a_chunk_lasts_at_least_one_second() {
        let rate = 1_000usize;
        let mut wav = vec![0.5f32; 5_000];
        for s in &mut wav[0..300] {
            *s = 0.0;
        }
        let chunks = split_into_chunks(&wav, rate, 1.5, 1.0);
        assert!(
            chunks.iter().all(|c| c.samples.len() >= rate),
            "{:?}",
            chunks.iter().map(|c| c.samples.len()).collect::<Vec<_>>()
        );
        assert_eq!(chunks[0].samples.len(), 1_000);
    }

    #[test]
    fn the_quietest_window_is_the_first_at_equal_energy() {
        assert_eq!(quietest(&[1.0, 0.0, 0.0, 0.0, 1.0], 2), 2);
        assert_eq!(quietest(&[0.0; 6], 2), 1);
    }
}
