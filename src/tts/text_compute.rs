//! Les moteurs de lecture qui prennent le texte (Qwen3-TTS, MagpieTTS) : le
//! calcul est dans l'app (MLX, Swift), derrière `TextCompute` ; le cœur
//! garde le choix de la voix et de la langue, le repli, la coupe des
//! silences et le minutage estimé (`words: None` : le lecteur répartit la
//! durée selon les lettres de la forme parlée).

use anyhow::{bail, ensure, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::PcmBuffer;
use crate::models::{ModelPackage, Runtime};
use crate::sync::LockExt;
use crate::text::language::Lang;

use super::{is_cancelled, Cancel, Readiness, Synthesis, TtsEngine, TtsParams, TtsSession};

/// Ce que le calcul rend : l'audio mono et sa fréquence (24 000 Hz pour
/// Qwen3, 22 050 pour Magpie ; la lecture rééchantillonne).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechRaw {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Le calcul d'un modèle de lecture à texte, fourni par l'app.
///
/// Un modèle par dossier : la lecture et l'aperçu de la bibliothèque peuvent
/// tenir chacun le leur. `load` et `release` vont par paires, par dossier.
/// Le même dossier peut être chargé deux fois à la fois (l'aperçu et le
/// moteur de lecture sur la même voix) : le calcul compte les `load` par
/// dossier et ne libère qu'au dernier `release`.
/// Les appels arrivent de fils d'arrière-plan et bloquent jusqu'au
/// résultat ; le calcul les sérialise lui-même (un seul verrou MLX).
pub trait TextCompute: Send + Sync {
    fn load(&self, model_dir: &Path, runtime: &str) -> Result<()>;
    /// Une phrase courte, pour compiler ce qui doit l'être.
    fn warm_up(&self, model_dir: &Path) -> Result<()>;
    /// `Err(Cancelled)` quand `cancel` le dit, avant ou en route.
    fn synthesize(&self, model_dir: &Path, text: &str, language: &str, voice: &str, cancel: &Cancel)
        -> Result<SpeechRaw>;
    fn release(&self, model_dir: &Path);
}

/// Le nom du moteur, passé au calcul de l'app avec le dossier.
pub fn runtime_name(runtime: Runtime) -> Option<&'static str> {
    match runtime {
        Runtime::Qwen3TtsMlx => Some("qwen3_tts_mlx"),
        Runtime::MagpieMlx => Some("magpie_mlx"),
        _ => None,
    }
}

/// Sous ce niveau (−50 dBFS), un échantillon est du silence.
const SILENCE: f32 = 0.003_162_3;
/// Le silence gardé de chaque côté de la parole.
const KEEP_SECONDS: f32 = 0.030;
/// Échecs du calcul d'affilée avant le repli, comme Kokoro.
const MAX_COMPUTE_FAILURES: usize = 3;

/// La parole sans le silence de tête et de queue (30 ms gardés de chaque
/// côté). Sans minutage par mot, le surlignage se répartit sur toute la
/// durée du son : un blanc de début le décalerait. Vide : rien d'audible.
pub fn trim_silence(samples: &[f32], sample_rate: u32) -> &[f32] {
    let keep = (sample_rate as f32 * KEEP_SECONDS) as usize;
    let Some(first) = samples.iter().position(|s| s.abs() > SILENCE) else {
        return &samples[..0];
    };
    let last = samples.iter().rposition(|s| s.abs() > SILENCE).unwrap_or(first);
    &samples[first.saturating_sub(keep)..(last + 1 + keep).min(samples.len())]
}

fn check(raw: &SpeechRaw) -> Result<()> {
    ensure!(raw.sample_rate > 0, "sortie invalide : fréquence nulle");
    ensure!(raw.samples.iter().all(|s| s.is_finite()), "sortie invalide : valeur non finie");
    Ok(())
}

struct Inner {
    compute: Arc<dyn TextCompute>,
    package: &'static ModelPackage,
    dir: PathBuf,
    /// Pourquoi la voix est passée en repli, si elle l'est.
    broken: Mutex<Option<String>>,
    fallback: Arc<dyn TtsEngine>,
    failures: AtomicUsize,
    /// La part du calcul est rendue (repli, ou moteur relâché) : une fois.
    released: AtomicBool,
}

