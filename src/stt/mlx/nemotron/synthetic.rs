//! Les essais sans poids réels : un Nemotron synthétique (un bloc
//! Conformer de largeur 64, invite de 4, vocabulaire de 4 mots, tout nul
//! sauf le biais du réseau joint, qui choisit le jeton de chaque pas) : le
//! flux du reconnaisseur, le plafond de jetons par trame, les jetons
//! spéciaux, l'invite, le découpage des `feed`, la libération, et chaque
//! fichier refusé avec une erreur qui le nomme.

use super::model::Nemotron;
use super::stream::Session;
use super::*;
use crate::stt::mlx::canary::synthetic::{safetensors, zeros, Tensor};
use crate::stt::mlx::parakeet::tests::serial;
use std::path::PathBuf;

const D: usize = 64;
const FF: usize = 4 * D;
const CHANNELS: usize = 8;
/// 128 bandes après trois étages causaux (`⌊n / 2⌋ + 1`) : 17.
const FREQ: usize = 17;
const KERNEL: usize = 9;
const PROMPTS: usize = 4;
const PROMPT_HIDDEN: usize = 16;
const PRED: usize = 8;
const JOINT: usize = 8;
/// `<unk>`, `▁oui` (1), `<en-US>` (2), `▁non` (3) ; le blanc est 4.
const VOCABULARY: [&str; 4] = ["<unk>", "▁oui", "<en-US>", "▁non"];
const OUI: usize = 1;
const TAG: usize = 2;
const BLANK: usize = VOCABULARY.len();
/// Deux jetons au plus par trame de l'encodeur.
const MAX_SYMBOLS: usize = 2;

fn scratch(name: &str) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-nemotron-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le `config.json` réduit : un bloc, l'invite (`fr` 1, `auto` 3), le
/// vocabulaire.
fn tiny_config() -> serde_json::Value {
    serde_json::json!({
        "model_type": "nemotron_asr",
        "preprocessor": {"normalize": "NA"},
        "encoder": {
            "feat_in": 128, "n_layers": 1, "d_model": D, "n_heads": 2, "ff_expansion_factor": 4,
            "subsampling_factor": 8, "subsampling_conv_channels": CHANNELS, "conv_kernel_size": KERNEL,
            "causal_downsampling": true, "conv_context_size": "causal", "conv_norm_type": "layer_norm",
            "att_context_size": [[56, 13]]
        },
        "prompt": {"num_prompts": PROMPTS, "prompt_hidden": PROMPT_HIDDEN,
                   "prompt_dictionary": {"fr": 1, "en": 2, "auto": 3}},
        "decoder": {"pred_hidden": PRED, "pred_rnn_layers": 1, "vocab_size": VOCABULARY.len(), "blank_as_pad": true},
        "joint": {"joint_hidden": JOINT, "activation": "relu", "encoder_hidden": D, "pred_hidden": PRED,
                  "num_classes": VOCABULARY.len()},
        "vocabulary": VOCABULARY,
        "default_att_context_size": [56, 13],
        "max_symbols": MAX_SYMBOLS
    })
}

