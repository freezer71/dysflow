//! Les essais sans poids réels : le décodeur pas à pas contre la suite
//! entière sur un petit décodeur au hasard (`CanaryDecoderCacheTests`, sur
//! le processeur), puis un Canary synthétique (un Conformer d'une couche,
//! largeur 64, un bloc de décodeur) : le reconnaisseur, le plafond de 1 024
//! jetons, les jetons spéciaux retirés, l'abandon, et chaque fichier refusé
//! avec une erreur qui le nomme.

use super::config::DecoderConfig;
use super::decoder::{Decoder, MAX_POSITIONS};
use super::model::{Canary, MAX_TOKENS};
use super::tokenizer::tests::{base64_encode, model_bytes};
use super::*;
use crate::stt::mlx::parakeet::tests::{serial, tiny_keys};
use crate::stt::mlx::weights::Weights;
use crate::tts::{self, Cancelled};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::{random, Array, Stream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------- le décodeur au hasard (CanaryDecoderCacheTests.swift) ----------

const D: i32 = 32;
const VOCAB: i32 = 50;

/// `makeDecoder` : 2 blocs, largeur 32, 4 têtes, MLP de 64, vocabulaire de
/// 50 ; poids tirés comme ceux de MLXNN (`Linear` : uniformes dans
/// ±1/√entrées ; `Embedding` : normaux ; normes : 1 et 0), en Float32.
fn random_decoder() -> anyhow::Result<Decoder> {
    let config = DecoderConfig {
        num_layers: 2,
        hidden_size: D,
        num_attention_heads: 4,
        inner_size: 64,
    };
    let mut seed = 7u64;
    let mut draw = |shape: &[i32], scale: f32, normal: bool| -> anyhow::Result<Array> {
        seed += 1;
        let key = random::key(seed)?;
        Ok(if normal {
            random::normal::<f32>(shape, None, None, &key)?.multiply(Array::from_f32(scale))?
        } else {
            random::uniform::<_, f32>(-scale, scale, shape, &key)?
        })
    };
    let mut arrays: Vec<(String, Array)> = Vec::new();
    let mut linear = |arrays: &mut Vec<(String, Array)>, name: String, input: i32, output: i32| {
        let scale = (1.0 / input as f32).sqrt();
        arrays.push((
            format!("{name}.weight"),
            draw(&[output, input], scale, false)?,
        ));
        arrays.push((format!("{name}.bias"), draw(&[output], scale, false)?));
        anyhow::Ok(())
    };
    let norm = |arrays: &mut Vec<(String, Array)>, name: String| -> anyhow::Result<()> {
        arrays.push((format!("{name}.weight"), mlx_rs::ops::ones::<f32>(&[D])?));
        arrays.push((format!("{name}.bias"), mlx_rs::ops::zeros::<f32>(&[D])?));
        Ok(())
    };
    for i in 0..config.num_layers {
        let p = format!("decoder.blocks.{i}");
        for a in ["self_attn", "cross_attn"] {
            for proj in ["q_proj", "k_proj", "v_proj", "out_proj"] {
                linear(&mut arrays, format!("{p}.{a}.{proj}"), D, D)?;
            }
        }
        linear(&mut arrays, format!("{p}.ff1"), D, config.inner_size)?;
        linear(&mut arrays, format!("{p}.ff2"), config.inner_size, D)?;
        for n in ["self_attn_norm", "cross_attn_norm", "ff_norm"] {
            norm(&mut arrays, format!("{p}.{n}"))?;
        }
    }
    linear(&mut arrays, "decoder.output_proj".into(), D, VOCAB)?;
    norm(&mut arrays, "decoder.embedding_layer_norm".into())?;
    norm(&mut arrays, "decoder.final_norm".into())?;
    let key = random::key(seed + 1)?;
    let embedding = random::normal::<f32>(&[VOCAB, D], None, None, &key)?
        .multiply(Array::from_f32((1.0 / D as f32).sqrt()))?;
    arrays.push(("decoder.embedding.weight".into(), embedding));
    let mut w = Weights::from_arrays(
        "Canary",
        std::path::Path::new("au-hasard"),
        arrays,
        mlx_rs::Dtype::Float32,
    )?;
    let decoder = Decoder::load(&mut w, &config, VOCAB)?;
    w.finish()?;
    Ok(decoder)
}

fn ids(tokens: &[i32]) -> Array {
    Array::from_slice(tokens, &[1, tokens.len() as i32])
}

/// Les logits de la dernière position, toute la suite repassée.
fn full(decoder: &Decoder, tokens: &[i32], encoder: &Array, mask: &Array) -> anyhow::Result<Array> {
    let logits = decoder.forward(&ids(tokens), encoder, Some(mask), 0)?;
    Ok(logits.try_index((0, logits.shape()[1] - 1))?)
}

fn step(
    decoder: &Decoder,
    tokens: &[i32],
    state: &mut super::decoder::State,
) -> anyhow::Result<Array> {
    let logits = decoder.step(&ids(tokens), state)?;
    Ok(logits.try_index((0, logits.shape()[1] - 1))?)
}

/// `assertClose` : écart max < 1e-4 et même argmax ; rend l'écart.
fn close(a: &Array, b: &Array, what: &str) -> anyhow::Result<f32> {
    let gap = mlx_rs::ops::abs(a.subtract(b)?)?.max(None)?;
    let (ia, ib) = (argmax_axis(a, -1, None)?, argmax_axis(b, -1, None)?);
    mlx_rs::transforms::eval([&gap, &ia, &ib])?;
    let gap: f32 = gap.try_item_exact()?;
    anyhow::ensure!(gap < 1e-4, "{what} : écart {gap:e}");
    anyhow::ensure!(
        ia.try_item_exact::<u32>()? == ib.try_item_exact::<u32>()?,
        "{what} : argmax"
    );
    Ok(gap)
}

/// `body` sur le processeur, sur le fil MLX. Sur un M5, MLX fait les
/// produits Float32 de plusieurs lignes en TF32 sur le GPU, mais pas ceux
/// d'une seule (le pas de un jeton) : la comparaison mêlerait deux
/// précisions (tâche 18). Le flux par défaut du fil est celui du
/// processeur pour tout `body` : chaque opération de `mlx-rs` le lit.
fn on_the_cpu<R: Send + 'static>(
    body: impl FnOnce() -> anyhow::Result<R> + Send + 'static,
) -> anyhow::Result<R> {
    let held = Held::load(|| Ok(()))?;
    held.with(move |_| mlx_rs::with_stream(&Stream::cpu(), body))
}

