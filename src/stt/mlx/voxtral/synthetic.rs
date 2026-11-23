//! Un Voxtral synthétique (poids tirés d'une graine, vocabulaire de
//! quelques octets) : le flux sans le vrai modèle. Fenêtre de 10 lignes pour
//! l'encodeur (ses caches repartent à chaque bloc) et de 6 pour le décodeur
//! (l'invite dépasse la fenêtre : le masque explicite), deux jetons de
//! marge à gauche. La ligne de la fin de texte est nulle : son logit vaut 0,
//! le flux ne s'arrête qu'à `max_tokens`.

use super::config::CONFIG_FILE;
use super::model::Voxtral;
use super::stream::Session;
use super::tokenizer::TEKKEN_FILE;
use super::*;
use crate::compute::mlx::budget;
use crate::stt::mlx::canary::synthetic::{safetensors, Tensor};
use crate::stt::mlx::parakeet::tests::serial;
use crate::stt::mlx::{need, BUDGET_KEY};
use mlx_rs::Stream;
use std::path::PathBuf;

const ENC: i32 = 8;
const DEC: i32 = 16;
const VOCAB: i32 = 48;
const SPECIAL: usize = 4;
const EOS: usize = 2;

fn tiny_config() -> serde_json::Value {
    serde_json::json!({
        "encoder_args": {
            "dim": ENC, "n_layers": 2, "n_heads": 2, "head_dim": 4, "hidden_dim": 16,
            "sliding_window": 10, "downsample_factor": 4,
            "audio_encoding_args": {"global_log_mel_max": 1.5}
        },
        "decoder": {
            "dim": DEC, "n_layers": 2, "n_heads": 4, "n_kv_heads": 2, "head_dim": 4,
            "hidden_dim": 32, "vocab_size": VOCAB, "sliding_window": 6,
            "ada_rms_norm_t_cond_dim": 4
        },
        "n_left_pad_tokens": 2,
        "streaming_pad_token_id": 3
    })
}

/// Les octets des jetons ordinaires : des lettres, et « é » coupé en deux
/// jetons (`0xC3`, `0xA9`).
fn tiny_vocabulary() -> serde_json::Value {
    let vocab: Vec<serde_json::Value> = (0..VOCAB as usize - SPECIAL)
        .map(|i| {
            let bytes: Vec<u8> = match i {
                0 => vec![0xC3],
                1 => vec![0xA9],
                2 => b" ".to_vec(),
                _ => vec![b'a' + (i % 26) as u8],
            };
            let encoded = base64(&bytes);
            serde_json::json!({"rank": i, "token_bytes": encoded})
        })
        .collect();
    serde_json::json!({
        "vocab": vocab,
        "config": {"default_num_special_tokens": SPECIAL},
        "special_tokens": [{"rank": 0}, {"rank": 1}, {"rank": 2}, {"rank": 3}]
    })
}

/// La base64 standard, avec `=`, d'un ou deux octets.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (a, b) = (
        bytes[0] as usize,
        bytes.get(1).copied().unwrap_or(0) as usize,
    );
    let mut out = vec![ALPHABET[a >> 2], ALPHABET[((a & 3) << 4) | (b >> 4)]];
    if bytes.len() == 2 {
        out.push(ALPHABET[(b & 15) << 2]);
        out.push(b'=');
    } else {
        out.extend_from_slice(b"==");
    }
    String::from_utf8(out).unwrap()
}

/// Un tirage uniforme dans `[-scale, scale)`, d'une graine (xorshift).
fn drawn(seed: &mut u64, name: &str, shape: &[usize], scale: f32) -> Tensor {
    let count = shape.iter().product();
    let values = (0..count)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            ((*seed >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * scale
        })
        .collect();
    Tensor {
        name: name.to_string(),
        shape: shape.to_vec(),
        values,
    }
}

fn ones(name: &str, dim: i32) -> Tensor {
    Tensor {
        name: name.to_string(),
        shape: vec![dim as usize],
        values: vec![1.0; dim as usize],
    }
}

