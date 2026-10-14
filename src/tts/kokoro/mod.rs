//! Kokoro-82M : phonétisation, voix, minutage des mots et moteur. Le calcul
//! passe par `KokoroBackend` (MLX dans l'app, ONNX Runtime au terminal).

pub mod align;
pub mod backend;
pub mod phonemes;
pub mod voices;
#[cfg(feature = "kokoro-onnx")]
pub mod onnx;

pub use backend::{check, KokoroBackend, KokoroRaw, SharedCompute};

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use self::phonemes::Phonemizer;
use self::voices::{load_voice, VoicePack};
use super::{Cancel, PcmBuffer, Readiness, Synthesis, TtsEngine, TtsParams, TtsSession};
use crate::text::language::Lang;

pub const SAMPLE_RATE: u32 = 24_000;
/// Échantillons par trame de durée.
pub const FRAME: usize = 600;
/// Au-delà, la phrase est coupée à une proposition.
pub const MAX_PHONEMES: usize = 510;
/// Échecs du calcul d'affilée avant que la voix passe en repli.
const MAX_COMPUTE_FAILURES: usize = 3;

/// Le vocabulaire des phonèmes, tiré de `config.json` : un symbole (point de
/// code) par identifiant. Un symbole inconnu est écarté.
#[derive(Debug, Clone)]
pub struct Vocab(HashMap<char, i64>);

impl Vocab {
    pub fn from_config(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("lecture de {}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&text).context("config.json illisible")?;
        let vocab = json["vocab"].as_object().context("config.json sans vocabulaire")?;
        Ok(Self::from_pairs(vocab.iter().filter_map(|(symbol, id)| {
            let mut chars = symbol.chars();
            match (chars.next(), chars.next(), id.as_i64()) {
                (Some(c), None, Some(id)) => Some((c, id)),
                _ => None,
            }
        })))
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (char, i64)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// Les symboles gardés et leurs identifiants, point de code par point
    /// de code : une voyelle nasale est la base puis le tilde.
    pub fn encode(&self, phonemes: &str) -> (Vec<char>, Vec<i64>) {
        phonemes.chars().filter_map(|c| self.0.get(&c).map(|&id| (c, id))).unzip()
    }
}

/// Coupe une suite de phonèmes trop longue : à la dernière fin de
/// proposition avant la limite (pas avant le quart, pour ne jamais laisser
/// un éclat de quelques symboles), sinon au dernier espace, sinon à la
/// limite.
fn split_phonemes(ps: &str, max: usize) -> Vec<String> {
    let mut rest: Vec<char> = ps.chars().collect();
    let mut parts = Vec::new();
    while rest.len() > max {
        let window = &rest[..max];
        let cut = window
            .iter()
            .rposition(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | ',' | '—' | '…'))
            .map(|i| i + 1)
            .filter(|&i| i >= max / 4)
            .or_else(|| window.iter().rposition(|c| *c == ' '))
            .filter(|&i| i > 0)
            .unwrap_or(max);
        parts.push(rest[..cut].iter().collect::<String>().trim().to_string());
        rest.drain(..cut);
    }
    let last = rest.iter().collect::<String>().trim().to_string();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

struct Inner {
    backend: Arc<dyn KokoroBackend>,
    phonemizer: Mutex<Box<dyn Phonemizer>>,
    vocab: Vocab,
    model_dir: PathBuf,
    voices: Mutex<HashMap<String, Arc<VoicePack>>>,
    /// Pourquoi la voix est passée en repli, si elle l'est.
    broken: Mutex<Option<String>>,
    fallback: Arc<dyn TtsEngine>,
    /// Échecs du calcul d'affilée.
    failures: AtomicUsize,
    /// La part du calcul est rendue (repli, ou moteur relâché) : une fois.
    released: AtomicBool,
}

impl Inner {
    fn voice(&self, name: &str) -> Result<Arc<VoicePack>> {
        let mut voices = self.voices.lock().unwrap();
        if let Some(voice) = voices.get(name) {
            return Ok(Arc::clone(voice));
        }
        let voice = Arc::new(load_voice(&self.model_dir, name)?);
        voices.insert(name.to_string(), Arc::clone(&voice));
        Ok(voice)
    }

    fn broken(&self) -> Option<String> {
        self.broken.lock().unwrap().clone()
    }

    /// Rend la part du calcul, une seule fois.
    fn release_compute(&self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.backend.release();
        }
    }

    /// La voix passe en repli : le calcul n'a plus d'usage.
    fn break_down(&self, reason: String) {
        *self.broken.lock().unwrap() = Some(reason);
        self.release_compute();
    }

    /// Compte un échec du calcul ; au troisième d'affilée, le repli.
    fn compute_failed(&self) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= MAX_COMPUTE_FAILURES {
            eprintln!("kokoro : {failures} échecs du calcul d'affilée, passage en repli");
            self.break_down("calcul Kokoro en échec".to_string());
        }
    }
}

