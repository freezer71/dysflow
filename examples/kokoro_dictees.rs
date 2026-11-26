//! Les dictées des fixtures de Parakeet (tâche 14b), parlées par Kokoro :
//! `tests/fixtures/mlx/make_dictees.sh` le lance, puis ramène le son à
//! 16 kHz par `afconvert`.
//!
//! Le chemin est celui de `dysflow --output` (`launch::export_audio`) :
//! - la configuration, la voix (`export::apply_voice`) et ses paramètres ;
//! - le document et ses phrases (`DocumentBuilder`) ;
//! - le moteur (`KokoroEngine` : phonèmes par `dysflow-espeak`, ligne de style
//!   par longueur, découpe à 510 phonèmes) ;
//! - `export::render` au débit 1.
//!
//! Un seul écart : chaque passage du calcul prend la graine 7
//! (`MlxBackend::infer_seeded`, celle de `kokoro-parite.json`). La graine
//! du produit vient de l'horloge (`RandomState::new`), et deux rendus y
//! diffèrent (rapport de la tâche 14b).
//!
//! Le son est écrit à 24 kHz en flottant 32 bits, sans quantification : la
//! seule passe en PCM 16 bits est celle d'`afconvert`.
//!
//!   kokoro_dictees render <dossier de Kokoro> <langue> <voix> <texte> <sortie.wav>
//!   kokoro_dictees sha <dictée 16 kHz.wav>
//!
//! `sha` lit le WAV comme les essais (`stt::file::Wav`, PCM 16 / 32 768) et
//! dit le nombre d'échantillons et le SHA-256 de leurs `f32` en petit
//! boutiste : l'empreinte des fixtures.

/// MLX n'existe que sur les Mac Apple Silicon : ailleurs (le contrôle de
/// MSRV en toutes features, sous Linux), l'exemple se compile vide.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod dictees {
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, ensure, Context, Result};
use dysflow::config::Config;
use dysflow::doc::builder::DocumentBuilder;
use dysflow::tts::kokoro::mlx::MlxBackend;
use dysflow::tts::kokoro::phonemes::EspeakProcess;
use dysflow::tts::kokoro::{KokoroBackend, KokoroEngine, KokoroRaw};
use dysflow::tts::{Cancel, Readiness, TtsEngine, TtsSession};
use sha2::{Digest, Sha256};

/// La graine de `kokoro-parite.json` (`MLXRandom.seed(7)` du test Swift).
const SEED: u64 = 7;

/// Le calcul MLX en Rust, chaque passage à la graine `SEED`.
struct Seeded(MlxBackend);

impl KokoroBackend for Seeded {
    fn label(&self) -> &'static str {
        "MLX (graine 7)"
    }

    fn load(&self, model_dir: &Path) -> Result<()> {
        self.0.load(model_dir)
    }

    /// Rien à préparer : le premier passage compile les noyaux.
    fn warm_up(&self) -> Result<()> {
        Ok(())
    }

    fn infer(&self, ids: &[i64], style: &[f32], _: &Cancel) -> Result<KokoroRaw> {
        self.0.infer_seeded(ids, style, SEED, || ())
    }

    fn release(&self) {
        self.0.release();
    }
}

/// Pas de repli : la voix du système ne doit jamais parler à la place de
/// Kokoro dans une fixture.
struct NoFallback;

impl TtsEngine for NoFallback {
    fn name(&self) -> String {
        "aucun repli".into()
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        bail!("Kokoro a échoué : pas de repli pour une fixture")
    }

    fn max_sessions(&self) -> usize {
        1
    }
}

fn render(dir: &Path, language: &str, voice: &str, text: &str, out: &Path) -> Result<()> {
    let model = dir.join("kokoro-v1_0.safetensors");
    ensure!(model.is_file(), "{} absent", model.display());

    // La configuration de `--output` : Kokoro, la voix demandée pour toutes
    // les langues, débit 1.
    let mut config = Config::default();
    config.engine.family = "kokoro".into();
    config.engine.engine = "auto".into();
    config.engine.model_path = model.to_string_lossy().into_owned();
    config.voice.speed = 1.0;
    dysflow::export::apply_voice(&mut config, Some(voice))?;

    let mut builder = DocumentBuilder::with_language(config.code_policy(), language);
    for block in dysflow::doc::blocks::split_blocks(text) {
        builder.push_block(&block);
    }
    builder.finish();
    let language = builder.language().to_string();
    let document = builder.into_document();
    ensure!(!document.sentences.is_empty(), "aucun contenu lisible");
    let params = config.tts_params_in(&language);
    ensure!(
        params.voice == voice,
        "voix retenue {} au lieu de {voice}",
        params.voice
    );

    let engine = KokoroEngine::load(
        Arc::new(Seeded(MlxBackend::new())),
        Box::new(EspeakProcess::locate()?),
        dir,
        Arc::new(NoFallback),
    )?;
    let mut session = engine.open_session()?;
    let sentences = document
        .sentences
        .iter()
        .map(|s| (s.text.as_str(), s.trailing_pause_ms));
    let pcm = dysflow::export::render(session.as_mut(), sentences, &params, config.voice.speed)?;
    ensure!(
        matches!(engine.readiness(), Readiness::Ready),
        "Kokoro est passé en repli"
    );
    ensure!(
        (pcm.sample_rate, pcm.channels) == (24_000, 1),
        "format inattendu : {} Hz × {}",
        pcm.sample_rate,
        pcm.channels
    );

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(out, spec).with_context(|| format!("{}", out.display()))?;
    for &s in pcm.samples.iter() {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    eprintln!(
        "kokoro_dictees : {} · voix {voice} · {language} · débit 1 · graine {SEED} · {} phrase(s) · {} échantillons à 24 kHz",
        engine.name(),
        document.sentences.len(),
        pcm.samples.len()
    );
    Ok(())
}

fn sha(path: &Path) -> Result<()> {
    let mut samples = Vec::new();
    for chunk in dysflow::stt::file::Wav::open(path)?.chunks() {
        samples.extend(chunk?);
    }
    let mut hasher = Sha256::new();
    for s in &samples {
        hasher.update(s.to_le_bytes());
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("{} échantillons f32 {digest}", samples.len());
    Ok(())
}

pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["render", dir, language, voice, text, out] => {
            render(&PathBuf::from(dir), language, voice, text, &PathBuf::from(out))
        }
        ["sha", wav] => sha(&PathBuf::from(wav)),
        _ => bail!(
            "usage : kokoro_dictees render <dossier de Kokoro> <langue> <voix> <texte> <sortie.wav>\n        kokoro_dictees sha <dictée 16 kHz.wav>"
        ),
    }
}
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> anyhow::Result<()> {
    dictees::main()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("kokoro_dictees : Mac Apple Silicon seulement");
}
