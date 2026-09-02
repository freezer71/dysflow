//! Journalisation des sorties natives.
//!
//! ggml et audio.cpp écrivent copieusement sur `stderr` — initialisation Metal,
//! compilation des noyaux, allocations. En plein écran, ces lignes se mêleraient
//! au rendu et le disloqueraient. Elles sont donc détournées vers un fichier, où
//! elles restent consultables, plutôt que supprimées.
//!
//! La redirection est rendue au moment où la garde est libérée : une erreur
//! survenant après la sortie du mode plein écran s'affiche donc normalement.

use std::path::PathBuf;

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
    use std::os::fd::AsRawFd;

    let path = log_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(&path).ok()?;

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
        unsafe {
            libc::dup2(self.saved, libc::STDERR_FILENO);
            libc::close(self.saved);
        }
    }
}
