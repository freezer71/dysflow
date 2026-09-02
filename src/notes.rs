//! Notes : une page de texte libre par document, à côté de la lecture.
//!
//! La note vit dans un fichier Markdown du répertoire de données, nommé par
//! le document — un mot lisible tiré de son nom, et une empreinte de sa
//! source pour que deux documents du même nom ne se marchent pas dessus.
//! Une note vide n'existe pas : on efface le fichier plutôt que d'en laisser
//! un blanc.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Le répertoire des notes, s'il y a un répertoire de données.
pub fn dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("notes"))
}

/// Le nom de fichier d'un document : `<mot>-<empreinte>.md`.
///
/// Un fichier est nommé par son chemin : le mot vient du nom de fichier, et
/// l'empreinte du chemin seul — renommer un titre dans le texte ne perd pas
/// la note. Ce qui n'a pas de chemin — le presse-papiers, un flux — est
/// nommé par son titre, et l'empreinte le prend en compte : deux collages
/// ont deux notes. Le mot est réduit à des lettres, chiffres et tirets pour
/// valoir sur tout système.
pub fn file_name(source: &str, title: &str) -> String {
    let is_path = source.contains('/') || source.contains('\\');
    let (word, salt) = if is_path {
        (stem(source), "")
    } else if title.trim().is_empty() {
        (source.to_string(), "")
    } else {
        (title.to_string(), title)
    };
    let mut slug = slugify(&word);
    if slug.is_empty() {
        slug = "note".to_string();
    }
    format!("{slug}-{:08x}.md", fingerprint(source, salt) as u32)
}

pub fn path(dir: &Path, source: &str, title: &str) -> PathBuf {
    dir.join(file_name(source, title))
}

/// La note d'un document, ou une chaîne vide s'il n'en a pas.
pub fn load(dir: &Path, source: &str, title: &str) -> String {
    std::fs::read_to_string(path(dir, source, title)).unwrap_or_default()
}

/// Écrit la note ; une note blanche efface le fichier.
pub fn save(dir: &Path, source: &str, title: &str, text: &str) -> Result<()> {
    let path = path(dir, source, title);
    if text.trim().is_empty() {
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("effacer {}", path.display()))?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("créer {}", dir.display()))?;
    std::fs::write(&path, text).with_context(|| format!("écrire {}", path.display()))
}

/// Un repère vers le passage en cours : « — Section · 0:42 ».
pub fn marker(section: &str, position: &str) -> String {
    match (section.trim().is_empty(), position.trim().is_empty()) {
        (true, true) => "—".to_string(),
        (true, false) => format!("— {position}"),
        (false, true) => format!("— {section}"),
        (false, false) => format!("— {section} · {position}"),
    }
}

/// Ajoute un repère à la fin de la note, sur sa propre ligne, suivi d'une
/// ligne vide où écrire.
pub fn append_marker(text: &mut String, marker: &str) {
    let trimmed = text.trim_end().len();
    text.truncate(trimmed);
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(marker);
    text.push('\n');
}

fn stem(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string())
}

/// Lettres et chiffres ASCII, en minuscules, séparés par des tirets ; les
/// lettres accentuées perdent leur accent plutôt que de disparaître.
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for c in text.chars().take(60) {
        let plain = unaccent(c);
        if plain.is_ascii_alphanumeric() {
            slug.push(plain.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

fn unaccent(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
        'ô' | 'ö' | 'ó' | 'õ' | 'Ô' | 'Ö' => 'o',
        'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        'ÿ' | 'ý' => 'y',
        other => other,
    }
}

/// FNV-1a sur 64 bits : stable d'une version de Rust à l'autre, ce que le
/// hacheur par défaut ne promet pas — et un fichier nommé par une empreinte
/// doit se retrouver.
fn fingerprint(source: &str, title: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.bytes().chain([0u8]).chain(title.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-notes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_file_is_named_by_its_path_whatever_its_title() {
        let a = file_name("/docs/Rapport d'étape.md", "Un titre");
        let b = file_name("/ailleurs/Rapport d'étape.md", "Un titre");
        assert!(a.starts_with("rapport-d-etape-"), "{a}");
        assert!(a.ends_with(".md"));
        assert_ne!(a, b, "deux chemins, deux notes");
        assert_eq!(a, file_name("/docs/Rapport d'étape.md", "Autre titre"), "le titre ne compte pas");
    }

    #[test]
    fn what_has_no_path_is_named_by_its_title() {
        assert!(file_name("presse-papiers", "").starts_with("presse-papiers-"));
        assert!(file_name("presse-papiers", "Un mail reçu").starts_with("un-mail-recu-"));
    }

    #[test]
    fn two_pastes_with_different_titles_keep_separate_notes() {
        assert_ne!(file_name("presse-papiers", "Un mail"), file_name("presse-papiers", "Un autre"));
    }

    #[test]
    fn a_title_of_symbols_still_gets_a_name() {
        assert!(file_name("???", "!!!").starts_with("note-"));
    }

    #[test]
    fn saving_then_loading_gives_the_text_back() {
        let dir = scratch("roundtrip");
        save(&dir, "a.md", "A", "première ligne\n").unwrap();
        assert_eq!(load(&dir, "a.md", "A"), "première ligne\n");
        assert_eq!(load(&dir, "b.md", "B"), "", "un autre document n'a rien");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_blank_note_removes_its_file() {
        let dir = scratch("blank");
        save(&dir, "a.md", "A", "quelque chose").unwrap();
        assert!(path(&dir, "a.md", "A").exists());
        save(&dir, "a.md", "A", "  \n").unwrap();
        assert!(!path(&dir, "a.md", "A").exists());
        // Effacer ce qui n'existe pas n'est pas une erreur.
        save(&dir, "a.md", "A", "").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_marker_names_the_section_and_the_time() {
        assert_eq!(marker("Introduction", "0:42"), "— Introduction · 0:42");
        assert_eq!(marker("", "0:42"), "— 0:42");
        assert_eq!(marker("Introduction", ""), "— Introduction");
    }

    #[test]
    fn a_marker_lands_on_its_own_line_with_room_to_write() {
        let mut text = String::new();
        append_marker(&mut text, "— Intro · 0:10");
        assert_eq!(text, "— Intro · 0:10\n");
        let mut text = "déjà écrit  \n\n".to_string();
        append_marker(&mut text, "— Suite · 1:00");
        assert_eq!(text, "déjà écrit\n\n— Suite · 1:00\n");
    }
}