/// `testStepByStepMatchesTheFullPass` : l'invite (5 jetons) puis chaque
/// jeton seul rendent les logits de la suite entière.
#[test]
fn step_by_step_matches_the_full_pass() {
    let (worst, offset) = on_the_cpu(|| {
        let decoder = random_decoder()?;
        let encoder = random::normal::<f32>(&[1, 20, D], None, None, &random::key(1)?)?;
        let mask = mlx_rs::ops::ones::<f32>(&[1, 20])?;
        let tokens: Vec<i32> = (0..16).map(|k| (k * 7 + 3) % VOCAB).collect();
        let mut state = decoder.make_state(&encoder, Some(&mask))?;
        let mut worst = close(
            &step(&decoder, &tokens[..5], &mut state)?,
            &full(&decoder, &tokens[..5], &encoder, &mask)?,
            "l'invite",
        )?;
        for k in 6..=tokens.len() {
            worst = worst.max(close(
                &step(&decoder, &tokens[k - 1..k], &mut state)?,
                &full(&decoder, &tokens[..k], &encoder, &mask)?,
                &format!("jeton {k}"),
            )?);
        }
        Ok((worst, state.offset()))
    })
    .unwrap();
    eprintln!("pas à pas contre suite entière, sur le processeur : écart max {worst:e}");
    assert_eq!(offset, 16);
}

