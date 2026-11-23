//! Garde le modèle de dictée chargé entre deux dictées, et le libère après un
//! temps sans usage (comme la voix, `keep_voice_minutes`).
//!
//! Port de `SpeechModelRunner.swift`. `Holder` enveloppe un `Recognizer` (ou
//! un `StreamingRecognizer`) et en est un lui-même : `prepare` ne charge que
//! si le dossier change, et une dictée garde le modèle par `hold` tant
//! qu'elle dure, si longues que soient ses pauses.
//!
//! Deux verrous. Celui du modèle (`loaded`) est tenu pendant le chargement,
//! la lecture et la libération : une libération attend la lecture en cours.
//! Celui de l'usage (`idle`) ne l'est que le temps de noter l'heure : noter un
//! usage n'attend jamais une lecture. Ordre : `loaded` puis `idle`.

use crate::stt::recognizer::{Hold, Recognizer, RecognizerStream, StreamingRecognizer};
use crate::stt::rolling::{Clock, SystemClock};
use crate::tts::Cancel;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

/// Le plus court délai de libération.
pub const SHORTEST_DELAY: Duration = Duration::from_secs(5);
/// Le délai de libération par défaut : 10 minutes.
pub const DEFAULT_DELAY: Duration = Duration::from_secs(10 * 60);

/// Le délai de libération d'après `keep_voice_minutes`, comme la voix :
/// négatif, jamais de lui-même ; 0, dès la dictée finie (après
/// `SHORTEST_DELAY`) ; sinon ce nombre de minutes sans usage.
pub fn idle_delay(keep_voice_minutes: i64) -> Option<Duration> {
    if keep_voice_minutes < 0 {
        return None;
    }
    let minutes = keep_voice_minutes.min(1_000_000) as u64;
    Some(Duration::from_secs(minutes * 60).max(SHORTEST_DELAY))
}

/// Un modèle gardé : `Recognizer` ou `StreamingRecognizer` comme lui.
pub struct Holder<M: Send + Sync + 'static> {
    inner: Arc<Inner<M>>,
}

struct Inner<M> {
    model: M,
    load: fn(&M, &Path) -> Result<()>,
    unload: fn(&M),
    clock: Arc<dyn Clock>,
    /// Le dossier du modèle chargé ; le verrou du modèle.
    loaded: Mutex<Option<PathBuf>>,
    idle: Mutex<Idle>,
    /// Le temps passé à lire (sous le verrou du modèle), chargements exclus.
    compute: Mutex<Duration>,
}

/// La libération différée : l'heure du dernier usage, les dictées qui gardent
/// le modèle, et un seul fil qui dort jusqu'à l'échéance.
struct Idle {
    delay: Option<Duration>,
    last_use: Duration,
    holds: usize,
    /// Le numéro du fil en cours : un fil remplacé ne fait plus rien.
    armed: u64,
    /// Un fil de numéro `armed` dort.
    sleeping: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn not_loaded() -> anyhow::Error {
    anyhow!(crate::tr!(
        "no dictation model loaded",
        "aucun modèle de dictée chargé"
    ))
}

impl<R: Recognizer + 'static> Holder<R> {
    pub fn new(model: R, release_after: Option<Duration>) -> Self {
        Self::with_clock(model, release_after, Arc::new(SystemClock::new()))
    }

    pub fn with_clock(model: R, release_after: Option<Duration>, clock: Arc<dyn Clock>) -> Self {
        Self::build(model, R::load, R::release, release_after, clock)
    }
}

impl<S: StreamingRecognizer + 'static> Holder<S> {
    pub fn streaming(model: S, release_after: Option<Duration>) -> Self {
        Self::streaming_with_clock(model, release_after, Arc::new(SystemClock::new()))
    }

    pub fn streaming_with_clock(
        model: S,
        release_after: Option<Duration>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::build(model, S::load, S::release, release_after, clock)
    }
}

