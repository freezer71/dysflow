//! Les essais sur un Whisper synthétique (une couche, largeur 8, sans poids
//! réels) : le reconnaisseur, l'abandon, et chaque fichier refusé avec une
//! erreur qui le nomme (fichier tronqué, clé absente, forme inattendue, clé
//! en trop, couche en clair d'un modèle quantifié, fichiers manquants,
//! disposition de transformers, vocabulaire non géré).

use super::model::Whisper;
use super::*;
use crate::stt::mlx::parakeet::tests::serial;
use crate::tts::{self, Cancelled};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------- un Whisper synthétique ----------

/// Un tenseur F32 : nom de mlx-whisper, forme, valeurs (nulles si vides).
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

const D: usize = 8;
const VOCAB: usize = 51865;

/// Les tenseurs d'un Whisper d'une couche, largeur 8. Tout est nul, sauf la
/// norme finale du décodeur (biais 1) et la ligne de `<|endoftext|>` (1) :
/// le premier pas, où la fin est interdite, rend le jeton 0 ; le second, la
/// fin.
fn tiny_tensors() -> Vec<Tensor> {
    let mut t = vec![
        zeros("encoder.conv1.weight", &[D, 3, 80]),
        zeros("encoder.conv1.bias", &[D]),
        zeros("encoder.conv2.weight", &[D, 3, D]),
        zeros("encoder.conv2.bias", &[D]),
        zeros("encoder.ln_post.weight", &[D]),
        zeros("encoder.ln_post.bias", &[D]),
        zeros("decoder.positional_embedding", &[448, D]),
        zeros("decoder.ln.weight", &[D]),
        Tensor {
            name: "decoder.ln.bias".into(),
            shape: vec![D],
            values: vec![1.0; D],
        },
        Tensor {
            name: "decoder.token_embedding.weight".into(),
            shape: vec![VOCAB, D],
            values: (0..VOCAB * D)
                .map(|i| if i / D == 50257 { 1.0 } else { 0.0 })
                .collect(),
        },
        zeros("alignment_heads", &[2, 2]),
    ];
    for stem in ["encoder", "decoder"] {
        let p = format!("{stem}.blocks.0");
        let mut attention = |a: &str| {
            for proj in ["query", "key", "value", "out"] {
                t.push(zeros(&format!("{p}.{a}.{proj}.weight"), &[D, D]));
                if proj != "key" {
                    t.push(zeros(&format!("{p}.{a}.{proj}.bias"), &[D]));
                }
            }
            t.push(zeros(&format!("{p}.{a}_ln.weight"), &[D]));
            t.push(zeros(&format!("{p}.{a}_ln.bias"), &[D]));
        };
        attention("attn");
        if stem == "decoder" {
            attention("cross_attn");
        }
        t.push(zeros(&format!("{p}.mlp1.weight"), &[4 * D, D]));
        t.push(zeros(&format!("{p}.mlp1.bias"), &[4 * D]));
        t.push(zeros(&format!("{p}.mlp2.weight"), &[D, 4 * D]));
        t.push(zeros(&format!("{p}.mlp2.bias"), &[D]));
        t.push(zeros(&format!("{p}.mlp_ln.weight"), &[D]));
        t.push(zeros(&format!("{p}.mlp_ln.bias"), &[D]));
    }
    t
}

fn tiny_config(quantized: bool) -> serde_json::Value {
    let mut config = serde_json::json!({
        "n_mels": 80, "n_audio_ctx": 1500, "n_audio_state": D, "n_audio_head": 2,
        "n_audio_layer": 1, "n_vocab": VOCAB, "n_text_ctx": 448, "n_text_state": D,
        "n_text_head": 2, "n_text_layer": 1, "model_type": "whisper"
    });
    if quantized {
        config["quantization"] = serde_json::json!({"group_size": 64, "bits": 4});
    }
    config
}

const WEIGHTS: &str = "model.safetensors";