/// Les tenseurs d'un Nemotron synthétique, aux noms NeMo du paquet. Tout
/// est nul, sauf le biais du réseau joint : son plus grand est `winner`,
/// que chaque pas rend.
fn tiny_tensors(winner: usize) -> Vec<Tensor> {
    let c = CHANNELS;
    let mut t = vec![
        zeros("encoder.pre_encode.conv.0.weight", &[c, 3, 3, 1]),
        zeros("encoder.pre_encode.conv.0.bias", &[c]),
        zeros("encoder.pre_encode.out.weight", &[D, c * FREQ]),
        zeros("encoder.pre_encode.out.bias", &[D]),
    ];
    for (depthwise, pointwise) in [(2, 3), (5, 6)] {
        t.push(zeros(
            &format!("encoder.pre_encode.conv.{depthwise}.weight"),
            &[c, 3, 3, 1],
        ));
        t.push(zeros(
            &format!("encoder.pre_encode.conv.{depthwise}.bias"),
            &[c],
        ));
        t.push(zeros(
            &format!("encoder.pre_encode.conv.{pointwise}.weight"),
            &[c, 1, 1, c],
        ));
        t.push(zeros(
            &format!("encoder.pre_encode.conv.{pointwise}.bias"),
            &[c],
        ));
    }
    let l = "encoder.layers.0";
    for norm in [
        "norm_feed_forward1",
        "norm_self_att",
        "norm_conv",
        "norm_feed_forward2",
        "norm_out",
        "conv.batch_norm",
    ] {
        t.push(zeros(&format!("{l}.{norm}.weight"), &[D]));
        t.push(zeros(&format!("{l}.{norm}.bias"), &[D]));
    }
    for ffn in ["feed_forward1", "feed_forward2"] {
        t.push(zeros(&format!("{l}.{ffn}.linear1.weight"), &[FF, D]));
        t.push(zeros(&format!("{l}.{ffn}.linear2.weight"), &[D, FF]));
    }
    for proj in [
        "linear_q",
        "linear_k",
        "linear_v",
        "linear_out",
        "linear_pos",
    ] {
        t.push(zeros(&format!("{l}.self_attn.{proj}.weight"), &[D, D]));
    }
    t.push(zeros(&format!("{l}.self_attn.pos_bias_u"), &[2, D / 2]));
    t.push(zeros(&format!("{l}.self_attn.pos_bias_v"), &[2, D / 2]));
    t.push(zeros(
        &format!("{l}.conv.pointwise_conv1.weight"),
        &[2 * D, 1, D],
    ));
    t.push(zeros(
        &format!("{l}.conv.depthwise_conv.weight"),
        &[D, KERNEL, 1],
    ));
    t.push(zeros(
        &format!("{l}.conv.pointwise_conv2.weight"),
        &[D, 1, D],
    ));
    t.push(zeros(
        "prompt_kernel.0.weight",
        &[PROMPT_HIDDEN, D + PROMPTS],
    ));
    t.push(zeros("prompt_kernel.0.bias", &[PROMPT_HIDDEN]));
    t.push(zeros("prompt_kernel.2.weight", &[D, PROMPT_HIDDEN]));
    t.push(zeros("prompt_kernel.2.bias", &[D]));
    let rnn = "decoder.prediction.dec_rnn.lstm.0";
    t.push(zeros("decoder.prediction.embed.weight", &[BLANK + 1, PRED]));
    t.push(zeros(&format!("{rnn}.Wx"), &[4 * PRED, PRED]));
    t.push(zeros(&format!("{rnn}.Wh"), &[4 * PRED, PRED]));
    t.push(zeros(&format!("{rnn}.bias"), &[4 * PRED]));
    t.push(zeros("joint.pred.weight", &[JOINT, PRED]));
    t.push(zeros("joint.pred.bias", &[JOINT]));
    t.push(zeros("joint.enc.weight", &[JOINT, D]));
    t.push(zeros("joint.enc.bias", &[JOINT]));
    t.push(zeros("joint.joint_net.2.weight", &[BLANK + 1, JOINT]));
    t.push(Tensor {
        name: "joint.joint_net.2.bias".into(),
        shape: vec![BLANK + 1],
        values: (0..=BLANK)
            .map(|i| if i == winner { 1.0 } else { 0.0 })
            .collect(),
    });
    t
}

const WEIGHTS: &str = "model.safetensors";

fn synthetic(name: &str, config: &serde_json::Value, weights: &[u8]) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join(config::CONFIG_FILE), config.to_string()).unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    dir
}

fn complete(name: &str, winner: usize) -> PathBuf {
    synthetic(name, &tiny_config(), &safetensors(&tiny_tensors(winner)))
}

fn load_error(dir: &std::path::Path) -> String {
    let _serial = serial();
    let err = NemotronMlx::new().load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None, "le besoin retiré");
    format!("{err:#}")
}

/// Une seconde d'audio (101 trames de mel) en 14 trames de l'encodeur
/// (`⌊n / 2⌋ + 1`, trois fois).
const FRAMES: usize = 14;