impl<M: Send + Sync + 'static> Holder<M> {
    fn build(
        model: M,
        load: fn(&M, &Path) -> Result<()>,
        unload: fn(&M),
        release_after: Option<Duration>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let last_use = clock.now();
        Self {
            inner: Arc::new(Inner {
                model,
                load,
                unload,
                clock,
                loaded: Mutex::new(None),
                idle: Mutex::new(Idle {
                    delay: release_after,
                    last_use,
                    holds: 0,
                    armed: 0,
                    sleeping: false,
                }),
                compute: Mutex::new(Duration::ZERO),
            }),
        }
    }

    /// Sans usage pendant ce temps, le modèle est libéré ; `None` : jamais de
    /// lui-même (`idle_delay`).
    pub fn release_after(&self) -> Option<Duration> {
        lock(&self.inner.idle).delay
    }

    /// Un fil qui dort sur l'ancien délai est remplacé : un délai raccourci
    /// libère à temps.
    pub fn set_release_after(&self, delay: Option<Duration>) {
        let mut idle = lock(&self.inner.idle);
        if idle.delay == delay {
            return;
        }
        idle.delay = delay;
        if idle.sleeping {
            idle.sleeping = false;
            self.inner.arm(&mut idle);
        }
    }

    /// Le dossier du modèle chargé. Attend une lecture en cours.
    pub fn loaded_directory(&self) -> Option<PathBuf> {
        lock(&self.inner.loaded).clone()
    }

    /// Le temps de lecture cumulé depuis la création, chargements exclus.
    pub fn compute_time(&self) -> Duration {
        *lock(&self.inner.compute)
    }

    /// Libère le modèle s'il vient de `directory` (la fin d'un essai du
    /// micro) ; un autre, chargé entre-temps par une dictée, reste.
    pub fn release_if_loaded(&self, directory: &Path) {
        let mut loaded = lock(&self.inner.loaded);
        if loaded.as_deref() == Some(directory) {
            self.inner.unload(&mut loaded);
        }
    }

    fn prepare_model(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        let inner = &self.inner;
        inner.touch();
        let mut loaded = lock(&inner.loaded);
        let result = (|| {
            if loaded.as_deref() == Some(model_dir) {
                return Ok(None);
            }
            inner.unload(&mut loaded);
            will_load();
            let start = inner.clock.now();
            (inner.load)(&inner.model, model_dir)?;
            *loaded = Some(model_dir.to_path_buf());
            Ok(Some(inner.clock.now().saturating_sub(start)))
        })();
        // Sous le verrou : une libération qui l'attendait voit cet usage.
        inner.touch();
        result
    }

    /// `body` sur le modèle chargé, sous son verrou ; échoue s'il n'y en a
    /// pas. Compte comme un usage, et comme du calcul.
    fn with_model<T>(&self, body: impl FnOnce(&M) -> Result<T>) -> Result<T> {
        let inner = &self.inner;
        inner.touch();
        let loaded = lock(&inner.loaded);
        let result = if loaded.is_none() {
            Err(not_loaded())
        } else {
            let start = inner.clock.now();
            let result = body(&inner.model);
            *lock(&inner.compute) += inner.clock.now().saturating_sub(start);
            result
        };
        inner.touch();
        drop(loaded);
        result
    }

    /// Libère tout de suite (le modèle choisi supprimé), dictée en cours ou
    /// non ; attend une lecture en cours.
    fn release_now(&self) {
        {
            let mut idle = lock(&self.inner.idle);
            idle.armed += 1;
            idle.sleeping = false;
        }
        let mut loaded = lock(&self.inner.loaded);
        self.inner.unload(&mut loaded);
    }

    fn hold_model(&self) -> Hold {
        lock(&self.inner.idle).holds += 1;
        let inner = self.inner.clone();
        Hold::new(move || {
            let mut idle = lock(&inner.idle);
            idle.holds -= 1;
            idle.last_use = inner.clock.now();
            inner.arm(&mut idle);
        })
    }
}