/// Un dossier de Whisper synthétique : `config.json`, le petit vocabulaire
/// des essais (`mini.tiktoken`) et `model.safetensors`.
fn synthetic(name: &str, config: &serde_json::Value, weights: &[u8]) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-whisper-{name}"));
    std::fs::write(dir.join(model::CONFIG_FILE), config.to_string()).unwrap();
    std::fs::copy(
        repo().join("macos/Tests/Fixtures/mini.tiktoken"),
        dir.join(model::TIKTOKEN_FILE),
    )
    .unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    dir
}

fn complete(name: &str) -> PathBuf {
    synthetic(name, &tiny_config(false), &safetensors(&tiny_tensors()))
}

fn load_error(dir: &Path) -> String {
    let _serial = serial();
    let err = WhisperMlx::new().load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None, "le besoin retiré");
    format!("{err:#}")
}

#[test]
fn a_synthetic_whisper_loads_and_transcribes() {
    let _serial = serial();
    let dir = complete("complet");
    let recognizer = WhisperMlx::new();
    recognizer.load(&dir).unwrap();
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    assert!(recognizer.memory().is_some());
    let text = recognizer
        .transcribe(&[0.1; 16_000], "fr", &Cancel::never())
        .unwrap();
    assert_eq!(text, "Hello", "jeton 0 de mini.tiktoken, puis la fin");
    // Plus de 30 s : deux fenêtres, deux textes collés.
    let long = recognizer
        .transcribe(
            &vec![0.1; audio::CHUNK_SAMPLES + 1_600],
            "en",
            &Cancel::never(),
        )
        .unwrap();
    assert_eq!(long, "Hello Hello");
    assert_eq!(
        recognizer
            .transcribe(&[0.1; MIN_SAMPLES - 1], "fr", &Cancel::never())
            .unwrap(),
        "",
        "trop court : rien, sans lancer le modèle"
    );
    recognizer.warm_up().unwrap();
    let err = recognizer
        .transcribe(&[0.1; 16_000], "xx", &Cancel::never())
        .unwrap_err()
        .to_string();
    assert!(err.contains("xx") && err.contains("Whisper"), "{err}");
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert_eq!(recognizer.memory(), None);
}

/// Un abandon demandé pendant le passage le coupe après l'invite : `Cancelled`,
/// jamais un échec.
#[test]
fn a_cancel_during_the_pass_stops_it() {
    let _serial = serial();
    let recognizer = WhisperMlx::new();
    recognizer.load(&complete("abandon")).unwrap();
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    // Avant le passage, puis sous le verrou : non ; après l'invite : oui.
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
    let err = WhisperMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::never())
        .unwrap_err();
    assert!(format!("{err}").contains("Whisper"), "{err}");
}

