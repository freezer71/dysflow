//! Le fil MLX. `Array` et `Stream` ne sont pas `Send`, et MLX garde ses flux
//! par fil : tout modèle naît, calcule et meurt sur ce seul fil, qui tient
//! `gpu` pendant chacun de ses travaux, et jusqu'à ce que le GPU ait fini
//! ce qu'ils ont lancé (`async_eval` revient avant).
//!
//! Qui appelle attend le résultat ; il ne doit donc jamais tenir `gpu`
//! (l'appel est alors refusé), et un travail n'attend jamais un autre fil qui
//! calcule ou qui prend `gpu`. Une libération et un changement de plafonds
//! n'attendent pas : ils sont permis partout.

use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{c_int, c_void};
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};

use super::metallib;
use crate::compute::gpu;

const THREAD: &str = "dysflow-mlx";

/// Les modèles, par numéro de case. N'existe que sur le fil MLX.
type Slots = HashMap<u64, Box<dyn Any>>;
/// Un travail, qui rend sa fin : elle passe quand ses flux ont fini, avec
/// l'erreur qu'ils ont pu garder.
type Job = Box<dyn FnOnce(&mut Slots) -> Finish + Send>;
type Finish = Box<dyn FnOnce(Result<()>)>;

thread_local! {
    static ON_WORKER: Cell<bool> = const { Cell::new(false) };
    /// Les noyaux Metal sont chargés (lu et écrit sur le fil MLX seulement).
    static READY: Cell<bool> = const { Cell::new(false) };
}

/// Un modèle possédé par le fil MLX. `Send + Sync` sans `unsafe` : il n'y a
/// ici qu'un numéro de case, la valeur ne quitte jamais son fil.
///
/// Le lâcher ne l'attend pas (un `Drop` peut avoir lieu dans un travail) :
/// le fil MLX le libère ensuite, puis vide le cache. Un modèle calcule sur
/// les flux par défaut du fil (`Stream::gpu()`, `Stream::cpu()` avec
/// `with_stream`) : un flux créé (`with_device` en crée un à chaque appel)
/// reste à jamais, et ajoute ~15 µs à la fin de tous les travaux suivants.
pub struct Held<T: 'static> {
    slot: u64,
    // `fn() -> T` : parle de `T` sans en posséder, donc `Send + Sync`.
    value: PhantomData<fn() -> T>,
}

impl<T: 'static> Held<T> {
    /// Construit la valeur sur le fil MLX et attend. Refusé, sans lancer
    /// `build`, depuis un fil qui tient `gpu` (un travail MLX compris). Un
    /// échec ou une panique de `build` est une erreur, cache vidé.
    pub fn load(build: impl FnOnce() -> Result<T> + Send + 'static) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let slot = NEXT.fetch_add(1, Ordering::Relaxed);
        // Attendu avant d'être rangé : une erreur apparue après `async_eval`
        // ne laisse pas de case sans propriétaire.
        let built = move || build().and_then(|value| settle().map(|()| value));
        call(move |slots| match guarded(built) {
            Ok(value) => {
                slots.insert(slot, Box::new(value));
                Ok(())
            }
            Err(err) => {
                report(guarded(clear_cache));
                Err(err)
            }
        })?;
        Ok(Self { slot, value: PhantomData })
    }

    /// `body` sur le fil MLX, sous `gpu` ; attend son résultat. Refusé, sans
    /// lancer `body`, depuis un fil qui tient `gpu`. Une panique de `body`
    /// est une erreur ; la valeur reste, telle qu'il l'a laissée.
    pub fn with<R: Send + 'static>(
        &self,
        body: impl FnOnce(&mut T) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        let slot = self.slot;
        call(move |slots| {
            let value = slots.get_mut(&slot).and_then(|value| value.downcast_mut::<T>()).ok_or_else(|| {
                anyhow!(crate::tr!("the MLX model is no longer loaded", "le modèle MLX n'est plus chargé"))
            })?;
            body(value)
        })
    }
}

impl<T: 'static> Drop for Held<T> {
    fn drop(&mut self) {
        let slot = self.slot;
        post(Box::new(move |slots| {
            report(guarded(|| {
                drop(slots.remove(&slot));
                Ok(())
            }));
            // Même si la libération a paniqué.
            report(guarded(clear_cache));
            Box::new(report)
        }));
    }
}

