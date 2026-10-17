//! Moteur de repli fondé sur la synthèse fournie par le système.
//!
//! Il existe pour que le lecteur soit utilisable immédiatement — avant tout
//! téléchargement de modèle. La qualité relève de Kokoro ; ici seuls la voix,
//! le débit et la hauteur sont honorés.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::sync::LockExt;

use super::{PcmBuffer, TtsEngine, TtsParams, TtsSession};

/// Compteur de fichiers temporaires, combiné au PID.
///
/// Deux instances du lecteur lancées en parallèle écrivaient auparavant dans le
/// même chemin et se corrompaient mutuellement.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Supprime son fichier quoi qu'il arrive, y compris sur chemin d'erreur.
struct TempFile(PathBuf);

impl TempFile {
    fn new(extension: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("sillon-{}-{}.{}", std::process::id(), id, extension);
        TempFile(std::env::temp_dir().join(name))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    MacosSay,
    EspeakNg,
}

pub struct SystemEngine {
    backend: Option<Backend>,
}

impl SystemEngine {
    pub fn new() -> Self {
        let backend = if cfg!(target_os = "macos") && which("say") {
            Some(Backend::MacosSay)
        } else if which("espeak-ng") {
            Some(Backend::EspeakNg)
        } else {
            None
        };
        Self { backend }
    }
}

impl Default for SystemEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsEngine for SystemEngine {
    fn name(&self) -> String {
        match self.backend {
            Some(Backend::MacosSay) => "système (say)".to_string(),
            Some(Backend::EspeakNg) => "système (espeak-ng)".to_string(),
            None => "aucun".to_string(),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        let backend = self.backend.ok_or_else(|| {
            anyhow!("aucune synthèse système trouvée (ni `say` ni `espeak-ng`)")
        })?;
        Ok(Box::new(SystemSession { backend }))
    }

    fn max_sessions(&self) -> usize {
        match self.backend {
            // Les `say` se sérialisent de toute façon (voir `SAY_LOCK`) :
            // d'autres workers ne feraient qu'attendre leur tour.
            Some(Backend::MacosSay) => 1,
            // Chaque synthèse est un processus : au-delà de quelques-uns en
            // parallèle, la machine passe son temps à les démarrer.
            _ => 4,
        }
    }
}

struct SystemSession {
    backend: Backend,
}

impl TtsSession for SystemSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        match self.backend {
            Backend::MacosSay => synthesize_say(text, params),
            Backend::EspeakNg => {
                let wav = TempFile::new("wav");
                run_espeak(text, params, wav.path())?;
                decode_wav(wav.path())
            }
        }
    }
}

/// Un seul `say` à la fois dans le processus.
///
/// Mesuré : plusieurs `say` simultanés rendent parfois, avec un code de
/// sortie nul et rien sur stderr, un WAV complet de 128 échantillons muets
/// (5 ms) — 4 % des sorties à 4 en parallèle, 1 % à 2 — ou ne se terminent
/// jamais, le fichier pourtant écrit en entier. Le service de synthèse du
/// système perd ou croise ses avis de fin de parole entre clients.
static SAY_LOCK: Mutex<()> = Mutex::new(());

/// En deçà, une sortie de `say` pour un texte qui a des lettres ou des
/// chiffres n'a rien dit : le mot le plus court (« a ») dure 0,5 s, la
/// sortie vide 5 ms.
const MIN_SPOKEN: Duration = Duration::from_millis(50);

/// Ce que donne un essai de `say` quand il ne réussit pas franchement.
enum SayAttempt {
    Spoken(PcmBuffer),
    /// Raté passager du service de synthèse : un nouvel essai a sa chance.
    Glitch(String),
}

/// `say`, essayé deux fois s'il rate : une phrase sautée en silence est
/// pire qu'une erreur, qui se voit et se dit.
fn synthesize_say(text: &str, params: &TtsParams) -> Result<PcmBuffer> {
    let mut glitch = String::new();
    for _ in 0..2 {
        match say_once(text, params)? {
            SayAttempt::Spoken(pcm) => return Ok(pcm),
            SayAttempt::Glitch(why) => glitch = why,
        }
    }
    Err(anyhow!("`say` a raté deux fois de suite : {glitch}"))
}

fn say_once(text: &str, params: &TtsParams) -> Result<SayAttempt> {
    let wav = TempFile::new("wav");
    {
        let _seul = SAY_LOCK.locked();
        if let Some(glitch) = run_say(text, params, wav.path())? {
            return Ok(SayAttempt::Glitch(glitch));
        }
    }
    let pcm = decode_wav(wav.path())?;
    if has_speakable(text) && pcm.duration < MIN_SPOKEN {
        return Ok(SayAttempt::Glitch(format!("sortie quasi vide ({:?})", pcm.duration)));
    }
    Ok(SayAttempt::Spoken(pcm))
}

/// Vrai quand le texte a de quoi être dit : de la ponctuation seule rend,
/// légitimement, la même sortie muette de 5 ms.
fn has_speakable(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

/// Délai au-delà duquel un `say` est tenu pour bloqué : très au-dessus du
/// temps mesuré (0,75 s de lancement, puis ~10 ms par caractère).
fn say_deadline(text: &str) -> Duration {
    Duration::from_secs(5) + Duration::from_millis(30) * text.chars().count() as u32
}

/// Lance `say` vers `out`. `Ok(Some(_))` : raté passager (bloqué, tué).
fn run_say(text: &str, params: &TtsParams, out: &Path) -> Result<Option<String>> {
    // 175 mots/minute est le débit neutre de `say`. Toujours ce débit : la
    // vitesse s'applique au son à la lecture, comme pour les autres voix.
    let rate = 175u32;
    let mut cmd = Command::new("say");
    cmd.arg("-r").arg(rate.to_string());
    if !params.voice.is_empty() {
        cmd.arg("-v").arg(&params.voice);
    }
    cmd.arg("--data-format=LEF32@24000");
    cmd.arg("-o").arg(out);
    // `--` protège un texte qui commencerait par un tiret.
    cmd.arg("--").arg(text);

    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = cmd.spawn().context("échec du lancement de `say`")?;
    let deadline = Instant::now() + say_deadline(text);
    let status = loop {
        if let Some(status) = child.try_wait().context("attente de `say`")? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Some(format!("bloqué plus de {:?}", say_deadline(text))));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        return Err(anyhow!("`say` a échoué : {}", stderr.trim()));
    }
    Ok(None)
}