/// Le reconnaisseur : charge, ouvre un flux, rend le texte au fil des
/// tranches (seulement ce qui l'allonge), puis le texte entier ; chaque
/// trame rend `▁oui` au plus `max_symbols` fois. Il se prépare et se libère.
#[test]
fn a_synthetic_nemotron_streams_at_most_max_symbols_per_frame() {
    let _serial = serial();
    let dir = complete("complet", OUI);
    let recognizer = NemotronMlx::new();
    recognizer.load(&dir).unwrap();
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    assert!(recognizer.memory().is_some());
    let mut stream = recognizer.open_stream("fr").unwrap();
    let audio = vec![0.1f32; 16_000];
    let mut fed = String::new();
    let mut growing = 0;
    for block in audio.chunks(1_280) {
        let piece = stream.feed(block).unwrap();
        growing += usize::from(!piece.is_empty());
        fed.push_str(&piece);
    }
    let text = stream.finish().unwrap();
    assert_eq!(text, vec!["oui"; FRAMES * MAX_SYMBOLS].join(" "));
    assert!(growing >= 1, "du texte avant la fin");
    assert!(text.starts_with(fed.trim_start()), "{fed:?}");
    assert_eq!(stream.feed(&[0.0; 1_280]).unwrap(), "", "après finish");
    recognizer.warm_up().unwrap();
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert_eq!(recognizer.memory(), None);
}

/// Le même audio, quel que soit le découpage des `feed`, rend les mêmes
/// jetons et le même texte ; seules des tranches entières de trames gelées
/// sont lues avant `finish`.
#[test]
fn the_text_does_not_depend_on_how_a_synthetic_stream_is_fed() {
    let _serial = serial();
    let dir = complete("decoupage", OUI);
    let held = Held::load(move || Nemotron::load(&dir)).unwrap();
    let runs = held
        .with(|model| {
            let audio: Vec<f32> = (0..40_000)
                .map(|i| ((i % 97) as f32 - 48.0) / 480.0)
                .collect();
            let mut runs = Vec::new();
            for sizes in [
                &[][..],
                &[8_960],
                &[1_280],
                &[1, 160, 1_279, 4_000, 37, 9_000, 2],
            ] {
                let mut session = Session::new(model, "fr", Some(CHUNK_MS));
                let mut chunks = Vec::new();
                let mut trace = |p: &mlx_rs::Array| {
                    chunks.push(p.shape()[1]);
                    Ok(())
                };
                let parts: Vec<&[f32]> = if sizes.is_empty() {
                    vec![&audio]
                } else {
                    let mut parts = Vec::new();
                    let (mut at, mut i) = (0, 0);
                    while at < audio.len() {
                        let end = (at + sizes[i % sizes.len()]).min(audio.len());
                        parts.push(&audio[at..end]);
                        at = end;
                        i += 1;
                    }
                    parts
                };
                for part in parts {
                    session.step(model, part, &mut trace)?;
                }
                session.finish(model, &mut trace)?;
                runs.push((chunks, session.tokens(), session.text().to_string()));
            }
            Ok(runs)
        })
        .unwrap();
    // 2,5 s : 251 trames de mel, quatre tranches de 56 puis 27 ; 7 trames
    // de l'encodeur par tranche entière, 5 pour la dernière : les 33 de la
    // lecture d'un coup (`⌊n / 2⌋ + 1` trois fois).
    assert_eq!(runs[0].0, [7, 7, 7, 7, 5]);
    for run in &runs[1..] {
        assert_eq!(run, &runs[0]);
    }
    assert_eq!(runs[0].1.len(), 33 * MAX_SYMBOLS);
}

/// Un jeton spécial (`<en-US>`) est lu, mais ni gardé ni écrit.
#[test]
fn a_special_token_is_neither_kept_nor_written() {
    let _serial = serial();
    let dir = complete("special", TAG);
    let recognizer = NemotronMlx::new();
    recognizer.load(&dir).unwrap();
    let mut stream = recognizer.open_stream("fr").unwrap();
    assert_eq!(stream.feed(&[0.1; 16_000]).unwrap(), "");
    assert_eq!(stream.finish().unwrap(), "");
}

