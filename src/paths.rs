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
    #[cfg(test)]
    return Some(test_root("config"));
    #[cfg(not(test))]
    dirs::config_dir().map(|base| adopt(&base))
}

/// Données : modèles, signets, positions, journal.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(test)]
    return Some(test_root("data"));
    #[cfg(not(test))]
    dirs::data_local_dir().map(|base| adopt(&base))
}

/// Cache : documents extraits, données de langue.
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(test)]
    return Some(test_root("cache"));
    #[cfg(not(test))]
    dirs::cache_dir().map(|base| adopt(&base))
}

/// Sous `cargo test`, jamais les répertoires de l'utilisateur : un dossier
/// temporaire propre au processus, créé au premier usage. Aucun test du
/// moteur ne peut ainsi lire ni écrire ses signets, positions ou modèles.
#[cfg(test)]
fn test_root(kind: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sillon-tests-{}", std::process::id())).join(kind);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Le répertoire du lecteur sous `base`, en reprenant celui de l'ancien nom
/// s'il est seul.
///
/// Ni copie ni fusion : un renommage, ou rien. Si les deux existent, on n'y
/// touche pas — l'ancien reste là, intact, pour qui voudra y regarder.
/// Écrit un fichier d'un seul tenant : temporaire à côté, puis renommage.
///
/// Un `fs::write` direct tronque avant d'écrire : une coupure au mauvais
/// moment laisse un fichier vide, et des signets ou une configuration
/// disparaissent en silence. Le renommage, lui, est atomique : l'ancien
/// contenu reste en place jusqu'à ce que le nouveau soit entier.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "fichier".to_string());
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let outcome = std::fs::write(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    outcome
}

/// Relit un fichier JSON de données (positions, signets, langues).
///
/// Absent, il rend la valeur par défaut. Illisible, il est mis de côté en
/// `.json.bak` avant de repartir à neuf, pour que la sauvegarde suivante ne
/// l'écrase pas, et l'avertissement rendu dit où il est allé.
pub fn load_json<T: serde::de::DeserializeOwned + Default>(path: &Path, what: &str) -> (T, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (T::default(), None),
        Err(err) => return (T::default(), Some(format!("{what} illisibles : {err}"))),
    };
    match serde_json::from_str(&text) {
        Ok(value) => (value, None),
        Err(err) => {
            let backup = path.with_extension("json.bak");
            let notice = match std::fs::rename(path, &backup) {
                Ok(()) => format!("{what} d'un ancien format mis de côté dans {} ({err})", backup.display()),
                Err(_) => format!("{what} illisibles, ignorés : {err}"),
            };
            (T::default(), Some(notice))
        }
    }
}

/// Écrit une valeur en JSON lisible, d'un seul coup (`write_atomic`).
pub fn save_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    use anyhow::Context;
    let text = serde_json::to_string_pretty(value)?;
    write_atomic(path, text.as_bytes()).with_context(|| format!("écriture de {}", path.display()))
}

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

    #[test]
    fn user_directories_are_temporary_in_test_builds() {
        let temp = std::env::temp_dir();
        for dir in [data_dir(), config_dir(), cache_dir()] {
            assert!(dir.unwrap().starts_with(&temp), "un test ne touche pas aux fichiers de l'utilisateur");
        }
    }

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


    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_no_temporary_behind() {
        let base = scratch();
        let file = base.join("etat.json");
        std::fs::write(&file, "ancien").unwrap();

        write_atomic(&file, b"nouveau").expect("écriture");

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "nouveau");
        let leftovers: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "etat.json")
            .collect();
        assert!(leftovers.is_empty(), "restes : {leftovers:?}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn an_atomic_write_creates_the_parent_directory() {
        let base = scratch();
        let file = base.join("profond").join("etat.json");
        write_atomic(&file, b"x").expect("écriture");
        assert!(file.is_file());
        std::fs::remove_dir_all(&base).ok();
    }
}