impl<M: Send + Sync + 'static> Inner<M> {
    /// Sous `loaded`.
    fn unload(&self, loaded: &mut Option<PathBuf>) {
        if loaded.take().is_some() {
            (self.unload)(&self.model);
        }
    }

    /// Note un usage : la libération attend le délai à partir de maintenant.
    fn touch(self: &Arc<Self>) {
        let mut idle = lock(&self.idle);
        idle.last_use = self.clock.now();
        self.arm(&mut idle);
    }

    /// Sous `idle` : lance le fil de libération s'il n'y en a pas, qu'un
    /// délai est réglé et qu'aucune dictée ne garde le modèle.
    fn arm(self: &Arc<Self>, idle: &mut Idle) {
        if idle.delay.is_none() || idle.sleeping || idle.holds > 0 {
            return;
        }
        idle.armed += 1;
        let id = idle.armed;
        let weak = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("dysflow-dictee-liberation".into())
            .spawn(move || idle_loop(weak, id));
        // Sans fil, le modèle reste chargé : jamais libéré à tort.
        idle.sleeping = spawned.is_ok();
    }

    /// Le temps jusqu'à l'échéance ; `None` si le fil `id` doit s'arrêter
    /// (remplacé, plus de délai, ou une dictée garde le modèle).
    fn remaining(&self, id: u64) -> Option<Duration> {
        let mut idle = lock(&self.idle);
        if idle.armed != id {
            return None;
        }
        match idle.delay {
            Some(delay) if idle.holds == 0 => {
                Some((idle.last_use + delay).saturating_sub(self.clock.now()))
            }
            _ => {
                idle.sleeping = false;
                None
            }
        }
    }

    /// Sous le verrou du modèle : un usage noté pendant son attente (en fin
    /// de lecture) repousse la libération, et un fil neuf dort jusqu'à la
    /// nouvelle échéance.
    fn release_if_idle(self: &Arc<Self>, id: u64) {
        let mut loaded = lock(&self.loaded);
        let due = {
            let mut idle = lock(&self.idle);
            if idle.armed != id {
                return;
            }
            idle.sleeping = false;
            let due = idle.holds == 0
                && idle
                    .delay
                    .is_some_and(|delay| self.clock.now().saturating_sub(idle.last_use) >= delay);
            if !due {
                self.arm(&mut idle);
            }
            due
        };
        if due {
            self.unload(&mut loaded);
        }
    }
}

/// Dort jusqu'à l'échéance (dernier usage + délai), se rendort si un usage
/// l'a repoussée, puis libère. Ne garde pas le modèle pendant qu'il dort.
fn idle_loop<M: Send + Sync + 'static>(weak: Weak<Inner<M>>, id: u64) {
    loop {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        let Some(wait) = inner.remaining(id) else {
            return;
        };
        if wait.is_zero() {
            inner.release_if_idle(id);
            return;
        }
        let pause = inner.clock.pause(wait);
        drop(inner);
        std::thread::sleep(pause);
    }
}

impl<R: Recognizer + 'static> Recognizer for Holder<R> {
    fn load(&self, model_dir: &Path) -> Result<()> {
        self.prepare_model(model_dir, &|| {}).map(drop)
    }

    fn warm_up(&self) -> Result<()> {
        self.with_model(|model| model.warm_up())
    }

    /// Échoue si aucun modèle n'est chargé (`prepare` d'abord, ou libéré
    /// entre-temps).
    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String> {
        self.with_model(|model| model.transcribe(samples, language, cancel))
    }

    fn release(&self) {
        self.release_now();
    }

    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        self.prepare_model(model_dir, will_load)
    }

    fn hold(&self) -> Hold {
        self.hold_model()
    }

    fn memory(&self) -> Option<u64> {
        self.inner.model.memory()
    }
}