impl Inner {
    fn broken(&self) -> Option<String> {
        self.broken.locked().clone()
    }

    fn release_compute(&self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.compute.release(&self.dir);
        }
    }

    /// Compte un échec du calcul ; au troisième d'affilée, le repli.
    fn compute_failed(&self) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= MAX_COMPUTE_FAILURES {
            eprintln!("{} : {failures} échecs du calcul d'affilée, passage en repli", self.package.id);
            *self.broken.locked() = Some(format!("calcul {} en échec", self.package.display_name));
            self.release_compute();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.release_compute();
    }
}

/// Un moteur à texte : une session (le calcul est en série).
pub struct TextComputeEngine {
    inner: Arc<Inner>,
}

impl TextComputeEngine {
    /// Charge le modèle et le prépare (« Bonjour. ») : quelques secondes,
    /// dans le fil de chargement différé. Une préparation ratée rend le
    /// modèle chargé.
    pub fn load(
        compute: Arc<dyn TextCompute>,
        package: &'static ModelPackage,
        dir: &Path,
        fallback: Arc<dyn TtsEngine>,
    ) -> Result<Self> {
        let Some(runtime) = runtime_name(package.runtime) else {
            bail!("{} ne lit pas à partir du texte", package.display_name);
        };
        compute.load(dir, runtime)?;
        if let Err(err) = compute.warm_up(dir) {
            compute.release(dir);
            return Err(err);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                compute,
                package,
                dir: dir.to_path_buf(),
                broken: Mutex::new(None),
                fallback,
                failures: AtomicUsize::new(0),
                released: AtomicBool::new(false),
            }),
        })
    }
}

impl TtsEngine for TextComputeEngine {
    fn name(&self) -> String {
        match self.inner.broken() {
            Some(_) => format!("{} (repli)", self.inner.fallback.name()),
            None => format!("{} · MLX", self.inner.package.display_name),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(TextComputeSession { inner: Arc::clone(&self.inner), fallback: None }))
    }

    fn max_sessions(&self) -> usize {
        1
    }

    fn readiness(&self) -> Readiness {
        match self.inner.broken() {
            Some(reason) => Readiness::Fallback(reason),
            None => Readiness::Ready,
        }
    }

    /// La tête de phrase d'abord : le premier son arrive plus tôt sur un
    /// modèle lent. En repli, la voix de repli parle d'un seul tenant.
    fn splits_first_clause(&self) -> bool {
        self.inner.broken().is_none()
    }
}

struct TextComputeSession {
    inner: Arc<Inner>,
    fallback: Option<Box<dyn TtsSession>>,
}

impl TextComputeSession {
    fn fallback(&mut self) -> Result<&mut Box<dyn TtsSession>> {
        if self.fallback.is_none() {
            self.fallback = Some(self.inner.fallback.open_session()?);
        }
        Ok(self.fallback.as_mut().expect("ouverte juste au-dessus"))
    }
}

