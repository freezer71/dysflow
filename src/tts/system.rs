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
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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
        let name = format!("sillage-{}-{}.{}", std::process::id(), id, extension);
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
    /// La synthèse du système est cherchée une fois par processus : chaque
    /// moteur (et chaque repli) en construit un.
    pub fn new() -> Self {
        static BACKEND: OnceLock<Option<Backend>> = OnceLock::new();
        let backend = *BACKEND.get_or_init(|| {
            if cfg!(target_os = "macos") && which("say") {
                Some(Backend::MacosSay)
            } else if which("espeak-ng") {
                Some(Backend::EspeakNg)
            } else {
                None
            }
        });
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
    retry_once(|| say_once(text, params))
}

/// Rejoue `attempt` une fois après un raté passager ; une vraie erreur
/// (voix inconnue, `say` en échec) ressort aussitôt, sans nouvel essai.
fn retry_once(mut attempt: impl FnMut() -> Result<SayAttempt>) -> Result<PcmBuffer> {
    let mut glitch = String::new();
    for _ in 0..2 {
        match attempt()? {
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
        let cmd = say_command(text, params, wav.path());
        if let Some(glitch) = run_with_deadline(cmd, say_deadline(text))? {
            return Ok(SayAttempt::Glitch(glitch));
        }
    }
    Ok(judge(text, decode_wav(wav.path())?))
}

/// Une sortie quasi vide pour un texte qui a de quoi être dit est un raté.
fn judge(text: &str, pcm: PcmBuffer) -> SayAttempt {
    if has_speakable(text) && pcm.duration < MIN_SPOKEN {
        return SayAttempt::Glitch(format!("sortie quasi vide ({:?})", pcm.duration));
    }
    SayAttempt::Spoken(pcm)
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

fn say_command(text: &str, params: &TtsParams, out: &Path) -> Command {
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
    cmd
}

/// Lance `cmd` et attend sa fin, au plus `deadline`. `Ok(Some(_))` : raté
/// passager — bloqué, donc tué et ramassé. Un code de sortie non nul est
/// une vraie erreur, avec ce que le programme a dit sur stderr.
///
/// L'attente ne sonde pas : stderr se ferme quand le programme se termine,
/// un fil le lit jusqu'au bout et le rend ; le fil appelant dort jusque-là,
/// ou jusqu'à l'échéance.
fn run_with_deadline(mut cmd: Command, deadline: Duration) -> Result<Option<String>> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("échec du lancement de `{program}`"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut pipe) = child.stderr.take() {
        crate::sync::spawn_named("sillage-stderr", move || {
            let mut stderr = String::new();
            let _ = pipe.read_to_string(&mut stderr);
            let _ = tx.send(stderr);
        });
    }
    let stderr = match rx.recv_timeout(deadline) {
        Ok(stderr) => stderr,
        Err(RecvTimeoutError::Disconnected) => String::new(),
        Err(RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Some(format!("bloqué plus de {deadline:?}")));
        }
    };
    let status = child.wait().with_context(|| format!("attente de `{program}`"))?;
    if !status.success() {
        return Err(anyhow!("`{program}` a échoué : {}", stderr.trim()));
    }
    Ok(None)
}

/// Délai au-delà duquel `espeak-ng` est tenu pour bloqué : il synthétise
/// bien plus vite que le temps réel.
fn espeak_deadline(text: &str) -> Duration {
    Duration::from_secs(10) + Duration::from_millis(30) * text.chars().count() as u32
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

    match run_with_deadline(cmd, espeak_deadline(text))? {
        Some(blocked) => Err(anyhow!("`espeak-ng` {blocked}")),
        None => Ok(()),
    }
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
    use std::time::Instant;

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

    fn spoken() -> PcmBuffer {
        PcmBuffer::new(vec![0.1; 24_000], 24_000, 1)
    }

    /// La sortie ratée de `say` : 128 échantillons muets, 5 ms.
    fn empty() -> PcmBuffer {
        PcmBuffer::new(vec![0.0; 128], 24_000, 1)
    }

    #[test]
    fn one_glitch_then_speech_succeeds_after_one_retry() {
        let mut calls = 0;
        let pcm = retry_once(|| {
            calls += 1;
            Ok(if calls == 1 { SayAttempt::Glitch("vide".into()) } else { SayAttempt::Spoken(spoken()) })
        })
        .expect("le second essai parle");
        assert_eq!(calls, 2);
        assert!(!pcm.is_empty());
    }

    #[test]
    fn two_glitches_are_a_visible_error_not_silence() {
        let mut calls = 0;
        let err = retry_once(|| {
            calls += 1;
            Ok(SayAttempt::Glitch(format!("raté {calls}")))
        })
        .expect_err("jamais de son vide");
        assert_eq!(calls, 2, "un seul nouvel essai");
        assert!(err.to_string().contains("raté 2"), "{err}");
    }

    #[test]
    fn a_real_error_is_not_retried() {
        let mut calls = 0;
        let err = retry_once(|| {
            calls += 1;
            Err(anyhow!("voix inconnue"))
        })
        .expect_err("l'erreur ressort");
        assert_eq!(calls, 1);
        assert!(err.to_string().contains("voix inconnue"));
    }

    #[test]
    fn a_near_empty_output_is_a_glitch_only_when_there_was_something_to_say() {
        assert!(matches!(judge("Bonjour le monde.", empty()), SayAttempt::Glitch(_)));
        assert!(matches!(judge("…", empty()), SayAttempt::Spoken(_)));
        assert!(matches!(judge("Oui.", spoken()), SayAttempt::Spoken(_)));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_zero_exit_is_an_error_with_its_stderr() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("echo 'voix introuvable' >&2; exit 1");
        let err = run_with_deadline(cmd, Duration::from_secs(5)).expect_err("échec");
        assert!(err.to_string().contains("voix introuvable"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_finished_command_is_no_glitch() {
        let outcome = run_with_deadline(Command::new("true"), Duration::from_secs(5)).expect("succès");
        assert!(outcome.is_none());
    }

    /// Un `say` bloqué est tué et ramassé : le processus n'existe plus,
    /// pas même en zombie, et l'essai compte comme raté.
    #[cfg(unix)]
    #[test]
    fn a_blocked_command_is_killed_reaped_and_counts_as_a_glitch() {
        let pid_file = TempFile::new("pid");
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(format!("echo $$ > '{}'; exec sleep 30", pid_file.path().display()));
        let started = Instant::now();
        let outcome = run_with_deadline(cmd, Duration::from_millis(300)).expect("pas une erreur");
        assert!(outcome.is_some_and(|why| why.contains("bloqué")));
        assert!(started.elapsed() < Duration::from_secs(5), "tué à l'échéance");

        let pid = std::fs::read_to_string(pid_file.path()).expect("pid écrit");
        let alive = Command::new("kill")
            .arg("-0")
            .arg(pid.trim())
            .stderr(Stdio::null())
            .status()
            .expect("kill -0")
            .success();
        assert!(!alive, "le processus bloqué doit être tué et ramassé");
    }

    /// Probabiliste et lent (~16 s de vrais `say`) : passé 3 fois sur 3 avant
    /// la correction, les ratés du service étant rares (0 à 6 % des sorties).
    /// Le comportement est couvert ci-dessus de façon déterministe ; à lancer
    /// à la main pour mesurer :
    /// `cargo test --lib parallel_macos_syntheses -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
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
