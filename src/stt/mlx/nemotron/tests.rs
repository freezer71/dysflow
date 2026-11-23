//! Les essais de Nemotron en MLX sur le vrai modèle, ignorés par défaut :
//! `DYSFLOW_MODELS` désigne un dossier qui contient
//! `nemotron_35_asr_streaming_8bit/`, et ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh`
//! (les noyaux Metal du Swift, voir l'en-tête du script) : par entrée, le
//! mel de tout l'audio, chaque tranche de l'encodeur après l'invite (forme,
//! moyenne, moyenne absolue, à l'octet), puis pas par pas le texte nouveau
//! et les jetons nouveaux, les jetons de `finish`, tous les jetons et les
//! textes ; enfin les autres découpages. Le build livré a sa garde,
//! `shipped_smoke` (`--livre`).

use super::model::Nemotron;
use super::stream::{Delta, Session};
use super::*;
use crate::stt::mlx::parakeet::model::stats;
use crate::stt::mlx::parakeet::tests::{inputs, model_dir_with, serial, sha256};
use crate::stt::mlx::shipped;
use std::path::PathBuf;

const ID: &str = "nemotron_35_asr_streaming_8bit";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

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

fn load() -> Held<Nemotron> {
    let dir = model_dir();
    Held::load(move || Nemotron::load(&dir)).unwrap()
}

/// `samples` coupés selon `sizes`, repris en boucle ; `[]` : d'un coup
/// (`NemotronFixtureTests.pieces`).
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

#[derive(serde::Deserialize, Debug, PartialEq)]
struct FixtureDelta {
    step: usize,
    text: String,
    tokens: Vec<usize>,
}

#[derive(serde::Deserialize)]
struct Split {
    split: String,
    tokens: Vec<usize>,
    text: String,
}

#[derive(serde::Deserialize)]
struct SplitDescription {
    name: String,
    sizes: Vec<usize>,
}

#[derive(serde::Deserialize)]
struct Case {
    input: String,
    language: String,
    mel: Stats,
    chunks: Vec<Stats>,
    steps: usize,
    deltas: Vec<FixtureDelta>,
    finish_tokens: Vec<usize>,
    tokens: Vec<usize>,
    session_text: String,
    text: String,
    splits: Vec<Split>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    compute_dtype: String,
    quantization: String,
    chunk_ms: u32,
    step_samples: usize,
    inputs: Vec<Input>,
    splits: Vec<SplitDescription>,
    cases: Vec<Case>,
}

type Stat = (Vec<i32>, f32, f32);

/// Un passage d'une session, copié hors du fil MLX.
struct Run {
    chunks: Vec<Stat>,
    /// Le texte entier de la session et ses jetons nouveaux, après chaque
    /// pas.
    steps: Vec<(String, Vec<usize>)>,
    finish: Delta,
    tokens: Vec<usize>,
    session_text: String,
}

/// Ce que le Rust rend pour une entrée.
struct Measured {
    mel: Stat,
    reference: Run,
    splits: Vec<Run>,
}

/// Une session sur `parts`, comme le générateur : chaque tranche après
/// l'invite (`trace`, si `traced`), le texte et les jetons de chaque pas.
fn session_run(
    model: &mut Nemotron,
    language: &str,
    parts: &[Vec<f32>],
    traced: bool,
) -> anyhow::Result<Run> {
    let mut session = Session::new(model, language, Some(CHUNK_MS));
    let mut chunks = Vec::new();
    let mut trace = |prompted: &mlx_rs::Array| -> anyhow::Result<()> {
        if traced {
            chunks.push(stats(prompted)?);
        }
        Ok(())
    };
    let mut steps = Vec::new();
    for part in parts {
        let delta = session.step(model, part, &mut trace)?;
        steps.push((session.text().to_string(), delta.tokens));
    }
    let finish = session.finish(model, &mut trace)?;
    Ok(Run {
        chunks,
        steps,
        finish,
        tokens: session.tokens(),
        session_text: session.text().to_string(),
    })
}

/// Les textes que `NemotronStream` rendrait, pas par pas (`feed`), puis à
/// la fin (`finish`) : `NemotronSession` de l'app.
fn app_texts(run: &Run) -> (Vec<String>, String) {
    let mut shown = String::new();
    let fed = run
        .steps
        .iter()
        .map(|(text, _)| grown(&mut shown, &tokenizer::tidy(text)))
        .collect();
    (fed, tokenizer::tidy(&run.session_text).trim().to_string())
}

fn bits(s: &Stat) -> (Vec<i32>, u32, u32) {
    (s.0.clone(), s.1.to_bits(), s.2.to_bits())
}

fn expected_bits(s: &Stats) -> (Vec<i32>, u32, u32) {
    (s.shape.clone(), s.mean.to_bits(), s.mean_abs.to_bits())
}