/// `testPositionsPastTheTableRepeatTheLastOne` : au-delà de la table des
/// positions (1 024), la dernière est répétée, comme dans la suite entière.
#[test]
fn positions_past_the_table_repeat_the_last_one() {
    let worst = on_the_cpu(|| {
        let decoder = random_decoder()?;
        let encoder = random::normal::<f32>(&[1, 8, D], None, None, &random::key(2)?)?;
        let mask = mlx_rs::ops::ones::<f32>(&[1, 8])?;
        let tokens: Vec<i32> = (0..1030).map(|k| (k * 11 + 1) % VOCAB).collect();
        let mut state = decoder.make_state(&encoder, Some(&mask))?;
        step(&decoder, &tokens[..1020], &mut state)?.eval()?;
        let mut worst = 0f32;
        for k in 1021..=tokens.len() {
            worst = worst.max(close(
                &step(&decoder, &tokens[k - 1..k], &mut state)?,
                &full(&decoder, &tokens[..k], &encoder, &mask)?,
                &format!("position {}", k - 1),
            )?);
        }
        anyhow::ensure!(state.offset() as usize == tokens.len());
        anyhow::ensure!(state.offset() > MAX_POSITIONS);
        Ok(worst)
    })
    .unwrap();
    eprintln!("au-delà de la table, sur le processeur : écart max {worst:e}");
}

