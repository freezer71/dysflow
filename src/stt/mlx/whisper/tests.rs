//! Les essais de Whisper en MLX. Ceux qui lancent le vrai modèle sont
//! ignorés par défaut : `DYSFLOW_MODELS` désigne un dossier qui contient
//! `whisper_tiny_4bit/`, `whisper_small_4bit/` et
//! `whisper_large_v3_turbo_4bit/`, et ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh`
//! (les noyaux Metal du Swift, voir l'en-tête du script et celui des essais
//! de Parakeet) : caractéristiques et sortie de l'encodeur identiques à
//! l'octet (forme, moyenne, moyenne absolue), et chaque invite, jeton et
//! texte le sien. `stages` le vérifie élément par élément. Le build livré a
//! sa garde, `shipped_smoke` (`--livre`).

use super::model::{Whisper, WindowText};
use super::*;
use crate::stt::mlx::parakeet::model::stats;
use crate::stt::mlx::parakeet::tests::{
    cycled, inputs as parakeet_inputs, model_dir_with, serial, sha256,
};
use crate::stt::mlx::shipped;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Le dossier de `id` sous `DYSFLOW_MODELS` ; sans lui, l'essai échoue.
fn model_dir(id: &str) -> PathBuf {
    model_dir_with(id, model::TIKTOKEN_FILE)
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
struct Window {
    mel: Stats,
    encoder: Stats,
    tokens: Vec<i64>,
}

#[derive(serde::Deserialize)]
struct Case {
    input: String,
    language: String,
    prompt: Vec<i64>,
    windows: Vec<Window>,
    text: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    model: String,
    revision: String,
    encoder_dtype: String,
    quantization: String,
    inputs: Vec<Input>,
    cases: Vec<Case>,
}

/// Les entrées de Parakeet, plus `trente_cinq_secondes` (deux fenêtres),
/// construites comme le générateur Swift.
fn inputs() -> Vec<(&'static str, Vec<f32>)> {
    let mut inputs = parakeet_inputs();
    let long = cycled(&[&inputs[0].1, &inputs[1].1], 560_000);
    inputs.push(("trente_cinq_secondes", long));
    inputs
}

type Stat = (Vec<i32>, f32, f32);

/// Ce que le Rust rend pour une entrée, copié hors du fil MLX.
struct Measured {
    stages: Vec<(Stat, Stat)>,
    windows: Vec<WindowText>,
    text: String,
}

/// Comme le générateur : chaque fenêtre par ses étages, puis le passage.
fn measure(whisper: &Whisper, samples: &[f32], language: &str) -> anyhow::Result<Measured> {
    let mut stages = Vec::new();
    for window in audio::windows(samples) {
        let features = audio::features(window, whisper.config.num_mel_bins)?;
        features.eval()?;
        let encoded = whisper.encoder.forward(&features)?;
        encoded.eval()?;
        stages.push((stats(&features)?, stats(&encoded)?));
    }
    let windows = whisper.transcribe(samples, language, &|| Ok(()))?;
    let text = Whisper::text(&windows);
    Ok(Measured {
        stages,
        windows,
        text,
    })
}

fn bits(s: &Stat) -> (Vec<i32>, u32, u32) {
    (s.0.clone(), s.1.to_bits(), s.2.to_bits())
}

fn expected_bits(s: &Stats) -> (Vec<i32>, u32, u32) {
    (s.shape.clone(), s.mean.to_bits(), s.mean_abs.to_bits())
}

fn tokens(m: &Measured) -> usize {
    m.windows.iter().map(|w| w.tokens.len()).sum()
}

fn expected_tokens(c: &Case) -> usize {
    c.windows.iter().map(|w| w.tokens.len()).sum()
}

/// La fixture et ce que le Rust rend pour chacune de ses entrées, après les
/// vérifications communes (modèle, révision, quantification, entrées
/// identiques à celles du Swift). Chaque mesure est imprimée avant toute
/// assertion.
/// Le plafond de jetons d'une fenêtre, d'après la `Config` chargée, comme
/// `Whisper::transcribe_window` : `max_tokens`, au plus les positions
/// restantes après l'invite, au moins 1.
struct Caps {
    max_tokens: i32,
    positions: i32,
}

impl Caps {
    fn window(&self, prompt: usize) -> usize {
        let prompt = i32::try_from(prompt).unwrap_or(i32::MAX);
        let cap = self.max_tokens.min(self.positions - prompt - 1).max(1);
        usize::try_from(cap).unwrap_or(0)
    }
}

fn run_fixture(id: &str) -> (Fixture, Vec<Measured>, Caps) {
    let path = repo().join(format!("tests/fixtures/mlx/{id}.json"));
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(fixture.model, id);
    let package = crate::models::CATALOG
        .iter()
        .find(|p| p.id == id)
        .expect("paquet du catalogue");
    assert_eq!(fixture.revision, package.revision, "révision du catalogue");
    assert_eq!(fixture.encoder_dtype, "float32");
    let dir = model_dir(id);
    let loaded = config::Config::load(&dir).unwrap();
    let caps = Caps {
        max_tokens: loaded.max_tokens(),
        positions: loaded.max_target_positions,
    };
    let q = loaded.quantization.expect("4 bits");
    assert_eq!(
        fixture.quantization,
        format!("bits {}, group_size {}", q.bits, q.group_size)
    );

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
    let held = Held::load(move || Whisper::load(&dir)).unwrap();
    let jobs: Vec<(Vec<f32>, String)> = inputs
        .iter()
        .zip(&fixture.cases)
        .map(|((_, samples), case)| (samples.clone(), case.language.clone()))
        .collect();
    let measured = held
        .with(move |whisper| {
            jobs.iter()
                .map(|(samples, language)| measure(whisper, samples, language))
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .unwrap();
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let mel_same = got.stages.len() == case.windows.len()
            && got
                .stages
                .iter()
                .zip(&case.windows)
                .all(|(s, w)| bits(&s.0) == expected_bits(&w.mel));
        let encoder: Vec<(f32, f32)> = got
            .stages
            .iter()
            .zip(&case.windows)
            .map(|(s, w)| {
                (
                    (s.1 .1 - w.encoder.mean).abs(),
                    (s.1 .2 - w.encoder.mean_abs).abs(),
                )
            })
            .collect();
        eprintln!(
            "{id} {} ({}) : mel {} ; encodeur Δ(moyenne, |moyenne|) {encoder:?} ; {} jetons contre {} ; {:?}",
            case.input,
            case.language,
            if mel_same { "identique" } else { "DIFFÉRENT" },
            tokens(got),
            expected_tokens(case),
            got.text,
        );
    }
    (fixture, measured, caps)
}

/// P2 : chaque entrée, chaque fenêtre, du mel au texte, à l'identique.
fn parity(id: &str) {
    let (fixture, measured, _) = run_fixture(id);
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.windows.len(), case.windows.len(), "{name} : fenêtres");
        for (i, ((stage, window), expected)) in got
            .stages
            .iter()
            .zip(&got.windows)
            .zip(&case.windows)
            .enumerate()
        {
            assert_eq!(
                bits(&stage.0),
                expected_bits(&expected.mel),
                "{name} {i} : mel"
            );
            assert_eq!(
                bits(&stage.1),
                expected_bits(&expected.encoder),
                "{name} {i} : encodeur"
            );
            assert_eq!(window.prompt, case.prompt, "{name} {i} : invite");
            assert_eq!(window.tokens, expected.tokens, "{name} {i} : jetons");
        }
        assert_eq!(got.text, case.text, "{name} : texte");
    }
}

