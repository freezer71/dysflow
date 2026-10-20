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
    let mut board = arboard::Clipboard::new().context(crate::tr!("accessing the clipboard", "accès au presse-papiers"))?;
    #[cfg(feature = "ocr")]
    if let Ok(image) = board.get_image() {
        let width = image.width as u32;
        let height = image.height as u32;
        let page = ocr::read_rgba(&image.bytes, width, height)?;
        return Ok(layout::to_markdown(&[page]));
    }
    #[cfg(feature = "ocr")]
    let absent = crate::tr!("the clipboard holds neither image nor text", "le presse-papiers ne contient ni image ni texte");
    #[cfg(not(feature = "ocr"))]
    let absent = crate::tr!(
        "the clipboard holds no text; \
         an image needs a binary built with the `ocr` option",
        "le presse-papiers ne contient pas de texte ; \
         une image demande un binaire compilé avec l'option `ocr`"
    );
    board.get_text().context(absent)
}

/// Reconnaît le texte d'une image donnée par ses octets (PNG, TIFF…).
///
/// L'app lit elle-même l'image du presse-papiers, après en avoir vérifié le
/// contenu : relire le presse-papiers ici pourrait tomber sur une copie
/// arrivée entre-temps — dissimulée, peut-être. Ni presse-papiers ni repli
/// sur du texte, donc.
pub fn image_bytes(bytes: &[u8]) -> Result<String> {
    #[cfg(feature = "ocr")]
    {
        let page = ocr::read_encoded(bytes)?;
        Ok(layout::to_markdown(&[page]))
    }
    #[cfg(not(feature = "ocr"))]
    {
        let _ = bytes;
        anyhow::bail!(crate::tr!(
            "an image needs a binary built with the `ocr` option",
            "une image demande un binaire compilé avec l'option `ocr`"
        ))
    }
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
        // Lu octet par octet et non comme de l'UTF-8 strict : un fichier en
        // latin-1 se lit avec quelques caractères de remplacement plutôt
        // que d'être refusé.
        Kind::Text => crate::input::decode_text(
            &std::fs::read(path).with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))?,
        ),
        Kind::Pdf => layout::to_markdown(&pdf::pages(path)?),
        #[cfg(feature = "ocr")]
        Kind::Image => layout::to_markdown(&ocr::pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Image => anyhow::bail!(crate::tr!(
            en = "{} is an image: this binary was built without the `ocr` option",
            fr = "{} est une image : ce binaire a été compilé sans l'option `ocr`",
            path.display(),
        )),
        #[cfg(feature = "ocr")]
        Kind::Directory => layout::to_markdown(&ocr::directory_pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Directory => anyhow::bail!(crate::tr!(
            en = "{} is a folder: this binary was built without the `ocr` option",
            fr = "{} est un dossier : ce binaire a été compilé sans l'option `ocr`",
            path.display(),
        )),
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
        let path = crate::paths::test_scratch("source-texte").join("source-texte.md");
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
        let out = load_with_cache(&fixture("spec.pdf"), false).expect("la fixture doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
        assert!(out.contains("2. Contraintes"), "seconde page absente : {out}");
    }

    #[cfg(feature = "ocr")]
    #[test]
    fn an_image_is_read_by_the_recogniser() {
        let out = load_with_cache(&fixture("capture.png"), false).expect("l'image doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
        assert!(
            out.contains("## 1. Contexte"),
            "la reconstruction n'a pas eu lieu : {out}"
        );
    }

    #[cfg(not(feature = "ocr"))]
    #[test]
    fn an_image_says_the_binary_was_built_without_ocr() {
        let path = crate::paths::test_scratch("source-image").join("source-image.png");
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

    #[cfg(feature = "ocr")]
    #[test]
    fn image_bytes_are_recognised_without_touching_the_clipboard() {
        let bytes = std::fs::read(fixture("capture.png")).expect("la fixture doit se lire");
        let out = image_bytes(&bytes).expect("l'image doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
    }

    #[cfg(feature = "ocr")]
    #[test]
    fn bytes_that_are_not_an_image_are_an_error() {
        assert!(image_bytes(b"du texte, pas une image").is_err());
    }

    #[cfg(not(feature = "ocr"))]
    #[test]
    fn image_bytes_say_the_binary_was_built_without_ocr() {
        let err = image_bytes(b"\x89PNG\r\n\x1a\n").unwrap_err().to_string();
        assert!(err.contains("ocr"), "l'erreur doit nommer l'option manquante : {err}");
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


    #[test]
    fn a_latin1_text_file_is_read_rather_than_refused() {
        let path = crate::paths::test_scratch("latin1").join("latin1.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFcaf\xE9 cr\xE8me\n").unwrap();
        let out = load_with_cache(&path, false).expect("un texte mal encodé se lit quand même");
        assert!(out.starts_with("caf"), "BOM ou octet fautif en tête : {out:?}");
        assert!(out.contains("cr\u{FFFD}me"), "{out:?}");
        std::fs::remove_file(&path).ok();
    }
}
