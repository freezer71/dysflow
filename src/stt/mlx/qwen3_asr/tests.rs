//! Les essais de Qwen3-ASR en MLX sur les vrais modèles, ignorés par défaut :
//! `DYSFLOW_MODELS` désigne un dossier qui contient `qwen3_asr_06b_8bit/` et
//! `qwen3_asr_17b_4bit/`, et ils échouent sans lui. Un essai par modèle :
//! lancés à la suite (`--test-threads=1`), un seul modèle est en mémoire.
//!
//! La parité (`parity_*`) ne vaut que dans le build de
//! `scripts/mlx-parite.sh` (les noyaux Metal du Swift, voir l'en-tête du
//! script et celui des essais de Parakeet) : mel et sortie de l'encodeur
//! identiques à l'octet (forme, moyenne, moyenne absolue), jetons audio,
//! invite, jetons et texte, plus les invites de 13 jetons audio en
//! français, en anglais et sans langue. Le build livré a sa garde,
//! `shipped_smoke_*` (`--livre`).

use super::audio::split_into_chunks;
use super::model::{Qwen3Asr, CHUNK_DURATION, MAX_TOKENS, MIN_CHUNK_DURATION};
use super::*;
use crate::stt::mlx::parakeet::model::stats;
use crate::stt::mlx::parakeet::tests::{
    cycled, inputs as parakeet_inputs, model_dir_with, serial, sha256,
};
use crate::stt::mlx::shipped;
use std::path::PathBuf;

const SMALL: &str = "qwen3_asr_06b_8bit";
const LARGE: &str = "qwen3_asr_17b_4bit";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Le dossier du modèle sous `DYSFLOW_MODELS` ; sans lui, l'essai échoue.
fn model_dir(id: &str) -> PathBuf {
    model_dir_with(id, config::CONFIG_FILE)
}

fn load(id: &str) -> Held<Qwen3Asr> {
    let dir = model_dir(id);
    Held::load(move || Qwen3Asr::load(&dir)).unwrap()
}

/// `WhisperFixtureTests.language` : `en` pour l'anglais, `fr` pour le reste.
fn language(input: &str) -> &'static str {
    if input == "anglais" {
        "en"
    } else {
        "fr"
    }
}

/// Les entrées de Whisper (`WhisperFixtureTests.inputs`) : celles de
/// Parakeet, plus `trente_cinq_secondes`.
fn inputs() -> Vec<(&'static str, Vec<f32>)> {
    let mut inputs = parakeet_inputs();
    let long = cycled(&[&inputs[0].1, &inputs[1].1], 560_000);
    inputs.push(("trente_cinq_secondes", long));
    inputs
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
    language: String,
    mel: Stats,
    encoder: Stats,
    audio_tokens: usize,
    prompt: Vec<u32>,
    tokens: Vec<u32>,
    text: String,
}

#[derive(serde::Deserialize, Debug, PartialEq)]
struct Prompts {
    fr: Vec<u32>,
    en: Vec<u32>,
    none: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    encoder_dtype: String,
    quantization: String,
    max_tokens: usize,
    prompts: Prompts,
    inputs: Vec<Input>,
    cases: Vec<Case>,
}

type Stat = (Vec<i32>, f32, f32);

/// Ce que le Rust rend pour une entrée, copié hors du fil MLX.
struct Measured {
    mel: Stat,
    encoder: Stat,
    encoder_dtype: String,
    audio_tokens: usize,
    prompt: Vec<u32>,
    tokens: Vec<u32>,
    text: String,
}

/// Comme le générateur : la tranche que `generate` lit, son mel et
/// l'encodeur, puis `generate` (le chemin de l'app), le texte sans espaces
/// autour.
fn measure(asr: &Qwen3Asr, samples: &[f32], language: &str) -> anyhow::Result<Measured> {
    let chunks = split_into_chunks(
        samples,
        audio::SAMPLE_RATE,
        CHUNK_DURATION,
        MIN_CHUNK_DURATION,
    );
    anyhow::ensure!(chunks.len() == 1, "{} tranches", chunks.len());
    let (features, audio_tokens) = asr.features(&chunks[0].samples)?;
    features.eval()?;
    let mel = stats(&features)?;
    let encoded = asr.encode(&features)?;
    let encoder = stats(&encoded)?;
    let encoder_dtype = format!("{:?}", encoded.dtype()).to_lowercase();
    let transcript = asr.generate(samples, Some(language), MAX_TOKENS, &|| Ok(()))?;
    anyhow::ensure!(
        transcript.chunks.len() == 1,
        "{} traces",
        transcript.chunks.len()
    );
    let chunk = transcript.chunks[0].clone();
    Ok(Measured {
        mel,
        encoder,
        encoder_dtype,
        audio_tokens,
        prompt: chunk.prompt,
        tokens: chunk.tokens,
        text: transcript.text.trim().to_string(),
    })
}

fn bits(s: &Stat) -> (Vec<i32>, u32, u32) {
    (s.0.clone(), s.1.to_bits(), s.2.to_bits())
}