/// Le flux ouvert n'est pas sous le verrou du modèle : une dictée le garde
/// par `hold`. `release` pendant un flux ouvert est l'affaire du modèle.
impl<S: StreamingRecognizer + 'static> StreamingRecognizer for Holder<S> {
    fn load(&self, model_dir: &Path) -> Result<()> {
        self.prepare_model(model_dir, &|| {}).map(drop)
    }

    fn warm_up(&self) -> Result<()> {
        self.with_model(|model| model.warm_up())
    }

    fn open_stream(&self, language: &str) -> Result<Box<dyn RecognizerStream>> {
        self.with_model(|model| model.open_stream(language))
    }

    fn release(&self) {
        self.release_now();
    }

    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> Result<Option<Duration>> {
        self.prepare_model(model_dir, will_load)
    }

    fn hold(&self) -> Hold {
        self.hold_model()
    }

    fn memory(&self) -> Option<u64> {
        self.inner.model.memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::rolling::ManualClock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Chargements et libérations demandés au modèle (les « vidages du
    /// cache » du Swift).
    #[derive(Default)]
    struct Counts {
        loads: AtomicUsize,
        releases: AtomicUsize,
    }

    impl Counts {
        fn loads(&self) -> usize {
            self.loads.load(Ordering::SeqCst)
        }
        fn releases(&self) -> usize {
            self.releases.load(Ordering::SeqCst)
        }
    }

    /// Rend « <nombre d'échantillons> <langue> ». Le chargement et la
    /// lecture prennent `load_cost` et `read_cost` sur l'horloge simulée.
    struct Echo {
        counts: Arc<Counts>,
        clock: Arc<ManualClock>,
        load_cost: Duration,
        read_cost: Duration,
    }

    impl Recognizer for Echo {
        fn load(&self, _model_dir: &Path) -> Result<()> {
            self.counts.loads.fetch_add(1, Ordering::SeqCst);
            self.clock.advance(self.load_cost);
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn transcribe(&self, samples: &[f32], language: &str, _cancel: &Cancel) -> Result<String> {
            self.clock.advance(self.read_cost);
            Ok(format!("{} {language}", samples.len()))
        }
        fn release(&self) {
            self.counts.releases.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct Fixture {
        holder: Holder<Echo>,
        counts: Arc<Counts>,
        clock: Arc<ManualClock>,
    }

    fn fixture_with(
        release_after: Option<Duration>,
        load_cost: Duration,
        read_cost: Duration,
    ) -> Fixture {
        let counts = Arc::new(Counts::default());
        let clock = ManualClock::new();
        let echo = Echo {
            counts: counts.clone(),
            clock: clock.clone(),
            load_cost,
            read_cost,
        };
        Fixture {
            holder: Holder::with_clock(echo, release_after, clock.clone()),
            counts,
            clock,
        }
    }

    fn fixture(release_after: Option<Duration>) -> Fixture {
        fixture_with(release_after, Duration::ZERO, Duration::ZERO)
    }

    fn prepare(holder: &Holder<Echo>, dir: &str) -> Result<Option<Duration>> {
        Recognizer::prepare(holder, Path::new(dir), &|| {})
    }

    fn transcribe(holder: &Holder<Echo>, samples: &[f32]) -> Result<String> {
        holder.transcribe(samples, "fr", &Cancel::never())
    }

    /// Attend (en temps réel, 5 s au plus) que `done` soit vrai : le fil de
    /// libération voit l'horloge simulée en moins d'une milliseconde.
    fn wait_until(done: impl Fn() -> bool) -> bool {
        let start = std::time::Instant::now();
        while !done() {
            if start.elapsed() > Duration::from_secs(5) {
                return false;
            }
            std::thread::sleep(ms(1));
        }
        true
    }

    #[test]
    fn loads_once_then_transcribes() {
        let f = fixture(Some(DEFAULT_DELAY));
        prepare(&f.holder, "/x").unwrap();
        prepare(&f.holder, "/x").unwrap();
        assert_eq!(transcribe(&f.holder, &[0.0, 0.0, 0.0]).unwrap(), "3 fr");
        assert_eq!(f.counts.loads(), 1);
        assert_eq!(f.counts.releases(), 0);
    }

    /// Un vrai chargement est annoncé et chronométré ; un modèle déjà là,
    /// non.
    #[test]
    fn only_a_real_load_is_timed() {
        let f = fixture(Some(DEFAULT_DELAY));
        let announced = AtomicUsize::new(0);
        let announce = || {
            announced.fetch_add(1, Ordering::SeqCst);
        };
        let first = Recognizer::prepare(&f.holder, Path::new("/x"), &announce).unwrap();
        let second = Recognizer::prepare(&f.holder, Path::new("/x"), &announce).unwrap();
        assert!(first.is_some());
        assert!(second.is_none(), "déjà en mémoire : rien à chronométrer");
        assert_eq!(announced.load(Ordering::SeqCst), 1);
    }

    /// Un autre dossier : l'ancien modèle est libéré (une fois), puis le
    /// nouveau chargé.
    #[test]
    fn another_directory_reloads() {
        let f = fixture(Some(DEFAULT_DELAY));
        prepare(&f.holder, "/a").unwrap();
        prepare(&f.holder, "/b").unwrap();
        assert_eq!(f.counts.loads(), 2);
        assert_eq!(f.counts.releases(), 1);
    }

    #[test]
    fn transcribing_before_preparing_fails() {
        let f = fixture(Some(DEFAULT_DELAY));
        assert!(transcribe(&f.holder, &[]).is_err());
    }

    /// Sans usage pendant `release_after`, le modèle part tout seul ;
    /// transcrire échoue ensuite.
    #[test]
    fn the_model_is_released_when_idle() {
        let f = fixture(Some(ms(50)));
        prepare(&f.holder, "/x").unwrap();
        f.clock.advance(ms(50));
        assert!(wait_until(|| f.counts.releases() == 1), "libéré");
        assert!(transcribe(&f.holder, &[]).is_err(), "libéré");
    }

    /// Une dictée en cours garde le modèle (`hold`) : il reste tant qu'elle
    /// dure, si espacées que soient ses lectures, puis part le délai écoulé.
    #[test]
    fn keep_alive_holds_the_model_then_it_goes() {
        let f = fixture(Some(ms(150)));
        prepare(&f.holder, "/x").unwrap();
        let hold = Recognizer::hold(&f.holder);
        for _ in 0..10 {
            f.clock.advance(ms(50));
            std::thread::sleep(ms(2));
        }
        assert_eq!(f.counts.releases(), 0, "libéré pendant la dictée");
        assert!(f.holder.loaded_directory().is_some());
        drop(hold);
        f.clock.advance(ms(150));
        assert!(wait_until(|| f.counts.releases() == 1));
        assert_eq!(f.counts.releases(), 1);
        assert!(f.holder.loaded_directory().is_none());
    }

    /// `keep_voice_minutes`, comme la voix : négatif, jamais ; 0, peu après
    /// (jamais entre deux relectures) ; sinon ces minutes.
    #[test]
    fn the_delay_follows_keep_voice_minutes() {
        assert_eq!(idle_delay(-1), None);
        assert_eq!(idle_delay(0), Some(SHORTEST_DELAY));
        assert_eq!(idle_delay(3), Some(Duration::from_secs(180)));
    }

    /// Le délai change en route : l'usage suivant le prend.
    #[test]
    fn the_delay_can_change() {
        let f = fixture(None);
        assert_eq!(f.holder.release_after(), None);
        f.holder.set_release_after(Some(ms(50)));
        prepare(&f.holder, "/x").unwrap();
        f.clock.advance(ms(50));
        assert!(wait_until(|| f.counts.releases() == 1), "libéré");
    }

    /// `release` (le modèle choisi supprimé) : le modèle part tout de suite.
    #[test]
    fn release_unloads_at_once() {
        let f = fixture(Some(DEFAULT_DELAY));
        prepare(&f.holder, "/x").unwrap();
        Recognizer::release(&f.holder);
        assert_eq!(f.counts.releases(), 1);
        assert!(transcribe(&f.holder, &[]).is_err(), "libéré");
    }

    /// Le temps passé à calculer (sous le verrou), pour le débit de l'essai
    /// du micro : il grandit à chaque transcription, jamais au chargement.
    #[test]
    fn compute_time_adds_up_transcriptions_only() {
        let f = fixture_with(Some(DEFAULT_DELAY), ms(200), ms(50));
        assert_eq!(f.holder.compute_time(), Duration::ZERO);
        prepare(&f.holder, "/x").unwrap();
        assert_eq!(
            f.holder.compute_time(),
            Duration::ZERO,
            "le chargement n'est pas du calcul"
        );
        transcribe(&f.holder, &[]).unwrap();
        transcribe(&f.holder, &[]).unwrap();
        assert!(f.holder.compute_time() >= ms(100));
        assert!(f.holder.compute_time() < ms(190));
    }

    /// Le dossier du modèle chargé : l'essai du micro sait s'il doit
    /// libérer un autre modèle avant de mesurer la mémoire du sien.
    #[test]
    fn the_loaded_directory_is_known() {
        let f = fixture(Some(DEFAULT_DELAY));
        assert_eq!(f.holder.loaded_directory(), None);
        prepare(&f.holder, "/a").unwrap();
        assert_eq!(f.holder.loaded_directory(), Some(PathBuf::from("/a")));
        Recognizer::release(&f.holder);
        assert_eq!(f.holder.loaded_directory(), None);
    }

    /// La fin d'un essai du micro libère son modèle, mais jamais celui
    /// qu'une dictée a chargé entre-temps.
    #[test]
    fn release_if_loaded_only_unloads_that_directory() {
        let f = fixture(Some(DEFAULT_DELAY));
        prepare(&f.holder, "/a").unwrap();
        f.holder.release_if_loaded(Path::new("/b"));
        assert_eq!(f.holder.loaded_directory(), Some(PathBuf::from("/a")));
        assert_eq!(f.counts.releases(), 0);
        f.holder.release_if_loaded(Path::new("/a"));
        assert_eq!(f.holder.loaded_directory(), None);
        assert_eq!(f.counts.releases(), 1);
    }
}
