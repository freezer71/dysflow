//! Les essais de Canary en MLX sur le vrai modèle, ignorés par défaut :
//! `DYSFLOW_MODELS` désigne un dossier qui contient `canary_1b_v2_q8/`, et
//! ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh`
//! (les noyaux Metal du Swift, voir l'en-tête du script et celui des essais
//! de Parakeet) : mel et sortie de l'encodeur identiques à l'octet (forme,
//! moyenne, moyenne absolue), longueur encodée, invite, jetons et texte. Le
//! build livré a sa garde, `shipped_smoke` (`--livre`).
//!
//! Le décodeur livré calcule en bf16 (échelles, biais et normes du fichier) :
//! le pas à pas (une ligne) et la suite entière (plusieurs) passent par des
//! noyaux qui n'arrondissent pas aux mêmes endroits (tâche 18). Leur
//! comparaison sur le vrai modèle tolère 1e-3 relatif au maximum des logits,
//! et exige les mêmes jetons et le même texte.

use super::model::{Canary, Encoded, Transcript};
use super::*;
use crate::stt::mlx::parakeet::model::stats;
use crate::stt::mlx::parakeet::tests::{inputs, model_dir_with, serial, sha256};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::Array;
use std::path::PathBuf;

const ID: &str = "canary_1b_v2_q8";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Le dossier du modèle sous `DYSFLOW_MODELS` ; sans lui, l'essai échoue.
fn model_dir() -> PathBuf {
    model_dir_with(ID, config::CONFIG_FILE)
}

fn language(input: &str) -> &'static str {
    if input == "anglais" {
        "en"
    } else {
        "fr"
    }
}

fn load() -> Held<Canary> {
    let dir = model_dir();
    Held::load(move || Canary::load(&dir)).unwrap()
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
    encoded_length: i32,
    prompt: Vec<i64>,
    tokens: Vec<i64>,
    text: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    encoder_dtype: String,
    quantization: String,
    max_tokens: usize,
    inputs: Vec<Input>,
    cases: Vec<Case>,
}

type Stat = (Vec<i32>, f32, f32);

/// Ce que le Rust rend pour une entrée, copié hors du fil MLX.
struct Measured {
    mel: Stat,
    encoder: Stat,
    encoder_dtype: String,
    encoded_length: i32,
    transcript: Transcript,
    text: String,
}