/// `work` sur le fil MLX, sans modèle ; attend son résultat. Mêmes refus
/// que `Held::with`.
pub(super) fn run<R: Send + 'static>(work: impl FnOnce() -> Result<R> + Send + 'static) -> Result<R> {
    call(move |_| work())
}

/// `work` sur le fil MLX, sans l'attendre : tout de suite si on y est (le
/// reste du travail en cours le voit), sinon avant tout travail demandé
/// ensuite.
pub(super) fn soon(work: impl FnOnce() -> Result<()> + Send + 'static) {
    let work = move || report(guarded(|| ready().and_then(|()| work())));
    if ON_WORKER.get() {
        work();
    } else {
        post(Box::new(move |_| {
            work();
            Box::new(report)
        }));
    }
}

fn call<R: Send + 'static>(work: impl FnOnce(&mut Slots) -> Result<R> + Send + 'static) -> Result<R> {
    // Le fil MLX tient `gpu` pendant ses travaux : ce seul test couvre aussi
    // l'appel fait depuis un travail.
    if gpu::held_by_this_thread() {
        return Err(anyhow!(crate::tr!(
            "MLX cannot be called while holding the GPU lock or from inside an MLX job",
            "MLX ne s'appelle pas en tenant le verrou du GPU ni depuis un travail MLX",
        )));
    }
    let (reply, result) = crossbeam_channel::bounded(1);
    post(Box::new(move |slots| {
        let done = guarded(|| {
            ready()?;
            work(slots)
        });
        // L'erreur du travail prime sur celle de ses flux.
        Box::new(move |settled| {
            let _ = reply.send(done.and_then(|value| settled.map(|()| value)));
        })
    }));
    result
        .recv()
        .map_err(|_| anyhow!(crate::tr!("the MLX thread did not answer", "le fil MLX n'a pas répondu")))?
}

/// Avant le premier appel MLX. Un échec n'est pas retenu : le travail
/// suivant réessaie.
fn ready() -> Result<()> {
    if !READY.get() {
        metallib::install()?;
        READY.set(true);
    }
    Ok(())
}

/// Attend que Metal ait lâché les tampons de tous les flux, puis vide le
/// cache : `eval` revient quand le GPU a fini, un peu avant que le tampon de
/// commandes lâche les siens, qui iraient sinon dans le cache juste après
/// son vidage.
fn clear_cache() -> Result<()> {
    let settled = settle();
    mlx_rs::memory::clear_cache()?;
    settled
}

/// Attend la fin de tout ce qui est parti : `async_eval` (et une erreur ou
/// une panique après lui) laisse du calcul en vol. MLX liste ses flux, et
/// tous sont nés sur ce fil : ceux par défaut du GPU et du CPU comme ceux
/// qu'un travail a créés. Rien à attendre avant `ready`, et rien de MLX n'est
/// alors appelé. Un flux en erreur n'empêche pas d'attendre les autres.
fn settle() -> Result<()> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CStream {
        ctx: *mut c_void,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CStreams {
        ctx: *mut c_void,
    }
    extern "C" {
        // mlx-c, que `mlx-sys` lie ; `mlx-rs` 0.34 ne les expose pas.
        fn mlx_vector_stream_new() -> CStreams;
        fn mlx_vector_stream_size(all: CStreams) -> usize;
        fn mlx_vector_stream_get(res: *mut CStream, all: CStreams, index: usize) -> c_int;
        fn mlx_vector_stream_free(all: CStreams) -> c_int;
        fn mlx_get_streams(res: *mut CStreams) -> c_int;
        fn mlx_stream_new() -> CStream;
        fn mlx_stream_free(stream: CStream) -> c_int;
        fn mlx_synchronize(stream: CStream) -> c_int;
    }

    if !READY.get() {
        return Ok(());
    }
    // SAFETY : signatures de mlx-c 0.7 (`stream.h`, `vector.h`). Le vecteur et
    // chaque flux sont créés ici et rendus une fois. Appelées sur le fil MLX
    // une fois `ready`, qui a installé le gestionnaire d'erreurs de `mlx-rs`
    // (une erreur rend 1).
    let settled = unsafe {
        let mut all = mlx_vector_stream_new();
        let mut settled = mlx_get_streams(&mut all) == 0;
        for index in 0..mlx_vector_stream_size(all) {
            let mut stream = mlx_stream_new();
            settled &= mlx_vector_stream_get(&mut stream, all, index) == 0 && mlx_synchronize(stream) == 0;
            mlx_stream_free(stream);
        }
        mlx_vector_stream_free(all);
        settled
    };
    if !settled {
        return Err(anyhow!(crate::tr!("an MLX stream ended in error", "un flux de MLX a fini en erreur")));
    }
    Ok(())
}

