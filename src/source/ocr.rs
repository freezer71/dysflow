//! Reconnaissance de texte, par Tesseract compilé dans le binaire.
//!
//! Le moteur rend des [`Page`] comme le fait un PDF : la reconstruction du
//! module `layout` s'applique ensuite sans rien savoir de l'origine du texte.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::pdf::Page;

/// Langues reconnues, dans l'ordre de préférence.
///
/// Tesseract accepte plusieurs langues à la fois ; le français d'abord, car
/// c'est celui des documents lus, l'anglais ensuite pour les termes techniques.
const LANGUAGES: &str = "fra+eng";

/// Données de langue, embarquées pour que rien ne soit à installer.
const FRA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/tessdata/fra.traineddata"
));
const ENG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/tessdata/eng.traineddata"
));

/// Dépose les données de langue dans le cache et rend leur répertoire.
///
/// La taille sert de témoin : réécrire cinq mégaoctets à chaque lancement
/// serait inutile, mais un fichier tronqué doit être remplacé.
fn tessdata_dir() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .context("répertoire de cache introuvable")?
        .join("rs-reader")
        .join("tessdata");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("création de {}", dir.display()))?;
    for (lang, data) in [("fra", FRA), ("eng", ENG)] {
        let file = dir.join(format!("{lang}.traineddata"));
        let current = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        if current != data.len() as u64 {
            // Écriture puis renommage : deux lecteurs lancés en même temps ne
            // doivent jamais voir un fichier à moitié écrit.
            let temp = dir.join(format!("{lang}.{}.tmp", std::process::id()));
            std::fs::write(&temp, data)
                .with_context(|| format!("écriture de {}", temp.display()))?;
            std::fs::rename(&temp, &file)
                .with_context(|| format!("mise en place de {}", file.display()))?;
        }
    }
    Ok(dir)
}

/// Un moteur de reconnaissance, prêt à lire.
pub struct Ocr {
    api: tesseract_rs::TesseractAPI,
}

impl Ocr {
    /// Ouvre un moteur, données de langue comprises.
    pub fn new() -> Result<Self> {
        let dir = tessdata_dir()?;
        let api = tesseract_rs::TesseractAPI::new();
        api.init(dir.to_string_lossy().as_ref(), LANGUAGES)
            .map_err(|e| anyhow::anyhow!("ouverture du moteur de reconnaissance : {e}"))?;
        Ok(Self { api })
    }

    /// Reconnaît le texte d'une image déjà décodée.
    ///
    /// L'image est ramenée en niveaux de gris : Tesseract y travaille mieux, et
    /// la couleur n'apporte rien à de la reconnaissance de texte.
    pub fn read(&self, image: &image::DynamicImage) -> Result<Page> {
        let grey = image.to_luma8();
        let (width, height) = grey.dimensions();
        self.api
            .set_image(grey.as_raw(), width as i32, height as i32, 1, width as i32)
            .map_err(|e| anyhow::anyhow!("chargement de l'image : {e}"))?;
        let text = self
            .api
            .get_utf8_text()
            .map_err(|e| anyhow::anyhow!("reconnaissance du texte : {e}"))?;
        Ok(Page {
            lines: text.lines().map(|l| l.trim_end().to_string()).collect(),
        })
    }
}

/// Reconnaît le texte d'un fichier image.
pub fn pages(path: &Path) -> Result<Vec<Page>> {
    let image =
        image::open(path).with_context(|| format!("lecture de l'image {}", path.display()))?;
    Ok(vec![Ocr::new()?.read(&image)?])
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
    fn a_capture_is_read_with_its_accents() {
        let read = pages(&fixture("capture.png")).expect("l'image doit se lire");
        assert_eq!(read.len(), 1, "une image fait une page");
        let text = read[0].lines.join("\n");
        assert!(text.contains("Spécification"), "accents perdus : {text}");
        assert!(text.contains("règlements différés"), "accents perdus : {text}");
        assert!(text.contains("1. Contexte"), "structure perdue : {text}");
    }

    #[test]
    fn a_file_that_is_not_an_image_says_so() {
        let err = pages(&fixture("spec.pdf")).unwrap_err();
        assert!(
            err.to_string().contains("spec.pdf"),
            "l'erreur doit nommer le fichier : {err}"
        );
    }

    #[test]
    fn the_language_data_lands_on_disk() {
        let dir = tessdata_dir().expect("le répertoire doit être préparé");
        for lang in ["fra", "eng"] {
            let file = dir.join(format!("{lang}.traineddata"));
            assert!(file.is_file(), "{} manque", file.display());
            let size = std::fs::metadata(&file).expect("taille").len();
            assert!(size > 100_000, "{} est tronqué : {size} octets", file.display());
        }
    }

    #[test]
    fn preparing_twice_is_harmless() {
        let first = tessdata_dir().expect("premier appel");
        let second = tessdata_dir().expect("second appel");
        assert_eq!(first, second);
    }
}
