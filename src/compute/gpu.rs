//! Un seul verrou devant le GPU, pour les deux copies de MLX : celle de
//! mlx-swift (`MLXGate.shared`, par `gpu_lock` et `gpu_unlock`) et celle de
//! Rust (`compute::mlx`). Présent partout, même sans la feature `mlx` : l'app
//! l'appelle toujours.
//!
//! Il couvre le calcul jusqu'à sa fin sur le GPU : qui le prend pour calculer
//! attend ses flux avant de le rendre (`async_eval` revient avant cette fin).
//!
//! Sur macOS, un `os_unfair_lock` : qui attend transmet sa priorité à qui
//! tient le verrou. Ce verrou arrête le processus si le fil qui le tient le
//! reprend, ou si un autre le rend : le compte par fil ci-dessous rend les
//! deux gestes sans danger.
//!
//! Il n'est pas équitable : un fil qui le reprend aussitôt rendu passe avant
//! le fil qu'il vient de réveiller. Le fil MLX, qui enchaîne ses travaux,
//! affamait ainsi la copie Swift (deux appelants Rust en boucle : pas un
//! passage en 5 s). Il laisse donc passer qui attend (`let_waiters_pass`).

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

/// Les fils qui attendent le verrou.
static WAITING: AtomicUsize = AtomicUsize::new(0);
/// Les prises du verrou depuis le lancement.
static TURNS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Les prises en cours sur ce fil ; 0 : il ne tient pas le verrou.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Prend le verrou, en attendant s'il le faut. Le fil qui le tient déjà passe
/// sans attendre ; chaque `lock` a son `unlock`.
pub fn lock() {
    let depth = DEPTH.get();
    if depth == 0 {
        WAITING.fetch_add(1, SeqCst);
        raw::lock();
        WAITING.fetch_sub(1, SeqCst);
        TURNS.fetch_add(1, SeqCst);
    }
    DEPTH.set(depth + 1);
}

/// Rend une prise, toujours sur le fil qui l'a faite : le verrou part avec
/// la dernière. Sans effet sur un fil qui ne le tient pas.
pub fn unlock() {
    match DEPTH.get() {
        0 => {}
        1 => {
            DEPTH.set(0);
            raw::unlock();
        }
        depth => DEPTH.set(depth - 1),
    }
}

/// `body` sous le verrou, rendu à toute sortie, panique comprise.
pub fn run<T>(body: impl FnOnce() -> T) -> T {
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            unlock();
        }
    }
    lock();
    let _release = Release;
    body()
}

/// Vrai si ce fil tient le verrou : il ne doit alors attendre aucun fil qui
/// le prend (le fil MLX).
pub fn held_by_this_thread() -> bool {
    DEPTH.get() > 0
}

/// Pour le fil MLX, avant de reprendre le verrou : si quelqu'un l'attend,
/// il passe d'abord. 10 ms au plus, le temps qu'un fil réveillé prenne la
/// main : au-delà (le verrou est tenu par un long calcul), attendre dans
/// `lock` vaut mieux, la priorité s'y transmet.
#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn let_waiters_pass() {
    use std::time::{Duration, Instant};

    let (turn, since) = (TURNS.load(SeqCst), Instant::now());
    while WAITING.load(SeqCst) > 0 && TURNS.load(SeqCst) == turn && since.elapsed() < Duration::from_millis(10) {
        std::thread::sleep(Duration::from_micros(50));
    }
}

#[cfg(target_os = "macos")]
mod raw {
    use std::cell::UnsafeCell;

    struct Lock(UnsafeCell<libc::os_unfair_lock>);

    // SAFETY : l'`os_unfair_lock` est fait pour être partagé entre fils ; il
    // n'est touché que par ses propres fonctions, atomiques.
    unsafe impl Sync for Lock {}

    static LOCK: Lock = Lock(UnsafeCell::new(libc::OS_UNFAIR_LOCK_INIT));

