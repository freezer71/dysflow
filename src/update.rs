//! `dysflow --update` : relance l'installateur officiel, qui installe la
//! dernière version publiée — ou dit que celle-ci est déjà là.
//!
//! Le binaire installé vit dans `<préfixe>/lib/dysflow/dysflow` (install.sh,
//! scripts/install.sh) : ce préfixe est passé à l'installateur, pour mettre à
//! jour sur place une installation faite ailleurs que dans `~/.local`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// L'installateur, servi par le site depuis la branche main du dépôt.
pub const INSTALLER: &str = "https://dysflow.koumalabs.org/install.sh";

/// Télécharge et lance l'installateur, sortie et erreurs à l'écran.
pub fn run() -> Result<()> {
    let mut command = Command::new("sh");
    command.arg("-c").arg(format!("curl -fsSL {INSTALLER} | sh"));
    let prefix = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
        .and_then(|exe| prefix_of(&exe));
    if let Some(prefix) = prefix {
        command.env("PREFIX", prefix);
    }
    let status = command.status().context(crate::tr!("cannot start sh for the update", "impossible de lancer sh pour la mise à jour"))?;
    if !status.success() {
        bail!(crate::tr!(en = "the update failed ({status})", fr = "la mise à jour a échoué ({status})"));
    }
    Ok(())
}

/// `<préfixe>/lib/dysflow/dysflow` → `<préfixe>` ; `None` pour un binaire
/// rangé autrement (`cargo install`, arbre de compilation) : l'installateur
/// prend alors son préfixe par défaut.
fn prefix_of(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let lib = dir.parent()?;
    let installed = exe.file_name()? == "dysflow" && dir.file_name()? == "dysflow" && lib.file_name()? == "lib";
    installed.then(|| lib.parent().map(Path::to_path_buf)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_comes_from_the_installed_layout() {
        assert_eq!(
            prefix_of(Path::new("/home/u/.local/lib/dysflow/dysflow")),
            Some(PathBuf::from("/home/u/.local"))
        );
        assert_eq!(prefix_of(Path::new("/usr/local/lib/dysflow/dysflow")), Some(PathBuf::from("/usr/local")));
    }

    #[test]
    fn another_layout_keeps_the_default_prefix() {
        assert_eq!(prefix_of(Path::new("/home/u/.cargo/bin/dysflow")), None);
        assert_eq!(prefix_of(Path::new("/src/dysflow/target/release/dysflow")), None);
    }
}
