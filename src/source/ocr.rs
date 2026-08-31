//! Reconnaissance de texte, par Tesseract compilé dans le binaire.
//!
//! Le moteur rend des [`Page`] comme le fait un PDF : la reconstruction du
//! module `layout` s'applique ensuite sans rien savoir de l'origine du texte.

use anyhow::{Context, Result};
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

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