impl Drop for Inner {
    /// Au repos : poids et caches du calcul libérés, espeak-ng arrêté.
    fn drop(&mut self) {
        self.release_compute();
        if let Ok(mut phonemizer) = self.phonemizer.lock() {
            phonemizer.stop();
        }
    }
}

/// Le moteur Kokoro : une session (le calcul est en série).
pub struct KokoroEngine {
    inner: Arc<Inner>,
}

impl KokoroEngine {
    /// Lit le vocabulaire et charge le calcul : quelques secondes, appelé
    /// dans le fil de chargement différé.
    pub fn load(
        backend: Arc<dyn KokoroBackend>,
        phonemizer: Box<dyn Phonemizer>,
        model_dir: &Path,
        fallback: Arc<dyn TtsEngine>,
    ) -> Result<Self> {
        let vocab = Vocab::from_config(&model_dir.join("config.json"))?;
        backend.load(model_dir)?;
        Ok(Self::with_parts(backend, phonemizer, vocab, model_dir.to_path_buf(), fallback))
    }

    pub fn with_parts(
        backend: Arc<dyn KokoroBackend>,
        phonemizer: Box<dyn Phonemizer>,
        vocab: Vocab,
        model_dir: PathBuf,
        fallback: Arc<dyn TtsEngine>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend,
                phonemizer: Mutex::new(phonemizer),
                vocab,
                model_dir,
                voices: Mutex::new(HashMap::new()),
                broken: Mutex::new(None),
                fallback,
                failures: AtomicUsize::new(0),
                released: AtomicBool::new(false),
            }),
        }
    }
}