fn expected_bits(s: &Stats) -> (Vec<i32>, u32, u32) {
    (s.shape.clone(), s.mean.to_bits(), s.mean_abs.to_bits())
}

/// `ParakeetFixtureTests.quantization` : `bits` et `group_size` tels
/// qu'écrits dans `config.json` (« ? » s'il manque).
fn quantization_text(dir: &std::path::Path) -> String {
    let text = std::fs::read_to_string(dir.join(config::CONFIG_FILE)).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    match json.get("quantization").and_then(|q| q.as_object()) {
        None => "aucune".into(),
        Some(q) => {
            let field = |k: &str| q.get(k).map_or("?".to_string(), |v| v.to_string());
            format!("bits {}, group_size {}", field("bits"), field("group_size"))
        }
    }
}

/// La fixture de `id`, ce que le Rust rend pour chacune de ses entrées et
/// ses invites de 13 jetons audio, après les vérifications communes (modèle,
/// révision, quantification, plafond, entrées identiques à celles du
/// Swift). Chaque mesure est imprimée avant toute assertion.
fn run_fixture(id: &'static str) -> (Fixture, Vec<Measured>, Prompts) {
    let path = repo().join(format!("tests/fixtures/mlx/{id}.json"));
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(fixture.model, id);
    let package = crate::models::CATALOG
        .iter()
        .find(|p| p.id == id)
        .expect("paquet du catalogue");
    assert_eq!(fixture.revision, package.revision, "révision du catalogue");
    assert_eq!(fixture.max_tokens, MAX_TOKENS);
    let dir = model_dir(id);
    assert_eq!(fixture.quantization, quantization_text(&dir));

    let inputs = inputs();
    assert_eq!(inputs.len(), fixture.inputs.len());
    for ((name, samples), input) in inputs.iter().zip(&fixture.inputs) {
        assert_eq!((*name, samples.len()), (input.name.as_str(), input.samples));
        assert_eq!(
            sha256(samples),
            input.sha256,
            "entrée {name} différente du Swift"
        );
    }
    assert_eq!(fixture.cases.len(), inputs.len());

    let _serial = serial();
    let held = load(id);
    let jobs: Vec<(Vec<f32>, String)> = inputs
        .iter()
        .zip(&fixture.cases)
        .map(|((name, samples), case)| {
            assert_eq!(case.language, language(name), "{name} : langue");
            (samples.clone(), case.language.clone())
        })
        .collect();
    let (measured, prompts) = held
        .with(move |asr| {
            let prompts = Prompts {
                fr: asr.prompt(13, Some("fr"))?,
                en: asr.prompt(13, Some("en"))?,
                none: asr.prompt(13, None)?,
            };
            let measured = jobs
                .iter()
                .map(|(samples, language)| measure(asr, samples, language))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok((measured, prompts))
        })
        .unwrap();
    drop(held);
    for (case, got) in fixture.cases.iter().zip(&measured) {
        eprintln!(
            "{id} {} ({}) : mel {} ; encodeur {} {} Δ(moyenne, |moyenne|) ({:e}, {:e}) ; {} jetons audio contre {} ; invite {} ; {} jetons contre {} ({}) ; {:?}",
            case.input,
            case.language,
            if bits(&got.mel) == expected_bits(&case.mel) { "identique" } else { "DIFFÉRENT" },
            got.encoder_dtype,
            if bits(&got.encoder) == expected_bits(&case.encoder) { "identique" } else { "DIFFÉRENT" },
            (got.encoder.1 - case.encoder.mean).abs(),
            (got.encoder.2 - case.encoder.mean_abs).abs(),
            got.audio_tokens,
            case.audio_tokens,
            if got.prompt == case.prompt { "identique" } else { "DIFFÉRENTE" },
            got.tokens.len(),
            case.tokens.len(),
            if got.tokens == case.tokens { "identiques" } else { "DIFFÉRENTS" },
            got.text,
        );
    }
    (fixture, measured, prompts)
}

