//! Où le lecteur range ce qui lui appartient.
//!
//! Un seul nom de répertoire pour la configuration, les données et le cache,
//! et un seul endroit pour le changer. Le lecteur s'est d'abord appelé
//! `rs-reader`, puis `sillon` (jusqu'au 18 oct. 2026), puis `sillage`
//! (jusqu'au 25 oct. 2026) : un répertoire d'un
//! ancien nom est repris tel quel — les modèles y pèsent des gigaoctets, les
//! recopier serait absurde, et perdre les positions ne se pardonne pas.

use std::path::{Path, PathBuf};

/// Nom du lecteur, tel qu'il apparaît dans les chemins.
pub const APP: &str = "dysflow";
/// Noms sous lesquels les répertoires ont pu être créés avant, du plus
/// récent au plus ancien : le premier trouvé est repris.
const FORMER: &[&str] = &["sillage", "sillon", "rs-reader"];

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
    test_scratch(kind)
}

/// Un dossier de travail pour un test, créé vide sous la racine des tests
/// de ce processus.
///
/// Les tests en laissaient des milliers dans le dossier temporaire du
/// système (8 400 dossiers, 1 Go, relevés le 18 oct. 2026) : tout passe
/// désormais par une seule racine, retirée à la sortie du processus.
#[cfg(test)]
pub(crate) fn test_scratch(name: &str) -> PathBuf {
    let dir = test_base().join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// `dysflow-tests-<pid>` : créée au premier usage, retirée par `atexit` à la
/// sortie du processus de test. Au même moment, les racines laissées par un
/// processus mort (test interrompu, plantage) partent aussi.
#[cfg(test)]
fn test_base() -> PathBuf {
    static INIT: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("{TEST_PREFIX}{}", std::process::id()));
    INIT.call_once(|| {
        sweep_dead_test_roots();
        extern "C" fn remove_test_root() {
            let base = std::env::temp_dir().join(format!("{TEST_PREFIX}{}", std::process::id()));
            let _ = std::fs::remove_dir_all(base);
        }
        // SAFETY : `remove_test_root` ne prend aucun argument et ne panique pas.
        unsafe { libc::atexit(remove_test_root) };
    });
    base
}

#[cfg(test)]
const TEST_PREFIX: &str = "dysflow-tests-";

/// Retire les racines de test dont le processus n'existe plus.
#[cfg(test)]
fn sweep_dead_test_roots() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix(TEST_PREFIX)).and_then(|p| p.parse::<i32>().ok())
        else {
            continue;
        };
        // SAFETY : le signal 0 ne fait que demander si le processus existe.
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

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
    // Un temporaire par écriture, pas seulement par processus : deux fils qui
    // écrivent le même fichier en même temps partageaient le même temporaire —
    // l'un le tronquait pendant que l'autre le renommait (le fichier publié
    // pouvait être en cours d'écriture), et le second renommage ne le trouvait
    // plus (« No such file or directory »).
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = path.with_file_name(format!(".{name}.{}-{write}.tmp", std::process::id()));
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
/// `what` nomme ces données au pluriel, dans les deux langues de l'interface.
pub fn load_json<T: serde::de::DeserializeOwned + Default>(path: &Path, what: crate::i18n::Tr) -> (T, Option<String>) {
    let what = what.get();
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (T::default(), None),
        Err(err) => {
            return (T::default(), Some(crate::tr!(en = "unreadable {what}: {err}", fr = "{what} illisibles : {err}")));
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => (value, None),
        Err(err) => {
            let backup = path.with_extension("json.bak");
            let notice = match std::fs::rename(path, &backup) {
                Ok(()) => crate::tr!(
                    en = "{what} in an old format set aside in {} ({err})",
                    fr = "{what} d'un ancien format mis de côté dans {} ({err})",
                    backup.display(),
                ),
                Err(_) => crate::tr!(en = "unreadable {what}, ignored: {err}", fr = "{what} illisibles, ignorés : {err}"),
            };
            (T::default(), Some(notice))
        }
    }
}