fn run_espeak(text: &str, params: &TtsParams, out: &Path) -> Result<()> {
    let words_per_minute = 175u32;
    // espeak attend une hauteur dans 0..99, 50 étant le neutre.
    let pitch = (50.0 + params.pitch_shift * 25.0).clamp(0.0, 99.0) as u32;
    let mut cmd = Command::new("espeak-ng");
    cmd.arg("-s").arg(words_per_minute.to_string());
    cmd.arg("-p").arg(pitch.to_string());
    if !params.voice.is_empty() {
        cmd.arg("-v").arg(&params.voice);
    } else if !params.language.is_empty() {
        cmd.arg("-v").arg(&params.language);
    }
    cmd.arg("-w").arg(out);
    cmd.arg("--").arg(text);

    let output = cmd.output().context("échec du lancement de `espeak-ng`")?;
    if !output.status.success() {
        return Err(anyhow!(
            "`espeak-ng` a échoué : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Décode un WAV en échantillons flottants, quel que soit son format source.
fn decode_wav(path: &Path) -> Result<PcmBuffer> {
    let reader = hound::WavReader::open(path)
        .with_context(|| format!("lecture de {}", path.display()))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .context("décodage des échantillons flottants")?,
        hound::SampleFormat::Int => {
            // Normalise vers [-1, 1] selon la profondeur réelle.
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<Vec<_>, _>>()
                .context("décodage des échantillons entiers")?
        }
    };

    if samples.is_empty() {
        return Err(anyhow!("la synthèse n'a produit aucun échantillon"));
    }
    Ok(PcmBuffer::new(samples, spec.sample_rate, spec.channels))
}

fn which(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_files_are_unique_and_removed() {
        let path = {
            let a = TempFile::new("wav");
            let b = TempFile::new("wav");
            assert_ne!(a.path(), b.path());
            std::fs::write(a.path(), b"x").unwrap();
            assert!(a.path().exists());
            a.path().to_path_buf()
        };
        assert!(!path.exists(), "le fichier temporaire doit être supprimé");
    }

    #[test]
    fn engine_reports_its_backend() {
        let engine = SystemEngine::new();
        assert!(!engine.name().is_empty());
    }

    #[test]
    fn a_near_empty_output_counts_only_for_speakable_text() {
        assert!(has_speakable("Oui."));
        assert!(has_speakable("1"));
        assert!(!has_speakable("…"));
        assert!(!has_speakable(" - . "));
    }

    #[test]
    fn the_deadline_grows_with_the_text() {
        assert!(say_deadline("Oui.") >= Duration::from_secs(5));
        let long = "mot ".repeat(250);
        assert!(say_deadline(&long) > Duration::from_secs(30));
    }

    #[test]
    fn macos_say_runs_one_session_at_a_time() {
        let engine = SystemEngine { backend: Some(Backend::MacosSay) };
        assert_eq!(engine.max_sessions(), 1);
        let engine = SystemEngine { backend: Some(Backend::EspeakNg) };
        assert_eq!(engine.max_sessions(), 4);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_synthesis_produces_audio() {
        let engine = SystemEngine::new();
        let Ok(mut session) = engine.open_session() else {
            return; // Pas de synthèse système sur cette machine.
        };
        let pcm = session
            .synthesize("Bonjour le monde.", &TtsParams::default())
            .expect("synthèse");
        assert!(!pcm.is_empty());
        assert!(pcm.duration.as_millis() > 100, "durée: {:?}", pcm.duration);
        // La durée doit venir du signal, pas d'une estimation.
        let expected = pcm.samples.len() as f64
            / (pcm.sample_rate as f64 * pcm.channels as f64);
        assert!((pcm.duration.as_secs_f64() - expected).abs() < 1e-6);
    }

    /// `say` lancé plusieurs fois en même temps rendait parfois, sans erreur,
    /// un fichier de 128 échantillons muets (5 ms) : la phrase était sautée
    /// sans bruit. Ici, plusieurs sessions synthétisent en parallèle, comme
    /// les workers du lecteur : chaque phrase doit avoir vraiment été dite.
    #[cfg(target_os = "macos")]
    #[test]
    fn parallel_macos_syntheses_never_come_back_empty() {
        let engine = std::sync::Arc::new(SystemEngine::new());
        if engine.open_session().is_err() {
            return; // Pas de synthèse système sur cette machine.
        }
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let engine = std::sync::Arc::clone(&engine);
                std::thread::spawn(move || {
                    let mut session = engine.open_session().expect("session");
                    (0..5)
                        .map(|_| {
                            session
                                .synthesize("Bonjour le monde.", &TtsParams::default())
                                .map(|pcm| pcm.duration)
                                .map_err(|e| e.to_string())
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for thread in threads {
            for outcome in thread.join().expect("thread") {
                let duration = outcome.expect("synthèse");
                assert!(duration.as_millis() > 100, "phrase sautée, durée : {duration:?}");
            }
        }
    }
}

