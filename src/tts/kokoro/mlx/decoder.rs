//! Le décodeur de Kokoro et son générateur iSTFTNet (`KokoroDecoder.swift`,
//! `Blocks/SineGenerator.swift`, `Blocks/Utilities.swift`) : source
//! harmonique, STFT, montées en résolution, iSTFT. Points d'arrêt après
//! `encode`, après chaque bloc de `decode` et après chaque étage du
//! générateur, comme le Swift.

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::{IntoStrideBy, NewAxis, TryIndexOp};
use mlx_rs::random::RandomState;
use mlx_rs::fft::{irfft, rfft};
use mlx_rs::{complex64, ops, random, Array};

use super::config::{Config, Istftnet, Weights};
use super::modules::{
    conv_channels_first, leaky_relu, scalar, shape, upsample_nearest, AdaInResBlock1, AdainResBlock, Conv1d,
    ConvOptions, Linear, WeightNormedConv,
};
use super::Stop;

/// Le Swift fixe ces valeurs dans `KokoroGenerator` (`SourceModule`).
const SOURCE_RATE: f32 = 24_000.0;
const HARMONICS: i32 = 8;
const VOICED_THRESHOLD: f32 = 10.0;
const SINE_AMP: f32 = 0.1;
const NOISE_STD: f32 = 0.003;

pub struct Decoder {
    f0_conv: WeightNormedConv,
    n_conv: WeightNormedConv,
    encode: AdainResBlock,
    decode: Vec<AdainResBlock>,
    asr_res: WeightNormedConv,
    generator: Generator,
}

impl Decoder {
    pub fn load(w: &mut Weights, config: &Config) -> Result<Self> {
        let (dim_in, style, asr) = (config.hidden_dim, config.style_dim, config.asr_res_dim);
        let decoder_dim = config.istftnet.upsample_initial_channel * 2;
        let out_dim = config.istftnet.upsample_initial_channel;
        let halving = ConvOptions { stride: 2, ..ConvOptions::padded(1) };
        let mut decode = Vec::new();
        for i in 0..4 {
            let last = i == 3;
            decode.push(AdainResBlock::load(
                w,
                &format!("decoder.decode.{i}"),
                decoder_dim + 2 + asr,
                if last { out_dim } else { decoder_dim },
                style,
                last,
            )?);
        }
        Ok(Self {
            f0_conv: WeightNormedConv::load(w, "decoder.F0_conv", shape(1, 1, 3), halving)?,
            n_conv: WeightNormedConv::load(w, "decoder.N_conv", shape(1, 1, 3), halving)?,
            encode: AdainResBlock::load(w, "decoder.encode", dim_in + 2, decoder_dim, style, false)?,
            decode,
            asr_res: WeightNormedConv::load(w, "decoder.asr_res.0", shape(dim_in, asr, 1), ConvOptions::padded(0))?,
            generator: Generator::load(w, style, &config.istftnet)?,
        })
    }

    pub fn forward(
        &self,
        asr: &Array,
        f0: &Array,
        n: &Array,
        s: &Array,
        rng: &mut RandomState,
        stop: &Stop,
    ) -> Result<Array> {
        let f0_down = conv_channels_first(&self.f0_conv, &f0.expand_dims(1)?)?;
        let n_down = conv_channels_first(&self.n_conv, &n.expand_dims(1)?)?;

        let mut x = ops::concatenate(&[asr, &f0_down, &n_down], 1)?;
        x = self.encode.forward(&x, s)?;
        stop.here(&x)?;

        let asr_res = conv_channels_first(&self.asr_res, asr)?;
        let mut res = true;
        for block in &self.decode {
            if res {
                x = ops::concatenate(&[&x, &asr_res, &f0_down, &n_down], 1)?;
            }
            x = block.forward(&x, s)?;
            if block.upsamples() {
                res = false;
            }
            stop.here(&x)?;
        }
        self.generator.forward(&x, s, f0, rng, stop)
    }
}

struct Generator {
    source: Linear,
    noise_convs: Vec<Conv1d>,
    noise_res: Vec<AdaInResBlock1>,
    ups: Vec<WeightNormedConv>,
    resblocks: Vec<AdaInResBlock1>,
    conv_post: WeightNormedConv,
    kernels: usize,
    n_fft: i32,
    hop: i32,
    /// Montée totale des trames de F0 aux échantillons (300).
    upsample: i32,
}