impl TtsSession for TextComputeSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        Ok(self.synthesize_timed(text, &[], params)?.pcm)
    }

    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        self.synthesize_cancellable(text, words, params, &Cancel::never())
    }

    /// L'abandon va au calcul, qui le consulte en route ; il ne compte pas
    /// vers le repli.
    fn synthesize_cancellable(
        &mut self,
        text: &str,
        words: &[String],
        params: &TtsParams,
        cancel: &Cancel,
    ) -> Result<Synthesis> {
        if self.inner.broken().is_some() {
            return self.fallback()?.synthesize_cancellable(text, words, params, cancel);
        }
        cancel.check()?;
        let language = Lang::parse(&params.language).unwrap_or(Lang::Fr).code();
        let family = self.inner.package.family;
        let voice = crate::models::voice_for_language(family, language, Some(params.voice.as_str()));
        let raw = self
            .inner
            .compute
            .synthesize(&self.inner.dir, text, language, voice, cancel)
            .and_then(|raw| check(&raw).map(|()| raw));
        let raw = match raw {
            Ok(raw) => raw,
            Err(err) if is_cancelled(&err) => return Err(err),
            Err(err) => {
                self.inner.compute_failed();
                return Err(err);
            }
        };
        let kept = trim_silence(&raw.samples, raw.sample_rate);
        if kept.is_empty() {
            self.inner.compute_failed();
            bail!("{} n'a rien dit", self.inner.package.display_name);
        }
        self.inner.failures.store(0, Ordering::SeqCst);
        Ok(Synthesis { pcm: PcmBuffer::new(kept.to_vec(), raw.sample_rate, 1), words: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CATALOG;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::AtomicUsize;

    /// Un calcul en mémoire : rend `samples` à `rate`, retient chaque appel.
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<(PathBuf, String, String, String)>>,
        loads: AtomicUsize,
        releases: Mutex<Vec<PathBuf>>,
        samples: Vec<f32>,
        rate: u32,
        fail: bool,
        fail_warm_up: bool,
    }

    impl Fake {
        fn speaking(samples: Vec<f32>, rate: u32) -> Self {
            Self { samples, rate, ..Default::default() }
        }
    }

    impl TextCompute for Fake {
        fn load(&self, _: &Path, _: &str) -> Result<()> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn warm_up(&self, _: &Path) -> Result<()> {
            anyhow::ensure!(!self.fail_warm_up, "préparation ratée");
            Ok(())
        }
        fn synthesize(&self, dir: &Path, text: &str, language: &str, voice: &str, cancel: &Cancel) -> Result<SpeechRaw> {
            cancel.check()?;
            self.calls.locked().push((dir.to_path_buf(), text.into(), language.into(), voice.into()));
            anyhow::ensure!(!self.fail, "calcul en échec");
            Ok(SpeechRaw { samples: self.samples.clone(), sample_rate: self.rate })
        }
        fn release(&self, dir: &Path) {
            self.releases.locked().push(dir.to_path_buf());
        }
    }

    fn package(id: &str) -> &'static ModelPackage {
        CATALOG.iter().find(|p| p.id == id).unwrap()
    }

    fn engine(fake: &Arc<Fake>, id: &str, dir: &str) -> TextComputeEngine {
        let compute: Arc<dyn TextCompute> = Arc::clone(fake) as Arc<dyn TextCompute>;
        TextComputeEngine::load(compute, package(id), Path::new(dir), Arc::new(SilentEngine)).unwrap()
    }

    fn params(voice: &str, language: &str) -> TtsParams {
        TtsParams { voice: voice.into(), language: language.into(), ..TtsParams::default() }
    }

    /// 0,1 s de silence, 0,2 s de son, 0,1 s de silence, à 1 kHz.
    fn padded() -> Vec<f32> {
        let mut s = vec![0.0; 100];
        s.extend(std::iter::repeat_n(0.5, 200));
        s.extend(vec![0.0; 100]);
        s
    }

    #[test]
    fn trims_leading_and_trailing_silence_keeping_30_ms() {
        let samples = padded();
        let kept = trim_silence(&samples, 1_000);
        assert_eq!(kept.len(), 30 + 200 + 30);
        assert_eq!(kept[30], 0.5);
    }

    #[test]
    fn a_quiet_but_audible_signal_is_kept_whole() {
        let samples = vec![0.01; 50];
        assert_eq!(trim_silence(&samples, 1_000).len(), 50);
        assert!(trim_silence(&[0.0; 50], 1_000).is_empty());
    }

    #[test]
    fn the_sentence_is_trimmed_keeps_its_rate_and_has_no_word_timing() {
        let fake = Arc::new(Fake::speaking(padded(), 1_000));
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let s = e.open_session().unwrap().synthesize_timed("Bonjour.", &["bonjour".into()], &params("sofia", "fr")).unwrap();
        assert_eq!(s.pcm.sample_rate, 1_000, "la fréquence du calcul, telle quelle");
        assert_eq!(s.pcm.samples.len(), 30 + 200 + 30, "30 ms gardés de chaque côté");
        assert!(s.words.is_none(), "estimation par lettres");
    }

    #[test]
    fn the_voice_of_another_model_reaches_the_compute_as_the_default() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "qwen3_tts_17b_customvoice_8bit", "/m/q17");
        let mut session = e.open_session().unwrap();
        session.synthesize("Bonjour.", &params("p-fenrir60", "fr-FR")).unwrap();
        session.synthesize("Hello.", &params("", "en")).unwrap();
        session.synthesize("Salut.", &params("eric", "de")).unwrap();
        let calls = fake.calls.locked().clone();
        assert_eq!((calls[0].2.as_str(), calls[0].3.as_str()), ("fr", "vivian"));
        assert_eq!((calls[1].2.as_str(), calls[1].3.as_str()), ("en", "aiden"));
        assert_eq!((calls[2].2.as_str(), calls[2].3.as_str()), ("fr", "eric"), "langue inconnue : le français");
    }

    #[test]
    fn each_engine_speaks_with_its_own_model() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let reading = engine(&fake, "qwen3_tts_17b_customvoice_8bit", "/m/q17");
        let preview = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        preview.open_session().unwrap().synthesize("Aperçu.", &params("leo", "fr")).unwrap();
        drop(preview);
        reading.open_session().unwrap().synthesize("Lecture.", &params("vivian", "fr")).unwrap();
        let calls = fake.calls.locked().clone();
        assert_eq!(calls[0].0, PathBuf::from("/m/magpie"));
        assert_eq!(calls[1].0, PathBuf::from("/m/q17"), "l'aperçu relâché ne touche pas la lecture");
        assert_eq!(*fake.releases.locked(), [PathBuf::from("/m/magpie")]);
    }

    #[test]
    fn dropping_the_engine_releases_its_model_once() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "qwen3_tts_06b_base_8bit", "/m/q06");
        let session = e.open_session().unwrap();
        drop(e);
        assert!(fake.releases.locked().is_empty(), "une session vit encore");
        drop(session);
        assert_eq!(fake.releases.locked().len(), 1);
    }

    #[test]
    fn a_failed_warm_up_releases_what_was_loaded() {
        let fake = Arc::new(Fake { fail_warm_up: true, ..Fake::speaking(vec![0.5], 24_000) });
        let compute: Arc<dyn TextCompute> = Arc::clone(&fake) as Arc<dyn TextCompute>;
        let err = TextComputeEngine::load(compute, package("magpie_tts_357m_8bit"), Path::new("/m/x"), Arc::new(SilentEngine));
        assert!(err.is_err());
        assert_eq!(fake.loads.load(Ordering::SeqCst), 1);
        assert_eq!(fake.releases.locked().len(), 1, "chaque load a son release");
    }

    #[test]
    fn a_cancelled_synthesis_never_counts_as_a_failure() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let mut session = e.open_session().unwrap();
        for _ in 0..5 {
            let err = session
                .synthesize_cancellable("x", &[], &params("sofia", "fr"), &Cancel::when(|| true))
                .unwrap_err();
            assert!(is_cancelled(&err), "{err}");
        }
        assert_eq!(e.readiness(), Readiness::Ready);
    }

    #[test]
    fn three_failures_in_a_row_fall_back_to_the_system_voice() {
        let fake = Arc::new(Fake { fail: true, ..Fake::speaking(vec![0.5; 10], 24_000) });
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let mut session = e.open_session().unwrap();
        for _ in 0..3 {
            assert!(session.synthesize("x", &params("sofia", "fr")).is_err());
        }
        assert!(matches!(e.readiness(), Readiness::Fallback(_)));
        assert!(e.name().contains("(repli)"));
        assert!(!e.splits_first_clause());
        assert_eq!(fake.releases.locked().len(), 1, "le modèle part avec le repli");
    }

    #[test]
    fn a_silent_or_invalid_output_is_a_failure() {
        let silent = Arc::new(Fake::speaking(vec![0.0; 100], 24_000));
        assert!(engine(&silent, "magpie_tts_357m_8bit", "/m/a").open_session().unwrap().synthesize("x", &params("", "fr")).is_err());
        let nan = Arc::new(Fake::speaking(vec![f32::NAN; 10], 24_000));
        assert!(engine(&nan, "magpie_tts_357m_8bit", "/m/b").open_session().unwrap().synthesize("x", &params("", "fr")).is_err());
    }

    #[test]
    fn the_engine_is_named_after_its_package() {
        let fake = Arc::new(Fake::speaking(vec![0.5], 24_000));
        assert_eq!(engine(&fake, "qwen3_tts_06b_base_8bit", "/m/q").name(), "Qwen3 0.6B · voix française · MLX");
        assert_eq!(runtime_name(Runtime::KokoroMlx), None);
    }
}