/// `ParakeetFixtureTests.quantization`.
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
/// vérifications communes (modèle, révision, quantification, tranche, pas,
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
    assert_eq!(fixture.compute_dtype, "bfloat16");
    assert_eq!(fixture.chunk_ms, CHUNK_MS);
    assert_eq!(fixture.step_samples, 1_280, "le pas de 80 ms de la dictée");
    assert_eq!(
        crate::stt::dictation::STREAM_STEP.as_millis(),
        80,
        "le pas du flux de la dictée"
    );
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
    let step = fixture.step_samples;
    let splits: Vec<Vec<usize>> = fixture.splits.iter().map(|s| s.sizes.clone()).collect();
    let jobs: Vec<(Vec<f32>, String)> = inputs
        .iter()
        .zip(&fixture.cases)
        .map(|((name, samples), case)| {
            assert_eq!(case.language, language(name), "{name} : langue");
            (samples.clone(), case.language.clone())
        })
        .collect();
    let measured = held
        .with(move |model| {
            jobs.iter()
                .map(|(samples, language)| {
                    let mel = stats(&model.mel(samples)?)?;
                    let reference = session_run(model, language, &pieces(samples, &[step]), true)?;
                    let splits = splits
                        .iter()
                        .map(|sizes| session_run(model, language, &pieces(samples, sizes), false))
                        .collect::<anyhow::Result<_>>()?;
                    Ok(Measured {
                        mel,
                        reference,
                        splits,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let same_chunks = got.reference.chunks.len() == case.chunks.len()
            && got
                .reference
                .chunks
                .iter()
                .zip(&case.chunks)
                .all(|(g, e)| bits(g) == expected_bits(e));
        let (_, text) = app_texts(&got.reference);
        eprintln!(
            "{ID} {} ({}) : mel {} ; {} tranches contre {}, {} ; {} jetons contre {} ; {:?}",
            case.input,
            case.language,
            if bits(&got.mel) == expected_bits(&case.mel) {
                "identique"
            } else {
                "DIFFÉRENT"
            },
            got.reference.chunks.len(),
            case.chunks.len(),
            if same_chunks {
                "identiques"
            } else {
                "DIFFÉRENTES"
            },
            got.reference.tokens.len(),
            case.tokens.len(),
            text,
        );
        for (index, (g, e)) in got.reference.chunks.iter().zip(&case.chunks).enumerate() {
            if bits(g) != expected_bits(e) {
                eprintln!(
                    "  tranche {index} : {:?} Δ(moyenne, |moyenne|) ({:e}, {:e})",
                    g.0,
                    (g.1 - e.mean).abs(),
                    (g.2 - e.mean_abs).abs()
                );
            }
        }
        for (split, run) in fixture.splits.iter().zip(&got.splits) {
            eprintln!(
                "  découpage {} : {} jetons, {:?}",
                split.name,
                run.tokens.len(),
                app_texts(run).1
            );
        }
    }
    (fixture, measured)
}

/// Les pas où le texte rendu ou les jetons nouveaux ne sont pas vides,
/// comme la fixture les écrit.
fn deltas(run: &Run) -> Vec<FixtureDelta> {
    let (fed, _) = app_texts(run);
    run.steps
        .iter()
        .zip(fed)
        .enumerate()
        .filter(|(_, ((_, tokens), text))| !text.is_empty() || !tokens.is_empty())
        .map(|(step, ((_, tokens), text))| FixtureDelta {
            step,
            text,
            tokens: tokens.clone(),
        })
        .collect()
}

/// P2 : chaque entrée, du mel au texte, tranche par tranche et pas par pas,
/// à l'identique ; puis chaque autre découpage, jetons et texte.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
fn parity() {
    let (fixture, measured) = run_fixture();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(bits(&got.mel), expected_bits(&case.mel), "{name} : mel");
        let run = &got.reference;
        assert_eq!(run.chunks.len(), case.chunks.len(), "{name} : tranches");
        for (index, (g, e)) in run.chunks.iter().zip(&case.chunks).enumerate() {
            assert_eq!(bits(g), expected_bits(e), "{name} : tranche {index}");
        }
        assert_eq!(run.steps.len(), case.steps, "{name} : pas");
        assert_eq!(deltas(run), case.deltas, "{name} : pas par pas");
        assert_eq!(
            run.finish.tokens, case.finish_tokens,
            "{name} : jetons de finish"
        );
        assert_eq!(run.tokens, case.tokens, "{name} : jetons");
        assert_eq!(
            run.session_text, case.session_text,
            "{name} : texte de la session"
        );
        assert_eq!(app_texts(run).1, case.text, "{name} : texte");
        assert_eq!(got.splits.len(), case.splits.len());
        for (split, (run, expected)) in fixture
            .splits
            .iter()
            .zip(got.splits.iter().zip(&case.splits))
        {
            assert_eq!(split.name, expected.split);
            assert_eq!(
                run.tokens, expected.tokens,
                "{name}, {} : jetons",
                split.name
            );
            assert_eq!(
                app_texts(run).1,
                expected.text,
                "{name}, {} : texte",
                split.name
            );
        }
    }
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT, à l'octet. Pour chaque entrée : le
/// mel identique à l'octet (forme, moyenne, moyenne absolue) et le même
/// nombre de tranches (celles de l'audio) ; sur la parole (R69,
/// `stt::mlx::shipped`), le nombre de jetons à 10 % près de la fixture
/// (arrondi vers le bas) et le texte. Sans parole, le transducteur finit
/// toujours (borné par ses trames) : rien d'autre. L'encodeur n'est pas
/// comparé (les noyaux livrés de `depthwise_conv_2d` et `layer_norm`
/// arrondissent autrement). Nemotron est donné pour le français et
/// l'anglais, et sa langue est imposée : le texte est exigé sur toute la
/// parole, et pour chaque découpage.
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
        let run = &got.reference;
        assert_eq!(run.chunks.len(), case.chunks.len(), "{name} : tranches");
        shipped::steps(name, run.tokens.len(), case.tokens.len(), None, "");
        shipped::text(name, "texte", &app_texts(run).1, &case.text);
        for (split, run) in fixture.splits.iter().zip(&got.splits) {
            let what = format!("{} : texte", split.name);
            shipped::text(name, &what, &app_texts(run).1, &case.text);
        }
    }
}

// ---------- le flux du reconnaisseur ----------

/// « Même texte quel que soit le découpage des `feed` » : par le vrai
/// `StreamingRecognizer` (`NemotronMlx`, ce que la dictée appelle), chaque
/// entrée donnée d'un coup, par tranches de 560 ms, par pas de 80 ms et par
/// morceaux irréguliers, plus petits qu'un pas compris, rend le texte de la
/// fixture à `finish` ; les morceaux rendus par `feed`, bout à bout, sont un
/// début de ce texte, et `feed` ne rend jamais rien qui ne l'allonge.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_text_does_not_depend_on_how_the_audio_is_fed() {
    let path = repo().join(format!("tests/fixtures/mlx/{ID}.json"));
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let _serial = serial();
    let recognizer = NemotronMlx::new();
    recognizer.load(&model_dir()).unwrap();
    let splits: [(&str, &[usize]); 5] = [
        ("entier", &[]),
        ("tranches_560ms", &[8_960]),
        ("pas_80ms", &[1_280]),
        ("irregulier", &[1, 160, 1_279, 4_000, 37, 9_000, 2]),
        ("petits", &[3, 500, 17, 1_024, 333]),
    ];
    for ((name, samples), case) in inputs().iter().zip(&fixture.cases) {
        for (split, sizes) in splits {
            let mut stream = recognizer.open_stream(language(name)).unwrap();
            let mut fed = String::new();
            for part in pieces(samples, sizes) {
                fed.push_str(&stream.feed(&part).unwrap());
            }
            let text = stream.finish().unwrap();
            eprintln!("{name}, {split} : {text:?}");
            assert_eq!(text, case.text, "{name}, {split} : texte");
            assert!(
                tokenizer::tidy(&case.session_text).starts_with(&fed),
                "{name}, {split} : {fed:?} n'est pas un début du texte"
            );
            assert_eq!(stream.feed(&[0.0; 1_280]).unwrap(), "", "après finish");
            assert_eq!(stream.finish().unwrap(), text, "finish de nouveau");
        }
    }
    recognizer.release();
}

// ---------- fumée (NemotronSmokeTests.swift) ----------

/// Le modèle se charge d'un dossier local ; une session de flux en
/// français, tranches de 560 ms, reçoit une seconde de silence par blocs de
/// 80 ms (le pas de la dictée) et se termine sans planter ; après `finish`,
/// un bloc de plus n'est plus lu. La lecture d'un coup du Swift
/// (`generate`) n'a pas d'équivalent : le moteur Rust n'est qu'en flux.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn reads_and_streams_one_second_of_silence_without_crashing() {
    let _serial = serial();
    let held = load();
    let (finished, after) = held
        .with(|model| {
            let mut session = Session::new(model, "fr", Some(CHUNK_MS));
            let silence = [0.0f32; 16_000];
            for block in silence.chunks(1_280) {
                session.step(model, block, &mut |_| Ok(()))?;
            }
            session.finish(model, &mut |_| Ok(()))?;
            let finished = session.is_finished();
            let after = session.step(model, &[0.0; 1_280], &mut |_| Ok(()))?;
            Ok((finished, after))
        })
        .unwrap();
    assert!(finished);
    assert_eq!(after, Delta::default(), "finish scelle le flux");
}

/// La préparation passe par un flux entier, puis un flux s'ouvre encore ;
/// libéré, le modèle refuse d'en ouvrir un, et un flux resté ouvert échoue
/// au lieu de lire.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn warm_up_then_release_closes_the_open_streams() {
    let _serial = serial();
    let recognizer = NemotronMlx::new();
    recognizer.load(&model_dir()).unwrap();
    recognizer.warm_up().unwrap();
    assert!(recognizer.memory().is_some());
    let mut stream = recognizer.open_stream("fr").unwrap();
    assert_eq!(stream.feed(&[0.0; 1_280]).unwrap(), "");
    recognizer.release();
    assert!(recognizer.memory().is_none());
    assert!(recognizer.open_stream("fr").is_err());
    let err = stream.feed(&[0.0; 1_280]).unwrap_err().to_string();
    assert!(err.contains("Nemotron"), "{err}");
}
