//! Les essais sans poids réels : un Qwen3-ASR synthétique (une couche
//! d'encodeur de largeur 8, une couche de décodeur de largeur 8, un
//! vocabulaire d'octets sans fusions et les six jetons du gabarit) : le
//! reconnaisseur, le garde-fou des répétitions, le plafond de jetons,
//! l'invite, l'abandon, et chaque fichier refusé avec une erreur qui le
//! nomme.
//!
//! Les plongements valent 1, `model.norm` 1 et la ligne `WINNER` de
//! `lm_head` 1 : chaque pas rend `WINNER` (« o »), jamais une fin (les ids
//! de fin, 151 643 et 151 645, sont hors de ce vocabulaire).

use super::model::{Qwen3Asr, MAX_TOKENS};
use super::*;
use crate::compute::mlx::tokenizer::bpe;
use crate::stt::mlx::canary::synthetic::{safetensors, zeros, Tensor};
use crate::stt::mlx::parakeet::tests::serial;
use crate::tts::{self, Cancelled};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const D: usize = 8;
const HIDDEN: usize = 4;
const FFN: usize = 16;
const MELS: usize = 128;
/// 256 octets, puis le gabarit.
const IM_START: usize = 256;
const IM_END: usize = 257;
const AUDIO_START: usize = 258;
const AUDIO_END: usize = 259;
const AUDIO_PAD: usize = 260;
const ASR_TEXT: usize = 261;
const VOCAB: usize = 262;
const WINNER: usize = b'o' as usize;
const WEIGHTS: &str = "model.safetensors";

fn tiny_config() -> serde_json::Value {
    serde_json::json!({
        "thinker_config": {
            "audio_config": {
                "num_mel_bins": MELS, "encoder_layers": 1, "encoder_attention_heads": 2,
                "encoder_ffn_dim": FFN, "d_model": D, "max_source_positions": 1500,
                "n_window": 50, "output_dim": D, "n_window_infer": 800,
                "downsample_hidden_size": HIDDEN
            },
            "text_config": {
                "vocab_size": VOCAB, "hidden_size": D, "intermediate_size": FFN,
                "num_hidden_layers": 1, "num_attention_heads": 2, "num_key_value_heads": 1,
                "head_dim": 4, "rms_norm_eps": 1e-6, "tie_word_embeddings": false,
                "rope_theta": 1000000.0
            },
            "audio_token_id": AUDIO_PAD,
            "audio_start_token_id": AUDIO_START,
            "audio_end_token_id": AUDIO_END
        },
        "support_languages": ["English", "French"]
    })
}

fn filled(name: &str, shape: &[usize], value: f32) -> Tensor {
    Tensor {
        name: name.to_string(),
        shape: shape.to_vec(),
        values: vec![value; shape.iter().product()],
    }
}

