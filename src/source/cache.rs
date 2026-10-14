//! Mémorisation du markdown produit par une extraction coûteuse.

use anyhow::Result;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Octets lus en tête pour distinguer deux fichiers de même taille.
const SAMPLE: usize = 64 * 1024;

/// Version de la reconstruction dont le cache garde le résultat.
///
/// À incrémenter quand `layout` ou l'extraction changent ce qu'ils rendent :
/// sans cela, un document déjà lu resterait servi dans son ancienne forme
/// jusqu'à un `--no-cache` que personne ne pense à passer.
pub const CACHE_VERSION: u32 = 2;

/// Empreinte d'un fichier : sa taille, sa date, son début.
///
/// Il ne s'agit pas de sécurité mais de reconnaître un document déjà lu ; lire
/// deux cents pages en entier pour cela coûterait plus que l'extraction.
pub fn fingerprint(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let mut bytes = Vec::with_capacity(SAMPLE + 16);
    bytes.extend_from_slice(&meta.len().to_le_bytes());
    if let Ok(time) = meta.modified() {
        if let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) {
            bytes.extend_from_slice(&since.as_secs().to_le_bytes());
        }
    }
    let mut head = vec![0u8; SAMPLE];
    let mut file = std::fs::File::open(path)?;
    let read = file.read(&mut head)?;
    bytes.extend_from_slice(&head[..read]);
    Ok(format!("v{CACHE_VERSION}-{:016x}", stable_hash(&bytes)))
}

/// FNV-1a sur 64 bits : figé, donc le même d'une version de Rust à l'autre.
///
/// `DefaultHasher` ne le promet pas, et un cache dont les clés changent avec
/// la toolchain se remplit de fichiers que plus rien ne relit.
pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Répertoire du cache, créé au besoin.
fn dir() -> Option<PathBuf> {
    let dir = crate::paths::cache_dir()?.join("documents");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Markdown déjà produit pour cette empreinte.
pub fn read(key: &str) -> Option<String> {
    read_in(&dir()?, key)
}

/// Mémorise le markdown produit.
///
/// Un cache qui échoue n'est pas une erreur : le document a été lu, c'est tout
/// ce qui compte.
pub fn write(key: &str, markdown: &str) {
    if let Some(dir) = dir() {
        write_in(&dir, key, markdown);
    }
}

/// Vide le cache des documents ; rend les octets libérés.
///
/// Rien n'est perdu : un document se ré-extrait à sa prochaine ouverture.
pub fn clear() -> Result<u64> {
    let Some(dir) = crate::paths::cache_dir().map(|d| d.join("documents")) else {
        return Ok(0);
    };
    if !dir.exists() {
        return Ok(0);
    }
    Ok(clear_in(&dir)?)
}

fn clear_in(dir: &Path) -> std::io::Result<u64> {
    let mut freed = 0;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "md") {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            match std::fs::remove_file(&path) {
                Ok(()) => freed += size,
                // Disparu entre-temps : déjà libéré, ce n'est pas une erreur.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(freed)
}

fn read_in(dir: &Path, key: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(format!("{key}.md"))).ok()
}

fn write_in(dir: &Path, key: &str, markdown: &str) {
    let _ = crate::paths::write_atomic(&dir.join(format!("{key}.md")), markdown.as_bytes());
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

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sillon-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn what_is_written_comes_back() {
        // Dans un répertoire à part : les tests n'écrivent jamais dans le
        // cache de l'utilisateur.
        let dir = scratch();
        write_in(&dir, "essai", "## Titre\n\nDu texte.");
        assert_eq!(read_in(&dir, "essai").as_deref(), Some("## Titre\n\nDu texte."));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unknown_key_yields_nothing() {
        let dir = scratch();
        assert_eq!(read_in(&dir, "cle-qui-n-existe-pas-du-tout"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_key_is_stable_from_one_build_to_the_next() {
        // `DefaultHasher` ne promet rien d'une version de Rust à l'autre : un
        // cache rempli hier serait orphelin demain. L'empreinte repose sur un
        // hachage figé, dont la valeur se vérifie ici.
        assert_eq!(stable_hash(b"abc"), 0xe71f_a219_0541_574b);
    }

    #[test]
    fn the_key_carries_the_version_of_the_reconstruction() {
        // Une amélioration de la reconstruction doit se voir sur les documents
        // déjà lus : la clé change avec elle.
        let dir = scratch();
        let file = dir.join("doc.bin");
        std::fs::write(&file, b"contenu").unwrap();
        let key = fingerprint(&file).unwrap();
        assert!(key.starts_with(&format!("v{CACHE_VERSION}-")), "{key}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clearing_removes_only_the_cached_documents_and_counts_their_bytes() {
        let dir = scratch();
        write_in(&dir, "a", "12345");
        write_in(&dir, "b", "123");
        std::fs::write(dir.join("garde.txt"), b"x").unwrap();
        assert_eq!(clear_in(&dir).unwrap(), 8);
        assert_eq!(read_in(&dir, "a"), None);
        assert!(dir.join("garde.txt").exists(), "seuls les .md partent");
        assert_eq!(clear_in(&dir).unwrap(), 0, "déjà vide");
        std::fs::remove_dir_all(&dir).ok();
    }
}