/// P2 : chaque entrée, du mel au texte, à l'identique ; les invites aussi.
fn parity(id: &'static str) {
    let (fixture, measured, prompts) = run_fixture(id);
    assert_eq!(prompts, fixture.prompts, "invites de 13 jetons audio");
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.encoder_dtype, fixture.encoder_dtype, "{name} : type");
        assert_eq!(bits(&got.mel), expected_bits(&case.mel), "{name} : mel");
        assert_eq!(
            bits(&got.encoder),
            expected_bits(&case.encoder),
            "{name} : encodeur"
        );
        assert_eq!(got.audio_tokens, case.audio_tokens, "{name} : jetons audio");
        assert_eq!(got.prompt, case.prompt, "{name} : invite");
        assert_eq!(got.tokens, case.tokens, "{name} : jetons");
        assert_eq!(got.text, case.text, "{name} : texte");
    }
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
fn parity_small() {
    parity(SMALL);
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
fn parity_large() {
    parity(LARGE);
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT, à l'octet. Pour chaque entrée : le
/// mel identique à l'octet (forme, moyenne, moyenne absolue) et l'invite ;
/// sur la parole (R69, `stt::mlx::shipped`), le nombre de pas du décodeur
/// (jetons gardés) à 10 % près de la fixture, arrondi vers le bas, et le
/// texte ; sans parole, la raison d'arrêt égale à celle du Swift (R70),
/// déduite du nombre de jetons (plafond atteint si et seulement s'il vaut
/// `MAX_TOKENS`, des deux côtés ; la fin de texte et le garde-fou des
/// répétitions sont tous deux « fini seul »). Qwen3-ASR est
/// multilingue (le catalogue le donne pour le français et l'anglais) et sa
/// langue imposée : le texte est exigé sur toute la parole. L'encodeur n'est
/// pas comparé (noyaux AOT, R54).
///
/// Mesuré le 23 nov. 2026 (build livré) : le 1.7B, sur silence, 2 pas
/// contre 3 (« Je. » contre « C'est. ») et sur dixieme 3 contre 2 (« En
/// fait. » contre « Je. »), l'encodeur AOT à environ 1e-7 ; les quatre
/// entrées de parole identiques ; le 0.6B identique partout.
fn shipped_smoke(id: &'static str) {
    let (fixture, measured, prompts) = run_fixture(id);
    assert_eq!(prompts, fixture.prompts, "invites de 13 jetons audio");
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(
            bits(&got.mel),
            expected_bits(&case.mel),
            "{name} : mel à l'octet"
        );
        assert_eq!(got.prompt, case.prompt, "{name} : invite");
        shipped::steps(name, got.tokens.len(), case.tokens.len(), "");
        shipped::text(name, "texte", &got.text, &case.text);
        shipped::stop(
            name,
            "décodage",
            shipped::Stop::from_count(got.tokens.len(), MAX_TOKENS),
            shipped::Stop::from_count(case.tokens.len(), MAX_TOKENS),
        );
    }
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
fn shipped_smoke_small() {
    shipped_smoke(SMALL);
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
fn shipped_smoke_large() {
    shipped_smoke(LARGE);
}

/// Le cas propre de la tâche, sans les poids (le vocabulaire et
/// `config.json` seulement) : les invites de 13 jetons audio en français
/// (finie par « language French<asr_text> »), en anglais et sans langue,
/// identiques à celles du Swift, pour les deux modèles.
#[test]
#[ignore = "demande le vocabulaire des modèles (DYSFLOW_MODELS)"]
fn the_prompts_are_those_of_the_swift() {
    use crate::compute::mlx::tokenizer::bpe::BpeTokenizer;
    for id in [SMALL, LARGE] {
        let path = repo().join(format!("tests/fixtures/mlx/{id}.json"));
        let fixture: Fixture =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let dir = model_dir(id);
        let tokenizer = BpeTokenizer::from_dir(&dir).unwrap();
        let config = config::Config::load(&dir).unwrap();
        let prompt = |language| model::prompt_ids(&tokenizer, &config, 13, language).unwrap();
        let prompts = Prompts {
            fr: prompt(Some("fr")),
            en: prompt(Some("en")),
            none: prompt(None),
        };
        assert_eq!(prompts, fixture.prompts, "{id}");
        let tail = tokenizer.decode(&prompts.fr[prompts.fr.len() - 4..]);
        assert!(
            tail.ends_with("language French<asr_text>"),
            "{id} : {tail:?}"
        );
        eprintln!(
            "{id} : invites fr ({}), en ({}), sans langue ({}) identiques",
            prompts.fr.len(),
            prompts.en.len(),
            prompts.none.len()
        );
    }
}

// ---------- fumée (Qwen3ASRSmokeTests.swift) ----------

/// Le modèle se charge d'un dossier local, avec le vocabulaire BPE, et lit
/// une seconde de silence sans langue imposée (il la reconnaît seul) sans
/// planter. Le gabarit garde un `<|audio_pad|>` par jeton audio.
fn reads_silence(id: &'static str) {
    let _serial = serial();
    let held = load(id);
    let (pads, transcript) = held
        .with(|asr| {
            let prompt = asr.prompt(13, None)?;
            let pad = asr.config.audio_token_id as u32;
            let pads = prompt.iter().filter(|&&t| t == pad).count();
            let transcript = asr.generate(&[0.0; 16_000], None, MAX_TOKENS, &|| Ok(()))?;
            crate::compute::mlx::clear_cache()?;
            Ok((pads, transcript))
        })
        .unwrap();
    assert_eq!(pads, 13, "{id}");
    assert_eq!(transcript.chunks.len(), 1, "{id}");
    eprintln!(
        "{id} : silence → {:?} ({} jetons)",
        transcript.text,
        transcript.chunks[0].tokens.len()
    );
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_small_model_reads_one_second_of_silence_without_crashing() {
    reads_silence(SMALL);
}

#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_large_model_reads_one_second_of_silence_without_crashing() {
    reads_silence(LARGE);
}
