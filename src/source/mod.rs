//! Entrée non textuelle : le PDF aujourd'hui, les images et les dossiers plus tard.
//!
//! Tout ce qui entre dans le lecteur passe par ici et en ressort en markdown ;
//! le reste du pipeline ignore d'où vient le texte.

pub mod kind;
pub mod layout;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod pdf;

use anyhow::{bail, Context, Result};
use std::path::Path;

pub use kind::Kind;

/// Rend le contenu d'un chemin sous forme de markdown, quelle qu'en soit
/// l'origine.
pub fn load(path: &Path) -> Result<String> {
    match kind::of_path(path)? {
        Kind::Text => std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display())),
        Kind::Pdf => Ok(layout::to_markdown(&pdf::pages(path)?)),
        #[cfg(feature = "ocr")]
        Kind::Image => Ok(layout::to_markdown(&ocr::pages(path)?)),
        #[cfg(not(feature = "ocr"))]
        Kind::Image => bail!(
            "{} est une image : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
        #[cfg(feature = "ocr")]
        Kind::Directory => Ok(layout::to_markdown(&ocr::directory_pages(path)?)),
        #[cfg(not(feature = "ocr"))]
        Kind::Directory => bail!(
            "{} est un dossier : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
    }
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
