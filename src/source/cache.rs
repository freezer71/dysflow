//! Mémorisation du markdown produit par une extraction coûteuse.

use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Octets lus en tête pour distinguer deux fichiers de même taille.
const SAMPLE: usize = 64 * 1024;

/// Empreinte d'un fichier : sa taille, sa date, son début.
///
/// Il ne s'agit pas de sécurité mais de reconnaître un document déjà lu ; lire
/// deux cents pages en entier pour cela coûterait plus que l'extraction.
pub fn fingerprint(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let mut hasher = DefaultHasher::new();
    meta.len().hash(&mut hasher);
    if let Ok(time) = meta.modified() {
        if let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) {
            since.as_secs().hash(&mut hasher);
        }
    }
    let mut head = vec![0u8; SAMPLE];
    let mut file = std::fs::File::open(path)?;
    let read = file.read(&mut head)?;
    head.truncate(read);
    head.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

/// Répertoire du cache, créé au besoin.
fn dir() -> Option<PathBuf> {
    let dir = crate::paths::cache_dir()?.join("documents");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Markdown déjà produit pour cette empreinte.
pub fn read(key: &str) -> Option<String> {
    std::fs::read_to_string(dir()?.join(format!("{key}.md"))).ok()
}

/// Mémorise le markdown produit.
///
/// Un cache qui échoue n'est pas une erreur : le document a été lu, c'est tout
/// ce qui compte.
pub fn write(key: &str, markdown: &str) {
    let Some(dir) = dir() else {
        return;
    };
    let file = dir.join(format!("{key}.md"));
    let temp = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::write(&temp, markdown).is_ok() {
        let _ = std::fs::rename(&temp, &file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn the_same_file_always_gives_the_same_key() {
        let one = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        let two = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        assert_eq!(one, two);
    }

    #[test]
    fn two_files_give_two_keys() {
        let one = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        let two = fingerprint(&fixture("rapport.pdf")).expect("empreinte");
        assert_ne!(one, two);
    }

    #[test]
    fn what_is_written_comes_back() {
        let key = "essai-du-cache-sillon";
        write(key, "## Titre\n\nDu texte.");
        assert_eq!(read(key).as_deref(), Some("## Titre\n\nDu texte."));
    }

    #[test]
    fn an_unknown_key_yields_nothing() {
        assert_eq!(read("cle-qui-n-existe-pas-du-tout"), None);
    }
}
