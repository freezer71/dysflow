//! Les essais de Parakeet en MLX. Ceux qui lancent le vrai modèle sont
//! ignorés par défaut : `DYSFLOW_MODELS` désigne un dossier qui contient
//! `parakeet_tdt_v3/` et `parakeet_tdt_v2/`, et ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh` :
//!
//!   DYSFLOW_MODELS=… ./scripts/mlx-parite.sh
//!
//! Le Swift calcule avec les noyaux Metal de mlx-swift : JIT, plus le
//! `default.metallib` qu'Xcode compile. Le build livré du Rust prend le
//! mlx.metallib précompilé de mlx-sys (CMake), même commit de MLX, dont la
//! Conv2d par canal et `layer_norm` arrondissent autrement d'une ulp
//! `bfloat16` : l'encodeur s'écarte de ~1e-6 en moyenne et le décodage
//! glouton finit par changer de jeton (rapport de la tâche 14). Avec les noyaux du Swift, le Rust est
//! identique à l'octet, mel et encodeur compris, et chaque pas, jeton et
//! texte est le sien (`conformer::stages` le vérifie étage par étage). La
//! qualité du build livré se juge au banc (P5), pas ici.

use super::model::{stats, Parakeet, Stop, TdtTrace};
use super::*;
use crate::tts::{self, Cancelled};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Les essais qui chargent posent et retirent le besoin `"dictee"` dans les
/// plafonds globaux de MLX : un à la fois.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Le dossier de `id` sous `DYSFLOW_MODELS` ; sans lui, l'essai échoue.
fn model_dir(id: &str) -> PathBuf {
    let models =
        std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS : le dossier des modèles");
    let dir = PathBuf::from(models).join(id);
    assert!(
        dir.join("config.json").is_file(),
        "pas de {id} dans {}",
        dir.display()
    );
    dir
}

// ---------- la fixture du Swift ----------

#[derive(serde::Deserialize)]
struct Stats {
    shape: Vec<i32>,
    mean: f32,
    mean_abs: f32,
}

#[derive(serde::Deserialize)]
struct Input {
    name: String,
    samples: usize,
    sha256: String,
}

#[derive(serde::Deserialize)]
struct Case {
    input: String,
    mel: Stats,
    encoder: Stats,
    encoded_length: i32,
    steps: Vec<[usize; 3]>,
    tokens: Vec<usize>,
    text: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    compute_dtype: String,
    quantization: String,
    inputs: Vec<Input>,
    cases: Vec<Case>,
}