/// Plusieurs positions après l'invite : refusé (le Swift s'arrêterait sur
/// `precondition`).
#[test]
fn several_positions_after_the_prompt_are_refused() {
    let err = on_the_cpu(|| {
        let decoder = random_decoder()?;
        let encoder = mlx_rs::ops::zeros::<f32>(&[1, 4, D])?;
        let mut state = decoder.make_state(&encoder, None)?;
        step(&decoder, &[1, 2], &mut state)?;
        step(&decoder, &[3, 4], &mut state).map(drop)
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("Canary"), "{err}");
}

// ---------- un Canary synthétique ----------

fn repo_scratch(name: &str) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-canary-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le vocabulaire : `<unk>`, la fin (1), l'invite (2, 3, 4), `<|fr|>` (5),
/// `▁oui` (6), `<|en|>` (7).
const PIECES: [(&str, u64); 8] = [
    ("<unk>", 2),
    ("<|endoftext|>", 3),
    ("<|startofcontext|>", 4),
    ("<|startoftranscript|>", 4),
    ("<|emo:undefined|>", 4),
    ("<|fr|>", 4),
    ("▁oui", 1),
    ("<|en|>", 4),
];
const OUI: usize = 6;
const EN: usize = 7;
const TINY_VOCAB: usize = PIECES.len();
const TINY_D: usize = 64;
const TINY_INNER: usize = 32;

/// Le `config.json` réduit : l'encodeur de Parakeet des essais, un bloc de
/// décodeur, le vocabulaire en base64.
fn tiny_config() -> serde_json::Value {
    serde_json::json!({
        "encoder": {
            "feat_in": 128, "n_layers": 1, "d_model": TINY_D, "n_heads": 2, "ff_expansion_factor": 4,
            "subsampling_factor": 8, "self_attention_model": "rel_pos", "subsampling": "dw_striding",
            "conv_kernel_size": 9, "subsampling_conv_channels": 8, "pos_emb_max_len": 5000,
            "use_bias": false, "xscaling": false
        },
        "transf_decoder": {"num_layers": 1, "hidden_size": TINY_D, "num_attention_heads": 2, "inner_size": TINY_INNER},
        "vocab_size": TINY_VOCAB,
        "enc_output_dim": TINY_D,
        "tokenizer": {"model_base64": base64_encode(&model_bytes(&PIECES))}
    })
}

/// Un tenseur F32 : nom du fichier, forme, valeurs (nulles si vides).
struct Tensor {
    name: String,
    shape: Vec<usize>,
    values: Vec<f32>,
}

fn zeros(name: &str, shape: &[usize]) -> Tensor {
    Tensor {
        name: name.to_string(),
        shape: shape.to_vec(),
        values: Vec::new(),
    }
}

/// Les tenseurs d'un Canary synthétique, aux noms MLX du paquet. Tout est
/// nul, sauf le biais de la tête : son plus grand est `winner`, que chaque
/// pas rend.
fn tiny_tensors(winner: usize) -> Vec<Tensor> {
    let mut t: Vec<Tensor> = tiny_keys()
        .into_iter()
        .filter(|(k, _)| k.starts_with("encoder."))
        .map(|(k, s)| zeros(&k, &s))
        .collect();
    let (d, inner) = (TINY_D, TINY_INNER);
    t.push(zeros(
        "transf_decoder.token_embedding.weight",
        &[TINY_VOCAB, d],
    ));
    for n in ["embedding_layer_norm", "final_layer_norm"] {
        t.push(zeros(&format!("transf_decoder.{n}.weight"), &[d]));
        t.push(zeros(&format!("transf_decoder.{n}.bias"), &[d]));
    }
    let p = "transf_decoder.layers.0";
    for sub in ["first_sub_layer", "second_sub_layer"] {
        for proj in ["linear_q", "linear_k", "linear_v", "linear_out"] {
            t.push(zeros(&format!("{p}.{sub}.{proj}.weight"), &[d, d]));
            t.push(zeros(&format!("{p}.{sub}.{proj}.bias"), &[d]));
        }
    }
    t.push(zeros(
        &format!("{p}.third_sub_layer.linear1.weight"),
        &[inner, d],
    ));
    t.push(zeros(
        &format!("{p}.third_sub_layer.linear1.bias"),
        &[inner],
    ));
    t.push(zeros(
        &format!("{p}.third_sub_layer.linear2.weight"),
        &[d, inner],
    ));
    t.push(zeros(&format!("{p}.third_sub_layer.linear2.bias"), &[d]));
    for n in ["layer_norm_1", "layer_norm_2", "layer_norm_3"] {
        t.push(zeros(&format!("{p}.{n}.weight"), &[d]));
        t.push(zeros(&format!("{p}.{n}.bias"), &[d]));
    }
    t.push(zeros("head.classifier.weight", &[TINY_VOCAB, d]));
    t.push(Tensor {
        name: "head.classifier.bias".into(),
        shape: vec![TINY_VOCAB],
        values: (0..TINY_VOCAB)
            .map(|i| if i == winner { 1.0 } else { 0.0 })
            .collect(),
    });
    t
}

/// Un safetensors des tenseurs.
fn safetensors(tensors: &[Tensor]) -> Vec<u8> {
    let (mut header, mut data) = (serde_json::Map::new(), Vec::<u8>::new());
    for t in tensors {
        let count = t.shape.iter().product::<usize>();
        let start = data.len();
        for i in 0..count {
            data.extend_from_slice(&t.values.get(i).copied().unwrap_or(0.0).to_le_bytes());
        }
        header.insert(
            t.name.clone(),
            serde_json::json!({"dtype": "F32", "shape": t.shape, "data_offsets": [start, data.len()]}),
        );
    }
    let mut json = serde_json::Value::Object(header).to_string();
    while !json.len().is_multiple_of(8) {
        json.push(' ');
    }
    let mut file = (json.len() as u64).to_le_bytes().to_vec();
    file.extend_from_slice(json.as_bytes());
    file.extend_from_slice(&data);
    file
}

const WEIGHTS: &str = "model.safetensors";

fn synthetic(name: &str, config: &serde_json::Value, weights: &[u8]) -> PathBuf {
    let dir = repo_scratch(name);
    std::fs::write(dir.join(config::CONFIG_FILE), config.to_string()).unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    dir
}

fn complete(name: &str, winner: usize) -> PathBuf {
    synthetic(name, &tiny_config(), &safetensors(&tiny_tensors(winner)))
}

fn load_error(dir: &std::path::Path) -> String {
    let _serial = serial();
    let err = CanaryMlx::new().load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None, "le besoin retiré");
    format!("{err:#}")
}

/// Le reconnaisseur : charge, transcrit, s'arrête à 1 024 jetons (chaque
/// pas rend `▁oui`, jamais la fin ; les positions dépassent la table), rien
/// sous 0,1 s, se prépare, se libère.
#[test]
fn a_synthetic_canary_loads_and_stops_at_1024_tokens() {
    let _serial = serial();
    let dir = complete("complet", OUI);
    let recognizer = CanaryMlx::new();
    recognizer.load(&dir).unwrap();
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    assert!(recognizer.memory().is_some());
    let text = recognizer
        .transcribe(&[0.1; 16_000], "fr", &Cancel::never())
        .unwrap();
    assert_eq!(text, vec!["oui"; MAX_TOKENS].join(" "));
    assert_eq!(
        recognizer
            .transcribe(&[0.1; MIN_SAMPLES - 1], "fr", &Cancel::never())
            .unwrap(),
        "",
        "trop court : rien, sans lancer le modèle"
    );
    recognizer.warm_up().unwrap();
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert_eq!(recognizer.memory(), None);
}

