//! `config.json` de Voxtral Realtime, comme `VoxtralRealtimeConfig.swift` :
//! les mêmes clés et les mêmes valeurs par défaut, `audio_encoding_args` à
//! la racine ou sous `encoder_args`, la quantification affine globale. Les
//! tailles sont bornées : un fichier absurde est une erreur qui le nomme,
//! jamais une panique. Ce que le flux du Swift suppose sans le vérifier
//! (16 kHz, 160 échantillons par trame et 1 280 par jeton, écrits en dur
//! dans `VoxtralRealtimeConstants` ; 128 bandes, écrites en dur dans la
//! première convolution) est exigé ici.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use crate::stt::mlx::weights::Quantization;

pub const CONFIG_FILE: &str = "config.json";
/// `config.json` plus gros que cela : refusé sans être lu (celui du
/// catalogue fait 1,5 ko).
const MAX_CONFIG_BYTES: u64 = 1 << 20;
/// Une dimension au plus (couches, têtes, largeurs, vocabulaire).
const MAX_DIM: i64 = 1 << 18;
/// Les jetons de marge à gauche au plus (32 ici).
const MAX_LEFT_PAD: i64 = 1 << 10;
/// Le retard du texte au plus, en millisecondes (480 ici).
const MAX_DELAY_MS: i64 = 60_000;

/// `VoxtralRealtimeConstants` : ce que le Swift écrit en dur.
pub const SAMPLE_RATE: i32 = 16_000;
pub const HOP_LENGTH: i32 = 160;
/// Un jeton : 1 280 échantillons (80 ms).
pub const SAMPLES_PER_TOKEN: i32 = 1_280;
/// Les trames de mel d'un jeton (1 280 / 160).
pub const FRAMES_PER_TOKEN: i32 = SAMPLES_PER_TOKEN / HOP_LENGTH;
/// Les bandes du mel : la première convolution du Swift en attend 128.
pub const MEL_BINS: i32 = 128;

/// `VoxtralRealtimeAudioEncodingConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioConfig {
    pub sampling_rate: i32,
    pub frame_rate: f32,
    pub num_mel_bins: i32,
    pub hop_length: i32,
    pub window_size: i32,
    pub global_log_mel_max: f32,
}

/// `VoxtralRealtimeEncoderConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct EncoderConfig {
    pub dim: i32,
    pub n_layers: i32,
    pub n_heads: i32,
    pub head_dim: i32,
    pub hidden_dim: i32,
    pub norm_eps: f32,
    pub rope_theta: f32,
    pub sliding_window: i32,
    pub downsample_factor: i32,
}

/// `VoxtralRealtimeDecoderConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecoderConfig {
    pub dim: i32,
    pub n_layers: i32,
    pub n_heads: i32,
    pub n_kv_heads: i32,
    pub head_dim: i32,
    pub hidden_dim: i32,
    pub vocab_size: i32,
    pub norm_eps: f32,
    pub rope_theta: f32,
    pub sliding_window: i32,
    /// `ada_rms_norm_t_cond_dim` si `ada_rms_norm_t_cond`, sinon rien.
    pub ada_dim: Option<i32>,
}

/// `VoxtralRealtimeConfig`, tel que le flux le lit.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub encoder: EncoderConfig,
    pub decoder: DecoderConfig,
    pub audio: AudioConfig,
    pub transcription_delay_ms: i32,
    pub bos_token_id: i32,
    pub eos_token_id: i32,
    pub streaming_pad_token_id: i32,
    pub n_left_pad_tokens: i32,
    pub quantization: Option<Quantization>,
}

#[derive(Deserialize)]
#[serde(default)]
struct RawAudio {
    sampling_rate: i64,
    frame_rate: f32,
    num_mel_bins: i64,
    hop_length: i64,
    window_size: i64,
    global_log_mel_max: f32,
}