/// Une panique devient une erreur qui garde son message.
fn guarded<R>(work: impl FnOnce() -> Result<R>) -> Result<R> {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|payload| {
        let why = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("?");
        Err(anyhow!(crate::tr!(en = "the MLX job panicked: {}", fr = "le travail MLX a paniqué : {}", why)))
    })
}

/// L'échec d'un travail que personne n'attend.
fn report(done: Result<()>) {
    if let Err(err) = done {
        eprintln!("{}", crate::tr!(en = "dysflow: MLX: {}", fr = "dysflow : MLX : {}", format!("{err:#}")));
    }
}

/// Dépose un travail ; le fil démarre au premier.
fn post(job: Job) {
    static JOBS: OnceLock<Sender<Job>> = OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (sender, jobs) = crossbeam_channel::unbounded();
        crate::sync::spawn_named(THREAD, move || serve(jobs));
        sender
    });
    // Le fil ne s'arrête jamais ; s'il le faisait, `call` verrait sa réponse
    // perdue, et une libération n'a personne à prévenir.
    #[cfg(test)]
    QUEUED.fetch_add(1, Ordering::SeqCst);
    let _ = jobs.send(job);
}

/// Les essais : travaux déposés que le fil MLX n'a pas encore pris.
#[cfg(test)]
static QUEUED: AtomicU64 = AtomicU64::new(0);

/// Les travaux en attente derrière celui qui tourne : un essai sait ainsi,
/// sans attendre au hasard, qu'un appel est rangé dans la file.
#[cfg(test)]
pub(crate) fn queued() -> u64 {
    QUEUED.load(Ordering::SeqCst)
}