/// L'invite porte la langue source et cible ; un jeton `<|…|>` que le
/// décodage laisse passer (ici `<|en|>`, défini par l'utilisateur) reste
/// dans le texte de `generate` et part du texte de l'app.
#[test]
fn the_special_tokens_are_removed_from_the_text() {
    let _serial = serial();
    let dir = complete("speciaux", EN);
    let held = Held::load(move || Canary::load(&dir)).unwrap();
    let transcript = held
        .with(|c| c.generate(&[0.1; 16_000], "fr", 3, &|| Ok(())))
        .unwrap();
    assert_eq!(transcript.prompt, [2, 3, 4, 5, 5]);
    assert_eq!(transcript.tokens, [EN as i64; 3]);
    assert_eq!(transcript.text, "<|en|><|en|><|en|>");
    assert_eq!(tokenizer::clean(&transcript.text), "");
    let english = held
        .with(|c| c.generate(&[0.1; 16_000], "en", 1, &|| Ok(())))
        .unwrap();
    assert_eq!(english.prompt, [2, 3, 4, 7, 7]);
}

/// La fin (`<|endoftext|>`) dès le premier pas : aucun jeton, aucun texte.
#[test]
fn the_end_of_text_stops_the_decoding() {
    let _serial = serial();
    let recognizer = CanaryMlx::new();
    recognizer.load(&complete("fin", 1)).unwrap();
    let text = recognizer
        .transcribe(&[0.1; 16_000], "fr", &Cancel::never())
        .unwrap();
    assert_eq!(text, "");
    recognizer.release();
}

/// Un abandon demandé pendant le passage le coupe après l'encodeur :
/// `Cancelled`, jamais un échec.
#[test]
fn a_cancel_during_the_pass_stops_it() {
    let _serial = serial();
    let recognizer = CanaryMlx::new();
    recognizer.load(&complete("abandon", OUI)).unwrap();
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    // Avant le passage, puis sous le verrou : non ; après l'encodeur : oui.
    let cancel = Cancel::when(move || counter.fetch_add(1, Ordering::SeqCst) >= 2);
    let err = recognizer
        .transcribe(&[0.1; 16_000], "fr", &cancel)
        .unwrap_err();
    assert!(tts::is_cancelled(&err), "{err:#}");
    assert!(err.downcast_ref::<Cancelled>().is_some());
    assert_eq!(
        asked.load(Ordering::SeqCst),
        3,
        "arrêté au premier point du passage"
    );
    recognizer.release();
}

#[test]
fn a_transcription_without_a_model_is_an_error() {
    let err = CanaryMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::never())
        .unwrap_err();
    assert!(format!("{err}").contains("Canary"), "{err}");
}

#[test]
fn a_cancel_before_the_pass_is_not_a_failure() {
    let err = CanaryMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::when(|| true))
        .unwrap_err();
    assert!(tts::is_cancelled(&err), "{err:#}");
}

// ---------- les fichiers refusés, chacun nommé ----------