/// Les tenseurs, aux noms du paquet (`decoder.…`, `encoder.…`).
fn tiny_tensors() -> Vec<Tensor> {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut t = Vec::new();
    let (e, d) = (ENC as usize, DEC as usize);
    let mut w = |t: &mut Vec<Tensor>, name: String, shape: &[usize], scale: f32| {
        t.push(drawn(&mut seed, &name, shape, scale));
    };
    for (i, input) in [(0, 128), (1, e)] {
        let p = format!("encoder.conv_layers_{i}_conv.conv");
        w(&mut t, format!("{p}.weight"), &[e, 3, input], 0.2);
        w(&mut t, format!("{p}.bias"), &[e], 0.1);
    }
    for l in 0..2 {
        let p = format!("encoder.transformer_layers.{l}");
        t.push(ones(&format!("{p}.attention_norm.weight"), ENC));
        t.push(ones(&format!("{p}.ffn_norm.weight"), ENC));
        for (m, bias) in [("wq", true), ("wk", false), ("wv", true), ("wo", true)] {
            w(&mut t, format!("{p}.attention.{m}.weight"), &[e, e], 0.4);
            if bias {
                w(&mut t, format!("{p}.attention.{m}.bias"), &[e], 0.1);
            }
        }
        w(&mut t, format!("{p}.feed_forward_w1.weight"), &[16, e], 0.4);
        w(&mut t, format!("{p}.feed_forward_w3.weight"), &[16, e], 0.4);
        w(&mut t, format!("{p}.feed_forward_w2.weight"), &[e, 16], 0.3);
        w(&mut t, format!("{p}.feed_forward_w2.bias"), &[e], 0.1);
    }
    t.push(ones("encoder.transformer_norm.weight", ENC));
    w(
        &mut t,
        "encoder.audio_language_projection_0.weight".into(),
        &[d, 4 * e],
        0.3,
    );
    w(
        &mut t,
        "encoder.audio_language_projection_2.weight".into(),
        &[d, d],
        0.3,
    );
    for l in 0..2 {
        let p = format!("decoder.layers.{l}");
        t.push(ones(&format!("{p}.attention_norm.weight"), DEC));
        t.push(ones(&format!("{p}.ffn_norm.weight"), DEC));
        w(&mut t, format!("{p}.attention.wq.weight"), &[16, d], 0.3);
        w(&mut t, format!("{p}.attention.wk.weight"), &[8, d], 0.3);
        w(&mut t, format!("{p}.attention.wv.weight"), &[8, d], 0.3);
        w(&mut t, format!("{p}.attention.wo.weight"), &[d, 16], 0.3);
        w(
            &mut t,
            format!("{p}.ada_rms_norm_t_cond.ada_down.weight"),
            &[4, d],
            0.3,
        );
        w(
            &mut t,
            format!("{p}.ada_rms_norm_t_cond.ada_up.weight"),
            &[d, 4],
            0.3,
        );
        w(&mut t, format!("{p}.feed_forward_w1.weight"), &[32, d], 0.3);
        w(&mut t, format!("{p}.feed_forward_w3.weight"), &[32, d], 0.3);
        w(&mut t, format!("{p}.feed_forward_w2.weight"), &[d, 32], 0.3);
    }
    t.push(ones("decoder.norm.weight", DEC));
    let mut embeddings = drawn(
        &mut seed,
        "decoder.tok_embeddings.weight",
        &[VOCAB as usize, d],
        1.0,
    );
    embeddings.values[EOS * d..(EOS + 1) * d].fill(0.0);
    t.push(embeddings);
    t
}

fn scratch(name: &str) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-voxtral-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn synthetic(name: &str, tensors: &[Tensor]) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join(CONFIG_FILE), tiny_config().to_string()).unwrap();
    std::fs::write(dir.join(TEKKEN_FILE), tiny_vocabulary().to_string()).unwrap();
    std::fs::write(dir.join("model.safetensors"), safetensors(tensors)).unwrap();
    dir
}

fn complete(name: &str) -> PathBuf {
    synthetic(name, &tiny_tensors())
}

/// Trois secondes d'un son qui change : une sinusoïde glissante.
fn audio() -> Vec<f32> {
    (0..48_000)
        .map(|i| {
            let t = i as f32 / 16_000.0;
            0.3 * (2.0 * crate::stt::mlx::dsp::SWIFT_PI * (200.0 + 300.0 * t) * t).sin()
        })
        .collect()
}