#[test]
fn a_cancel_before_the_pass_is_not_a_failure() {
    let err = WhisperMlx::new()
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
    let err = load_error(&synthetic("tronque", &tiny_config(false), &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn truncated_tensor_data_is_an_error() {
    let mut bytes = safetensors(&tiny_tensors());
    bytes.truncate(bytes.len() - 100);
    let err = load_error(&synthetic("donnees-coupees", &tiny_config(false), &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let tensors: Vec<Tensor> = tiny_tensors()
        .into_iter()
        .filter(|t| t.name != "decoder.blocks.0.cross_attn.value.bias")
        .collect();
    let err = load_error(&synthetic(
        "cle-absente",
        &tiny_config(false),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("model.decoder.layers.0.encoder_attn.v_proj.bias"),
        "{err}"
    );
}

#[test]
fn an_unexpected_shape_is_an_error_that_names_the_key_and_the_file() {
    let mut tensors = tiny_tensors();
    tensors[0] = zeros("encoder.conv1.weight", &[D, 3, 40]);
    let err = load_error(&synthetic(
        "forme",
        &tiny_config(false),
        &safetensors(&tensors),
    ));
    assert!(
        err.contains("model.encoder.conv1.weight") && err.contains(WEIGHTS),
        "{err}"
    );
    assert!(err.contains("[8, 3, 80]"), "{err}");
}

#[test]
fn a_key_unknown_to_the_model_is_an_error_that_names_it() {
    let mut tensors = tiny_tensors();
    tensors.push(zeros("decoder.blocks.1.mlp1.bias", &[4 * D]));
    let err = load_error(&synthetic(
        "cle-en-trop",
        &tiny_config(false),
        &safetensors(&tensors),
    ));
    assert!(err.contains("model.decoder.layers.1.fc1.bias"), "{err}");
}

#[test]
fn a_dense_layer_of_a_quantized_model_is_an_error() {
    let err = load_error(&synthetic(
        "quantifie",
        &tiny_config(true),
        &safetensors(&tiny_tensors()),
    ));
    assert!(err.contains(".scales"), "{err}");
}

#[test]
fn missing_files_are_named() {
    for file in [model::CONFIG_FILE, model::TIKTOKEN_FILE] {
        let dir = complete(&format!("sans-{file}"));
        std::fs::remove_file(dir.join(file)).unwrap();
        let err = load_error(&dir);
        assert!(err.contains(file), "{err}");
    }
    let dir = complete("sans-poids");
    std::fs::remove_file(dir.join(WEIGHTS)).unwrap();
    assert!(load_error(&dir).contains("safetensors"));
}

#[test]
fn the_transformers_layout_is_refused() {
    let tensors = [zeros("model.encoder.conv1.weight", &[D, 80, 3])];
    let err = load_error(&synthetic(
        "transformers",
        &tiny_config(false),
        &safetensors(&tensors),
    ));
    assert!(err.contains("transformers"), "{err}");
}

#[test]
fn an_unsupported_vocabulary_is_refused_before_the_weights() {
    let mut config = tiny_config(false);
    config["n_vocab"] = serde_json::json!(51864);
    let err = load_error(&synthetic("vocabulaire", &config, b"pas des poids"));
    assert!(
        err.contains("51864") && err.contains(model::CONFIG_FILE),
        "{err}"
    );
}

#[test]
fn too_few_decoder_positions_for_the_prompt_are_refused_by_name() {
    // 5 positions : l'invite (4) et pas un jeton de plus ; sans ce refus, le
    // décodeur lirait `embed_positions` hors de ses bornes.
    let mut config = tiny_config(false);
    config["n_text_ctx"] = serde_json::json!(5);
    let err = load_error(&synthetic("positions", &config, b"pas des poids"));
    assert!(
        err.contains("max_target_positions") && err.contains(model::CONFIG_FILE),
        "{err}"
    );
}

#[test]
fn a_width_too_large_is_refused_before_the_sinusoids() {
    let mut config = tiny_config(false);
    config["n_audio_state"] = serde_json::json!(65_536);
    config["n_text_state"] = serde_json::json!(65_536);
    let err = load_error(&synthetic("largeur", &config, b"pas des poids"));
    assert!(
        err.contains("d_model") && err.contains("4096") && err.contains(model::CONFIG_FILE),
        "{err}"
    );
}

#[test]
fn a_config_too_large_is_refused_without_reading_it() {
    let dir = complete("config-geante");
    let file = std::fs::File::create(dir.join(model::CONFIG_FILE)).unwrap();
    file.set_len((1 << 20) + 1).unwrap();
    let err = load_error(&dir);
    assert!(
        err.contains(model::CONFIG_FILE) && err.contains("1048577"),
        "{err}"
    );
}

#[test]
fn a_generation_config_changes_the_suppressed_tokens() {
    // Sans jetons interdits au premier pas, la fin vient tout de suite.
    let dir = complete("generation");
    std::fs::write(
        dir.join("generation_config.json"),
        r#"{"begin_suppress_tokens": [], "suppress_tokens": []}"#,
    )
    .unwrap();
    let _serial = serial();
    let held = Held::load(move || Whisper::load(&dir)).unwrap();
    let windows = held
        .with(|w| w.transcribe(&[0.1; 16_000], "fr", &|| Ok(())))
        .unwrap();
    assert_eq!(windows[0].tokens, Vec::<i64>::new());
}
