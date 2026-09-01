//! Entrée non textuelle : le PDF aujourd'hui, les images et les dossiers plus tard.
//!
//! Tout ce qui entre dans le lecteur passe par ici et en ressort en markdown ;
//! le reste du pipeline ignore d'où vient le texte.

pub mod cache;
pub mod kind;
pub mod layout;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod pdf;

use anyhow::{Context, Result};
use std::path::Path;

pub use kind::Kind;

/// Lit le presse-papiers : son image reconnue, ou son texte tel quel.
///
/// L'image passe d'abord : ce qu'on copie depuis un PDF verrouillé ou une
/// visio arrive en image, et c'est alors elle qui porte le contenu. Elle
/// demande la reconnaissance ; le texte, lui, ne demande rien.
#[cfg(feature = "clipboard")]
pub fn clipboard() -> Result<String> {
    let mut board = arboard::Clipboard::new().context("accès au presse-papiers")?;
    #[cfg(feature = "ocr")]
    if let Ok(image) = board.get_image() {
        let width = image.width as u32;
        let height = image.height as u32;
        let page = ocr::read_rgba(&image.bytes, width, height)?;
        return Ok(layout::to_markdown(&[page]));
    }
    #[cfg(feature = "ocr")]
    let absent = "le presse-papiers ne contient ni image ni texte";
    #[cfg(not(feature = "ocr"))]
    let absent = "le presse-papiers ne contient pas de texte ; \
                  une image demande un binaire compilé avec l'option `ocr`";
    board.get_text().context(absent)
}

/// Rend le contenu d'un chemin sous forme de markdown, quelle qu'en soit
/// l'origine.
pub fn load(path: &Path) -> Result<String> {
    load_with_cache(path, true)
}

/// Comme [`load`], en pouvant refuser le cache.
///
/// Une reconnaissance de texte coûte des secondes par page ; la refaire à
/// chaque ouverture d'une spécification relue sur plusieurs jours serait une
/// dépense pure.
pub fn load_with_cache(path: &Path, cached: bool) -> Result<String> {
    let kind = kind::of_path(path)?;
    // Un fichier texte est déjà du markdown : le mémoriser n'ajouterait qu'une
    // copie à entretenir.
    let key = match (cached, kind) {
        (true, Kind::Text) | (false, _) => None,
        (true, _) => cache::fingerprint(path).ok(),
    };
    if let Some(found) = key.as_deref().and_then(cache::read) {
        return Ok(found);
    }

    let markdown = match kind {
        Kind::Text => std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display()))?,
        Kind::Pdf => layout::to_markdown(&pdf::pages(path)?),
        #[cfg(feature = "ocr")]
        Kind::Image => layout::to_markdown(&ocr::pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Image => anyhow::bail!(
            "{} est une image : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
        #[cfg(feature = "ocr")]
        Kind::Directory => layout::to_markdown(&ocr::directory_pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Directory => anyhow::bail!(
            "{} est un dossier : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
    };
    if let Some(key) = key {
        cache::write(&key, &markdown);
    }
    Ok(markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_text_file_comes_back_unchanged() {
        let path = std::env::temp_dir().join("rs-reader-source-texte.md");
        std::fs::write(&path, "# Titre\n\nUn paragraphe.\n").expect("écriture du témoin");
        let out = load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(out.expect("lecture"), "# Titre\n\nUn paragraphe.\n");
    }

    #[test]
    fn a_second_reading_comes_from_the_cache() {
        let path = fixture("rapport.pdf");
        let key = cache::fingerprint(&path).expect("empreinte");
        let first = load(&path).expect("première lecture");
        assert!(
            cache::read(&key).is_some(),
            "la première lecture doit avoir mémorisé le document"
        );
        assert_eq!(load(&path).expect("seconde lecture"), first);
    }

    #[test]
    fn refusing_the_cache_still_reads_the_document() {
        let path = fixture("rapport.pdf");
        let direct = load_with_cache(&path, false).expect("lecture sans cache");
        assert!(direct.contains("## 1. Contexte"), "document tronqué : {direct}");
    }

    #[test]
    fn a_pdf_comes_back_as_markdown() {
        let out = load(&fixture("spec.pdf")).expect("la fixture doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
        assert!(out.contains("2. Contraintes"), "seconde page absente : {out}");
    }

    #[cfg(feature = "ocr")]
    #[test]
    fn an_image_is_read_by_the_recogniser() {
        let out = load(&fixture("capture.png")).expect("l'image doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
        assert!(
            out.contains("## 1. Contexte"),
            "la reconstruction n'a pas eu lieu : {out}"
        );
    }

    #[cfg(not(feature = "ocr"))]
    #[test]
    fn an_image_says_the_binary_was_built_without_ocr() {
        let path = std::env::temp_dir().join("rs-reader-source-image.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("écriture du témoin");
        let err = load(&path).unwrap_err();
        let _ = std::fs::remove_file(&path);
        let message = err.to_string();
        assert!(message.contains("image"), "l'erreur doit dire ce qui bloque : {message}");
        assert!(
            message.contains("ocr"),
            "l'erreur doit nommer l'option manquante : {message}"
        );
    }

    #[test]
    fn a_directory_without_images_says_so() {
        // Le dossier temporaire du système ne contient pas d'images de page ;
        // le message diffère selon que l'OCR est compilé, la plainte non.
        let err = load(&std::env::temp_dir()).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("dossier") || message.contains("aucune image"),
            "l'erreur doit dire ce qui bloque : {message}"
        );
    }
}