impl Default for RawAudio {
    fn default() -> Self {
        Self {
            sampling_rate: 16_000,
            frame_rate: 12.5,
            num_mel_bins: 128,
            hop_length: 160,
            window_size: 400,
            global_log_mel_max: 1.5,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawEncoder {
    dim: i64,
    n_layers: i64,
    n_heads: i64,
    head_dim: i64,
    hidden_dim: i64,
    norm_eps: f32,
    rope_theta: f32,
    sliding_window: i64,
    causal: bool,
    downsample_factor: i64,
    audio_encoding_args: Option<RawAudio>,
}

impl Default for RawEncoder {
    fn default() -> Self {
        Self {
            dim: 1280,
            n_layers: 32,
            n_heads: 32,
            head_dim: 64,
            hidden_dim: 5120,
            norm_eps: 1e-5,
            rope_theta: 1_000_000.0,
            sliding_window: 750,
            causal: true,
            downsample_factor: 4,
            audio_encoding_args: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawDecoder {
    dim: i64,
    n_layers: i64,
    n_heads: i64,
    n_kv_heads: i64,
    head_dim: i64,
    hidden_dim: i64,
    vocab_size: i64,
    norm_eps: f32,
    rope_theta: f32,
    sliding_window: i64,
    tied_embeddings: bool,
    ada_rms_norm_t_cond: bool,
    ada_rms_norm_t_cond_dim: i64,
}

impl Default for RawDecoder {
    fn default() -> Self {
        Self {
            dim: 3072,
            n_layers: 26,
            n_heads: 32,
            n_kv_heads: 8,
            head_dim: 128,
            hidden_dim: 9216,
            vocab_size: 131_072,
            norm_eps: 1e-5,
            rope_theta: 1_000_000.0,
            sliding_window: 8192,
            tied_embeddings: true,
            ada_rms_norm_t_cond: true,
            ada_rms_norm_t_cond_dim: 32,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct Raw {
    encoder_args: Option<RawEncoder>,
    decoder: Option<RawDecoder>,
    audio_encoding_args: Option<RawAudio>,
    transcription_delay_ms: i64,
    bos_token_id: i64,
    eos_token_id: i64,
    streaming_pad_token_id: i64,
    n_left_pad_tokens: i64,
}

impl Default for Raw {
    fn default() -> Self {
        Self {
            encoder_args: None,
            decoder: None,
            audio_encoding_args: None,
            transcription_delay_ms: 480,
            bos_token_id: 1,
            eos_token_id: 2,
            streaming_pad_token_id: 32,
            n_left_pad_tokens: 32,
        }
    }
}

fn unreadable(path: &Path, err: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(crate::tr!(
        en = "unreadable Voxtral configuration ({}): {err}",
        fr = "configuration de Voxtral illisible ({}) : {err}",
        path.display(),
    ))
}

impl Config {
    /// `config.json` de `dir`, borné en taille, puis vérifié.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let file = std::fs::File::open(&path).map_err(|err| unreadable(&path, &err))?;
        let mut text = String::new();
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|err| unreadable(&path, &err))?;
        ensure!(
            text.len() as u64 <= MAX_CONFIG_BYTES,
            crate::tr!(
                en = "Voxtral configuration ({}) larger than {MAX_CONFIG_BYTES} bytes",
                fr = "configuration de Voxtral ({}) de plus de {MAX_CONFIG_BYTES} octets",
                path.display(),
            )
        );
        let json: Value = serde_json::from_str(&text).map_err(|err| unreadable(&path, &err))?;
        Self::from_json(&json).with_context(|| path.display().to_string())
    }

    pub fn from_json(json: &Value) -> Result<Self> {
        let raw = Raw::deserialize(json).context("config.json")?;
        let encoder = raw.encoder_args.unwrap_or_default();
        let decoder = raw.decoder.unwrap_or_default();
        // À la racine, sinon sous `encoder_args`, sinon les défauts.
        let audio = raw
            .audio_encoding_args
            .or(encoder.audio_encoding_args)
            .unwrap_or_default();
        ensure!(
            encoder.causal,
            crate::tr!(
                "Voxtral encoder not ported to Rust: only the causal encoder is",
                "encodeur de Voxtral non porté en Rust : seul l'encodeur causal l'est"
            )
        );
        ensure!(
            decoder.tied_embeddings,
            crate::tr!(
                "Voxtral decoder not ported to Rust: only tied embeddings (tied_embeddings) are",
                "décodeur de Voxtral non porté en Rust : seuls les plongements liés (tied_embeddings) le sont"
            )
        );
        let config = Self {
            encoder: EncoderConfig {
                dim: dim("encoder_args.dim", encoder.dim)?,
                n_layers: dim("encoder_args.n_layers", encoder.n_layers)?,
                n_heads: dim("encoder_args.n_heads", encoder.n_heads)?,
                head_dim: dim("encoder_args.head_dim", encoder.head_dim)?,
                hidden_dim: dim("encoder_args.hidden_dim", encoder.hidden_dim)?,
                norm_eps: encoder.norm_eps,
                rope_theta: encoder.rope_theta,
                sliding_window: dim("encoder_args.sliding_window", encoder.sliding_window)?,
                downsample_factor: dim(
                    "encoder_args.downsample_factor",
                    encoder.downsample_factor,
                )?,
            },
            decoder: DecoderConfig {
                dim: dim("decoder.dim", decoder.dim)?,
                n_layers: dim("decoder.n_layers", decoder.n_layers)?,
                n_heads: dim("decoder.n_heads", decoder.n_heads)?,
                n_kv_heads: dim("decoder.n_kv_heads", decoder.n_kv_heads)?,
                head_dim: dim("decoder.head_dim", decoder.head_dim)?,
                hidden_dim: dim("decoder.hidden_dim", decoder.hidden_dim)?,
                vocab_size: dim("decoder.vocab_size", decoder.vocab_size)?,
                norm_eps: decoder.norm_eps,
                rope_theta: decoder.rope_theta,
                sliding_window: dim("decoder.sliding_window", decoder.sliding_window)?,
                ada_dim: if decoder.ada_rms_norm_t_cond {
                    Some(dim(
                        "decoder.ada_rms_norm_t_cond_dim",
                        decoder.ada_rms_norm_t_cond_dim,
                    )?)
                } else {
                    None
                },
            },
            audio: AudioConfig {
                sampling_rate: dim("sampling_rate", audio.sampling_rate)?,
                frame_rate: audio.frame_rate,
                num_mel_bins: dim("num_mel_bins", audio.num_mel_bins)?,
                hop_length: dim("hop_length", audio.hop_length)?,
                window_size: dim("window_size", audio.window_size)?,
                global_log_mel_max: audio.global_log_mel_max,
            },
            transcription_delay_ms: bounded(
                "transcription_delay_ms",
                raw.transcription_delay_ms,
                0,
                MAX_DELAY_MS,
            )?,
            bos_token_id: bounded("bos_token_id", raw.bos_token_id, 0, MAX_DIM)?,
            eos_token_id: bounded("eos_token_id", raw.eos_token_id, 0, MAX_DIM)?,
            streaming_pad_token_id: bounded(
                "streaming_pad_token_id",
                raw.streaming_pad_token_id,
                0,
                MAX_DIM,
            )?,
            n_left_pad_tokens: bounded(
                "n_left_pad_tokens",
                raw.n_left_pad_tokens,
                1,
                MAX_LEFT_PAD,
            )?,
            quantization: quantization(json)?,
        };
        config.check()?;
        Ok(config)
    }

    /// Ce que le flux suppose : sinon une division par zéro, une forme
    /// impossible, un index hors des tables ou un temps faux plus loin.
    fn check(&self) -> Result<()> {
        let (a, e, d) = (&self.audio, &self.encoder, &self.decoder);
        // `VoxtralRealtimeConstants` (numAudioTokens, numDelayTokens) et la
        // session (`samplingRate / frameRate`) doivent dire la même chose.
        let per_token = a.sampling_rate as f32 / a.frame_rate;
        ensure!(
            a.sampling_rate == SAMPLE_RATE
                && a.hop_length == HOP_LENGTH
                && per_token.is_finite()
                && per_token as i32 == SAMPLES_PER_TOKEN
                && a.num_mel_bins == MEL_BINS,
            crate::tr!(
                en = "Voxtral audio not ported to Rust ({} Hz, {} frames per second, hop {}, {} bands): only 16 kHz, 12.5 tokens per second, hop 160 and 128 bands are",
                fr = "audio de Voxtral non porté en Rust ({} Hz, {} trames par seconde, pas de {}, {} bandes) : seuls 16 kHz, 12,5 jetons par seconde, un pas de 160 et 128 bandes le sont",
                a.sampling_rate,
                a.frame_rate,
                a.hop_length,
                a.num_mel_bins,
            )
        );
        // `VoxtralRealtimeMelStream` : la fenêtre couvre au moins un pas et
        // sa moitié droite ; la marge à gauche dépasse sa moitié (sinon la
        // marge en miroir n'est pas des zéros).
        ensure!(
            a.window_size % 2 == 0
                && a.window_size - a.hop_length - a.window_size / 2 >= 0
                && (self.n_left_pad_tokens * SAMPLES_PER_TOKEN) > a.window_size / 2
                && a.window_size <= 1 << 14,
            crate::tr!(
                en = "Voxtral: a mel window of {} samples does not fit a hop of {}",
                fr = "Voxtral : une fenêtre de mel de {} échantillons ne va pas avec un pas de {}",
                a.window_size,
                a.hop_length,
            )
        );
        // RoPE entrelacé : des paires ; l'attention groupée : des têtes de
        // requête par groupe entier.
        ensure!(
            e.head_dim % 2 == 0
                && d.head_dim % 2 == 0
                && d.n_heads % d.n_kv_heads == 0
                && d.dim % 2 == 0
                && e.rope_theta.is_finite()
                && e.rope_theta > 0.0
                && d.rope_theta.is_finite()
                && d.rope_theta > 0.0
                && e.norm_eps.is_finite()
                && d.norm_eps.is_finite(),
            crate::tr!(
                "inconsistent Voxtral configuration",
                "configuration de Voxtral incohérente"
            )
        );
        let ids = [
            ("bos_token_id", self.bos_token_id),
            ("eos_token_id", self.eos_token_id),
            ("streaming_pad_token_id", self.streaming_pad_token_id),
        ];
        for (name, id) in ids {
            ensure!(
                id < d.vocab_size,
                crate::tr!(
                    en = "Voxtral: {name} = {id}, outside a vocabulary of {}",
                    fr = "Voxtral : {name} = {id}, hors d'un vocabulaire de {}",
                    d.vocab_size,
                )
            );
        }
        Ok(())
    }
}

/// Une dimension dans 1…`MAX_DIM`.
fn dim(name: &str, v: i64) -> Result<i32> {
    bounded(name, v, 1, MAX_DIM)
}

fn bounded(name: &str, v: i64, low: i64, high: i64) -> Result<i32> {
    ensure!(
        (low..=high).contains(&v),
        crate::tr!(
            en = "Voxtral configuration: {name} = {v}, outside {low}…{high}",
            fr = "configuration de Voxtral : {name} = {v}, hors de {low}…{high}",
        )
    );
    Ok(v as i32)
}

/// `quantization` avec `group_size` et `bits` ; absente : aucune. Seule la
/// quantification affine globale est portée : un mode autre qu'affine, ou
/// un réglage par couche (une autre clé que celles que le Swift lit ou
/// ignore), est refusé par son nom.
fn quantization(json: &Value) -> Result<Option<Quantization>> {
    let Some(q) = json.get("quantization").filter(|q| !q.is_null()) else {
        return Ok(None);
    };
    let Some(object) = q.as_object() else {
        bail!(crate::tr!(
            "Voxtral: quantization is not an object",
            "Voxtral : quantization n'est pas un objet"
        ));
    };
    const KNOWN: [&str; 6] = [
        "group_size",
        "bits",
        "mode",
        "quant_method",
        "linear_class",
        "quantization_mode",
    ];
    if let Some(layer) = object.keys().find(|k| !KNOWN.contains(&k.as_str())) {
        bail!(crate::tr!(
            en = "Voxtral: per-layer quantization ({layer}) not ported to Rust",
            fr = "Voxtral : quantification par couche ({layer}) non portée en Rust",
        ));
    }
    if let Some(mode) = object.get("mode").and_then(Value::as_str) {
        ensure!(
            mode == "affine",
            crate::tr!(
                en = "Voxtral: quantization mode {mode:?} not ported to Rust (affine only)",
                fr = "Voxtral : quantification {mode:?} non portée en Rust (affine seulement)",
            )
        );
    }
    let quantization = Quantization::deserialize(q).context("quantization")?;
    ensure!(
        quantization.bits > 0 && quantization.group_size > 0,
        crate::tr!(
            en = "Voxtral: quantization of {} bits by groups of {}",
            fr = "Voxtral : quantification de {} bits par groupes de {}",
            quantization.bits,
            quantization.group_size,
        )
    );
    Ok(Some(quantization))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Les défauts de `VoxtralRealtimeConfig` : un `{}` donne le modèle du
    /// catalogue, retard de 480 ms.
    #[test]
    fn the_defaults_are_those_of_the_swift() {
        let c = Config::from_json(&json!({})).unwrap();
        assert_eq!(c.transcription_delay_ms, 480);
        assert_eq!(
            (c.bos_token_id, c.eos_token_id, c.streaming_pad_token_id),
            (1, 2, 32)
        );
        assert_eq!(c.n_left_pad_tokens, 32);
        assert_eq!(
            (c.encoder.dim, c.encoder.n_layers, c.encoder.sliding_window),
            (1280, 32, 750)
        );
        assert_eq!(c.encoder.downsample_factor, 4);
        assert_eq!(
            (c.decoder.dim, c.decoder.n_layers, c.decoder.n_kv_heads),
            (3072, 26, 8)
        );
        assert_eq!(c.decoder.ada_dim, Some(32));
        assert_eq!(c.decoder.sliding_window, 8192);
        assert_eq!(c.audio.window_size, 400);
        assert_eq!(c.audio.global_log_mel_max, 1.5);
        assert_eq!(c.quantization, None);
    }

    /// `audio_encoding_args` : à la racine d'abord, sinon sous
    /// `encoder_args` (le fichier du catalogue).
    #[test]
    fn the_audio_arguments_are_read_at_the_root_or_under_the_encoder() {
        let nested = json!({"encoder_args": {"audio_encoding_args": {"global_log_mel_max": 2.0}}});
        assert_eq!(
            Config::from_json(&nested).unwrap().audio.global_log_mel_max,
            2.0
        );
        let both = json!({
            "audio_encoding_args": {"global_log_mel_max": 3.0},
            "encoder_args": {"audio_encoding_args": {"global_log_mel_max": 2.0}}
        });
        assert_eq!(
            Config::from_json(&both).unwrap().audio.global_log_mel_max,
            3.0
        );
        let off = json!({"decoder": {"ada_rms_norm_t_cond": false}});
        assert_eq!(Config::from_json(&off).unwrap().decoder.ada_dim, None);
    }

    #[test]
    fn the_quantization_is_read_and_its_unported_forms_are_refused() {
        let q = json!({"quantization": {"group_size": 64, "bits": 4, "mode": "affine"}});
        assert_eq!(
            Config::from_json(&q).unwrap().quantization,
            Some(Quantization {
                group_size: 64,
                bits: 4
            })
        );
        for (bad, needle) in [
            (
                json!({"quantization": {"group_size": 32, "bits": 4, "mode": "mxfp4"}}),
                "mxfp4",
            ),
            (
                json!({"quantization": {"group_size": 64, "bits": 4, "decoder.layers.0.attention.wq": {"bits": 8}}}),
                "decoder.layers.0.attention.wq",
            ),
            (json!({"quantization": {"group_size": 0, "bits": 4}}), "0"),
            (json!({"quantization": 4}), "quantization"),
        ] {
            let err = format!("{:#}", Config::from_json(&bad).unwrap_err());
            assert!(err.contains(needle), "{err}");
        }
    }

    /// Chaque réglage que le flux ne sait pas lire, ou qui ferait une
    /// division par zéro, une forme impossible ou un index hors des tables,
    /// est refusé par son nom.
    #[test]
    fn absurd_or_unported_configurations_are_refused_by_name() {
        let cases = [
            (json!({"encoder_args": {"dim": 0}}), "encoder_args.dim"),
            (
                json!({"encoder_args": {"n_layers": -1}}),
                "encoder_args.n_layers",
            ),
            (
                json!({"encoder_args": {"sliding_window": 0}}),
                "encoder_args.sliding_window",
            ),
            (
                json!({"encoder_args": {"downsample_factor": 0}}),
                "encoder_args.downsample_factor",
            ),
            (json!({"encoder_args": {"causal": false}}), "causal"),
            (json!({"encoder_args": {"head_dim": 63}}), "Voxtral"),
            (
                json!({"decoder": {"vocab_size": 1_000_000_000}}),
                "decoder.vocab_size",
            ),
            (json!({"decoder": {"n_kv_heads": 7}}), "Voxtral"),
            (
                json!({"decoder": {"tied_embeddings": false}}),
                "tied_embeddings",
            ),
            (
                json!({"decoder": {"ada_rms_norm_t_cond_dim": 0}}),
                "ada_rms_norm_t_cond_dim",
            ),
            (json!({"decoder": {"rope_theta": -1.0}}), "Voxtral"),
            (
                json!({"audio_encoding_args": {"sampling_rate": 8000}}),
                "8000",
            ),
            (
                json!({"audio_encoding_args": {"hop_length": 0}}),
                "hop_length",
            ),
            (
                json!({"audio_encoding_args": {"frame_rate": 0.0}}),
                "16 kHz",
            ),
            (json!({"audio_encoding_args": {"num_mel_bins": 80}}), "80"),
            (json!({"audio_encoding_args": {"window_size": 200}}), "200"),
            (json!({"audio_encoding_args": {"window_size": 401}}), "401"),
            (json!({"n_left_pad_tokens": 0}), "n_left_pad_tokens"),
            (
                json!({"transcription_delay_ms": -80}),
                "transcription_delay_ms",
            ),
            (json!({"eos_token_id": 131_072}), "eos_token_id"),
            (
                json!({"streaming_pad_token_id": -1}),
                "streaming_pad_token_id",
            ),
            (json!({"decoder": "non"}), "config.json"),
        ];
        for (bad, needle) in cases {
            let err = format!("{:#}", Config::from_json(&bad).unwrap_err());
            assert!(err.contains(needle), "{bad} : {err}");
        }
    }

    #[test]
    fn a_config_too_large_or_unreadable_is_refused_by_name() {
        let dir = crate::paths::test_scratch("stt-mlx-voxtral-config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(CONFIG_FILE), "{err}");
        std::fs::write(dir.join(CONFIG_FILE), "{").unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(CONFIG_FILE), "{err}");
        let big = format!("{{\"x\": \"{}\"}}", "a".repeat(MAX_CONFIG_BYTES as usize));
        std::fs::write(dir.join(CONFIG_FILE), big).unwrap();
        let err = format!("{:#}", Config::load(&dir).unwrap_err());
        assert!(err.contains(&MAX_CONFIG_BYTES.to_string()), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