/// `resolvePromptIndex` : la langue, sinon celle par défaut (`auto`), sinon
/// 0 ; la session prend la tranche de 560 ms (7 trames) ou la native.
#[test]
fn the_prompt_follows_the_language_then_the_default() {
    let _serial = serial();
    let dir = complete("invite", OUI);
    let held = Held::load(move || Nemotron::load(&dir)).unwrap();
    let indices = held
        .with(|model| {
            let found = [
                model.prompt_index("fr"),
                model.prompt_index("en"),
                model.prompt_index("xx"),
            ];
            model.config.default_language = "absente".into();
            Ok((found, model.prompt_index("xx")))
        })
        .unwrap();
    assert_eq!(indices, ([1, 2, 3], 0));
}

#[test]
fn a_stream_without_a_model_is_an_error() {
    let err = NemotronMlx::new()
        .open_stream("fr")
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("Nemotron"), "{err}");
}

/// Libéré, ou remplacé par un autre chargement, le modèle laisse les flux
/// ouverts en erreur, sans panique.
#[test]
fn a_stream_outliving_its_model_fails() {
    let _serial = serial();
    let dir = complete("liberation", OUI);
    let recognizer = NemotronMlx::new();
    recognizer.load(&dir).unwrap();
    let mut old = recognizer.open_stream("fr").unwrap();
    recognizer.load(&dir).unwrap();
    assert!(old.feed(&[0.0; 1_280]).is_err(), "modèle remplacé");
    let mut stream = recognizer.open_stream("fr").unwrap();
    stream.feed(&[0.0; 1_280]).unwrap();
    recognizer.release();
    assert!(stream.feed(&[0.0; 1_280]).is_err());
    assert!(stream.finish().is_err());
}

// ---------- les fichiers refusés, chacun nommé ----------

#[test]
fn truncated_weights_are_an_error() {
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
        .filter(|t| t.name != "encoder.layers.0.self_attn.pos_bias_v")
        .collect();
    let err = load_error(&synthetic(
        "cle-absente",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(err.contains("encoder.layers.0.self_attn.posBiasV"), "{err}");
}

#[test]
fn an_unexpected_shape_is_an_error_that_names_the_key_and_the_file() {
    let mut tensors = tiny_tensors(OUI);
    tensors.retain(|t| t.name != "prompt_kernel.0.weight");
    tensors.push(zeros("prompt_kernel.0.weight", &[PROMPT_HIDDEN, D]));
    let err = load_error(&synthetic("forme", &tiny_config(), &safetensors(&tensors)));
    assert!(
        err.contains("prompt_kernel.linear0.weight") && err.contains(WEIGHTS),
        "{err}"
    );
    assert!(
        err.contains("[16, 64]") && err.contains("[16, 68]"),
        "{err}"
    );
}

#[test]
fn a_key_unknown_to_the_model_is_an_error_that_names_it() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros("encoder.layers.1.norm_out.weight", &[D]));
    let err = load_error(&synthetic(
        "cle-en-trop",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(err.contains("encoder.layers.1.norm_out.weight"), "{err}");
}

#[test]
fn a_quantized_layer_without_a_quantization_in_the_config_is_an_error() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros("joint.enc.scales", &[JOINT, 1]));
    let err = load_error(&synthetic(
        "quantifie",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("joint.enc") && err.contains("quantization"),
        "{err}"
    );
}

#[test]
fn a_quantized_pointwise_convolution_is_refused_by_name() {
    let mut tensors = tiny_tensors(OUI);
    tensors.push(zeros(
        "encoder.layers.0.conv.pointwise_conv2.scales",
        &[D, 1],
    ));
    let err = load_error(&synthetic(
        "ponctuelle",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("encoder.layers.0.conv.pointwise_conv2.scales"),
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
fn an_unreadable_config_is_an_error_that_names_it() {
    let dir = complete("config-illisible", OUI);
    std::fs::write(dir.join(config::CONFIG_FILE), "{ pas du json").unwrap();
    let err = load_error(&dir);
    assert!(
        err.contains(config::CONFIG_FILE) && err.contains("Nemotron"),
        "{err}"
    );
}