impl Generator {
    fn load(w: &mut Weights, style: i32, net: &Istftnet) -> Result<Self> {
        let p = |name: &str| format!("decoder.generator.{name}");
        let rates = &net.upsample_rates;
        let ch0 = net.upsample_initial_channel;
        let spectrum = net.gen_istft_n_fft + 2;
        let (mut ups, mut noise_convs, mut noise_res, mut resblocks) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, (&u, &k)) in rates.iter().zip(&net.upsample_kernel_sizes).enumerate() {
            let (ch_in, ch_out) = (ch0 >> i, ch0 >> (i + 1));
            let options = ConvOptions { stride: u, encode: true, ..ConvOptions::padded((k - u) / 2) };
            ups.push(WeightNormedConv::load(w, &p(&format!("ups.{i}")), shape(ch_out, ch_in, k), options)?);
            let conv = p(&format!("noise_convs.{i}"));
            let res = p(&format!("noise_res.{i}"));
            if i + 1 < rates.len() {
                let stride: i32 = rates[i + 1..].iter().product();
                noise_convs.push(Conv1d::load(w, &conv, shape(spectrum, ch_out, stride * 2), stride, (stride + 1) / 2)?);
                noise_res.push(AdaInResBlock1::load(w, &res, ch_out, 7, &[1, 3, 5], style)?);
            } else {
                noise_convs.push(Conv1d::load(w, &conv, shape(spectrum, ch_out, 1), 1, 0)?);
                noise_res.push(AdaInResBlock1::load(w, &res, ch_out, 11, &[1, 3, 5], style)?);
            }
            for (j, (&rk, rd)) in net.resblock_kernel_sizes.iter().zip(&net.resblock_dilation_sizes).enumerate() {
                let key = p(&format!("resblocks.{}", i * net.resblock_kernel_sizes.len() + j));
                resblocks.push(AdaInResBlock1::load(w, &key, ch_out, rk, rd, style)?);
            }
        }
        let last = ch0 >> rates.len();
        Ok(Self {
            source: Linear::load(w, &p("m_source.l_linear"), HARMONICS + 1, 1)?,
            noise_convs,
            noise_res,
            ups,
            resblocks,
            conv_post: WeightNormedConv::load(w, &p("conv_post"), shape(last, spectrum, 7), ConvOptions::padded(3))?,
            kernels: net.resblock_kernel_sizes.len(),
            n_fft: net.gen_istft_n_fft,
            hop: net.gen_istft_hop_size,
            upsample: rates.iter().product::<i32>() * net.gen_istft_hop_size,
        })
    }

    fn forward(&self, x: &Array, s: &Array, f0: &Array, rng: &mut RandomState, stop: &Stop) -> Result<Array> {
        let f0_up = upsample_nearest(&f0.try_index(NewAxis)?.transpose_axes(&[0, 2, 1])?, self.upsample)?;
        let har_source = self.source_module(&f0_up, rng)?;
        let har_flat = har_source.transpose_axes(&[0, 2, 1])?.squeeze_axes(&[1])?;

        let spectrum = stft(&har_flat.try_index(0)?, self.n_fft, self.hop, self.n_fft)?;
        let magnitude = ops::stack(&[spectrum.abs()?], 0)?;
        let phase = ops::stack(&[ops::atan2(spectrum.imag()?, spectrum.real()?)?], 0)?;
        let har = ops::concatenate(&[magnitude, phase], 1)?.swap_axes(2, 1)?;

        let mut h = x.clone();
        let stages = self.ups.len();
        for i in 0..stages {
            h = leaky_relu(&h, 0.1)?;
            let x_source = self.noise_res[i].forward(&self.noise_convs[i].forward(&har)?.swap_axes(2, 1)?, s)?;
            h = self.ups[i].conv_transpose(&h.swap_axes(2, 1)?)?.swap_axes(2, 1)?;
            if i == stages - 1 {
                h = ops::pad(&h, &[(0, 0), (0, 0), (1, 0)][..], None, None)?;
            }
            h = h.add(x_source)?;
            let mut sum: Option<Array> = None;
            for block in &self.resblocks[i * self.kernels..(i + 1) * self.kernels] {
                let out = block.forward(&h, s)?;
                sum = Some(match sum {
                    Some(sum) => sum.add(out)?,
                    None => out,
                });
            }
            let Some(sum) = sum else { bail!("générateur sans bloc résiduel") };
            h = sum.divide(scalar(self.kernels as f32))?;
            stop.here(&h)?;
        }

        h = leaky_relu(&h, 0.01)?;
        h = conv_channels_first(&self.conv_post, &h)?;
        let bins = self.n_fft / 2 + 1;
        let magnitude = h.try_index((.., ..bins, ..))?.exp()?;
        let phase = h.try_index((.., bins.., ..))?.sin()?;
        // `KokoroSTFT.inverse`, sur l'unique élément du lot.
        let continuous = unwrap_phase(&phase.try_index(0)?)?;
        let rotation = Array::from_complex(complex64::new(0.0, 1.0)).multiply(continuous)?.exp()?;
        let complex = magnitude.try_index(0)?.multiply(rotation)?;
        let audio = istft(&complex, self.hop, self.n_fft)?;
        Ok(ops::stack(&[audio], 0)?.expand_dims(1)?)
    }

    /// `SourceModule` : la somme pondérée des harmoniques. Son bruit (rendu,
    /// jamais lu) prend tout de même sa clé, comme en Swift : la suite des
    /// tirages reste la même.
    fn source_module(&self, f0: &Array, rng: &mut RandomState) -> Result<Array> {
        let (sine, _uv) = sine_generator(f0, self.upsample, rng)?;
        let merged = ops::tanh(self.source.forward(&sine)?)?;
        rng.next_key()?;
        Ok(merged)
    }
}