/// Comme le générateur : le mel et l'encodeur, puis `generate` (le chemin
/// de l'app) et `clean`.
fn measure(canary: &mut Canary, samples: &[f32], language: &str) -> anyhow::Result<Measured> {
    let mel = canary.mel(samples)?;
    mel.eval()?;
    let mel_stats = stats(&mel)?;
    let encoded = canary.encode(&mel)?;
    let encoder = stats(&encoded.hidden)?;
    let encoded_length = encoded.lengths.try_as_slice::<i32>()?[0];
    let encoder_dtype = format!("{:?}", encoded.hidden.dtype()).to_lowercase();
    let transcript = canary.generate(samples, language, model::MAX_TOKENS, &|| Ok(()))?;
    let text = tokenizer::clean(&transcript.text);
    Ok(Measured {
        mel: mel_stats,
        encoder,
        encoder_dtype,
        encoded_length,
        transcript,
        text,
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

/// La fixture et ce que le Rust rend pour chacune de ses entrées, après les
/// vérifications communes (modèle, révision, quantification, plafond,
/// entrées identiques à celles du Swift). Chaque mesure est imprimée avant
/// toute assertion.
fn run_fixture() -> (Fixture, Vec<Measured>) {
    let path = repo().join(format!("tests/fixtures/mlx/{ID}.json"));
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(fixture.model, ID);
    let package = crate::models::CATALOG
        .iter()
        .find(|p| p.id == ID)
        .expect("paquet du catalogue");
    assert_eq!(fixture.revision, package.revision, "révision du catalogue");
    assert_eq!(fixture.max_tokens, model::MAX_TOKENS);
    let dir = model_dir();
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
    let held = load();
    let jobs: Vec<(Vec<f32>, String)> = inputs
        .iter()
        .zip(&fixture.cases)
        .map(|((name, samples), case)| {
            assert_eq!(case.language, language(name), "{name} : langue");
            (samples.clone(), case.language.clone())
        })
        .collect();
    let measured = held
        .with(move |canary| {
            jobs.iter()
                .map(|(samples, language)| measure(canary, samples, language))
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        eprintln!(
            "{ID} {} ({}) : mel {} ; encodeur {} Δ(moyenne, |moyenne|) ({:e}, {:e}) ; {} jetons contre {} ; {:?}",
            case.input,
            case.language,
            if bits(&got.mel) == expected_bits(&case.mel) {
                "identique"
            } else {
                "DIFFÉRENT"
            },
            got.encoder_dtype,
            (got.encoder.1 - case.encoder.mean).abs(),
            (got.encoder.2 - case.encoder.mean_abs).abs(),
            got.transcript.tokens.len(),
            case.tokens.len(),
            got.text,
        );
    }
    (fixture, measured)
}

/// P2 : chaque entrée, du mel au texte, à l'identique.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
fn parity() {
    let (fixture, measured) = run_fixture();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.encoder_dtype, fixture.encoder_dtype, "{name} : type");
        assert_eq!(bits(&got.mel), expected_bits(&case.mel), "{name} : mel");
        assert_eq!(
            bits(&got.encoder),
            expected_bits(&case.encoder),
            "{name} : encodeur"
        );
        assert_eq!(got.encoded_length, case.encoded_length, "{name} : longueur");
        assert_eq!(got.transcript.prompt, case.prompt, "{name} : invite");
        assert_eq!(got.transcript.tokens, case.tokens, "{name} : jetons");
        assert_eq!(got.text, case.text, "{name} : texte");
    }
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT, à l'octet. Pour chaque entrée : le
/// mel identique à l'octet, l'invite, le nombre de pas du décodeur (jetons
/// gardés) à 10 % près de la fixture, arrondi vers le bas, et le texte.
/// Canary est multilingue (le catalogue le donne pour le français et
/// l'anglais) et sa langue imposée : le texte est exigé sur toutes les
/// entrées.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
fn shipped_smoke() {
    let (fixture, measured) = run_fixture();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(
            bits(&got.mel),
            expected_bits(&case.mel),
            "{name} : mel à l'octet"
        );
        assert_eq!(got.transcript.prompt, case.prompt, "{name} : invite");
        let (steps, expected) = (got.transcript.tokens.len(), case.tokens.len());
        let bound = expected / 10;
        assert!(
            steps.abs_diff(expected) <= bound,
            "{name} : {steps} pas contre {expected}, au-delà de la borne de {bound}"
        );
        assert_eq!(got.text, case.text, "{name} : texte");
    }
}

// ---------- fumée (CanarySmokeTests.swift) ----------

/// Le modèle se charge d'un dossier local, trouve son vocabulaire dans
/// `config.json` et transcrit une seconde de silence en français sans
/// planter : l'invite porte `<|fr|>`, et la génération trouve une fin avant
/// le plafond de 1 024 jetons.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn transcribes_one_second_of_silence_in_french_without_crashing() {
    let _serial = serial();
    let held = load();
    let (transcript, fr) = held
        .with(|canary| {
            let transcript =
                canary.generate(&[0.0; 16_000], "fr", model::MAX_TOKENS, &|| Ok(()))?;
            let fr = canary.tokenizer.prompt(&canary.config, "fr", "fr");
            Ok((transcript, fr))
        })
        .unwrap();
    assert_eq!(transcript.prompt, fr);
    let fr_token = transcript.prompt.get(3).copied();
    assert!(
        fr_token.is_some() && transcript.prompt.get(4).copied() == fr_token,
        "langue source et cible : fr ({:?})",
        transcript.prompt
    );
    assert!(
        transcript.tokens.len() < model::MAX_TOKENS,
        "la génération n'a pas trouvé de fin sur du silence"
    );
}

// ---------- cache contre suite entière, sur le vrai modèle ----------

/// Les logits de la dernière position, `[vocabulaire]`, en Float32.
fn last_logits(logits: &Array) -> anyhow::Result<Array> {
    let time = logits.shape()[1];
    Ok(logits
        .try_index((0, time - 1))?
        .as_dtype(mlx_rs::Dtype::Float32)?)
}

/// Le décodage glouton de `generate` deux fois : pas à pas (cache), avec à
/// chaque pas la suite entière sur le même préfixe ; puis en repassant
/// toute la suite à chaque jeton. Rend l'écart relatif maximal des logits
/// (au maximum de leur valeur absolue), le nombre de pas où l'argmax
/// diffère, et les deux suites de jetons.
fn cache_against_full(
    canary: &Canary,
    encoded: &Encoded,
    language: &str,
) -> anyhow::Result<(f32, usize, Vec<i64>, Vec<i64>)> {
    let decoder = &canary.decoder;
    let prompt = canary.tokenizer.prompt(&canary.config, language, language);
    let eos = canary.tokenizer.eos(&canary.config);
    let ids = |tokens: &[i64]| {
        let t: Vec<i32> = tokens.iter().map(|&t| t as i32).collect();
        Array::from_slice(&t, &[1, t.len() as i32])
    };
    let full = |tokens: &[i64]| -> anyhow::Result<Array> {
        last_logits(&decoder.forward(&ids(tokens), &encoded.hidden, Some(&encoded.mask), 0)?)
    };
    let argmax = |x: &Array| -> anyhow::Result<i64> {
        Ok(i64::from(
            argmax_axis(x, -1, None)?.try_item_exact::<u32>()?,
        ))
    };

    let mut state = decoder.make_state(&encoded.hidden, Some(&encoded.mask))?;
    let (mut tokens, mut pending) = (prompt.clone(), prompt.clone());
    let (mut gap, mut largest, mut differs) = (0f32, 0f32, 0usize);
    let mut cached = Vec::new();
    for _ in 0..model::MAX_TOKENS {
        let s = last_logits(&decoder.step(&ids(&pending), &mut state)?)?;
        let f = full(&tokens)?;
        let g = mlx_rs::ops::abs(s.subtract(&f)?)?.max(None)?;
        let m = mlx_rs::ops::abs(&f)?.max(None)?;
        mlx_rs::transforms::eval([&s, &f, &g, &m])?;
        gap = gap.max(g.try_item_exact::<f32>()?);
        largest = largest.max(m.try_item_exact::<f32>()?);
        let next = argmax(&s)?;
        if next != argmax(&f)? {
            differs += 1;
        }
        if next == eos {
            break;
        }
        tokens.push(next);
        cached.push(next);
        pending = vec![next];
    }

    let mut tokens = prompt;
    let mut whole = Vec::new();
    for _ in 0..model::MAX_TOKENS {
        let f = full(&tokens)?;
        f.eval()?;
        let next = argmax(&f)?;
        if next == eos {
            break;
        }
        tokens.push(next);
        whole.push(next);
    }
    Ok((gap / largest.max(f32::MIN_POSITIVE), differs, cached, whole))
}

/// `CanaryCacheMeasureTests` (tâche 18), en essai : sur le vrai modèle en
/// bf16 tel que livré, sur le GPU, le pas à pas suit la suite entière à
/// 1e-3 relatif près (mesuré jusqu'à 8e-4 en Swift), avec les mêmes jetons
/// et le même texte, sur le français, l'anglais et les douze secondes.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_cache_follows_the_full_pass_on_the_shipped_model() {
    let _serial = serial();
    let held = load();
    let jobs: Vec<(&'static str, Vec<f32>)> = inputs()
        .into_iter()
        .filter(|(name, _)| ["francais", "anglais", "douze_secondes"].contains(name))
        .collect();
    assert_eq!(jobs.len(), 3);
    let results = held
        .with(move |canary| {
            jobs.iter()
                .map(|(name, samples)| {
                    let mel = canary.mel(samples)?;
                    let encoded = canary.encode(&mel)?;
                    let (relative, differs, cached, whole) =
                        cache_against_full(canary, &encoded, language(name))?;
                    let texts = (
                        tokenizer::clean(&canary.tokenizer.decode(&cached)),
                        tokenizer::clean(&canary.tokenizer.decode(&whole)),
                    );
                    Ok((*name, relative, differs, cached, whole, texts))
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();
    for (name, relative, differs, cached, whole, (cached_text, _)) in &results {
        eprintln!(
            "{name} : écart relatif max {relative:e}, argmax différent {differs}, {} jetons (cache) contre {} ; {cached_text:?}",
            cached.len(),
            whole.len()
        );
    }
    for (name, relative, differs, cached, whole, (cached_text, whole_text)) in results {
        assert!(relative <= 1e-3, "{name} : écart relatif {relative:e}");
        assert_eq!(differs, 0, "{name} : argmax");
        assert_eq!(cached, whole, "{name} : jetons");
        assert_eq!(cached_text, whole_text, "{name} : texte");
    }
}