/// Les tenseurs du fichier, sous les clés de Hugging Face (`thinker.`,
/// convolutions `[sorties, entrées, h, l]`) si `thinker`.
fn tiny_tensors(thinker: bool) -> Vec<Tensor> {
    let p = if thinker { "thinker." } else { "" };
    let mut t = Vec::new();
    for (i, input) in [(1, 1), (2, HIDDEN), (3, HIDDEN)] {
        let shape = if thinker {
            [HIDDEN, input, 3, 3]
        } else {
            [HIDDEN, 3, 3, input]
        };
        t.push(zeros(&format!("{p}audio_tower.conv2d{i}.weight"), &shape));
        t.push(zeros(&format!("{p}audio_tower.conv2d{i}.bias"), &[HIDDEN]));
    }
    t.push(zeros(
        &format!("{p}audio_tower.conv_out.weight"),
        &[D, HIDDEN * 16],
    ));
    let l = format!("{p}audio_tower.layers.0");
    for proj in ["q_proj", "k_proj", "v_proj", "out_proj"] {
        t.push(zeros(&format!("{l}.self_attn.{proj}.weight"), &[D, D]));
        t.push(zeros(&format!("{l}.self_attn.{proj}.bias"), &[D]));
    }
    for norm in ["self_attn_layer_norm", "final_layer_norm"] {
        t.push(filled(&format!("{l}.{norm}.weight"), &[D], 1.0));
        t.push(zeros(&format!("{l}.{norm}.bias"), &[D]));
    }
    t.push(zeros(&format!("{l}.fc1.weight"), &[FFN, D]));
    t.push(zeros(&format!("{l}.fc1.bias"), &[FFN]));
    t.push(zeros(&format!("{l}.fc2.weight"), &[D, FFN]));
    t.push(zeros(&format!("{l}.fc2.bias"), &[D]));
    t.push(filled(&format!("{p}audio_tower.ln_post.weight"), &[D], 1.0));
    t.push(zeros(&format!("{p}audio_tower.ln_post.bias"), &[D]));
    for (name, input) in [("proj1", D), ("proj2", D)] {
        t.push(zeros(&format!("{p}audio_tower.{name}.weight"), &[D, input]));
        t.push(zeros(&format!("{p}audio_tower.{name}.bias"), &[D]));
    }

    t.push(filled(
        &format!("{p}model.embed_tokens.weight"),
        &[VOCAB, D],
        1.0,
    ));
    let l = format!("{p}model.layers.0");
    for (proj, out) in [("q_proj", 8), ("k_proj", 4), ("v_proj", 4)] {
        t.push(zeros(&format!("{l}.self_attn.{proj}.weight"), &[out, D]));
    }
    t.push(zeros(&format!("{l}.self_attn.o_proj.weight"), &[D, 8]));
    for norm in ["q_norm", "k_norm"] {
        t.push(filled(&format!("{l}.self_attn.{norm}.weight"), &[4], 1.0));
    }
    for (proj, shape) in [
        ("gate_proj", [FFN, D]),
        ("up_proj", [FFN, D]),
        ("down_proj", [D, FFN]),
    ] {
        t.push(zeros(&format!("{l}.mlp.{proj}.weight"), &shape));
    }
    for norm in ["input_layernorm", "post_attention_layernorm"] {
        t.push(filled(&format!("{l}.{norm}.weight"), &[D], 1.0));
    }
    t.push(filled(&format!("{p}model.norm.weight"), &[D], 1.0));
    let mut head = zeros(&format!("{p}lm_head.weight"), &[VOCAB, D]);
    head.values = (0..VOCAB * D)
        .map(|i| if i / D == WINNER { 1.0 } else { 0.0 })
        .collect();
    t.push(head);
    t
}

/// `vocab.json` : chaque octet (par la table de GPT-2) à son rang ;
/// `merges.txt` sans fusion ; le gabarit en jetons ajoutés.
fn write_tokenizer(dir: &std::path::Path) {
    let vocab: serde_json::Map<String, serde_json::Value> = bpe::byte_encoder()
        .iter()
        .enumerate()
        .map(|(i, c)| (c.to_string(), serde_json::json!(i)))
        .collect();
    std::fs::write(
        dir.join(bpe::VOCAB_FILE),
        serde_json::Value::Object(vocab).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join(bpe::MERGES_FILE), "#version: 0.2\n").unwrap();
    let added: serde_json::Map<String, serde_json::Value> = [
        (IM_START, "<|im_start|>"),
        (IM_END, "<|im_end|>"),
        (AUDIO_START, "<|audio_start|>"),
        (AUDIO_END, "<|audio_end|>"),
        (AUDIO_PAD, "<|audio_pad|>"),
        (ASR_TEXT, "<asr_text>"),
    ]
    .into_iter()
    .map(|(id, content)| (id.to_string(), serde_json::json!({ "content": content })))
    .collect();
    let config = serde_json::json!({ "added_tokens_decoder": added });
    std::fs::write(dir.join(bpe::CONFIG_FILE), config.to_string()).unwrap();
}

fn synthetic(name: &str, config: &serde_json::Value, weights: &[u8]) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-qwen3-asr-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(config::CONFIG_FILE), config.to_string()).unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    write_tokenizer(&dir);
    dir
}

fn complete(name: &str) -> PathBuf {
    synthetic(name, &tiny_config(), &safetensors(&tiny_tensors(false)))
}

fn load_error(dir: &std::path::Path) -> String {
    let _serial = serial();
    let err = crate::i18n::with_ui_lang(crate::i18n::UiLang::Fr, || {
        Qwen3AsrMlx::new().load(dir).unwrap_err()
    });
    assert_eq!(budget::of(BUDGET_KEY), None, "le besoin retiré");
    format!("{err:#}")
}

/// Les ids d'un texte d'octets.
fn bytes(text: &str) -> Vec<u32> {
    text.bytes().map(u32::from).collect()
}