/// `SineGenerator` : la fondamentale et ses harmoniques, plus un bruit.
fn sine_generator(f0: &Array, upsample: i32, rng: &mut RandomState) -> Result<(Array, Array)> {
    let count = HARMONICS + 1;
    let harmonics = Array::from_iter((1..=count).map(|i| i as f32), &[1, 1, count]);
    let fn_ = f0.multiply(harmonics)?;
    let rad = fn_.divide(scalar(SOURCE_RATE))?.remainder(scalar(1.0))?;

    let batch = f0.shape()[0];
    let key = rng.next_key()?;
    let initial = random::uniform::<_, f32>(0.0f32, 1.0f32, &[batch, count], &key)?;
    let initial = ops::concatenate(&[Array::zeros::<f32>(&[batch, 1])?, initial.try_index((.., 1..))?], 1)?;
    let first = rad.try_index((.., 0..1, ..))?.add(initial.expand_dims(1)?)?;
    let rad = ops::concatenate(&[first, rad.try_index((.., 1.., ..))?], 1)?;

    let length = rad.shape()[1];
    let downscale = 1.0 / upsample as f32;
    let down_size = 1.max((length as f32 * downscale).ceil() as i32);
    let rad_down = interpolate1d(&rad.transpose_axes(&[0, 2, 1])?, down_size)?.transpose_axes(&[0, 2, 1])?;
    let phase_down = rad_down.cumsum(1, None, None)?.multiply(scalar(2.0 * std::f32::consts::PI))?;
    let phase_scaled = phase_down.transpose_axes(&[0, 2, 1])?.multiply(scalar(upsample as f32))?;
    let phase = interpolate1d(&phase_scaled, length)?.transpose_axes(&[0, 2, 1])?;

    let sine = phase.sin()?.multiply(scalar(SINE_AMP))?;
    let uv = f0.gt(scalar(VOICED_THRESHOLD))?.as_type::<f32>()?;
    let unvoiced = scalar(1.0).subtract(&uv)?.multiply(scalar(SINE_AMP))?.divide(scalar(3.0))?;
    let noise_amp = uv.multiply(scalar(NOISE_STD))?.add(unvoiced)?;
    let key = rng.next_key()?;
    let noise = noise_amp.multiply(random::normal::<f32>(sine.shape(), None, None, &key)?)?;
    Ok((sine.multiply(&uv)?.add(noise)?, uv))
}

/// `interpolate1d` : linéaire sur l'axe 2, coins alignés sur les centres.
fn interpolate1d(input: &Array, size: i32) -> Result<Array> {
    let width = input.shape()[2];
    if width == size || width < 1 || size < 1 {
        return Ok(input.clone());
    }
    let scale = width as f32 / size as f32;
    let coords: Vec<f32> = (0..size).map(|i| i as f32 * scale + 0.5 * scale - 0.5).collect();
    let coords = ops::clip(Array::from_slice(&coords, &[size]), (0.0f32, (width - 1) as f32))?;
    let low = coords.floor()?.as_type::<i32>()?;
    let high = ops::minimum(low.add(Array::from_int(1))?, Array::from_int(width - 1))?;
    let frac = coords.subtract(low.as_type::<f32>()?)?;
    let y_low = input.take_axis(&low, 2)?;
    let y_high = input.take_axis(&high, 2)?;
    let frac = frac.reshape(&[1, 1, size])?;
    Ok(y_low.multiply(scalar(1.0).subtract(&frac)?)?.add(y_high.multiply(frac)?)?)
}

fn hanning(length: i32) -> Result<Array> {
    if length == 1 {
        return Ok(scalar(1.0));
    }
    let n: Vec<f32> = (0..length).map(|i| (1 - length + 2 * i) as f32).collect();
    let n = Array::from_slice(&n, &[length]);
    let wave = n.multiply(scalar(std::f32::consts::PI / (length - 1) as f32))?.cos()?;
    Ok(scalar(0.5).add(scalar(0.5).multiply(wave)?)?)
}