#[test]
fn truncated_weights_are_an_error() {
    // Un en-tête annoncé de 1 000 octets, et 8 octets seulement.
    let mut bytes = 1000u64.to_le_bytes().to_vec();
    bytes.extend_from_slice(b"{\"a\":{\"d");
    let err = load_error(&synthetic("tronque", &tiny_config(), &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn truncated_tensor_data_is_an_error() {
    let mut bytes = safetensors(&tiny_tensors(OUI));
    bytes.truncate(bytes.len() - 100);
    let err = load_error(&synthetic("donnees-coupees", &tiny_config(), &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let tensors: Vec<Tensor> = tiny_tensors(OUI)
        .into_iter()
        .filter(|t| t.name != "transf_decoder.layers.0.second_sub_layer.linear_v.bias")
        .collect();
    let err = load_error(&synthetic(
        "cle-absente",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("decoder.blocks.0.cross_attn.v_proj.bias"),
        "{err}"
    );
}

#[test]
fn an_unexpected_shape_is_an_error_that_names_the_key_and_the_file() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros("head.classifier.weight", &[TINY_VOCAB, 32]));
    tensors.retain(|t| t.name != "head.classifier.weight" || t.shape[1] == 32);
    let err = load_error(&synthetic("forme", &tiny_config(), &safetensors(&tensors)));
    assert!(
        err.contains("decoder.output_proj.weight") && err.contains(WEIGHTS),
        "{err}"
    );
    assert!(err.contains("[8, 64]") && err.contains("[8, 32]"), "{err}");
}

#[test]
fn a_key_unknown_to_the_model_is_an_error_that_names_it() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros(
        "transf_decoder.layers.1.layer_norm_1.weight",
        &[TINY_D],
    ));
    let err = load_error(&synthetic(
        "cle-en-trop",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("decoder.blocks.1.self_attn_norm.weight"),
        "{err}"
    );
}

#[test]
fn a_quantized_layer_without_a_quantization_in_the_config_is_an_error() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros(
        "transf_decoder.token_embedding.scales",
        &[TINY_VOCAB, 1],
    ));
    let err = load_error(&synthetic(
        "quantifie",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("decoder.embedding") && err.contains("quantization"),
        "{err}"
    );
}

#[test]
fn missing_files_are_named() {
    let dir = complete("sans-config", OUI);
    std::fs::remove_file(dir.join(config::CONFIG_FILE)).unwrap();
    assert!(load_error(&dir).contains(config::CONFIG_FILE));
    let dir = complete("sans-poids", OUI);
    std::fs::remove_file(dir.join(WEIGHTS)).unwrap();
    assert!(load_error(&dir).contains("safetensors"));
}

#[test]
fn a_model_without_vocabulary_is_refused() {
    let mut config = tiny_config();
    config.as_object_mut().unwrap().remove("tokenizer");
    let err = load_error(&synthetic(
        "sans-vocabulaire",
        &config,
        &safetensors(&tiny_tensors(OUI)),
    ));
    assert!(err.contains("model_base64"), "{err}");
}

#[test]
fn the_nemo_layout_is_refused() {
    let tensors = [zeros(
        "transf_decoder._decoder.layers.0.layer_norm_1.weight",
        &[TINY_D],
    )];
    let err = load_error(&synthetic("nemo", &tiny_config(), &safetensors(&tensors)));
    assert!(err.contains("NeMo"), "{err}");
}

/// Un id de l'invite pris dans `config.json` (le vocabulaire n'a pas les
/// jetons spéciaux) hors de la table du décodeur : refusé, jamais lu hors
/// des bornes.
#[test]
fn a_prompt_token_outside_the_vocabulary_is_refused() {
    let mut config = tiny_config();
    config["tokenizer"]["model_base64"] =
        serde_json::json!(base64_encode(&model_bytes(&[("<unk>", 2), ("▁oui", 1)])));
    config["startofcontext_id"] = serde_json::json!(100);
    let _serial = serial();
    let recognizer = CanaryMlx::new();
    recognizer
        .load(&synthetic(
            "invite-hors",
            &config,
            &safetensors(&tiny_tensors(OUI)),
        ))
        .unwrap();
    let err = recognizer
        .transcribe(&[0.1; 16_000], "fr", &Cancel::never())
        .unwrap_err()
        .to_string();
    assert!(err.contains("100") && err.contains("Canary"), "{err}");
    recognizer.release();
}

#[test]
fn a_config_too_large_is_refused_without_reading_it() {
    let dir = complete("config-geante", OUI);
    let file = std::fs::File::create(dir.join(config::CONFIG_FILE)).unwrap();
    file.set_len((16 << 20) + 1).unwrap();
    let err = load_error(&dir);
    assert!(
        err.contains(config::CONFIG_FILE) && err.contains("16777217"),
        "{err}"
    );
}