/// Le reconnaisseur : charge, transcrit (le garde-fou des répétitions
/// arrête la tranche après 24 « o »), rien sous 0,1 s, se prépare, se
/// libère.
#[test]
fn a_synthetic_qwen3_asr_loads_and_stops_on_repetitions() {
    let _serial = serial();
    let dir = complete("complet");
    let recognizer = Qwen3AsrMlx::new();
    recognizer.load(&dir).unwrap();
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    assert!(recognizer.memory().is_some());
    let text = recognizer
        .transcribe(&[0.1; 16_000], "fr", &Cancel::never())
        .unwrap();
    assert_eq!(text, "o".repeat(24));
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

/// L'invite : le gabarit, un `<|audio_pad|>` par jeton audio (14 pour une
/// seconde), et « language French<asr_text> » en fin si la langue est
/// imposée ; le plafond de jetons tient pour toute la lecture.
#[test]
fn the_prompt_carries_the_audio_and_the_forced_language() {
    let _serial = serial();
    let dir = complete("invite");
    let held = Held::load(move || Qwen3Asr::load(&dir)).unwrap();
    let (french, none, capped) = held
        .with(|asr| {
            let french = asr.generate(&[0.1; 16_000], Some("fr"), 3, &|| Ok(()))?;
            let none = asr.generate(&[0.1; 16_000], None, 2, &|| Ok(()))?;
            let capped = asr.generate(&[0.1; 1_600], Some("en"), 1, &|| Ok(()))?;
            Ok((french, none, capped))
        })
        .unwrap();
    let mut head = vec![IM_START as u32];
    head.extend(bytes("system\n"));
    head.push(IM_END as u32);
    head.extend(bytes("\n"));
    head.push(IM_START as u32);
    head.extend(bytes("user\n"));
    head.push(AUDIO_START as u32);
    head.extend(std::iter::repeat_n(AUDIO_PAD as u32, 14));
    head.extend([AUDIO_END as u32, IM_END as u32]);
    head.extend(bytes("\n"));
    head.push(IM_START as u32);
    head.extend(bytes("assistant\n"));
    let mut fr = head.clone();
    fr.extend(bytes("language French"));
    fr.push(ASR_TEXT as u32);
    assert_eq!(french.chunks.len(), 1);
    assert_eq!(french.chunks[0].prompt, fr);
    assert_eq!(french.chunks[0].tokens, [WINNER as u32; 3]);
    assert_eq!(french.text, "ooo");
    assert_eq!(none.chunks[0].prompt, head);
    assert_eq!(none.text, "oo");
    // 0,1 s complété à une seconde : 14 jetons audio aussi (101 trames).
    let mut en = head;
    en.extend(bytes("language English"));
    en.push(ASR_TEXT as u32);
    assert_eq!(capped.chunks[0].prompt, en);
    assert_eq!(capped.chunks[0].tokens.len(), 1);
}

/// Les poids de Hugging Face (`thinker.`, convolutions de PyTorch) se
/// chargent comme ceux de mlx-community, et rendent le même texte.
#[test]
fn the_hugging_face_layout_is_sanitized() {
    let _serial = serial();
    let dir = synthetic("thinker", &tiny_config(), &safetensors(&tiny_tensors(true)));
    let recognizer = Qwen3AsrMlx::new();
    recognizer.load(&dir).unwrap();
    let text = recognizer
        .transcribe(&[0.1; 16_000], "en", &Cancel::never())
        .unwrap();
    assert_eq!(text, "o".repeat(24));
    recognizer.release();
}

/// Un abandon demandé pendant le passage le coupe après l'encodeur :
/// `Cancelled`, jamais un échec.
#[test]
fn a_cancel_during_the_pass_stops_it() {
    let _serial = serial();
    let recognizer = Qwen3AsrMlx::new();
    recognizer.load(&complete("abandon")).unwrap();
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
    let err = Qwen3AsrMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::never())
        .unwrap_err();
    assert!(format!("{err}").contains("Qwen3-ASR"), "{err}");
}

#[test]
fn a_cancel_before_the_pass_is_not_a_failure() {
    let err = Qwen3AsrMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::when(|| true))
        .unwrap_err();
    assert!(tts::is_cancelled(&err), "{err:#}");
}