fn window(length: i32) -> Result<Array> {
    Ok(hanning(length + 1)?.try_index(0..length)?)
}

/// `kokoroStft` : fenêtre de Hann, bords en miroir, trames par `as_strided`.
fn stft(x: &Array, n_fft: i32, hop: i32, win: i32) -> Result<Array> {
    let mut w = window(win)?;
    if w.shape()[0] < n_fft {
        w = ops::concatenate(&[w.clone(), Array::zeros::<f32>(&[n_fft - w.shape()[0]])?], 0)?;
    }
    let pad = n_fft / 2;
    let n = x.shape()[0];
    if n < pad + 2 {
        bail!("STFT : {n} échantillons pour une fenêtre de {n_fft}");
    }
    // x[1 ..< pad + 1] et x[-(pad + 1) ..< -1], chacun renversé.
    let prefix: Vec<i32> = (1..pad + 1).rev().collect();
    let suffix: Vec<i32> = ((n - pad - 1)..(n - 1)).rev().collect();
    let prefix = x.take_axis(Array::from_slice(&prefix, &[pad]), 0)?;
    let suffix = x.take_axis(Array::from_slice(&suffix, &[pad]), 0)?;
    let padded = ops::concatenate(&[&prefix, x, &suffix], 0)?;
    let frames = 1 + (padded.shape()[0] - n_fft) / hop;
    let framed = ops::as_strided(&padded, &[frames, n_fft], &[hop as i64, 1], None)?;
    Ok(rfft(framed.multiply(w)?, None, None)?.transpose()?)
}

/// `kokoroUnwrap` : la phase rendue continue le long de l'axe 1.
fn unwrap_phase(p: &Array) -> Result<Array> {
    let pi = std::f32::consts::PI;
    let period = 2.0 * pi;
    let low = -period / 2.0;
    let n = p.shape()[1];
    let diff = p.try_index((.., 1..n))?.subtract(p.try_index((.., 0..(n - 1)))?)?;
    let mut modulo = diff
        .subtract(scalar(low))?
        .remainder(scalar(period))?
        .add(scalar(period))?
        .remainder(scalar(period))?
        .add(scalar(low))?;
    let at_low = modulo.eq(scalar(low))?;
    let rising = ops::select(&diff.gt(scalar(0.0))?, scalar(period / 2.0), &modulo)?;
    modulo = ops::select(&at_low, &rising, &modulo)?;
    let mut correction = modulo.subtract(&diff)?;
    correction = ops::select(&diff.abs()?.lt(scalar(pi))?, scalar(0.0), &correction)?;
    let rest = p.try_index((.., 1..))?.add(correction.cumsum(1, None, None)?)?;
    Ok(ops::concatenate(&[p.try_index((.., 0..1))?, rest], 1)?)
}

/// `kokoroIstft` : recouvrement-addition, divisé par la somme des fenêtres².
fn istft(x: &Array, hop: i32, win: i32) -> Result<Array> {
    let w = window(win)?;
    let xt = x.transpose()?;
    let total = (xt.shape()[0] - 1) * hop + win;
    let mod_len = win / hop;
    let squared = w.multiply(&w)?;
    let repeats: Vec<&Array> = (0..(total / win + 1)).map(|_| &squared).collect();
    let all_squared = ops::concatenate(&repeats, 0)?;
    let output = irfft(&xt, None, 1)?.multiply(&w)?;

    let mut reconstructed: Option<Array> = None;
    let mut window_sum: Option<Array> = None;
    for i in 0..mod_len {
        let strided = output.try_index(((i..).stride_by(mod_len), ..))?.reshape(&[-1])?;
        let len = strided.shape()[0];
        let sums = all_squared.try_index(0..len)?;
        let before = Array::zeros::<f32>(&[i * hop])?;
        let after = Array::zeros::<f32>(&[0.max(total - i * hop - len)])?;
        let out = ops::concatenate(&[&before, &strided, &after], 0)?;
        let sum = ops::concatenate(&[&before, &sums, &after], 0)?;
        reconstructed = Some(match reconstructed {
            Some(r) => r.add(out)?,
            None => out,
        });
        window_sum = Some(match window_sum {
            Some(r) => r.add(sum)?,
            None => sum,
        });
    }
    let (Some(reconstructed), Some(window_sum)) = (reconstructed, window_sum) else {
        bail!("iSTFT : fenêtre plus courte que le pas");
    };
    let start = win / 2;
    let end = reconstructed.shape()[0] - win / 2;
    Ok(reconstructed.try_index(start..end)?.divide(window_sum.try_index(start..end)?)?)
}