/// Écrit une valeur en JSON lisible, d'un seul coup (`write_atomic`).
pub fn save_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    use anyhow::Context;
    let text = serde_json::to_string_pretty(value)?;
    write_atomic(path, text.as_bytes()).with_context(|| crate::tr!(en = "writing {}", fr = "écriture de {}", path.display()))
}

/// Le répertoire du lecteur sous `base`, en reprenant ceux des anciens noms.
///
/// Seul, l'ancien est renommé. Si les deux existent, ce qui manque au
/// nouveau y est déplacé, sans rien copier ; ce que les deux ont (même nom)
/// reste dans l'ancien, intact, pour qui voudra y regarder. Un répertoire
/// né avant un renommage laissait sinon dans l'ancien les modèles
/// téléchargés depuis (vu le 20 oct. 2026, entre `sillon` et `sillage`).
fn adopt(base: &Path) -> PathBuf {
    let current = base.join(APP);
    // `data_dir` est appelé à chaque lecture : une seule reprise par processus.
    static DONE: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    let mut done = DONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if done.iter().any(|b| b == base) {
        return current;
    }
    done.push(base.to_path_buf());
    for former in FORMER.iter().map(|name| base.join(name)).filter(|dir| dir.is_dir()) {
        // Un échec — droits, disque — laisse simplement le lecteur repartir
        // à neuf dans le nouveau répertoire.
        if !current.exists() {
            let _ = std::fs::rename(&former, &current);
        } else {
            merge_into(&former, &current);
        }
    }
    current
}

/// Déplace dans `to` ce qui n'y est pas encore, dossier par dossier ; un
/// dossier de l'ancien vidé ainsi disparaît.
///
/// Ce que `to` a déjà, le lecteur ne le lira plus jamais dans l'ancien : un
/// modèle en double y part sans être relu (il pèse des centaines de
/// mégaoctets, et 1,2 Go restaient ainsi dans `sillage`, relevés le 26 oct.
/// 2026), une autre copie seulement si elle est identique. Une version
/// différente d'une configuration, de signets ou de positions reste, pour ne
/// rien perdre.
fn merge_into(from: &Path, to: &Path) {
    merge_dir(from, to, false);
}

