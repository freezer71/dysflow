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
    let dir = crate::paths::cache_dir()
        .context("répertoire de cache introuvable")?
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

/// Fragment d'un nom de fichier, pour un tri qui compte comme un humain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Natural {
    Text(String),
    Number(u64),
}

/// Découpe un nom en fragments comparables : les chiffres comptent comme des
/// nombres, pas comme des lettres.
///
/// Un tri alphabétique placerait `page10` avant `page9`, et le document serait
/// dans le désordre.
fn natural_key(name: &str) -> Vec<Natural> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    let close_digits = |digits: &mut String, out: &mut Vec<Natural>| {
        if digits.is_empty() {
            return;
        }
        // Un nombre trop long pour tenir dans un u64 reste du texte.
        match digits.parse() {
            Ok(number) => out.push(Natural::Number(number)),
            Err(_) => out.push(Natural::Text(digits.clone())),
        }
        digits.clear();
    };
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !text.is_empty() {
                out.push(Natural::Text(std::mem::take(&mut text)));
            }
            digits.push(c);
        } else {
            close_digits(&mut digits, &mut out);
            text.push(c.to_ascii_lowercase());
        }
    }
    if !text.is_empty() {
        out.push(Natural::Text(text));
    }
    close_digits(&mut digits, &mut out);
    out
}

/// Reconnaît le texte de toutes les images d'un dossier, dans l'ordre.
pub fn directory_pages(dir: &Path) -> Result<Vec<Page>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("lecture de {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| matches!(super::kind::of_path(path), Ok(super::Kind::Image)))
        .collect();
    if files.is_empty() {
        anyhow::bail!("{} ne contient aucune image lisible", dir.display());
    }
    files.sort_by_key(|path| {
        natural_key(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    });

    // Un seul moteur pour tout le dossier : son ouverture coûte plus que la
    // lecture d'une page.
    let engine = Ocr::new()?;
    let mut pages = Vec::with_capacity(files.len());
    for file in files {
        let image = image::open(&file)
            .with_context(|| format!("lecture de l'image {}", file.display()))?;
        pages.push(engine.read(&image)?);
    }
    Ok(pages)
}

/// Reconnaît une image donnée en pixels bruts, comme le presse-papiers la rend.
pub fn read_rgba(pixels: &[u8], width: u32, height: u32) -> Result<Page> {
    let buffer = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .context("les pixels ne forment pas une image de cette taille")?;
    Ocr::new()?.read(&image::DynamicImage::ImageRgba8(buffer))
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
    fn raw_pixels_from_the_clipboard_are_recognised() {
        let image = image::open(fixture("capture.png")).expect("la fixture doit se décoder");
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        let page = read_rgba(&rgba, width, height).expect("les pixels doivent se lire");
        assert!(
            page.lines.join("\n").contains("Spécification"),
            "le presse-papiers n'a pas été reconnu : {:?}",
            page.lines
        );
    }

    #[test]
    fn page_ten_comes_after_page_nine() {
        let mut names = ["page10.png", "page9.png", "page1.png"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["page1.png", "page9.png", "page10.png"]);
    }

    #[test]
    fn names_without_numbers_stay_alphabetical() {
        let mut names = ["avant.png", "zeta.png", "beta.png"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["avant.png", "beta.png", "zeta.png"]);
    }

    #[test]
    fn a_directory_of_captures_is_read_in_order() {
        let dir = std::env::temp_dir().join("sillon-scans");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("création du dossier");
        // La même capture deux fois : l'ordre est vérifiable, le contenu connu.
        for name in ["page2.png", "page1.png"] {
            std::fs::copy(fixture("capture.png"), dir.join(name)).expect("copie");
        }
        let read = directory_pages(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let read = read.expect("le dossier doit se lire");
        assert_eq!(read.len(), 2, "une image par page");
        assert!(read[0].lines.join("\n").contains("Spécification"));
    }

    #[test]
    fn a_directory_without_images_says_so() {
        let dir = std::env::temp_dir().join("sillon-vide");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("création du dossier");
        let err = directory_pages(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(err.is_err(), "un dossier sans image doit le dire");
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