fn fixture(id: &str) -> Fixture {
    let path = repo().join(format!("tests/fixtures/mlx/{id}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// `clips` à la suite, recommencés, jusqu'à `count` échantillons : la même
/// construction que le générateur Swift (`cycled`).
fn cycled(clips: &[&[f32]], count: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        for clip in clips {
            let room = count - out.len();
            out.extend_from_slice(&clip[..clip.len().min(room)]);
        }
    }
    out
}

/// Les cinq entrées de la tâche, construites comme la fixture.
fn inputs() -> Vec<(&'static str, Vec<f32>)> {
    let wav = |path: &str| crate::stt::file::read_wav(&repo().join(path)).unwrap();
    let fr = wav("macos/Tests/Fixtures/dictee-charge.wav");
    let en = wav("tests/fixtures/mlx/dictee-en.wav");
    let twelve = cycled(&[&fr, &en], 192_000);
    let tenth = fr[16_000..17_600].to_vec();
    vec![
        ("francais", fr),
        ("anglais", en),
        ("douze_secondes", twelve),
        ("silence", vec![0.0; 16_000]),
        ("dixieme", tenth),
    ]
}

fn sha256(samples: &[f32]) -> String {
    let mut hasher = Sha256::new();
    for s in samples {
        hasher.update(s.to_le_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Ce que le Rust rend pour une entrée, copié hors du fil MLX.
struct Measured {
    mel: (Vec<i32>, f32, f32),
    encoder: (Vec<i32>, f32, f32),
    encoded_length: i32,
    steps: Vec<[usize; 3]>,
    tokens: Vec<usize>,
    text: String,
}

/// Les étages comme le générateur : mel, encodeur sur le mel au type de
/// calcul, le décodage tracé, puis le texte par `generate`.
fn measure(parakeet: &mut Parakeet, samples: &[f32]) -> anyhow::Result<Measured> {
    let never = Cancel::never();
    let stop = Stop(&never);
    let mel = parakeet.mel(samples)?;
    mel.eval()?;
    let mel_stats = stats(&mel)?;
    let (encoded, lengths) = parakeet.encode(&mel)?;
    let encoder = stats(&encoded)?;
    let encoded_length = lengths.try_as_slice::<i32>()?[0];
    let mut steps = Vec::new();
    let result = parakeet.decode(&mel, &stop, &mut |s: TdtTrace| {
        steps.push([s.time, s.token, s.decision])
    })?;
    let tokens = result.tokens().map(|t| t.id).collect();
    let text = parakeet.generate(samples, &stop)?;
    Ok(Measured {
        mel: mel_stats,
        encoder,
        encoded_length,
        steps,
        tokens,
        text,
    })
}

/// Les seuils, fixés avant toute mesure : mel en `f32` (mêmes opérations,
/// même MLX), sortie de l'encodeur en `bfloat16`. Jetons, pas du décodeur
/// et texte : à l'identique.
const MEL_TOLERANCE: f32 = 1e-5;
const ENCODER_MEAN_TOLERANCE: f32 = 1e-5;
const ENCODER_MEAN_ABS_TOLERANCE: f32 = 1e-4;

/// Ce que le Rust rend pour chaque entrée de la fixture `id`, après les
/// vérifications communes : modèle, révision, type de calcul,
/// quantification, et entrées identiques à celles du Swift (SHA-256).
/// Chaque mesure est imprimée avant toute assertion.
fn run_fixture(id: &str) -> (Fixture, Vec<Measured>) {
    let fixture = fixture(id);
    assert_eq!(fixture.model, id);
    let package = crate::models::CATALOG
        .iter()
        .find(|p| p.id == id)
        .expect("paquet du catalogue");
    assert_eq!(
        fixture.revision, package.revision,
        "la fixture n'est pas de la révision du catalogue"
    );
    assert_eq!(fixture.compute_dtype, "bfloat16");
    let dir = model_dir(id);
    let config = config::Config::load(&dir).unwrap();
    let quantization = match config.quantization {
        Some(q) => format!("bits {}, group_size {}", q.bits, q.group_size),
        None => "aucune".to_string(),
    };
    assert_eq!(fixture.quantization, quantization);

    let inputs = inputs();
    assert_eq!(inputs.len(), fixture.inputs.len());
    for ((name, samples), input) in inputs.iter().zip(&fixture.inputs) {
        assert_eq!((*name, samples.len()), (input.name.as_str(), input.samples));
        assert_eq!(
            sha256(samples),
            input.sha256,
            "entrée {name} différente de celle du Swift"
        );
    }

    let _serial = serial();
    let held = Held::load(move || Parakeet::load(&dir)).unwrap();
    let all: Vec<Vec<f32>> = inputs.iter().map(|(_, s)| s.clone()).collect();
    let measured = held
        .with(move |parakeet| {
            all.iter()
                .map(|samples| measure(parakeet, samples))
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();

    assert_eq!(measured.len(), fixture.cases.len());
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        eprintln!(
            "{id} {name} : mel Δmoyenne {:.3e} Δ|moyenne| {:.3e} ; encodeur Δmoyenne {:.3e} Δ|moyenne| {:.3e} ; {} pas contre {}, premier écart au pas {:?} ; {} jetons ; {:?}",
            (got.mel.1 - case.mel.mean).abs(),
            (got.mel.2 - case.mel.mean_abs).abs(),
            (got.encoder.1 - case.encoder.mean).abs(),
            (got.encoder.2 - case.encoder.mean_abs).abs(),
            got.steps.len(),
            case.steps.len(),
            first_step_difference(&got.steps, &case.steps),
            got.tokens.len(),
            got.text,
        );
    }
    (fixture, measured)
}

/// Le premier pas qui diffère, ou où l'une des deux suites s'arrête avant
/// l'autre ; `None` si elles sont identiques.
fn first_step_difference(got: &[[usize; 3]], expected: &[[usize; 3]]) -> Option<usize> {
    got.iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .or_else(|| (got.len() != expected.len()).then(|| got.len().min(expected.len())))
}

/// P2 : chaque entrée, étage par étage, du premier au dernier (l'échec nomme
/// le premier qui diverge) ; pas, jetons et texte à l'identique.
fn parity(id: &str) {
    let (fixture, measured) = run_fixture(id);
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.mel.0, case.mel.shape, "{name} : forme du mel");
        assert!(
            (got.mel.1 - case.mel.mean).abs() <= MEL_TOLERANCE,
            "{name} : moyenne du mel"
        );
        assert!(
            (got.mel.2 - case.mel.mean_abs).abs() <= MEL_TOLERANCE,
            "{name} : moyenne absolue du mel"
        );
        assert_eq!(
            got.encoder.0, case.encoder.shape,
            "{name} : forme de l'encodeur"
        );
        assert_eq!(
            got.encoded_length, case.encoded_length,
            "{name} : longueur encodée"
        );
        assert!(
            (got.encoder.1 - case.encoder.mean).abs() <= ENCODER_MEAN_TOLERANCE,
            "{name} : moyenne de l'encodeur"
        );
        assert!(
            (got.encoder.2 - case.encoder.mean_abs).abs() <= ENCODER_MEAN_ABS_TOLERANCE,
            "{name} : moyenne absolue de l'encodeur"
        );
        let first = first_step_difference(&got.steps, &case.steps);
        assert!(
            first.is_none(),
            "{name} : pas du décodeur, premier écart au pas {first:?} ({} contre {})",
            got.steps.len(),
            case.steps.len()
        );
        assert_eq!(got.tokens, case.tokens, "{name} : jetons");
        assert_eq!(got.text, case.text, "{name} : texte");
    }
}

/// La garde de fumée du build livré : pour chaque entrée, le mel identique
/// à l'octet (forme, moyenne, moyenne absolue), un nombre de pas à 10 % près
/// de la fixture (arrondi vers le bas : une entrée de moins de 10 pas doit
/// en avoir exactement autant), et le texte de la fixture pour les entrées
/// de `text_on`. L'encodeur n'y est pas comparé : c'est l'affaire de la
/// parité.
fn shipped_smoke(id: &str, text_on: &[&str]) {
    let (fixture, measured) = run_fixture(id);
    for name in text_on {
        assert!(
            fixture.cases.iter().any(|c| c.input == *name),
            "{name} : pas une entrée de la fixture"
        );
    }
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.mel.0, case.mel.shape, "{name} : forme du mel");
        assert_eq!(
            (got.mel.1.to_bits(), got.mel.2.to_bits()),
            (case.mel.mean.to_bits(), case.mel.mean_abs.to_bits()),
            "{name} : mel (moyenne, moyenne absolue) différent à l'octet"
        );
        let bound = case.steps.len() / 10;
        assert!(
            got.steps.len().abs_diff(case.steps.len()) <= bound,
            "{name} : {} pas contre {}, au-delà de la borne de {bound} (premier écart au pas {:?})",
            got.steps.len(),
            case.steps.len(),
            first_step_difference(&got.steps, &case.steps)
        );
        if text_on.contains(&name.as_str()) {
            assert_eq!(got.text, case.text, "{name} : texte");
        }
    }
}

/// P2 : la parité avec la fixture du Swift, étage par étage, dans le build
/// de `scripts/mlx-parite.sh` (noyaux du Swift ; voir l'en-tête).
mod parity {
    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn parakeet_tdt_v3() {
        super::parity("parakeet_tdt_v3");
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn parakeet_tdt_v2() {
        super::parity("parakeet_tdt_v2");
    }
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT (`parity`), à l'octet.
///
/// Les noyaux `depthwise_conv_2d` et `layer_norm` du build livré arrondissent
/// autrement d'une ulp `bfloat16` que ceux du Swift (R54, R54b ; en-tête) :
/// l'encodeur s'écarte un peu, et le décodage glouton peut changer une durée
/// ou un jeton. Sur les mêmes entrées, la parité JIT reste à l'octet. La
/// garde attrape donc une régression grossière (un noyau, une montée de
/// mlx-sys, `nn.rs`), pas un écart d'arrondi. Pour chaque entrée :
/// - le mel identique à l'octet (forme, moyenne, moyenne absolue) ;
/// - le nombre de pas à 10 % près de la fixture, arrondi vers le bas : une
///   entrée de moins de 10 pas doit en avoir exactement autant ;
/// - le texte de la fixture, pour v3 sur toutes les entrées, pour v2 (anglais
///   seulement) sur les entrées sans français.
///
/// L'encodeur n'y est pas comparé : il appartient à la parité.
///
/// Mesuré le 23 nov. 2026 (build livré, entrées de make_dictees.sh) :
/// - v3 : francais 29 pas contre 28 (premier écart au pas 3), anglais 26
///   contre 26 (pas 13), douze_secondes 83 contre 83 (pas 69), silence 13
///   et dixieme 2, identiques ; texte de la fixture sur les cinq entrées ;
/// - v2 : francais 53 contre 54 (pas 25 ; « fin » au lieu de « fan »),
///   anglais 25, douze_secondes 134, silence 13 et dixieme 3, pas
///   identiques ; texte de la fixture sur anglais et silence, et aussi,
///   sans être vérifié, sur douze_secondes et dixieme.
mod shipped_smoke {
    /// v3 : le texte de la fixture partout.
    const V3_TEXT: &[&str] = &[
        "francais",
        "anglais",
        "douze_secondes",
        "silence",
        "dixieme",
    ];
    /// v2 ne connaît que l'anglais : sur le français (francais, douze_secondes,
    /// dixieme), seul le nombre de pas est borné.
    const V2_TEXT: &[&str] = &["anglais", "silence"];

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn parakeet_tdt_v3() {
        super::shipped_smoke("parakeet_tdt_v3", V3_TEXT);
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn parakeet_tdt_v2() {
        super::shipped_smoke("parakeet_tdt_v2", V2_TEXT);
    }
}

// ---------- fumée (ParakeetSmokeTests.swift) ----------

fn loaded(id: &str) -> ParakeetMlx {
    let recognizer = ParakeetMlx::new();
    recognizer.load(&model_dir(id)).unwrap();
    recognizer
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn transcribes_one_second_of_silence_without_crashing() {
    let _serial = serial();
    let recognizer = loaded("parakeet_tdt_v3");
    let text = recognizer
        .transcribe(&[0.0; 16_000], "fr", &Cancel::never())
        .unwrap();
    assert_eq!(text, "");
    recognizer.release();
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_english_v2_loads_and_transcribes_one_second_of_silence() {
    let _serial = serial();
    let recognizer = loaded("parakeet_tdt_v2");
    let text = recognizer
        .transcribe(&[0.0; 16_000], "en", &Cancel::never())
        .unwrap();
    assert_eq!(text, "");
    recognizer.release();
}

// ---------- le reconnaisseur, sur le vrai modèle ----------

/// Le besoin `"dictee"` est posé au chargement et retiré à la libération ;
/// la mémoire est dite tant que le modèle est là.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_need_is_set_while_loaded() {
    let _serial = serial();
    let dir = model_dir("parakeet_tdt_v3");
    let recognizer = loaded("parakeet_tdt_v3");
    assert_eq!(budget::of(BUDGET_KEY), Some(need(&dir)));
    assert!(recognizer.memory().is_some_and(|bytes| bytes > 500_000_000));
    recognizer.warm_up().unwrap();
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
    assert_eq!(recognizer.memory(), None);
}

/// Un abandon demandé pendant le passage le coupe après l'encodeur :
/// `Cancelled`, jamais un échec.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn a_cancel_during_the_pass_stops_it() {
    let _serial = serial();
    let recognizer = loaded("parakeet_tdt_v3");
    let samples = inputs().swap_remove(2).1;
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    // Avant le passage, puis sous le verrou : non ; après l'encodeur : oui.
    let cancel = Cancel::when(move || counter.fetch_add(1, Ordering::SeqCst) >= 2);
    let err = recognizer.transcribe(&samples, "fr", &cancel).unwrap_err();
    assert!(tts::is_cancelled(&err), "{err:#}");
    assert!(err.downcast_ref::<Cancelled>().is_some());
    assert_eq!(
        asked.load(Ordering::SeqCst),
        3,
        "arrêté après l'encodeur, au premier point du passage"
    );
    recognizer.release();
}

// ---------- sans modèle ----------

#[test]
fn a_transcription_without_a_model_is_an_error() {
    let err = ParakeetMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::never())
        .unwrap_err();
    assert!(format!("{err}").contains("Parakeet"), "{err}");
}

#[test]
fn a_cancel_before_the_pass_is_not_a_failure() {
    let err = ParakeetMlx::new()
        .transcribe(&[0.0; 16_000], "fr", &Cancel::when(|| true))
        .unwrap_err();
    assert!(tts::is_cancelled(&err), "{err:#}");
}

/// Le `config.json` de v3, réduit : un encodeur d'une couche, petit.
fn tiny_config(quantization: bool) -> serde_json::Value {
    let mut config = serde_json::json!({
        "target": "nemo.collections.asr.models.rnnt_bpe_models.EncDecRNNTBPEModel",
        "model_defaults": {"tdt_durations": [0, 1, 2, 3, 4]},
        "preprocessor": {
            "sample_rate": 16000, "normalize": "per_feature", "window_size": 0.025, "window_stride": 0.01,
            "window": "hann", "features": 128, "n_fft": 512, "dither": 1e-05, "pad_to": 0, "pad_value": 0.0
        },
        "encoder": {
            "feat_in": 128, "n_layers": 1, "d_model": 64, "n_heads": 2, "ff_expansion_factor": 4,
            "subsampling_factor": 8, "self_attention_model": "rel_pos", "subsampling": "dw_striding",
            "conv_kernel_size": 9, "subsampling_conv_channels": 8, "pos_emb_max_len": 5000,
            "use_bias": false, "xscaling": false, "feat_out": null
        },
        "decoder": {"blank_as_pad": true, "vocab_size": 4, "prednet": {"pred_hidden": 16, "pred_rnn_layers": 1}},
        "joint": {
            "num_classes": 4, "vocabulary": ["<unk>", "▁a", "b", "."], "num_extra_outputs": 5,
            "jointnet": {"joint_hidden": 16, "activation": "relu", "encoder_hidden": 64, "pred_hidden": 16}
        },
        "decoding": {"model_type": "tdt", "durations": [0, 1, 2, 3, 4], "greedy": {"max_symbols": 10}}
    });
    if quantization {
        config["quantization"] = serde_json::json!({"group_size": 64, "bits": 8});
    }
    config
}

/// Un safetensors de tenseurs F32 nuls.
fn safetensors(tensors: &[(&str, &[usize])]) -> Vec<u8> {
    let (mut header, mut offset) = (serde_json::Map::new(), 0usize);
    for (name, shape) in tensors {
        let bytes = shape.iter().product::<usize>() * 4;
        header.insert(
            name.to_string(),
            serde_json::json!({"dtype": "F32", "shape": shape, "data_offsets": [offset, offset + bytes]}),
        );
        offset += bytes;
    }
    let mut json = serde_json::Value::Object(header).to_string();
    while !json.len().is_multiple_of(8) {
        json.push(' ');
    }
    let mut file = (json.len() as u64).to_le_bytes().to_vec();
    file.extend_from_slice(json.as_bytes());
    file.resize(file.len() + offset, 0);
    file
}

/// Le nom du fichier de poids des essais synthétiques.
const WEIGHTS: &str = "model.safetensors";

/// Un dossier de modèle synthétique : `config.json` (réduit, `-Infinity`
/// compris, comme les exports NeMo) et `model.safetensors`.
fn synthetic(name: &str, weights: &[u8]) -> PathBuf {
    let dir = crate::paths::test_scratch(&format!("stt-mlx-parakeet-{name}"));
    let config = tiny_config(false).to_string().replace("null", "-Infinity");
    std::fs::write(dir.join(config::CONFIG_FILE), config).unwrap();
    std::fs::write(dir.join(WEIGHTS), weights).unwrap();
    dir
}

fn load_error(dir: &Path) -> String {
    let _serial = serial();
    let recognizer = ParakeetMlx::new();
    let err = recognizer.load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None);
    format!("{err:#}")
}

#[test]
fn truncated_weights_are_an_error() {
    // Un en-tête annoncé de 1 000 octets, et 8 octets seulement.
    let mut bytes = 1000u64.to_le_bytes().to_vec();
    bytes.extend_from_slice(b"{\"a\":{\"d");
    let err = load_error(&synthetic("tronque", &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn truncated_tensor_data_is_an_error() {
    let mut bytes = safetensors(&[("encoder.pre_encode.conv.0.weight", &[8, 3, 3, 1])]);
    bytes.truncate(bytes.len() - 100);
    let err = load_error(&synthetic("donnees-coupees", &bytes));
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let err = load_error(&synthetic(
        "cle-absente",
        &safetensors(&[("autre.chose", &[4])]),
    ));
    assert!(err.contains("encoder.pre_encode.conv0.weight"), "{err}");
}

#[test]
fn a_wrong_shape_is_an_error_that_names_the_key() {
    // Renommée par `remap_key` : l'erreur nomme la clé du graphe.
    let bytes = safetensors(&[("encoder.pre_encode.conv.0.weight", &[8, 3, 3, 2])]);
    let err = load_error(&synthetic("mauvaise-forme", &bytes));
    assert!(err.contains("encoder.pre_encode.conv0.weight"), "{err}");
    assert!(
        err.contains("[8, 3, 3, 2]") && err.contains("[8, 3, 3, 1]"),
        "{err}"
    );
    assert!(err.contains(WEIGHTS), "{err}");
}

#[test]
fn an_unreadable_config_is_an_error_that_names_it() {
    let dir = synthetic("config-illisible", &safetensors(&[]));
    std::fs::write(dir.join(config::CONFIG_FILE), "{").unwrap();
    let err = load_error(&dir);
    assert!(err.contains("config.json"), "{err}");
}

#[test]
fn a_variant_that_is_not_tdt_is_refused() {
    let mut config = tiny_config(false);
    config["model_defaults"] = serde_json::json!({});
    let err = format!(
        "{:#}",
        config::Config::parse(&config.to_string()).unwrap_err()
    );
    assert!(err.contains("TDT"), "{err}");
}

/// Une durée 0 sans `max_symbols` : `tdt_step` pourrait boucler sans fin
/// sur une même trame, sous le verrou du GPU.
#[test]
fn a_zero_duration_without_max_symbols_is_refused() {
    let mut config = tiny_config(false);
    config["decoding"]["greedy"] = serde_json::json!({});
    let err = format!(
        "{:#}",
        config::Config::parse(&config.to_string()).unwrap_err()
    );
    assert!(err.contains("max_symbols"), "{err}");
    config["decoding"].as_object_mut().unwrap().remove("greedy");
    let err = format!(
        "{:#}",
        config::Config::parse(&config.to_string()).unwrap_err()
    );
    assert!(err.contains("max_symbols"), "{err}");
    // Sans durée 0, chaque pas avance : max_symbols n'est pas demandé.
    config["decoding"]["durations"] = serde_json::json!([1, 2, 3, 4, 5]);
    config["model_defaults"]["tdt_durations"] = serde_json::json!([1, 2, 3, 4, 5]);
    config::Config::parse(&config.to_string()).unwrap();
}

/// Sans `blank_as_pad`, l'identifiant du blanc (`vocab_size`) serait lu
/// hors de la table d'embedding de la prédiction.
#[test]
fn a_decoder_without_blank_as_pad_is_refused() {
    let mut config = tiny_config(false);
    config["decoder"]["blank_as_pad"] = serde_json::json!(false);
    let err = format!(
        "{:#}",
        config::Config::parse(&config.to_string()).unwrap_err()
    );
    assert!(err.contains("blank_as_pad"), "{err}");
}

#[test]
fn the_quantization_is_read_from_the_config() {
    let plain = config::Config::parse(&tiny_config(false).to_string()).unwrap();
    assert_eq!(plain.quantization, None);
    let quantized = config::Config::parse(&tiny_config(true).to_string()).unwrap();
    assert_eq!(
        quantized.quantization,
        Some(crate::stt::mlx::weights::Quantization {
            group_size: 64,
            bits: 8
        })
    );
    assert_eq!(
        (
            quantized.preprocessor.win_length(),
            quantized.preprocessor.hop_length()
        ),
        (400, 160)
    );
    assert_eq!(quantized.blank(), 4);
}

/// Un dossier complet de poids nuls, au graphe de `tiny_config` : il se
/// charge, et une clé en trop est refusée avec son nom.
#[test]
fn a_complete_synthetic_model_loads_and_an_extra_key_is_refused() {
    let keys = tiny_keys();
    let tensors: Vec<(&str, &[usize])> = keys
        .iter()
        .map(|(k, s)| (k.as_str(), s.as_slice()))
        .collect();
    let dir = synthetic("complet", &safetensors(&tensors));
    {
        let _serial = serial();
        let recognizer = ParakeetMlx::new();
        recognizer.load(&dir).unwrap();
        let text = recognizer
            .transcribe(&[0.0; 4_000], "fr", &Cancel::never())
            .unwrap();
        assert!(text.len() < 1_000);
        assert_eq!(
            recognizer
                .transcribe(&[0.0; MIN_SAMPLES - 1], "fr", &Cancel::never())
                .unwrap(),
            ""
        );
        recognizer.release();
        assert_eq!(budget::of(BUDGET_KEY), None);
    }
    let mut tensors = tensors;
    tensors.push(("encoder.de.trop", &[2]));
    let err = load_error(&synthetic("cle-en-trop", &safetensors(&tensors)));
    assert!(err.contains("encoder.de.trop"), "{err}");
}

/// Les clés du fichier (avant `remap_key`) et leurs formes pour
/// `tiny_config`.
fn tiny_keys() -> Vec<(String, Vec<usize>)> {
    let (d, ff, c, k) = (64usize, 256usize, 8usize, 9usize);
    let mut keys: Vec<(String, Vec<usize>)> = vec![
        ("encoder.pre_encode.conv.0.weight".into(), vec![c, 3, 3, 1]),
        ("encoder.pre_encode.conv.0.bias".into(), vec![c]),
        ("encoder.pre_encode.out.weight".into(), vec![d, c * 16]),
        ("encoder.pre_encode.out.bias".into(), vec![d]),
    ];
    for (depthwise, pointwise) in [(2, 3), (5, 6)] {
        keys.push((
            format!("encoder.pre_encode.conv.{depthwise}.weight"),
            vec![c, 3, 3, 1],
        ));
        keys.push((format!("encoder.pre_encode.conv.{depthwise}.bias"), vec![c]));
        keys.push((
            format!("encoder.pre_encode.conv.{pointwise}.weight"),
            vec![c, 1, 1, c],
        ));
        keys.push((format!("encoder.pre_encode.conv.{pointwise}.bias"), vec![c]));
    }
    let l = "encoder.layers.0";
    for norm in [
        "norm_feed_forward1",
        "norm_self_att",
        "norm_conv",
        "norm_feed_forward2",
        "norm_out",
    ] {
        keys.push((format!("{l}.{norm}.weight"), vec![d]));
        keys.push((format!("{l}.{norm}.bias"), vec![d]));
    }
    for ffn in ["feed_forward1", "feed_forward2"] {
        keys.push((format!("{l}.{ffn}.linear1.weight"), vec![ff, d]));
        keys.push((format!("{l}.{ffn}.linear2.weight"), vec![d, ff]));
    }
    for proj in [
        "linear_q",
        "linear_k",
        "linear_v",
        "linear_out",
        "linear_pos",
    ] {
        keys.push((format!("{l}.self_attn.{proj}.weight"), vec![d, d]));
    }
    keys.push((format!("{l}.self_attn.pos_bias_u"), vec![2, d / 2]));
    keys.push((format!("{l}.self_attn.pos_bias_v"), vec![2, d / 2]));
    keys.push((
        format!("{l}.conv.pointwise_conv1.weight"),
        vec![2 * d, 1, d],
    ));
    keys.push((format!("{l}.conv.depthwise_conv.weight"), vec![d, k, 1]));
    keys.push((format!("{l}.conv.pointwise_conv2.weight"), vec![d, 1, d]));
    for stat in ["weight", "bias", "running_mean", "running_var"] {
        keys.push((format!("{l}.conv.batch_norm.{stat}"), vec![d]));
    }
    let (p, vocab, classes) = (16usize, 4usize, 4 + 1 + 5);
    keys.push(("decoder.prediction.embed.weight".into(), vec![vocab + 1, p]));
    keys.push((
        "decoder.prediction.dec_rnn.lstm.0.Wx".into(),
        vec![4 * p, p],
    ));
    keys.push((
        "decoder.prediction.dec_rnn.lstm.0.Wh".into(),
        vec![4 * p, p],
    ));
    keys.push(("decoder.prediction.dec_rnn.lstm.0.bias".into(), vec![4 * p]));
    keys.push(("joint.pred.weight".into(), vec![p, p]));
    keys.push(("joint.pred.bias".into(), vec![p]));
    keys.push(("joint.enc.weight".into(), vec![p, d]));
    keys.push(("joint.enc.bias".into(), vec![p]));
    keys.push(("joint.joint_net.2.weight".into(), vec![classes, p]));
    keys.push(("joint.joint_net.2.bias".into(), vec![classes]));
    keys
}