fn merge_dir(from: &Path, to: &Path, models: bool) {
    let Ok(entries) = std::fs::read_dir(from) else { return };
    for entry in entries.flatten() {
        let target = to.join(entry.file_name());
        if !target.exists() {
            let _ = std::fs::rename(entry.path(), &target);
        } else if entry.path().is_dir() && target.is_dir() {
            merge_dir(&entry.path(), &target, models || entry.file_name() == "models");
        } else if entry.path().is_file() && target.is_file() && (models || same_contents(&entry.path(), &target)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    // N'efface qu'un dossier vide.
    let _ = std::fs::remove_dir(from);
}

/// Deux fichiers au contenu identique ; au moindre doute (lecture
/// impossible), non.
fn same_contents(a: &Path, b: &Path) -> bool {
    let same_size = matches!((std::fs::metadata(a), std::fs::metadata(b)), (Ok(x), Ok(y)) if x.len() == y.len());
    same_size && matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

/// Un chemin écrit dans un répertoire d'un ancien nom (`model_path` de la
/// configuration), ramené dans le répertoire actuel si le fichier y est.
pub fn relocated(path: &str) -> String {
    #[cfg(test)]
    return path.to_string();
    #[cfg(not(test))]
    match dirs::data_local_dir() {
        Some(base) => relocated_in(path, &base),
        None => path.to_string(),
    }
}

fn relocated_in(path: &str, base: &Path) -> String {
    let path_buf = Path::new(path);
    for former in FORMER {
        if let Ok(rest) = path_buf.strip_prefix(base.join(former)) {
            let moved = base.join(APP).join(rest);
            if moved.exists() {
                return moved.to_string_lossy().into_owned();
            }
        }
    }
    path.to_string()
}

/// Développe un `~` initial : un chemin littéral `~/...` n'existe pas.
pub fn expand_tilde(path: &str) -> String {
    expand_tilde_with(path, dirs::home_dir().as_deref())
}

/// `expand_tilde` avec un dossier personnel explicite (les tests n'ont pas à
/// dépendre de celui de la machine).
pub fn expand_tilde_with(path: &str, home: Option<&std::path::Path>) -> String {
    let Some(rest) = path.strip_prefix('~') else {
        return path.to_string();
    };
    let Some(home) = home else {
        return path.to_string();
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home.to_string_lossy().into_owned()
    } else {
        home.join(rest).to_string_lossy().into_owned()
    }
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
        let dir = crate::paths::test_scratch(&format!("paths-{:?}", std::thread::current().id()));
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

        assert_eq!(dir, base.join("dysflow"));
        assert!(dir.join("models").is_dir(), "les modèles suivent");
        assert!(dir.join("bookmarks.json").is_file(), "les signets suivent");
        assert!(!base.join("rs-reader").exists(), "l'ancien répertoire n'est pas dupliqué");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn what_the_new_directory_lacks_is_moved_from_the_former() {
        let base = scratch();
        let (old, new) = (base.join("sillon"), base.join("dysflow"));
        std::fs::create_dir_all(old.join("models").join("kokoro")).unwrap();
        std::fs::create_dir_all(old.join("models").join("phonon_2")).unwrap();
        std::fs::write(old.join("models").join("kokoro").join("a"), "ancien").unwrap();
        std::fs::write(old.join("models").join("phonon_2").join("p"), "phonon").unwrap();
        std::fs::write(old.join("config.toml"), "ancien").unwrap();
        std::fs::create_dir_all(new.join("models").join("kokoro")).unwrap();
        std::fs::write(new.join("models").join("kokoro").join("a"), "nouveau").unwrap();
        std::fs::write(new.join("config.toml"), "nouveau").unwrap();

        let dir = adopt(&base);

        assert_eq!(dir, new);
        assert_eq!(std::fs::read_to_string(new.join("models/phonon_2/p")).unwrap(), "phonon", "le modèle manquant suit");
        assert_eq!(std::fs::read_to_string(new.join("config.toml")).unwrap(), "nouveau", "le nouveau n'est pas écrasé");
        assert_eq!(std::fs::read_to_string(new.join("models/kokoro/a")).unwrap(), "nouveau");
        assert_eq!(std::fs::read_to_string(old.join("config.toml")).unwrap(), "ancien", "une version différente reste dans l'ancien");
        assert!(!old.join("models").exists(), "le modèle en double part, et le dossier vidé avec");
        std::fs::remove_dir_all(&base).ok();
    }

    /// Le cas relevé le 26 oct. 2026 : 1,2 Go de modèles restés dans
    /// `sillage` après la reprise, la même copie étant déjà dans `dysflow`.
    #[test]
    fn duplicates_left_in_the_former_directory_are_removed() {
        let base = scratch();
        let (old, new) = (base.join("sillage"), base.join("dysflow"));
        for dir in [&old, &new] {
            std::fs::create_dir_all(dir.join("models/kokoro/voices")).unwrap();
            std::fs::write(dir.join("models/kokoro/voices/af_bella.safetensors"), "voix").unwrap();
            std::fs::write(dir.join("bookmarks.json"), "{}").unwrap();
        }
        std::fs::write(old.join("positions.json"), "ancien").unwrap();
        std::fs::write(new.join("positions.json"), "nouveau").unwrap();

        adopt(&base);

        assert!(new.join("models/kokoro/voices/af_bella.safetensors").is_file(), "le nouveau garde son modèle");
        assert!(!old.join("models").exists(), "le modèle en double part");
        assert!(!old.join("bookmarks.json").exists(), "une copie identique part");
        assert_eq!(std::fs::read_to_string(old.join("positions.json")).unwrap(), "ancien", "une version différente reste");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn an_emptied_former_directory_disappears() {
        let base = scratch();
        std::fs::create_dir_all(base.join("rs-reader")).unwrap();
        std::fs::write(base.join("rs-reader").join("positions.json"), "{}").unwrap();
        std::fs::create_dir_all(base.join("dysflow")).unwrap();

        let dir = adopt(&base);

        assert!(dir.join("positions.json").is_file());
        assert!(!base.join("rs-reader").exists());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_model_path_in_the_former_directory_follows_the_move() {
        let base = scratch();
        let moved = base.join("dysflow").join("models").join("kokoro");
        std::fs::create_dir_all(&moved).unwrap();
        std::fs::write(moved.join("m.safetensors"), "").unwrap();
        let old = base.join("sillon").join("models").join("kokoro").join("m.safetensors");

        assert_eq!(relocated_in(&old.to_string_lossy(), &base), moved.join("m.safetensors").to_string_lossy());
        let absent = base.join("sillon").join("models").join("autre.gguf");
        assert_eq!(relocated_in(&absent.to_string_lossy(), &base), absent.to_string_lossy(), "absent : inchangé");
        assert_eq!(relocated_in("/ailleurs/m.gguf", &base), "/ailleurs/m.gguf");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_sillon_directory_is_adopted_before_rs_reader() {
        let base = scratch();
        std::fs::create_dir_all(base.join("sillon").join("models")).unwrap();
        std::fs::write(base.join("sillon").join("config.toml"), "sillon").unwrap();
        std::fs::create_dir_all(base.join("rs-reader")).unwrap();
        std::fs::write(base.join("rs-reader").join("config.toml"), "rs-reader").unwrap();

        let dir = adopt(&base);

        assert_eq!(dir, base.join("dysflow"));
        assert!(dir.join("models").is_dir(), "les modèles de sillon suivent");
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "sillon", "sillon passe avant");
        assert!(!base.join("sillon").exists(), "sillon n'est pas dupliqué");
        assert!(base.join("rs-reader").join("config.toml").is_file(), "le doublon du plus ancien reste intact");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_sillage_directory_is_adopted_before_sillon() {
        let base = scratch();
        std::fs::create_dir_all(base.join("sillage").join("models")).unwrap();
        std::fs::write(base.join("sillage").join("config.toml"), "sillage").unwrap();
        std::fs::create_dir_all(base.join("sillon")).unwrap();
        std::fs::write(base.join("sillon").join("config.toml"), "sillon").unwrap();

        let dir = adopt(&base);

        assert_eq!(dir, base.join("dysflow"));
        assert!(dir.join("models").is_dir(), "les modèles de sillage suivent");
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "sillage", "sillage passe avant");
        assert!(!base.join("sillage").exists(), "sillage n'est pas dupliqué");
        assert!(base.join("sillon").join("config.toml").is_file(), "le doublon du plus ancien reste intact");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn nothing_is_created_when_neither_exists() {
        let base = scratch();
        let dir = adopt(&base);
        assert_eq!(dir, base.join("dysflow"));
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

    /// Plusieurs fils du même processus écrivent le même fichier : chacun
    /// réussit, et le fichier final est l'une des écritures entière, jamais
    /// un mélange ni une écriture tronquée.
    #[test]
    fn concurrent_atomic_writes_in_one_process_all_succeed_whole() {
        let base = scratch();
        let file = base.join("config.toml");
        let contents: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'a' + i; 64 * 1024]).collect();
        for _ in 0..50 {
            std::thread::scope(|s| {
                let handles: Vec<_> = contents.iter().map(|c| s.spawn(|| write_atomic(&file, c))).collect();
                for h in handles {
                    h.join().unwrap().expect("chaque écriture réussit");
                }
            });
            let written = std::fs::read(&file).unwrap();
            assert!(contents.contains(&written), "fichier mélangé ou tronqué ({} octets)", written.len());
        }
        std::fs::remove_dir_all(&base).ok();
    }
}