/// Le plafond de l'app : 8 192 jetons pour toute la lecture.
#[test]
fn the_token_budget_is_the_swift_default() {
    assert_eq!(MAX_TOKENS, 8192);
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
    let mut bytes = safetensors(&tiny_tensors(false));
    bytes.truncate(bytes.len() - 100);
    let err = load_error(&synthetic("donnees-coupees", &tiny_config(), &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let tensors: Vec<Tensor> = tiny_tensors(false)
        .into_iter()
        .filter(|t| t.name != "model.layers.0.self_attn.k_norm.weight")
        .collect();
    let err = load_error(&synthetic(
        "cle-absente",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("model.layers.0.self_attn.k_norm.weight"),
        "{err}"
    );
}

#[test]
fn an_unexpected_shape_is_an_error_that_names_the_key_and_the_file() {
    let mut tensors = tiny_tensors(false);
    tensors.retain(|t| t.name != "audio_tower.proj2.weight");
    tensors.push(zeros("audio_tower.proj2.weight", &[D, 4]));
    let err = load_error(&synthetic("forme", &tiny_config(), &safetensors(&tensors)));
    assert!(
        err.contains("audio_tower.proj2.weight") && err.contains(WEIGHTS),
        "{err}"
    );
}

#[test]
fn a_key_unknown_to_the_model_is_an_error_that_names_it() {
    let mut tensors = tiny_tensors(false);
    tensors.push(zeros("model.layers.1.input_layernorm.weight", &[D]));
    let err = load_error(&synthetic(
        "cle-en-trop",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("model.layers.1.input_layernorm.weight"),
        "{err}"
    );
}

/// Un `lm_head` lié : écarté comme dans le Swift, pas une clé en trop.
#[test]
fn a_tied_lm_head_is_dropped() {
    let mut config = tiny_config();
    config["thinker_config"]["text_config"]["tie_word_embeddings"] = serde_json::json!(true);
    let _serial = serial();
    let dir = synthetic("lie", &config, &safetensors(&tiny_tensors(false)));
    let held = Held::load(move || Qwen3Asr::load(&dir)).unwrap();
    // Les plongements valent tous 1 : tous les logits sont égaux, l'argmax
    // est le premier id.
    let transcript = held
        .with(|asr| asr.generate(&[0.1; 16_000], Some("fr"), 2, &|| Ok(())))
        .unwrap();
    assert_eq!(transcript.chunks[0].tokens, [0, 0]);
}

#[test]
fn a_quantized_audio_encoder_is_refused() {
    let mut tensors = tiny_tensors(false);
    tensors.push(zeros("audio_tower.proj1.scales", &[D, 1]));
    let mut config = tiny_config();
    config["quantization"] = serde_json::json!({"group_size": 8, "bits": 8});
    let err = load_error(&synthetic(
        "encodeur-quantifie",
        &config,
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("audio_tower.proj1") && err.contains("encodeur audio"),
        "{err}"
    );
}

#[test]
fn a_quantized_layer_without_a_quantization_in_the_config_is_an_error() {
    let mut tensors = tiny_tensors(false);
    tensors.push(zeros("model.embed_tokens.scales", &[VOCAB, 1]));
    let err = load_error(&synthetic(
        "quantifie",
        &tiny_config(),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("model.embed_tokens") && err.contains("quantization"),
        "{err}"
    );
}

#[test]
fn an_encoder_wider_than_the_decoder_is_refused() {
    let mut config = tiny_config();
    config["thinker_config"]["audio_config"]["output_dim"] = serde_json::json!(16);
    let err = load_error(&synthetic(
        "largeurs",
        &config,
        &safetensors(&tiny_tensors(false)),
    ));
    assert!(
        err.contains("output_dim = 16") && err.contains("hidden_size = 8"),
        "{err}"
    );
}

#[test]
fn missing_files_are_named() {
    for file in [
        config::CONFIG_FILE,
        bpe::VOCAB_FILE,
        bpe::MERGES_FILE,
        bpe::CONFIG_FILE,
    ] {
        let dir = complete("sans-fichier");
        std::fs::remove_file(dir.join(file)).unwrap();
        let err = load_error(&dir);
        assert!(err.contains(file), "{file} : {err}");
    }
    let dir = complete("sans-poids");
    std::fs::remove_file(dir.join(WEIGHTS)).unwrap();
    assert!(load_error(&dir).contains("safetensors"));
}

#[test]
fn a_malformed_tokenizer_is_refused_by_name() {
    let dir = complete("vocabulaire-casse");
    std::fs::write(dir.join(bpe::VOCAB_FILE), b"[1, 2]").unwrap();
    let err = load_error(&dir);
    assert!(err.contains(bpe::VOCAB_FILE), "{err}");
}

#[test]
fn the_forced_aligner_is_refused() {
    let mut config = tiny_config();
    config["thinker_config"]["classify_num"] = serde_json::json!(5000);
    let err = load_error(&synthetic(
        "aligneur",
        &config,
        &safetensors(&tiny_tensors(false)),
    ));
    assert!(err.contains("qwen3_forced_aligner"), "{err}");
}

#[test]
fn a_config_too_large_is_refused_without_reading_it() {
    let dir = complete("config-geante");
    let file = std::fs::File::create(dir.join(config::CONFIG_FILE)).unwrap();
    file.set_len((1 << 20) + 1).unwrap();
    let err = load_error(&dir);
    assert!(
        err.contains(config::CONFIG_FILE) && err.contains("1048576"),
        "{err}"
    );
}