fn serve(jobs: Receiver<Job>) {
    // SAFETY : ne change que la priorité du fil appelant. Un refus le laisse
    // à la priorité par défaut, sans autre conséquence.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0);
    }
    ON_WORKER.set(true);
    let mut slots = Slots::new();
    for job in jobs {
        #[cfg(test)]
        QUEUED.fetch_sub(1, Ordering::SeqCst);
        // Entre deux travaux, la copie Swift qui attendait passe.
        gpu::let_waiters_pass();
        // Le seul endroit où le verrou est rendu : après la fin des flux,
        // que le travail ait réussi, échoué ou paniqué. `gpu::run` le rend à
        // toute sortie, et le fil continue.
        let _ = catch_unwind(AssertUnwindSafe(|| {
            gpu::run(|| {
                let finish = catch_unwind(AssertUnwindSafe(|| job(&mut slots)));
                let settled = guarded(settle);
                match finish {
                    Ok(finish) => finish(settled),
                    Err(_) => report(settled),
                }
            })
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute::gpu;
    use crate::compute::mlx::budget;
    use mlx_rs::{with_stream, Array, Stream};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    const PATIENCE: Duration = Duration::from_secs(20);

    /// `body` sur un autre fil, pour qu'un blocage soit un échec et non un
    /// test qui ne finit pas.
    fn without_hanging<R: Send + 'static>(body: impl FnOnce() -> R + Send + 'static) -> R {
        let (done, is_done) = mpsc::channel();
        std::thread::spawn(move || done.send(body()));
        is_done.recv_timeout(PATIENCE).expect("bloqué, ou paniqué")
    }

    /// Une valeur qui dit d'où elle est libérée : le nom du fil, et s'il tient
    /// le verrou.
    struct Probe(mpsc::Sender<(Option<String>, bool)>);

    impl Drop for Probe {
        fn drop(&mut self) {
            let name = std::thread::current().name().map(str::to_string);
            let _ = self.0.send((name, gpu::held_by_this_thread()));
        }
    }

    fn released_on_the_worker(drops: &mpsc::Receiver<(Option<String>, bool)>) {
        let dropped = drops.recv_timeout(PATIENCE).expect("jamais libéré");
        assert_eq!(dropped, (Some(THREAD.to_string()), true));
    }

    #[test]
    fn two_threads_never_compute_at_once() {
        let held = Held::load(|| Ok(0usize)).unwrap();
        let inside = Arc::new(AtomicUsize::new(0));
        let max_inside = Arc::new(AtomicUsize::new(0));
        let passes = AtomicUsize::new(0);
        fn enter(inside: &AtomicUsize, max_inside: &AtomicUsize) {
            max_inside.fetch_max(inside.fetch_add(1, SeqCst) + 1, SeqCst);
            std::thread::sleep(Duration::from_micros(50));
            inside.fetch_sub(1, SeqCst);
        }
        std::thread::scope(|scope| {
            for thread in 0..8 {
                let (held, inside, max_inside, passes) = (&held, &inside, &max_inside, &passes);
                scope.spawn(move || {
                    for _ in 0..50 {
                        if thread % 2 == 0 {
                            gpu::run(|| enter(inside, max_inside));
                        } else {
                            let (inside, max_inside) = (inside.clone(), max_inside.clone());
                            held.with(move |count| {
                                enter(&inside, &max_inside);
                                *count += 1;
                                Ok(())
                            })
                            .unwrap();
                        }
                        passes.fetch_add(1, SeqCst);
                    }
                });
            }
        });
        assert_eq!(max_inside.load(SeqCst), 1);
        assert_eq!(passes.load(SeqCst), 400);
        assert_eq!(held.with(|count| Ok(*count)).unwrap(), 200);
    }

    /// Mesuré sans cela : deux appelants en boucle, et un fil qui attendait
    /// `gpu` (la dictée Swift) n'était pas encore passé au bout de 5 s.
    ///
    /// Dans un processus à part : ce qui est promis est que le fil MLX cède,
    /// pas que le verrou soit équitable entre les autres fils. À côté des
    /// tests de charge, le témoin perdait contre leurs fils (trace : le fil
    /// MLX cède 25 fois sur 25, onze autres fils se disputent le verrou).
    #[test]
    fn a_waiting_thread_passes_between_two_queued_jobs() {
        const ALONE: &str = "DYSFLOW_TEST_ALONE";
        if std::env::var_os(ALONE).is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["compute::mlx::worker::tests::a_waiting_thread_passes_between_two_queued_jobs", "--exact"])
                .env(ALONE, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}");
            return;
        }
        without_hanging(|| {
            const QUEUED: usize = 60;
            let held = Held::load(|| Ok(())).unwrap();
            // Sans céder, le témoin gagne parfois une manche : il les lui
            // faut toutes.
            for round in 0..8 {
                let done = Arc::new(AtomicUsize::new(0));
                let (first_started, has_started) = mpsc::channel();
                let (queued, is_queued) = mpsc::channel();
                let waiter = {
                    let done = done.clone();
                    std::thread::spawn(move || {
                        has_started.recv_timeout(PATIENCE).unwrap();
                        for _ in 0..QUEUED {
                            let done = done.clone();
                            soon(move || {
                                std::thread::sleep(Duration::from_millis(2));
                                done.fetch_add(1, SeqCst);
                                Ok(())
                            });
                        }
                        queued.send(()).unwrap();
                        gpu::run(|| done.load(SeqCst))
                    })
                };
                held.with(move |_| {
                    first_started.send(())?;
                    is_queued.recv_timeout(PATIENCE)?;
                    // Le temps que l'autre fil attende le verrou.
                    std::thread::sleep(Duration::from_millis(20));
                    Ok(())
                })
                .unwrap();
                let passed_after = waiter.join().unwrap();
                assert!(passed_after < QUEUED / 4, "passé après {passed_after} travaux sur {QUEUED} (manche {round})");
            }
        });
    }

    #[test]
    fn a_held_value_need_not_be_send() {
        let held = Held::load(|| Ok(std::rc::Rc::new(6))).unwrap();
        assert_eq!(held.with(|v| Ok(**v)).unwrap(), 6);
    }

    #[test]
    fn a_held_value_is_shared_between_threads() {
        fn shared<T: Send + Sync>(_: &T) {}
        let held = Held::load(|| Ok(Rc::new(6))).unwrap();
        shared(&held);
        let read = std::thread::scope(|scope| scope.spawn(|| held.with(|v| Ok(**v))).join().unwrap());
        assert_eq!(read.unwrap(), 6);
    }

    #[test]
    fn a_nested_call_is_an_error_not_a_deadlock() {
        without_hanging(|| {
            let held = Arc::new(Held::load(|| Ok(1)).unwrap());
            let inner = held.clone();
            assert!(held.with(move |_| inner.with(|v| Ok(*v))).is_err());
            assert!(held.with(|_| Held::load(|| Ok(2)).map(drop)).is_err());
            assert_eq!(held.with(|v| Ok(*v)).unwrap(), 1);
        });
    }

    #[test]
    fn a_caller_holding_the_lock_gets_an_error_and_its_body_never_runs() {
        without_hanging(|| {
            let held = Held::load(|| Ok(1)).unwrap();
            let ran = Arc::new(AtomicUsize::new(0));
            let (in_load, in_with) = (ran.clone(), ran.clone());
            gpu::run(|| {
                let load = Held::load(move || Ok(in_load.fetch_add(1, SeqCst)));
                let with = held.with(move |_| Ok(in_with.fetch_add(1, SeqCst)));
                assert!(load.is_err());
                assert!(with.is_err());
            });
            assert_eq!(held.with(|v| Ok(*v)).unwrap(), 1);
            assert_eq!(ran.load(SeqCst), 0);
        });
    }

    #[test]
    fn a_failing_job_leaves_the_worker_usable() {
        let held = Held::load(|| Ok(1)).unwrap();
        let err = held.with(|_| -> Result<i32> { Err(anyhow!("refus voulu")) }).unwrap_err();
        assert!(err.to_string().contains("refus voulu"));
        assert_eq!(held.with(|v| Ok(*v)).unwrap(), 1);
    }

    /// Une erreur reste elle-même : un abandon doit rester reconnaissable.
    #[test]
    fn the_error_of_a_job_keeps_its_type() {
        let held = Held::load(|| Ok(1)).unwrap();
        let err = held.with(|_| -> Result<i32> { Err(crate::tts::Cancelled.into()) }).unwrap_err();
        assert!(crate::tts::is_cancelled(&err));
    }

    #[test]
    fn a_panicking_job_is_an_error_and_releases_the_lock() {
        without_hanging(|| {
            let held = Held::load(|| Ok(1)).unwrap();
            let err = held.with(|_| -> Result<i32> { panic!("panique voulue") }).unwrap_err();
            assert!(err.to_string().contains("panique voulue"), "{err}");
            let formatted = held.with(|v| -> Result<i32> { panic!("panique {v}") }).unwrap_err();
            assert!(formatted.to_string().contains("panique 1"), "{formatted}");
            gpu::run(|| {});
            assert_eq!(held.with(|v| Ok(*v)).unwrap(), 1);

            let failed = Held::<i32>::load(|| panic!("chargement voulu")).map(drop).unwrap_err();
            assert!(failed.to_string().contains("chargement voulu"), "{failed}");
            gpu::run(|| {});
            assert_eq!(held.with(|v| Ok(*v)).unwrap(), 1);
        });
    }

    #[test]
    fn a_panicking_release_leaves_the_worker_usable() {
        struct Fragile;
        impl Drop for Fragile {
            fn drop(&mut self) {
                panic!("libération voulue");
            }
        }
        without_hanging(|| {
            drop(Held::load(|| Ok(Fragile)).unwrap());
            assert_eq!(Held::load(|| Ok(1)).unwrap().with(|v| Ok(*v)).unwrap(), 1);
            gpu::run(|| {});
        });
    }

    #[test]
    fn a_value_is_built_used_and_dropped_on_the_worker_under_the_lock() {
        let (sender, drops) = mpsc::channel();
        let here = || (std::thread::current().name().map(str::to_string), gpu::held_by_this_thread());
        let on_worker = (Some(THREAD.to_string()), true);
        let held = Held::load(move || Ok((here(), Array::from_slice(&[1.0f32, 2.0], &[2]), Probe(sender)))).unwrap();
        let (built, used, sum) = held
            .with(move |(built, array, _)| {
                let sum = array.sum(None)?;
                sum.eval()?;
                Ok((built.clone(), here(), sum.try_item_exact::<f32>()?))
            })
            .unwrap();
        assert_eq!((built, used, sum), (on_worker.clone(), on_worker, 3.0));
        drop(held);
        released_on_the_worker(&drops);
    }

    #[test]
    fn a_failed_load_drops_what_it_built_on_the_worker() {
        let (sender, drops) = mpsc::channel();
        let load = Held::<i32>::load(move || {
            let _partial = Probe(sender);
            Err(anyhow!("poids tronqués"))
        });
        assert!(load.is_err());
        released_on_the_worker(&drops);
    }

    #[test]
    fn a_drop_inside_a_job_does_not_wait_for_it() {
        without_hanging(|| {
            let (sender, drops) = mpsc::channel();
            let outer = Held::load(|| Ok(1)).unwrap();
            let inner = Held::load(move || Ok(Probe(sender))).unwrap();
            let pending = outer
                .with(move |_| {
                    drop(inner);
                    Ok(drops.try_recv().is_err().then_some(drops))
                })
                .unwrap()
                .expect("libéré pendant le travail qui l'a lâché");
            released_on_the_worker(&pending);
        });
    }

    #[test]
    fn a_drop_under_the_lock_does_not_wait_for_the_worker() {
        without_hanging(|| {
            let (sender, drops) = mpsc::channel();
            let held = Held::load(move || Ok(Probe(sender))).unwrap();
            gpu::run(|| {
                drop(held);
                assert!(drops.try_recv().is_err(), "libéré pendant que le verrou est tenu");
            });
            released_on_the_worker(&drops);
        });
    }

    /// Lâché aussitôt calculé : le GPU a fini, mais Metal tient encore les
    /// tampons jusqu'à la fin de son tampon de commandes. Vidé avant, le
    /// cache les gardait (64 Mo restés, 5 passages sur 100).
    #[test]
    fn a_dropped_value_gives_its_buffers_back() {
        const SIDE: i32 = 2048;
        let bytes = (SIDE as usize).pow(2) * 4;
        let counted = || Ok(mlx_rs::memory::active_memory()? + mlx_rs::memory::cache_memory()?);
        for round in 0..100 {
            let (measured, loaded) = mpsc::channel();
            let held = Held::load(move || {
                let big = mlx_rs::ops::ones::<f32>(&[SIDE, SIDE])?;
                big.eval()?;
                measured.send(counted()?)?;
                Ok(big)
            })
            .unwrap();
            drop(held);
            let loaded = loaded.recv_timeout(PATIENCE).unwrap();
            assert!(loaded >= bytes);
            // Le travail suivant passe après la libération et le vidage du cache.
            // Une marge de moitié : les autres tests gardent de petits tableaux.
            let after = run(counted).unwrap();
            assert!(after + bytes / 2 <= loaded, "{after} après, {loaded} avant (tour {round})");
        }
    }

    /// Le budget posé dans un travail vaut pour la suite de ce travail ; posé
    /// d'un autre fil, pour le travail suivant.
    #[test]
    fn the_budget_reaches_mlx_from_a_job_and_from_outside() {
        const GIB: usize = 1 << 30;
        let need = |memory| budget::Need { memory, cache: 64 << 20 };
        let before = run(|| Ok(mlx_rs::memory::memory_limit()?)).unwrap();

        let inside = run(move || {
            budget::set("test:plafonds", need(96 * GIB));
            Ok(mlx_rs::memory::memory_limit()?)
        })
        .unwrap();
        budget::set("test:plafonds", need(128 * GIB));
        let outside = run(|| Ok(mlx_rs::memory::memory_limit()?)).unwrap();

        budget::remove("test:plafonds");
        run(move || Ok(mlx_rs::memory::set_memory_limit(before).map(drop)?)).unwrap();
        assert!((96 * GIB..128 * GIB).contains(&inside), "{inside}");
        assert!((128 * GIB..160 * GIB).contains(&outside), "{outside}");
    }

    #[test]
    fn the_worker_runs_at_user_initiated_priority() {
        extern "C" {
            fn qos_class_self() -> u32;
        }
        // SAFETY : `qos_class_self` ne lit que le fil appelant.
        let class = run(|| Ok(unsafe { qos_class_self() })).unwrap();
        assert_eq!(class, libc::qos_class_t::QOS_CLASS_USER_INITIATED as u32);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CArray {
        ctx: *mut std::ffi::c_void,
    }

    extern "C" {
        // mlx-c : le tableau est-il calculé, sans l'attendre.
        fn _mlx_array_is_available(res: *mut bool, array: CArray) -> std::ffi::c_int;
    }

    thread_local! {
        /// Sur le fil MLX : les tableaux partis en `async_eval`, gardés le
        /// temps que le témoin les regarde.
        static LAUNCHED: RefCell<Vec<Array>> = const { RefCell::new(Vec::new()) };
    }

    /// `body` dans un processus où ne tourne que le test `name` : ses
    /// tableaux (16 Mio) fausseraient `a_dropped_value_gives_its_buffers_back`,
    /// qui compte la mémoire de tout MLX (0 passage sur 5 à côté de lui), et
    /// le témoin n'a ici aucun autre fil devant lui.
    fn alone(name: &str, body: fn()) {
        const ALONE: &str = "DYSFLOW_TEST_ALONE";
        if std::env::var_os(ALONE).is_some() {
            return body();
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([&format!("compute::mlx::worker::tests::{name}"), "--exact"])
            .env(ALONE, "1")
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}{stderr}");
    }

    /// Une sortie de travail, un calcul : chacun avec son nom.
    type Exit = (&'static str, fn(Launch));
    type Graph = (&'static str, fn() -> Result<Array>);

    /// Ce que le travail donne au témoin.
    #[derive(Clone)]
    struct Launch {
        waiting: mpsc::Sender<()>,
        launched: mpsc::Sender<usize>,
    }

    impl Launch {
        fn start(&self, graph: fn() -> Result<Array>) -> Result<()> {
            let array = graph()?;
            self.waiting.send(())?;
            // Le temps que le témoin attende le verrou.
            std::thread::sleep(Duration::from_millis(5));
            mlx_rs::transforms::async_eval([&array])?;
            self.launched.send(array.as_ptr().ctx as usize)?;
            LAUNCHED.with_borrow_mut(|all| all.push(array));
            Ok(())
        }
    }

    /// Vrai si le calcul lancé par `exit` est fini quand un autre fil obtient
    /// le verrou : c'est ce que verrait la copie Swift de MLX.
    fn finished_when_the_lock_is_free(exit: impl FnOnce(Launch)) -> bool {
        let (waiting, is_waiting) = mpsc::channel();
        let (launched, has_launched) = mpsc::channel();
        let witness = std::thread::spawn(move || {
            is_waiting.recv_timeout(PATIENCE).unwrap();
            gpu::run(|| {
                let array: usize = has_launched.recv_timeout(PATIENCE).unwrap();
                let mut available = false;
                // SAFETY : le tableau vit dans `LAUNCHED` jusqu'au travail du
                // bas, et le fil MLX ne calcule rien tant que ce fil tient le
                // verrou : personne d'autre ne le touche.
                let done = unsafe { _mlx_array_is_available(&mut available, CArray { ctx: array as *mut _ }) };
                assert_eq!(done, 0);
                (array, available)
            })
        });
        exit(Launch { waiting, launched });
        let (array, available) = witness.join().unwrap();
        run(move || {
            LAUNCHED.with_borrow_mut(|all| {
                all.retain(|kept| {
                    let this = kept.as_ptr().ctx as usize == array;
                    if this {
                        // Une erreur du calcul est l'affaire du test.
                        let _ = kept.eval();
                    }
                    !this
                })
            });
            Ok(())
        })
        .unwrap();
        available
    }

    /// Un calcul de quelques dizaines de millisecondes, sur le flux en cours.
    fn long_work(side: i32, steps: usize) -> Result<Array> {
        let ones = mlx_rs::ops::ones::<f32>(&[side, side])?;
        let shrink = Array::from_f32(1.0 / side as f32);
        let mut product = ones.clone();
        for _ in 0..steps {
            product = product.matmul(&ones)?.multiply(&shrink)?;
        }
        Ok(product)
    }

    fn long_on_the_gpu() -> Result<Array> {
        long_work(2048, 8)
    }

    /// `async_eval` revient avant la fin du calcul : le verrou, lui, attend
    /// cette fin, par toute sortie d'un travail.
    #[test]
    fn the_lock_covers_the_gpu_work_whatever_the_way_out_of_the_job() {
        alone("the_lock_covers_the_gpu_work_whatever_the_way_out_of_the_job", every_way_out);
    }

    fn every_way_out() {
        struct Parting(Launch);
        impl Drop for Parting {
            fn drop(&mut self) {
                self.0.start(long_on_the_gpu).unwrap();
            }
        }
        struct PartingBadly(Launch);
        impl Drop for PartingBadly {
            fn drop(&mut self) {
                self.0.start(long_on_the_gpu).unwrap();
                panic!("libération voulue après async_eval");
            }
        }
        let exits: [Exit; 8] = [
            ("succès", |job| run(move || job.start(long_on_the_gpu)).unwrap()),
            ("erreur", |job| {
                let failed = run(move || -> Result<()> {
                    job.start(long_on_the_gpu)?;
                    Err(anyhow!("refus voulu"))
                });
                assert!(failed.unwrap_err().to_string().contains("refus voulu"));
            }),
            ("panique", |job| {
                let failed = run(move || -> Result<()> {
                    job.start(long_on_the_gpu)?;
                    panic!("panique voulue après async_eval")
                });
                assert!(failed.unwrap_err().to_string().contains("panique voulue"));
            }),
            ("chargement", |job| drop(Held::load(move || job.start(long_on_the_gpu)).unwrap())),
            ("chargement en échec", |job| {
                let failed = Held::<()>::load(move || {
                    job.start(long_on_the_gpu)?;
                    Err(anyhow!("poids tronqués"))
                });
                assert!(failed.is_err());
            }),
            ("libération", |job| drop(Held::load(move || Ok(Parting(job))).unwrap())),
            ("libération qui panique", |job| drop(Held::load(move || Ok(PartingBadly(job))).unwrap())),
            ("travail que personne n'attend", |job| soon(move || job.start(long_on_the_gpu))),
        ];
        let early: Vec<&str> =
            exits.into_iter().filter(|(_, exit)| !finished_when_the_lock_is_free(exit)).map(|(name, _)| name).collect();
        assert!(early.is_empty(), "verrou rendu pendant le calcul : {early:?}");
    }

    /// Tout flux où un travail peut laisser un calcul, pas seulement celui
    /// par défaut du GPU.
    #[test]
    fn the_lock_covers_every_stream_a_job_can_compute_on() {
        alone("the_lock_covers_every_stream_a_job_can_compute_on", every_stream);
    }

    fn every_stream() {
        fn on_its_own_stream() -> Result<Array> {
            with_stream(&Stream::new_with_device(&mlx_rs::Device::gpu()), long_on_the_gpu)
        }
        fn on_the_cpu() -> Result<Array> {
            with_stream(&Stream::cpu(), || long_work(1536, 6))
        }
        fn cpu_feeding_the_gpu() -> Result<Array> {
            let slow = on_the_cpu()?;
            Ok(slow.add(Array::from_f32(1.0))?)
        }
        fn gpu_feeding_the_cpu() -> Result<Array> {
            let slow = long_on_the_gpu()?;
            with_stream(&Stream::cpu(), || Ok(slow.add(Array::from_f32(1.0))?))
        }
        let streams: [Graph; 5] = [
            ("GPU par défaut", long_on_the_gpu),
            ("GPU créé par le travail", on_its_own_stream),
            ("CPU", on_the_cpu),
            ("CPU qui nourrit le GPU", cpu_feeding_the_gpu),
            ("GPU qui nourrit le CPU", gpu_feeding_the_cpu),
        ];
        let early: Vec<&str> = streams
            .into_iter()
            .filter(|&(_, graph)| !finished_when_the_lock_is_free(|job| run(move || job.start(graph)).unwrap()))
            .map(|(name, _)| name)
            .collect();
        assert!(early.is_empty(), "verrou rendu pendant le calcul : {early:?}");
    }

    /// Une erreur qui n'arrive qu'après `async_eval` est celle du travail qui
    /// l'a lancé, et le fil reste utilisable.
    #[test]
    fn an_error_after_async_eval_reaches_the_caller() {
        let failed = run(|| {
            let singular = mlx_rs::ops::zeros::<f32>(&[64, 64])?;
            let inverse = with_stream(&Stream::cpu(), || mlx_rs::linalg::inv(&singular))?;
            mlx_rs::transforms::async_eval([&inverse])?;
            Ok(())
        });
        assert!(failed.is_err(), "l'erreur du calcul est perdue");
        assert_eq!(crate::compute::mlx::smoke().unwrap(), 6.0);
    }
}
