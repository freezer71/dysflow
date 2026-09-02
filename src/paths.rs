//! Où le lecteur range ce qui lui appartient.
//!
//! Un seul nom de répertoire pour la configuration, les données et le cache,
//! et un seul endroit pour le changer. Le lecteur s'est d'abord appelé
//! `rs-reader` : un répertoire de l'ancien nom est repris tel quel — les
//! modèles y pèsent des gigaoctets, les recopier serait absurde, et perdre
//! les signets ne se pardonne pas.

use std::path::{Path, PathBuf};

/// Nom du lecteur, tel qu'il apparaît dans les chemins.
pub const APP: &str = "sillon";
/// Nom sous lequel les répertoires ont pu être créés avant.
const FORMER: &str = "rs-reader";

/// Configuration : `config.toml`.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|base| adopt(&base))
}

/// Données : modèles, signets, positions, journal.
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| adopt(&base))
}

/// Cache : documents extraits, données de langue.
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|base| adopt(&base))
}

/// Le répertoire du lecteur sous `base`, en reprenant celui de l'ancien nom
/// s'il est seul.
///
/// Ni copie ni fusion : un renommage, ou rien. Si les deux existent, on n'y
/// touche pas — l'ancien reste là, intact, pour qui voudra y regarder.
fn adopt(base: &Path) -> PathBuf {
    let current = base.join(APP);
    let former = base.join(FORMER);
    if !current.exists() && former.is_dir() {
        // Un échec — droits, disque — laisse simplement le lecteur repartir
        // à neuf dans le nouveau répertoire.
        let _ = std::fs::rename(&former, &current);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sillon-paths-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_directory_of_the_former_name_is_taken_over() {
        let base = scratch();
        std::fs::create_dir_all(base.join("rs-reader").join("models")).unwrap();
        std::fs::write(base.join("rs-reader").join("bookmarks.json"), "{}").unwrap();

        let dir = adopt(&base);

        assert_eq!(dir, base.join("sillon"));
        assert!(dir.join("models").is_dir(), "les modèles suivent");
        assert!(dir.join("bookmarks.json").is_file(), "les signets suivent");
        assert!(!base.join("rs-reader").exists(), "l'ancien répertoire n'est pas dupliqué");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn two_directories_are_left_alone() {
        let base = scratch();
        std::fs::create_dir_all(base.join("rs-reader")).unwrap();
        std::fs::create_dir_all(base.join("sillon")).unwrap();
        std::fs::write(base.join("sillon").join("config.toml"), "").unwrap();

        let dir = adopt(&base);

        assert_eq!(dir, base.join("sillon"));
        assert!(dir.join("config.toml").is_file(), "le nouveau n'est pas écrasé");
        assert!(base.join("rs-reader").is_dir(), "l'ancien reste intact");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn nothing_is_created_when_neither_exists() {
        let base = scratch();
        let dir = adopt(&base);
        assert_eq!(dir, base.join("sillon"));
        assert!(!dir.exists(), "le répertoire n'est créé qu'à la première écriture");
        std::fs::remove_dir_all(&base).ok();
    }
}
