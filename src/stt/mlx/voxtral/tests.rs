//! Les essais de Voxtral Realtime en MLX sur le vrai modèle, ignorés par
//! défaut : `DYSFLOW_MODELS` désigne un dossier qui contient
//! `voxtral_realtime_4bit/`, et ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh`
//! (les noyaux Metal du Swift, voir l'en-tête du script) : par entrée, le
//! mel du flux, chaque tranche de l'adaptateur (forme, moyenne, moyenne
//! absolue, à l'octet), puis pas par pas le texte et les jetons nouveaux,
//! ceux de `finish`, tous les jetons et les textes ; enfin les autres
//! découpages. Le build livré a sa garde, `shipped_smoke` (`--livre`).

use super::model::Voxtral;
use super::stream::{Delta, Session, Trace};
use super::*;
use crate::stt::mlx::parakeet::model::stats;
use crate::stt::mlx::parakeet::tests::{
    cycled, inputs as parakeet_inputs, model_dir_with, serial, sha256,
};
use std::cell::Cell;
use std::path::PathBuf;

const ID: &str = "voxtral_realtime_4bit";
pub(super) const STEP: usize = 1_280;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn model_dir() -> PathBuf {
    model_dir_with(ID, config::CONFIG_FILE)
}

fn load() -> Held<Voxtral> {
    let dir = model_dir();
    Held::load(move || Voxtral::load(&dir)).unwrap()
}

/// Les entrées de `WhisperFixtureTests.inputs` : celles de Parakeet, plus
/// 35 s des deux dictées en boucle.
pub(super) fn inputs() -> Vec<(&'static str, Vec<f32>)> {
    let mut inputs = parakeet_inputs();
    let long = cycled(&[&inputs[0].1, &inputs[1].1], 560_000);
    inputs.push(("trente_cinq_secondes", long));
    inputs
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

#[derive(serde::Deserialize)]
struct Block {
    step: usize,
    stats: Stats,
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
    mel: Stats,
    adapter: Vec<Block>,
    steps: usize,
    deltas: Vec<FixtureDelta>,
    finish_text: String,
    finish_tokens: Vec<usize>,
    tokens: Vec<usize>,
    finished: bool,
    session_text: String,
    text: String,
    splits: Vec<Split>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    quantization: String,
    delay_ms: i32,
    delay_tokens: i32,
    max_tokens: usize,
    step_samples: usize,
    inputs: Vec<Input>,
    splits: Vec<SplitDescription>,
    cases: Vec<Case>,
}

type Stat = (Vec<i32>, f32, f32);

/// Un passage d'une session, copié hors du fil MLX.
struct Run {
    mel: Option<Stat>,
    adapter: Vec<(usize, Stat)>,
    steps: Vec<Delta>,
    finish: Delta,
    tokens: Vec<usize>,
    finished: bool,
    session_text: String,
}

impl Run {
    /// Le texte de `VoxtralSession.finish` : sans espaces autour.
    fn text(&self) -> String {
        self.session_text.trim().to_string()
    }

    /// Les pas où le texte ou les jetons nouveaux ne sont pas vides, comme
    /// la fixture les écrit.
    fn deltas(&self) -> Vec<FixtureDelta> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.text.is_empty() || !d.tokens.is_empty())
            .map(|(step, d)| FixtureDelta {
                step,
                text: d.text.clone(),
                tokens: d.tokens.clone(),
            })
            .collect()
    }
}

/// Ce que le Rust rend pour une entrée.
struct Measured {
    reference: Run,
    splits: Vec<Run>,
}

/// Une session sur `parts`, comme le générateur : si `traced`, le mel de
/// tout le flux et chaque tranche de l'adaptateur, avec son pas.
fn session_run(model: &mut Voxtral, parts: &[Vec<f32>], traced: bool) -> anyhow::Result<Run> {
    let mut session = Session::new(model, DELAY_MS, MAX_TOKENS);
    let step = Cell::new(0);
    let mut mels = Vec::new();
    let mut adapter = Vec::new();
    let mut trace = |t: Trace| -> anyhow::Result<()> {
        if traced {
            match t {
                Trace::Mel(columns) => {
                    columns.eval()?;
                    mels.push(columns.clone());
                }
                Trace::Adapter(rows) => adapter.push((step.get(), stats(rows)?)),
            }
        }
        Ok(())
    };
    let mut steps = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        step.set(index);
        steps.push(session.step(model, part, &mut trace)?);
    }
    step.set(parts.len());
    let finish = session.finish(model, &mut trace)?;
    let mel = if traced {
        Some(stats(&mlx_rs::ops::concatenate(&mels, 1)?)?)
    } else {
        None
    };
    Ok(Run {
        mel,
        adapter,
        steps,
        finish,
        tokens: session.tokens().to_vec(),
        finished: session.is_finished(),
        session_text: session.text(),
    })
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

