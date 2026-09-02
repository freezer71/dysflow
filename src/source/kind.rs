//! Nature d'une entrée, décidée sur ses premiers octets.
//!
//! L'extension ment : un PDF enregistré en `.txt` reste un PDF, et une capture
//! d'écran renommée reste une image. Les octets tranchent, jamais le nom.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Pdf,
    Image,
    Directory,
}

/// Octets suffisants pour reconnaître toutes les signatures connues.
const HEAD: usize = 16;

/// Reconnaît une entrée à ses premiers octets.
pub fn sniff(head: &[u8]) -> Kind {
    if head.starts_with(b"%PDF-") {
        return Kind::Pdf;
    }
    // `ftyp` sert aussi aux vidéos : c'est la marque qui suit qui décide.
    let heif = head.len() >= 12
        && &head[4..8] == b"ftyp"
        && matches!(&head[8..12], b"heic" | b"heix" | b"hevc" | b"mif1" | b"avif");
    let webp = head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP";
    if head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"\xff\xd8\xff")
        || head.starts_with(b"II*\0")
        || head.starts_with(b"MM\0*")
        || head.starts_with(b"BM")
        || webp
        || heif
    {
        return Kind::Image;
    }
    Kind::Text
}

/// Reconnaît ce que désigne un chemin, sans le lire entièrement.
pub fn of_path(path: &Path) -> Result<Kind> {
    if path.is_dir() {
        return Ok(Kind::Directory);
    }
    let mut file =
        File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut head = [0u8; HEAD];
    let read = file
        .read(&mut head)
        .with_context(|| format!("lecture de {}", path.display()))?;
    Ok(sniff(&head[..read]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_is_recognised_by_its_header() {
        assert_eq!(sniff(b"%PDF-1.7\n1 0 obj"), Kind::Pdf);
    }

    #[test]
    fn the_usual_image_signatures_are_recognised() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Kind::Image);
        assert_eq!(sniff(b"\xff\xd8\xff\xe0\0\x10JFIF\0\x01"), Kind::Image);
        assert_eq!(sniff(b"II*\0\x08\0\0\0"), Kind::Image);
        assert_eq!(sniff(b"MM\0*\0\0\0\x08"), Kind::Image);
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Kind::Image);
        assert_eq!(sniff(b"BM\x36\0\0\0\0\0\0\0"), Kind::Image);
    }

    #[test]
    fn anything_else_is_read_as_text() {
        assert_eq!(sniff(b"# Titre\n\nUn paragraphe."), Kind::Text);
        assert_eq!(sniff(b""), Kind::Text);
    }

    #[test]
    fn a_video_is_not_mistaken_for_an_image() {
        // `ftyp` couvre les vidéos autant que le HEIC : seule la marque qui
        // suit distingue les deux.
        assert_eq!(sniff(b"\0\0\0\x18ftypmp42"), Kind::Text);
        assert_eq!(sniff(b"\0\0\0\x18ftypheic"), Kind::Image);
    }

    #[test]
    fn the_header_wins_over_the_extension() {
        let path = std::env::temp_dir().join("sillon-menteur.txt");
        std::fs::write(&path, b"%PDF-1.4\n1 0 obj\n").expect("écriture du témoin");
        let kind = of_path(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(kind.expect("lecture du témoin"), Kind::Pdf);
    }

    #[test]
    fn a_directory_is_recognised_without_being_opened() {
        assert_eq!(
            of_path(&std::env::temp_dir()).expect("lecture du dossier"),
            Kind::Directory
        );
    }

    #[test]
    fn a_missing_file_names_itself_in_the_error() {
        let err = of_path(Path::new("/introuvable/absolument")).unwrap_err();
        assert!(
            err.to_string().contains("introuvable"),
            "l'erreur doit nommer le fichier : {err}"
        );
    }
}