    pub(super) fn lock() {
        // SAFETY : un verrou statique, jamais déplacé ; `super::lock` ne le
        // reprend pas sur le fil qui le tient.
        unsafe { libc::os_unfair_lock_lock(LOCK.0.get()) }
    }

    pub(super) fn unlock() {
        // SAFETY : `super::unlock` n'appelle que sur le fil qui le tient.
        unsafe { libc::os_unfair_lock_unlock(LOCK.0.get()) }
    }
}

#[cfg(not(target_os = "macos"))]
mod raw {
    use crate::sync::{CondvarExt, LockExt};
    use std::sync::{Condvar, Mutex};

    static TAKEN: Mutex<bool> = Mutex::new(false);
    static FREED: Condvar = Condvar::new();

    pub(super) fn lock() {
        let mut taken = TAKEN.locked();
        while *taken {
            taken = FREED.wait_locked(taken);
        }
        *taken = true;
    }

    pub(super) fn unlock() {
        *TAKEN.locked() = false;
        FREED.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::mpsc;
    use std::time::Duration;

    const PATIENCE: Duration = Duration::from_secs(20);

    /// Un autre fil qui demande le verrou ; il dit quand il est passé.
    fn contender() -> mpsc::Receiver<()> {
        let (started, has_started) = mpsc::channel();
        let (passed, has_passed) = mpsc::channel();
        std::thread::spawn(move || {
            started.send(()).unwrap();
            run(|| {});
            passed.send(()).unwrap();
        });
        has_started.recv_timeout(PATIENCE).unwrap();
        has_passed
    }

    #[test]
    fn the_lock_lets_one_thread_in_at_a_time() {
        let (inside, max_inside) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let passes = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for thread in 0..8 {
                let (inside, max_inside, passes) = (&inside, &max_inside, &passes);
                scope.spawn(move || {
                    for _ in 0..50 {
                        let body = || {
                            max_inside.fetch_max(inside.fetch_add(1, SeqCst) + 1, SeqCst);
                            std::thread::sleep(Duration::from_micros(50));
                            inside.fetch_sub(1, SeqCst);
                            passes.fetch_add(1, SeqCst);
                        };
                        // Par `run`, ou par la paire qu'appelle Swift.
                        if thread % 2 == 0 {
                            run(body);
                        } else {
                            lock();
                            body();
                            unlock();
                        }
                    }
                });
            }
        });
        assert_eq!(max_inside.load(SeqCst), 1);
        assert_eq!(passes.load(SeqCst), 400);
    }

    #[test]
    fn locking_again_on_the_same_thread_is_safe_and_releases_only_at_the_last_unlock() {
        lock();
        lock();
        let other = contender();
        unlock();
        // Le verrou rendu avant d'affirmer : un échec ne bloque pas les autres tests.
        let still_held = held_by_this_thread();
        let waited = other.recv_timeout(Duration::from_millis(100)).is_err();
        unlock();
        assert!(still_held && waited, "rendu avant le dernier unlock");
        assert!(!held_by_this_thread());
        other.recv_timeout(PATIENCE).expect("verrou jamais rendu");
    }

    #[test]
    fn unlocking_without_the_lock_leaves_the_owner_in_place() {
        lock();
        let unlocked = std::thread::spawn(unlock).join();
        let other = contender();
        let waited = other.recv_timeout(Duration::from_millis(100)).is_err();
        unlock();
        unlocked.expect("unlock sans le verrou");
        assert!(waited, "rendu par un fil qui ne le tenait pas");
        other.recv_timeout(PATIENCE).expect("verrou jamais rendu");
    }

    #[test]
    fn a_panic_releases_the_lock() {
        let panicked = std::thread::spawn(|| {
            let caught = std::panic::catch_unwind(|| run(|| run(|| panic!("panique voulue"))));
            (caught.is_err(), held_by_this_thread())
        });
        assert_eq!(panicked.join().unwrap(), (true, false));
        contender().recv_timeout(PATIENCE).expect("verrou jamais rendu");
    }
}