fn read_fixture() -> Fixture {
    let path = repo().join(format!("tests/fixtures/mlx/{ID}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// La fixture et ce que le Rust rend pour chacune de ses entrées, après les
/// vérifications communes (modèle, révision, quantification, retard, pas,
/// entrées identiques à celles du Swift). Chaque mesure est imprimée avant
/// toute assertion.
fn run_fixture() -> (Fixture, Vec<Measured>) {
    let fixture = read_fixture();
    assert_eq!(fixture.model, ID);
    let package = crate::models::CATALOG
        .iter()
        .find(|p| p.id == ID)
        .expect("paquet du catalogue");
    assert_eq!(fixture.revision, package.revision, "révision du catalogue");
    assert_eq!(fixture.delay_ms, DELAY_MS);
    assert_eq!(fixture.delay_tokens, model::num_delay_tokens(DELAY_MS));
    assert_eq!(fixture.max_tokens, MAX_TOKENS);
    assert_eq!(fixture.step_samples, STEP, "le pas de 80 ms de la dictée");
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
    let splits: Vec<Vec<usize>> = fixture.splits.iter().map(|s| s.sizes.clone()).collect();
    let jobs: Vec<Vec<f32>> = inputs.into_iter().map(|(_, samples)| samples).collect();
    let measured = held
        .with(move |model| {
            jobs.iter()
                .map(|samples| {
                    let reference = session_run(model, &pieces(samples, &[STEP]), true)?;
                    let splits = splits
                        .iter()
                        .map(|sizes| session_run(model, &pieces(samples, sizes), false))
                        .collect::<anyhow::Result<_>>()?;
                    Ok(Measured { reference, splits })
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let run = &got.reference;
        let same_adapter = run.adapter.len() == case.adapter.len()
            && run
                .adapter
                .iter()
                .zip(&case.adapter)
                .all(|((step, g), e)| *step == e.step && bits(g) == expected_bits(&e.stats));
        let mel = run.mel.as_ref().map(bits);
        eprintln!(
            "{ID} {} : mel {} ; {} tranches de l'adaptateur contre {}, {} ; {} jetons contre {} ; {:?}",
            case.input,
            if mel == Some(expected_bits(&case.mel)) {
                "identique"
            } else {
                "DIFFÉRENT"
            },
            run.adapter.len(),
            case.adapter.len(),
            if same_adapter {
                "identiques"
            } else {
                "DIFFÉRENTES"
            },
            run.tokens.len(),
            case.tokens.len(),
            run.text(),
        );
        if let (Some(g), false) = (&run.mel, mel == Some(expected_bits(&case.mel))) {
            eprintln!(
                "  mel : {:?} Δ(moyenne, |moyenne|) ({:e}, {:e})",
                g.0,
                (g.1 - case.mel.mean).abs(),
                (g.2 - case.mel.mean_abs).abs()
            );
        }
        for ((step, g), e) in run.adapter.iter().zip(&case.adapter) {
            if *step != e.step || bits(g) != expected_bits(&e.stats) {
                eprintln!(
                    "  adaptateur, pas {step} contre {} : {:?} Δ(moyenne, |moyenne|) ({:e}, {:e})",
                    e.step,
                    g.0,
                    (g.1 - e.stats.mean).abs(),
                    (g.2 - e.stats.mean_abs).abs()
                );
            }
        }
        for (split, run) in fixture.splits.iter().zip(&got.splits) {
            eprintln!(
                "  découpage {} : {} jetons, {:?}",
                split.name,
                run.tokens.len(),
                run.text()
            );
        }
    }
    (fixture, measured)
}

/// P2 : chaque entrée, du mel au texte, tranche par tranche et pas par pas,
/// à l'identique ; puis chaque autre découpage, jetons et texte.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
fn parity() {
    let (fixture, measured) = run_fixture();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        let run = &got.reference;
        assert_eq!(
            run.mel.as_ref().map(bits),
            Some(expected_bits(&case.mel)),
            "{name} : mel"
        );
        assert_eq!(run.adapter.len(), case.adapter.len(), "{name} : adaptateur");
        for ((step, g), e) in run.adapter.iter().zip(&case.adapter) {
            assert_eq!(*step, e.step, "{name} : pas de la tranche");
            assert_eq!(
                bits(g),
                expected_bits(&e.stats),
                "{name} : adaptateur, pas {step}"
            );
        }
        assert_eq!(run.steps.len(), case.steps, "{name} : pas");
        assert_eq!(run.deltas(), case.deltas, "{name} : pas par pas");
        assert_eq!(
            run.finish.text, case.finish_text,
            "{name} : texte de finish"
        );
        assert_eq!(
            run.finish.tokens, case.finish_tokens,
            "{name} : jetons de finish"
        );
        assert_eq!(run.tokens, case.tokens, "{name} : jetons");
        assert_eq!(run.finished, case.finished, "{name} : fin du flux");
        assert_eq!(
            run.session_text, case.session_text,
            "{name} : texte de la session"
        );
        assert_eq!(run.text(), case.text, "{name} : texte");
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
            assert_eq!(run.text(), expected.text, "{name}, {} : texte", split.name);
        }
    }
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT, à l'octet. Pour chaque entrée : le
/// mel identique à l'octet (forme, moyenne, moyenne absolue), le nombre de
/// pas, le nombre de jetons à 10 % près de la fixture (arrondi vers le bas)
/// et le texte, pour chaque découpage aussi. L'encodeur n'est pas comparé
/// (les noyaux livrés arrondissent autrement, R54).
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
fn shipped_smoke() {
    let (fixture, measured) = run_fixture();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        let run = &got.reference;
        assert_eq!(
            run.mel.as_ref().map(bits),
            Some(expected_bits(&case.mel)),
            "{name} : mel à l'octet"
        );
        assert_eq!(run.steps.len(), case.steps, "{name} : pas");
        let (tokens, expected) = (run.tokens.len(), case.tokens.len());
        let bound = expected / 10;
        assert!(
            tokens.abs_diff(expected) <= bound,
            "{name} : {tokens} jetons contre {expected}, au-delà de la borne de {bound}"
        );
        assert_eq!(run.text(), case.text, "{name} : texte");
        for (split, run) in fixture.splits.iter().zip(&got.splits) {
            assert_eq!(run.text(), case.text, "{name}, {} : texte", split.name);
        }
    }
}

// ---------- le flux du reconnaisseur ----------

/// « Même texte quel que soit le découpage des `feed` » : par le vrai
/// `StreamingRecognizer` (`VoxtralMlx`, ce que la dictée appelle), chaque
/// entrée donnée d'un coup, par tranches de 480 ms, par pas de 80 ms et par
/// morceaux irréguliers, plus petits qu'un pas compris, rend le texte de la
/// fixture à `finish`. Les morceaux rendus par `feed`, bout à bout, sont un
/// début du texte de la session (l'app les ajoute l'un à l'autre).
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_text_does_not_depend_on_how_the_audio_is_fed() {
    let fixture = read_fixture();
    let _serial = serial();
    let recognizer = VoxtralMlx::new();
    recognizer.load(&model_dir()).unwrap();
    let splits: [(&str, &[usize]); 5] = [
        ("entier", &[]),
        ("tranches_480ms", &[7_680]),
        ("pas_80ms", &[1_280]),
        ("irregulier", &[1, 160, 1_279, 4_000, 37, 9_000, 2]),
        ("petits", &[3, 500, 17, 1_024, 333]),
    ];
    let mut failures = Vec::new();
    for ((name, samples), case) in inputs().iter().zip(&fixture.cases) {
        for (split, sizes) in splits {
            let mut stream = recognizer.open_stream("fr").unwrap();
            let mut fed = String::new();
            for part in pieces(samples, sizes) {
                fed.push_str(&stream.feed(&part).unwrap());
            }
            let text = stream.finish().unwrap();
            let prefix = case.session_text.starts_with(&fed);
            eprintln!("{name}, {split} : {text:?} ; pas à pas, un début : {prefix}");
            if text != case.text {
                failures.push(format!("{name}, {split} : texte {text:?}"));
            }
            if !prefix {
                failures.push(format!("{name}, {split} : {fed:?} n'est pas un début"));
            }
            assert_eq!(stream.feed(&[0.0; 1_280]).unwrap(), "", "après finish");
            assert_eq!(stream.finish().unwrap(), text, "finish de nouveau");
        }
    }
    recognizer.release();
    assert!(failures.is_empty(), "{failures:#?}");
}

/// La langue n'entre nulle part : la dictée française, ouverte en français,
/// en anglais, dans une langue inconnue ou sans langue, rend le même texte.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_language_of_the_dictation_is_ignored() {
    let _serial = serial();
    let recognizer = VoxtralMlx::new();
    recognizer.load(&model_dir()).unwrap();
    let samples = &inputs()[0].1;
    let texts: Vec<String> = ["fr", "en", "xx", ""]
        .iter()
        .map(|language| {
            let mut stream = recognizer.open_stream(language).unwrap();
            for part in samples.chunks(STEP) {
                stream.feed(part).unwrap();
            }
            stream.finish().unwrap()
        })
        .collect();
    recognizer.release();
    eprintln!("{texts:?}");
    assert!(!texts[0].is_empty());
    assert!(texts.iter().all(|t| t == &texts[0]), "{texts:?}");
}

/// À la fin d'un flux, la mémoire revient au plancher d'après la
/// préparation, et le cache de MLX est vide : `finish`, puis un flux lâché
/// en route.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_mlx_cache_is_emptied_when_a_stream_ends() {
    let _serial = serial();
    let recognizer = VoxtralMlx::new();
    recognizer.load(&model_dir()).unwrap();
    recognizer.warm_up().unwrap();
    let floor = crate::compute::mlx::memory().unwrap();
    let samples = &inputs()[2].1;
    let mut stream = recognizer.open_stream("fr").unwrap();
    for part in samples.chunks(STEP) {
        stream.feed(part).unwrap();
    }
    let during = crate::compute::mlx::memory().unwrap();
    stream.finish().unwrap();
    let after = crate::compute::mlx::memory().unwrap();
    let mut dropped = recognizer.open_stream("fr").unwrap();
    for part in samples[..80_000].chunks(STEP) {
        dropped.feed(part).unwrap();
    }
    drop(dropped);
    let after_drop = crate::compute::mlx::memory().unwrap();
    recognizer.release();
    let mib = |b: usize| b as f64 / (1 << 20) as f64;
    eprintln!(
        "plancher {:.1} Mio (cache {:.1}) ; pendant {:.1} (cache {:.1}) ; après finish {:.1} (cache {:.1}) ; après abandon {:.1} (cache {:.1})",
        mib(floor.active),
        mib(floor.cache),
        mib(during.active),
        mib(during.cache),
        mib(after.active),
        mib(after.cache),
        mib(after_drop.active),
        mib(after_drop.cache)
    );
    assert_eq!(after.cache, 0, "cache après finish");
    assert_eq!(after_drop.cache, 0, "cache après abandon");
    assert_eq!(after.active, floor.active, "mémoire active après finish");
    assert_eq!(
        after_drop.active, floor.active,
        "mémoire active après abandon"
    );
}

// ---------- fumée (VoxtralSmokeTests.swift) ----------

/// `testStreamsHalfASecondOfSilenceThenFinishesWithoutCrashing` : le modèle
/// se charge d'un dossier local ; une session reçoit 0,5 s de silence par
/// blocs de 80 ms et se termine sans planter ; après `finish`, un bloc de
/// plus n'est plus lu.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn streams_half_a_second_of_silence_then_finishes_without_crashing() {
    let _serial = serial();
    let held = load();
    let after = held
        .with(|model| {
            let mut session = Session::new(model, DELAY_MS, MAX_TOKENS);
            let silence = [0.0f32; 8_000];
            for block in silence.chunks(STEP) {
                session.step(model, block, &mut |_| Ok(()))?;
            }
            session.finish(model, &mut |_| Ok(()))?;
            session.step(model, &[0.0; STEP], &mut |_| Ok(()))
        })
        .unwrap();
    assert_eq!(after, Delta::default(), "finish scelle le flux");
}

/// La préparation passe par un flux entier, puis un flux s'ouvre encore ;
/// libéré, le modèle refuse d'en ouvrir un, et un flux resté ouvert échoue
/// au lieu de lire.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn warm_up_then_release_closes_the_open_streams() {
    let _serial = serial();
    let recognizer = VoxtralMlx::new();
    recognizer.load(&model_dir()).unwrap();
    recognizer.warm_up().unwrap();
    assert!(recognizer.memory().is_some());
    let mut stream = recognizer.open_stream("fr").unwrap();
    assert_eq!(stream.feed(&[0.0; STEP]).unwrap(), "");
    recognizer.release();
    assert!(recognizer.memory().is_none());
    assert!(recognizer.open_stream("fr").is_err());
    let err = stream.feed(&[0.0; STEP]).unwrap_err().to_string();
    assert!(err.contains("Voxtral"), "{err}");
}