fn pieces(samples: &[f32], sizes: &[usize]) -> Vec<Vec<f32>> {
    if sizes.is_empty() {
        return vec![samples.to_vec()];
    }
    let (mut out, mut at, mut i) = (Vec::new(), 0, 0);
    while at < samples.len() {
        let end = (at + sizes[i % sizes.len()]).min(samples.len());
        out.push(samples[at..end].to_vec());
        at = end;
        i += 1;
    }
    out
}

struct Run {
    fed: String,
    tokens: Vec<usize>,
    text: String,
    finished: bool,
}

/// `body` sur le fil MLX, sur le processeur (comme les essais de Canary).
pub(super) fn on_the_cpu_thread<R: Send + 'static>(
    body: impl FnOnce() -> anyhow::Result<R> + Send + 'static,
) -> anyhow::Result<R> {
    let held = Held::load(|| Ok(()))?;
    held.with(move |_| mlx_rs::with_stream(&Stream::cpu(), body))
}

/// Une session sur `parts`, sur le processeur (les produits Float32 de
/// plusieurs lignes ne passent pas en TF32 : le nombre de lignes de chaque
/// pas, qui dépend du découpage, ne change pas la précision).
fn on_the_cpu(dir: PathBuf, splits: Vec<Vec<usize>>, max_tokens: usize) -> Vec<Run> {
    let held = Held::load(move || Voxtral::load(&dir)).unwrap();
    held.with(move |model| {
        mlx_rs::with_stream(&Stream::cpu(), || {
            let samples = audio();
            splits
                .iter()
                .map(|sizes| {
                    let mut session = Session::new(model, DELAY_MS, max_tokens);
                    let mut fed = String::new();
                    for part in pieces(&samples, sizes) {
                        fed.push_str(&session.step(model, &part, &mut |_| Ok(()))?.text);
                    }
                    fed.push_str(&session.finish(model, &mut |_| Ok(()))?.text);
                    Ok(Run {
                        fed,
                        tokens: session.tokens().to_vec(),
                        text: session.text(),
                        finished: session.is_finished(),
                    })
                })
                .collect()
        })
    })
    .unwrap()
}

/// Les jetons et le texte ne dépendent pas du découpage de l'audio ; les
/// textes rendus pas à pas, bout à bout, sont le texte (aucun caractère
/// coupé n'est rendu avant d'être fini).
#[test]
fn a_synthetic_stream_gives_the_same_tokens_whatever_the_split() {
    let _serial = serial();
    let splits = vec![
        vec![],
        vec![7_680],
        vec![1_280],
        vec![1, 160, 1_279, 4_000, 37, 9_000, 2],
    ];
    let runs = on_the_cpu(complete("decoupages"), splits, MAX_TOKENS);
    let reference = &runs[0];
    // 3 s d'audio, + 2 + 6 + 1 + 10 jetons de marges, moins l'invite.
    assert!(reference.tokens.len() > 30, "{}", reference.tokens.len());
    assert!(!reference.finished, "la fin de texte ne gagne jamais");
    for (i, run) in runs.iter().enumerate() {
        assert_eq!(run.tokens, reference.tokens, "découpage {i} : jetons");
        assert_eq!(run.text, reference.text, "découpage {i} : texte");
        assert_eq!(run.fed, run.text, "découpage {i} : pas à pas");
    }
}

/// `maxTokens` : le jeton qui le dépasse est encore écrit, puis le flux est
/// fini et ne lit plus rien.
#[test]
fn the_stream_ends_after_max_tokens() {
    let _serial = serial();
    let runs = on_the_cpu(complete("max"), vec![vec![1_280]], 5);
    assert_eq!(runs[0].tokens.len(), 6);
    assert!(runs[0].finished);
    assert_eq!(runs[0].fed, runs[0].text);
}