/// La garde de fumée du build livré : pour chaque entrée, le mel identique à
/// l'octet (forme, moyenne, moyenne absolue) ; sur la parole (R69,
/// `stt::mlx::shipped`), le nombre de pas du décodeur (jetons gardés) à
/// 10 % près de la fixture, arrondi vers le bas, et le texte de la fixture ;
/// sans parole, la raison d'arrêt de chaque fenêtre égale à celle du Swift
/// (R70) : Whisper n'en garde pas de trace, elle est déduite du nombre de
/// jetons de la fenêtre, plafond atteint si et seulement s'il vaut le
/// plafond de `Caps` (la `Config` chargée), des deux côtés. Whisper tiny
/// atteint son plafond sur le silence dans le Swift (R64). L'encodeur n'y
/// est pas comparé : c'est l'affaire de la parité.
fn shipped_smoke(id: &str) {
    let (fixture, measured, caps) = run_fixture(id);
    for (case, got) in fixture.cases.iter().zip(&measured) {
        let name = &case.input;
        assert_eq!(got.stages.len(), case.windows.len(), "{name} : fenêtres");
        for (stage, expected) in got.stages.iter().zip(&case.windows) {
            assert_eq!(
                bits(&stage.0),
                expected_bits(&expected.mel),
                "{name} : mel à l'octet"
            );
        }
        shipped::steps(name, tokens(got), expected_tokens(case), "");
        shipped::text(name, "texte", &got.text, &case.text);
        assert_eq!(got.windows.len(), case.windows.len(), "{name} : fenêtres");
        for (i, (window, expected)) in got.windows.iter().zip(&case.windows).enumerate() {
            let cap = caps.window(case.prompt.len());
            shipped::stop(
                name,
                &format!("fenêtre {i}"),
                shipped::Stop::from_count(window.tokens.len(), cap),
                shipped::Stop::from_count(expected.tokens.len(), cap),
            );
        }
    }
}

