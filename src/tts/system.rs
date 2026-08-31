//! Moteur de repli fondé sur la synthèse fournie par le système.
//!
//! Il existe pour que le lecteur soit utilisable immédiatement — avant tout
//! téléchargement de modèle, et sur une machine sans CMake. La qualité et les
//! réglages fins (émotion, guidage, échantillonnage) relèvent d'audio.cpp ;
//! ici seuls la voix, le débit et la hauteur sont honorés.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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
        let name = format!("rs-reader-{}-{}.{}", std::process::id(), id, extension);
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
        // Chaque synthèse est un processus : au-delà de quelques-uns en
        // parallèle, la machine passe son temps à les démarrer.
        4
    }
}

struct SystemSession {
    backend: Backend,
}

impl TtsSession for SystemSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        let wav = TempFile::new("wav");
        match self.backend {
            Backend::MacosSay => run_say(text, params, wav.path())?,
            Backend::EspeakNg => run_espeak(text, params, wav.path())?,
        }
        decode_wav(wav.path())
    }
}

fn run_say(text: &str, params: &TtsParams, out: &Path) -> Result<()> {
    // 175 mots/minute est le débit neutre de `say`.
    let rate = (175.0 * params.speaking_rate).clamp(90.0, 720.0) as u32;
    let mut cmd = Command::new("say");
    cmd.arg("-r").arg(rate.to_string());
    if !params.voice.is_empty() {
        cmd.arg("-v").arg(&params.voice);
    }
    cmd.arg("--data-format=LEF32@24000");
    cmd.arg("-o").arg(out);
    // `--` protège un texte qui commencerait par un tiret.
    cmd.arg("--").arg(text);

    let output = cmd.output().context("échec du lancement de `say`")?;
    if !output.status.success() {
        return Err(anyhow!(
            "`say` a échoué : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn run_espeak(text: &str, params: &TtsParams, out: &Path) -> Result<()> {
    let words_per_minute = (175.0 * params.speaking_rate).clamp(80.0, 450.0) as u32;
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
}