/// Par le vrai `StreamingRecognizer` : la langue n'entre nulle part ; le
/// cache de MLX est vide après `finish` et après un flux lâché en route ;
/// libéré, le modèle fait échouer le flux resté ouvert, par son nom.
#[test]
fn a_synthetic_voxtral_ignores_the_language_and_empties_the_cache() {
    let _serial = serial();
    let dir = complete("reconnaisseur");
    let recognizer = VoxtralMlx::new();
    recognizer.load(&dir).unwrap();
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    recognizer.warm_up().unwrap();
    let samples = audio();
    let texts: Vec<String> = ["fr", "en", "zz", ""]
        .iter()
        .map(|language| {
            let mut stream = recognizer.open_stream(language).unwrap();
            for part in samples.chunks(1_280) {
                stream.feed(part).unwrap();
            }
            let text = stream.finish().unwrap();
            assert_eq!(
                crate::compute::mlx::memory().unwrap().cache,
                0,
                "{language}"
            );
            assert_eq!(stream.finish().unwrap(), text, "finish de nouveau");
            assert_eq!(stream.feed(&[0.0; 1_280]).unwrap(), "", "après finish");
            text
        })
        .collect();
    assert!(!texts[0].is_empty());
    assert!(texts.iter().all(|t| t == &texts[0]), "{texts:?}");

    let mut stream = recognizer.open_stream("fr").unwrap();
    stream.feed(&samples[..16_000]).unwrap();
    drop(stream);
    assert_eq!(
        crate::compute::mlx::memory().unwrap().cache,
        0,
        "flux lâché"
    );

    let mut stream = recognizer.open_stream("fr").unwrap();
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert!(recognizer.memory().is_none());
    let err = stream.feed(&[0.0; 1_280]).unwrap_err().to_string();
    assert!(err.contains("Voxtral"), "{err}");
    assert!(recognizer.open_stream("fr").is_err());
}

// ---------- les fichiers malformés ----------

fn load_error(dir: &std::path::Path) -> String {
    let _serial = serial();
    let err = VoxtralMlx::new().load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None, "le besoin retiré");
    format!("{err:#}")
}

#[test]
fn a_missing_key_is_named() {
    let tensors: Vec<Tensor> = tiny_tensors()
        .into_iter()
        .filter(|t| t.name != "decoder.layers.1.attention.wk.weight")
        .collect();
    let err = load_error(&synthetic("cle-absente", &tensors));
    assert!(
        err.contains("decoder.layers.1.attention.wk.weight"),
        "{err}"
    );
}

#[test]
fn a_key_of_the_wrong_shape_is_named() {
    let mut tensors = tiny_tensors();
    let norm = tensors
        .iter_mut()
        .find(|t| t.name == "encoder.transformer_norm.weight")
        .unwrap();
    norm.shape = vec![ENC as usize + 1];
    norm.values.push(1.0);
    let err = load_error(&synthetic("mauvaise-forme", &tensors));
    assert!(err.contains("encoder.transformer_norm.weight"), "{err}");
}

#[test]
fn an_extra_key_is_named() {
    let mut tensors = tiny_tensors();
    tensors.push(ones("decoder.layers.0.attention.wk.bias", 8));
    let err = load_error(&synthetic("cle-en-trop", &tensors));
    assert!(err.contains("decoder.layers.0.attention.wk.bias"), "{err}");
}

#[test]
fn a_missing_vocabulary_is_named() {
    let dir = complete("sans-vocabulaire");
    std::fs::remove_file(dir.join(TEKKEN_FILE)).unwrap();
    let err = load_error(&dir);
    assert!(err.contains(TEKKEN_FILE), "{err}");
}

#[test]
fn unreadable_weights_are_named() {
    let dir = complete("poids-illisibles");
    std::fs::write(
        dir.join("model.safetensors"),
        b"\x10\x00\x00\x00\x00\x00\x00\x00{\"a\":",
    )
    .unwrap();
    let err = load_error(&dir);
    assert!(err.contains("model.safetensors"), "{err}");
}

/// Une couche quantifiée sans quantification dans `config.json` : refusée,
/// nommée.
#[test]
fn scales_without_a_quantization_are_named() {
    let mut tensors = tiny_tensors();
    tensors.push(ones("decoder.layers.0.attention.wq.scales", 4));
    let err = load_error(&synthetic("echelles", &tensors));
    assert!(err.contains("decoder.layers.0.attention.wq"), "{err}");
    assert!(err.contains("quantization"), "{err}");
}
