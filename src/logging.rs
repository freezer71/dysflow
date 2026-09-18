//! Journalisation des sorties natives.
//!
//! ggml et audio.cpp écrivent copieusement sur `stderr` — initialisation Metal,
//! compilation des noyaux, allocations. En plein écran, ces lignes se mêleraient
//! au rendu et le disloqueraient. Elles sont donc détournées vers un fichier, où
//! elles restent consultables, plutôt que supprimées.
//!
//! La redirection est rendue au moment où la garde est libérée : une erreur
//! survenant après la sortie du mode plein écran s'affiche donc normalement.

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};

/// Descripteur d'origine de `stderr`, pour le rendre depuis un crochet de
/// panique qui n'a pas la garde sous la main. `-1` : rien à rendre.
#[cfg(unix)]
static SAVED_STDERR: AtomicI32 = AtomicI32::new(-1);

/// Emplacement du journal de session.
pub fn log_path() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("sillon.log"))
}

/// Garde qui restaure `stderr` lorsqu'elle est libérée.
pub struct StderrRedirect {
    #[cfg(unix)]
    saved: std::os::fd::RawFd,
}

/// Détourne `stderr` vers le journal.
///
/// Renvoie `None` si la redirection n'a pas pu être mise en place ; le lecteur
/// fonctionne alors normalement, au prix d'un affichage bruité.
#[cfg(unix)]
pub fn redirect_stderr() -> Option<StderrRedirect> {
    redirect_stderr_to(&log_path()?)
}

/// Rend `stderr` à son terminal sans attendre la garde.
///
/// Le crochet de panique restaurait l'écran puis écrivait son message sur
/// une sortie encore détournée vers le journal : l'utilisateur voyait un
/// terminal propre, et rien d'autre. La garde, elle, ne tombe qu'après.
#[cfg(unix)]
pub fn release_for_panic() {
    let saved = SAVED_STDERR.load(Ordering::SeqCst);
    if saved >= 0 {
        // SAFETY : `saved` vient de `dup` et reste ouvert tant que la garde vit ;
        // on ne le ferme pas ici, la garde s'en charge.
        unsafe { libc::dup2(saved, libc::STDERR_FILENO) };
    }
}

#[cfg(not(unix))]
pub fn release_for_panic() {}

#[cfg(unix)]
fn redirect_stderr_to(path: &Path) -> Option<StderrRedirect> {
    use std::os::fd::AsRawFd;

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(path).ok()?;

    // SAFETY : duplique le descripteur 2, ouvert par définition.
    let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
    if saved < 0 {
        return None;
    }
    // SAFETY : les deux descripteurs sont valides ; `dup2` ferme l'ancien 2.
    if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        // SAFETY : `saved` vient d'être obtenu par `dup`.
        unsafe { libc::close(saved) };
        return None;
    }
    SAVED_STDERR.store(saved, Ordering::SeqCst);
    Some(StderrRedirect { saved })
}

#[cfg(not(unix))]
pub fn redirect_stderr() -> Option<StderrRedirect> {
    None
}

#[cfg(unix)]
impl Drop for StderrRedirect {
    fn drop(&mut self) {
        // SAFETY : `saved` vient de `dup` et n'est restauré qu'une fois.
        SAVED_STDERR.store(-1, Ordering::SeqCst);
        unsafe {
            libc::dup2(self.saved, libc::STDERR_FILENO);
            libc::close(self.saved);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn inode_of_stderr() -> u64 {
        // SAFETY : `fstat` remplit une structure que nous fournissons.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::fstat(libc::STDERR_FILENO, &mut stat) }, 0);
        stat.st_ino as u64
    }

    #[test]
    fn a_panic_gets_its_stderr_back_before_the_message_is_printed() {
        // Le crochet de panique restaurait le terminal puis écrivait sur une
        // sortie d'erreur encore détournée vers le journal : l'utilisateur
        // voyait un écran propre et rien d'autre.
        let path = std::env::temp_dir().join(format!("sillon-log-{}.log", std::process::id()));
        let before = inode_of_stderr();
        let guard = redirect_stderr_to(&path).expect("redirection");
        assert_ne!(inode_of_stderr(), before, "stderr devrait viser le journal");

        release_for_panic();
        assert_eq!(inode_of_stderr(), before, "stderr devrait être rendue");

        // La garde qui tombe ensuite ne casse rien.
        drop(guard);
        assert_eq!(inode_of_stderr(), before);
        std::fs::remove_file(&path).ok();
    }
}