impl TtsEngine for KokoroEngine {
    fn name(&self) -> String {
        match self.inner.broken() {
            Some(_) => format!("{} (repli)", self.inner.fallback.name()),
            None => format!("Kokoro · {}", self.inner.backend.label()),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(KokoroSession { inner: Arc::clone(&self.inner), fallback: None }))
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

    /// En repli, c'est la voix de repli qui parle, d'un seul tenant.
    fn splits_first_clause(&self) -> bool {
        self.inner.broken().is_none()
    }
}

struct KokoroSession {
    inner: Arc<Inner>,
    /// Session de la voix de repli, ouverte au premier besoin.
    fallback: Option<Box<dyn TtsSession>>,
}

impl KokoroSession {
    fn fallback(&mut self) -> Result<&mut Box<dyn TtsSession>> {
        if self.fallback.is_none() {
            self.fallback = Some(self.inner.fallback.open_session()?);
        }
        Ok(self.fallback.as_mut().expect("ouverte juste au-dessus"))
    }
}

impl TtsSession for KokoroSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        Ok(self.synthesize_timed(text, &[], params)?.pcm)
    }

    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        self.synthesize_cancellable(text, words, params, &Cancel::never())
    }

    /// Le test d'abandon va au calcul, à chaque morceau. Un abandon n'est
    /// pas un échec : il ne compte pas vers le repli et ne remet pas le
    /// compte à zéro.
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
        let language = Lang::parse(&params.language).unwrap_or(Lang::Fr).code();
        let voice_name = match params.voice.trim() {
            "" => crate::models::default_voice_for_language("kokoro", language),
            name => name,
        };
        let voice = self.inner.voice(voice_name)?;

        // La phrase et ses mots seuls : un seul aller-retour vers espeak-ng.
        let mut items = Vec::with_capacity(words.len() + 1);
        items.push(text.to_string());
        items.extend(words.iter().cloned());
        let phonemes = match self.inner.phonemizer.lock().unwrap().phonemize(language, &items) {
            Ok(p) => p,
            Err(err) => {
                self.inner.break_down(format!("espeak-ng s'est arrêté ({err})"));
                bail!("phonétisation impossible : {err}");
            }
        };

        let mut samples = Vec::new();
        // Chaque morceau garde ses symboles et ses durées : l'alignement se
        // fait morceau par morceau, sa table bornée par 510 phonèmes.
        let mut chunks: Vec<(Vec<char>, Vec<u32>)> = Vec::new();
        for part in split_phonemes(&phonemes[0], MAX_PHONEMES) {
            let (kept, ids) = self.inner.vocab.encode(&part);
            if ids.is_empty() {
                continue;
            }
            let mut tokens = Vec::with_capacity(ids.len() + 2);
            tokens.push(0);
            tokens.extend(ids);
            tokens.push(0);
            let raw = self
                .inner
                .backend
                .infer(&tokens, voice.row(part.chars().count()), cancel)
                .and_then(|raw| check(&raw, tokens.len()).map(|()| raw));
            let raw = match raw {
                Ok(raw) => raw,
                Err(err) if super::is_cancelled(&err) => return Err(err),
                Err(err) => {
                    self.inner.compute_failed();
                    return Err(err);
                }
            };
            samples.extend_from_slice(&raw.samples);
            chunks.push((kept, raw.durations));
        }
        if samples.is_empty() {
            bail!("phrase sans phonème connu");
        }
        self.inner.failures.store(0, Ordering::SeqCst);
        let timing = if words.is_empty() {
            None
        } else {
            match align::chunked_word_spans(&chunks, &phonemes[1..]) {
                Ok(spans) => Some(spans),
                Err(failure) => {
                    // Aucun texte au journal : seulement la raison chiffrée.
                    eprintln!("kokoro : minutage par mot abandonné ({failure:?})");
                    None
                }
            }
        };
        Ok(Synthesis { pcm: PcmBuffer::new(samples, SAMPLE_RATE, 1), words: timing })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use crate::tts::{Readiness, TtsParams};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Good,
        Short,
        Nan,
        Fail,
        /// Un appel sur deux échoue.
        Flaky,
        /// Consulte le test d'abandon après les durées, comme MLX.
        Abandon,
    }

    /// Deux trames par jeton, du silence de la bonne longueur ; retient les
    /// appels (identifiants, première valeur de la ligne de style).
    struct FakeBackend {
        mode: Mode,
        calls: Mutex<Vec<(Vec<i64>, f32)>>,
        released: AtomicUsize,
        /// Abandons vus par le calcul.
        abandoned: AtomicUsize,
    }

    impl FakeBackend {
        fn new(mode: Mode) -> Arc<Self> {
            Arc::new(Self {
                mode,
                calls: Mutex::new(Vec::new()),
                released: AtomicUsize::new(0),
                abandoned: AtomicUsize::new(0),
            })
        }
    }

    impl KokoroBackend for FakeBackend {
        fn label(&self) -> &'static str {
            "faux"
        }
        fn load(&self, _: &Path) -> Result<()> {
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn infer(&self, ids: &[i64], style: &[f32], cancel: &crate::tts::Cancel) -> Result<KokoroRaw> {
            if self.mode == Mode::Abandon {
                self.abandoned.fetch_add(cancel.is_cancelled() as usize, Ordering::SeqCst);
                cancel.check()?;
            }
            let mut calls = self.calls.lock().unwrap();
            calls.push((ids.to_vec(), style[0]));
            if self.mode == Mode::Flaky && calls.len() % 2 == 1 {
                anyhow::bail!("calcul perdu une fois");
            }
            drop(calls);
            let durations = vec![2u32; ids.len()];
            let mut samples = vec![0.0; ids.len() * 2 * FRAME];
            match self.mode {
                Mode::Good => {}
                Mode::Short => {
                    samples.pop();
                }
                Mode::Nan => samples[0] = f32::NAN,
                Mode::Fail => anyhow::bail!("calcul perdu"),
                Mode::Flaky | Mode::Abandon => {}
            }
            Ok(KokoroRaw { samples, durations })
        }
        fn release(&self) {
            self.released.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Les phonèmes sont le texte lui-même ; `dead` : chaque appel échoue.
    struct FakePhonemizer {
        dead: bool,
        stopped: Arc<AtomicUsize>,
    }

    impl Phonemizer for FakePhonemizer {
        fn phonemize(&mut self, _: &str, items: &[String]) -> Result<Vec<String>> {
            anyhow::ensure!(!self.dead, "espeak-ng ne répond plus");
            Ok(items.to_vec())
        }
        fn stop(&mut self) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn vocab() -> Vocab {
        Vocab::from_pairs(('a'..='z').zip(43..).chain([(' ', 16), (',', 3), ('.', 4)]))
    }

    /// Un dossier de modèle avec une voix « rampe » : la ligne r vaut r.
    fn model_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-kokoro-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("voices")).unwrap();
        let ramp = VoicePack::from_values(
            (0..voices::ROWS * voices::WIDTH).map(|i| (i / voices::WIDTH) as f32).collect(),
        )
        .unwrap();
        std::fs::write(dir.join("voices").join("rampe.safetensors"), ramp.to_safetensors()).unwrap();
        dir
    }

    fn engine(backend: Arc<FakeBackend>, dead: bool, name: &str) -> (KokoroEngine, Arc<AtomicUsize>) {
        let stopped = Arc::new(AtomicUsize::new(0));
        let phonemizer = Box::new(FakePhonemizer { dead, stopped: Arc::clone(&stopped) });
        let engine = KokoroEngine::with_parts(backend, phonemizer, vocab(), model_dir(name), Arc::new(SilentEngine));
        (engine, stopped)
    }

    fn params(voice: &str) -> TtsParams {
        TtsParams { voice: voice.into(), language: "fr".into(), ..TtsParams::default() }
    }

    fn strings(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn a_sentence_is_framed_by_edge_tokens_and_takes_its_style_row() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), false, "bords");
        let mut session = engine.open_session().unwrap();
        let out = session.synthesize_timed("un deux", &strings("un deux"), &params("rampe")).unwrap();
        let calls = backend.calls.lock().unwrap();
        let (ids, style) = &calls[0];
        assert_eq!(ids.first(), Some(&0));
        assert_eq!(ids.last(), Some(&0));
        assert_eq!(ids.len(), 7 + 2, "« un deux » : 7 symboles");
        assert_eq!(*style, 6.0, "ligne de style : 7 phonèmes − 1");
        assert_eq!(out.pcm.sample_rate, SAMPLE_RATE);
        assert_eq!(out.pcm.samples.len(), 9 * 2 * FRAME);
        let words = out.words.expect("minutage");
        assert_eq!(words.len(), 2);
        assert!((words[1].end - out.pcm.duration.as_secs_f32()).abs() < 1e-4);
    }

    #[test]
    fn an_invalid_output_fails_the_sentence() {
        for mode in [Mode::Short, Mode::Nan] {
            let (engine, _) = engine(FakeBackend::new(mode), false, "invalide");
            let mut session = engine.open_session().unwrap();
            assert!(session.synthesize("un deux", &params("rampe")).is_err());
        }
    }

    #[test]
    fn a_sentence_over_510_phonemes_is_cut_at_a_clause_and_glued_back() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), false, "longue");
        let mut session = engine.open_session().unwrap();
        let text = "abcd efgh, ".repeat(60); // 660 symboles
        let words = strings(&text);
        let out = session.synthesize_timed(text.trim(), &words, &params("rampe")).unwrap();
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "deux morceaux");
        assert!(calls.iter().all(|(ids, _)| ids.len() <= MAX_PHONEMES + 2));
        let spans = out.words.expect("minutage recollé");
        assert_eq!(spans.len(), words.len());
        assert!((spans.last().unwrap().end - out.pcm.duration.as_secs_f32()).abs() < 1e-3);
    }

    #[test]
    fn a_long_unpunctuated_sentence_is_cut_at_spaces_and_still_timed() {
        // Plus de 1 500 phonèmes sans une virgule : coupée aux espaces,
        // alignée morceau par morceau (la table reste petite).
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), false, "sans-virgule");
        let mut session = engine.open_session().unwrap();
        let text = "abcd ".repeat(400);
        let words = strings(&text);
        let out = session.synthesize_timed(text.trim(), &words, &params("rampe")).unwrap();
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls.iter().all(|(ids, _)| ids.len() <= MAX_PHONEMES + 2));
        let spans = out.words.expect("minutage");
        assert_eq!(spans.len(), 400);
        assert!((spans.last().unwrap().end - out.pcm.duration.as_secs_f32()).abs() < 1e-3);
    }

    #[test]
    fn the_cut_of_the_first_clause_is_asked_for() {
        let (engine, _) = engine(FakeBackend::new(Mode::Good), false, "coupe");
        assert!(engine.splits_first_clause());
        assert_eq!(engine.max_sessions(), 1);
        assert!(engine.name().contains("Kokoro"));
    }

    #[test]
    fn dropping_the_voice_releases_the_compute_and_stops_espeak() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, stopped) = engine(Arc::clone(&backend), false, "repos");
        let session = engine.open_session().unwrap();
        drop(engine);
        assert_eq!(backend.released.load(Ordering::SeqCst), 0, "une session vit encore");
        drop(session);
        assert_eq!(backend.released.load(Ordering::SeqCst), 1);
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_phonemizer_that_dies_twice_hands_over_to_the_fallback() {
        let (engine, _) = engine(FakeBackend::new(Mode::Good), true, "repli");
        let mut session = engine.open_session().unwrap();
        assert!(session.synthesize("un deux", &params("rampe")).is_err(), "la phrase est sautée");
        assert!(matches!(engine.readiness(), Readiness::Fallback(_)));
        assert!(engine.name().contains("repli"));
        assert!(session.synthesize("trois", &params("rampe")).is_ok(), "la voix de repli parle");
        assert!(!engine.splits_first_clause(), "le repli ne coupe pas");
    }

    #[test]
    fn switching_to_the_fallback_releases_the_compute_once() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), true, "repli-libere");
        let mut session = engine.open_session().unwrap();
        assert!(session.synthesize("un", &params("rampe")).is_err());
        assert_eq!(backend.released.load(Ordering::SeqCst), 1, "les poids partent avec la voix");
        drop(session);
        drop(engine);
        assert_eq!(backend.released.load(Ordering::SeqCst), 1, "pas une seconde fois");
    }

    #[test]
    fn a_compute_that_keeps_failing_hands_over_to_the_fallback() {
        for mode in [Mode::Fail, Mode::Nan] {
            let backend = FakeBackend::new(mode);
            let (engine, _) = engine(Arc::clone(&backend), false, "calcul-perdu");
            let mut session = engine.open_session().unwrap();
            for _ in 0..MAX_COMPUTE_FAILURES {
                assert!(session.synthesize("un deux", &params("rampe")).is_err());
            }
            assert_eq!(engine.readiness(), Readiness::Fallback("calcul Kokoro en échec".into()));
            assert_eq!(backend.released.load(Ordering::SeqCst), 1);
            assert!(session.synthesize("trois", &params("rampe")).is_ok(), "la voix de repli parle");
        }
    }

    #[test]
    fn one_failure_between_successes_is_forgiven() {
        let backend = FakeBackend::new(Mode::Flaky);
        let (engine, _) = engine(Arc::clone(&backend), false, "pardon");
        let mut session = engine.open_session().unwrap();
        for _ in 0..5 {
            assert!(session.synthesize("un", &params("rampe")).is_err());
            assert!(session.synthesize("un", &params("rampe")).is_ok());
        }
        assert_eq!(engine.readiness(), Readiness::Ready);
        assert_eq!(backend.released.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_abandoned_synthesis_is_neither_a_failure_nor_a_fallback() {
        let backend = FakeBackend::new(Mode::Abandon);
        let (engine, _) = engine(Arc::clone(&backend), false, "abandon");
        let mut session = engine.open_session().unwrap();
        let abandon = crate::tts::Cancel::when(|| true);
        for _ in 0..MAX_COMPUTE_FAILURES * 2 {
            let err = session.synthesize_cancellable("un deux", &[], &params("rampe"), &abandon).unwrap_err();
            assert!(crate::tts::is_cancelled(&err), "{err}");
        }
        assert_eq!(backend.abandoned.load(Ordering::SeqCst), MAX_COMPUTE_FAILURES * 2, "le calcul voit l'abandon");
        assert_eq!(engine.inner.failures.load(Ordering::SeqCst), 0, "aucun échec compté");
        assert_eq!(engine.readiness(), Readiness::Ready, "pas de repli");
        assert_eq!(backend.released.load(Ordering::SeqCst), 0);
        let kept = crate::tts::Cancel::when(|| false);
        let out = session.synthesize_cancellable("un deux", &[], &params("rampe"), &kept).unwrap();
        assert!(!out.pcm.is_empty(), "une synthèse utile va au bout");
    }

    #[test]
    fn a_cut_never_leaves_a_sliver() {
        // Une virgule tout au début ne fait pas un morceau d'un symbole.
        let text = format!("a, {}", "bcde ".repeat(150));
        let parts = split_phonemes(&text, MAX_PHONEMES);
        assert!(parts.iter().all(|p| p.chars().count() >= MAX_PHONEMES / 4 || p == parts.last().unwrap()), "{:?}", parts.iter().map(|p| p.len()).collect::<Vec<_>>());
        assert!(parts.iter().all(|p| p.chars().count() <= MAX_PHONEMES));
        assert!(parts[0].starts_with("a, b"));
    }

    #[test]
    fn the_vocabulary_comes_from_the_configuration() {
        let dir = std::env::temp_dir().join(format!("sillon-vocab-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), r#"{"vocab": {"ɔ": 76, "̃": 17, " ": 16}, "n_token": 178}"#).unwrap();
        let vocab = Vocab::from_config(&dir.join("config.json")).unwrap();
        // Les voyelles nasales sont deux symboles : la base et le tilde.
        let (kept, ids) = vocab.encode("ɔ̃ x");
        assert_eq!(kept, ['ɔ', '\u{303}', ' ']);
        assert_eq!(ids, [76, 17, 16], "le x, inconnu, tombe");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_resting_reader_releases_kokoro() {
        use crate::audio::Player;
        use crate::config::Config;
        use crate::reader::Reader;
        let backend = FakeBackend::new(Mode::Good);
        let shared = Arc::clone(&backend);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut reader = Reader::new(
            config,
            Box::new(move |_| {
                let (engine, _) = engine(Arc::clone(&shared), false, "lecteur");
                Arc::new(engine) as Arc<dyn TtsEngine>
            }),
            Box::new(|_: &str| Player::silent()),
            std::time::Instant::now(),
        );
        reader.open_text("Une phrase.", "test", std::time::Instant::now());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while reader.session().is_some_and(|a| a.synth.get(0).is_none()) {
            assert!(std::time::Instant::now() < deadline, "jamais synthétisée");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        reader.rest();
        assert_eq!(backend.released.load(Ordering::SeqCst), 1, "poids et caches libérés au repos");
    }
}