/// P2 : la parité avec la fixture du Swift, dans le build de
/// `scripts/mlx-parite.sh` (noyaux du Swift).
mod parity {
    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn whisper_tiny_4bit() {
        super::parity("whisper_tiny_4bit");
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn whisper_small_4bit() {
        super::parity("whisper_small_4bit");
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn whisper_large_v3_turbo_4bit() {
        super::parity("whisper_large_v3_turbo_4bit");
    }
}

/// La garde de fumée du build livré (AOT : le mlx.metallib précompilé de
/// mlx-sys), par `scripts/mlx-parite.sh --livre`. Ce n'est pas la preuve de
/// parité, qui reste celle du build JIT, à l'octet. Whisper est
/// multilingue et sa langue imposée : le texte est exigé sur toutes les
/// entrées, françaises comme anglaise.
mod shipped_smoke {
    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn whisper_tiny_4bit() {
        super::shipped_smoke("whisper_tiny_4bit");
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn whisper_small_4bit() {
        super::shipped_smoke("whisper_small_4bit");
    }

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn whisper_large_v3_turbo_4bit() {
        super::shipped_smoke("whisper_large_v3_turbo_4bit");
    }
}

// ---------- fumée (WhisperSmokeTests.swift) ----------

/// Le modèle se charge d'un dossier local, trouve son vocabulaire dans
/// `multilingual.tiktoken` et transcrit une seconde de silence en français :
/// invite de 4 jetons (début, `fr`, transcription, sans horodatage), et la
/// génération trouve une fin avant le plafond.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn transcribes_one_second_of_silence_in_french_without_crashing() {
    let _serial = serial();
    let dir = model_dir("whisper_tiny_4bit");
    let held = Held::load(move || Whisper::load(&dir)).unwrap();
    let (windows, fr, max_tokens) = held
        .with(|whisper| {
            let windows = whisper.transcribe(&[0.0; 16_000], "fr", &|| Ok(()))?;
            let fr = whisper.tokenizer.resolve_language("fr");
            Ok((windows, fr, whisper.config.max_tokens() as usize))
        })
        .unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].prompt.len(), 4);
    assert_eq!(Some(windows[0].prompt[1]), fr, "langue : fr");
    assert!(
        windows[0].tokens.len() < max_tokens,
        "la génération n'a pas trouvé de fin sur du silence ({} jetons)",
        windows[0].tokens.len()
    );
}
