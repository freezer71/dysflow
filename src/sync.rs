//! Verrous qui survivent à une panique.
//!
//! Un fil qui panique en tenant un `Mutex` l'empoisonne ; un `unwrap` sur le
//! verrou propagerait alors la panique à tous les autres fils, jusqu'à l'app
//! macOS par la FFI. Les données protégées ici restent cohérentes entre deux
//! instructions (compteurs, files, états publiés) : on reprend la garde.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

pub trait LockExt<T> {
    /// `lock()`, en reprenant un verrou empoisonné.
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub trait CondvarExt {
    /// `wait()`, en reprenant un verrou empoisonné.
    fn wait_locked<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T>;
}

impl CondvarExt for Condvar {
    fn wait_locked<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        self.wait(guard).unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_poisoned_lock_is_recovered() {
        let shared = Arc::new(Mutex::new(1));
        let poisoner = Arc::clone(&shared);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("empoisonne");
        })
        .join();
        assert!(shared.is_poisoned());
        *shared.locked() += 1;
        assert_eq!(*shared.locked(), 2);
    }
}
