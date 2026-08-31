//! Extraction du texte d'un PDF qui en porte une couche.
//!
//! Le texte est rendu page par page, et non d'un bloc : les règles de
//! reconstruction ont besoin de savoir où une page finit.

use anyhow::{bail, Context, Result};
use pdf_extract::{output_doc_page, Document, ObjectId, PlainTextOutput};
use std::path::Path;

/// Une page telle que le PDF la livre, avant toute reconstruction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub lines: Vec<String>,
}

/// Rend le texte du document, une entrée par page.
pub fn pages(path: &Path) -> Result<Vec<Page>> {
    let mut doc =
        Document::load(path).with_context(|| format!("lecture de {}", path.display()))?;
    // Un PDF « protégé » sans mot de passe s'ouvre avec une chaîne vide ; les
    // autres ne s'ouvriront pas, et mieux vaut le dire que rendre un document
    // vide.
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        bail!("{} est protégé par un mot de passe", path.display());
    }
    if doc.get_pages().is_empty() {
        bail!("{} ne contient aucune page lisible", path.display());
    }
    let mut out = Vec::new();
    for (number, _) in doc.get_pages() {
        let mut text = String::new();
        {
            let mut sink = PlainTextOutput::new(&mut text);
            output_doc_page(&doc, &mut sink, number)
                .with_context(|| format!("page {number} de {}", path.display()))?;
        }
        out.push(Page {
            lines: text.lines().map(|line| line.trim_end().to_string()).collect(),
        });
    }
    Ok(out)
}

/// Images embarquées d'une page, dans leur forme d'origine.
///
/// Seul `/DCTDecode` est rendu : ses octets sont un JPEG complet. Les
/// télécopies en CCITT ou JBIG2 demanderaient un décodeur que le pur Rust
/// n'offre pas — mieux vaut une page vide qu'un texte inventé.
fn page_images(doc: &Document, page: ObjectId) -> Vec<Vec<u8>> {
    let Ok((own, inherited)) = doc.get_page_resources(page) else {
        return Vec::new();
    };
    let mut tables = Vec::new();
    if let Some(dict) = own {
        if let Ok(x) = dict.get(b"XObject") {
            tables.push(x.clone());
        }
    }
    for id in inherited {
        if let Ok(dict) = doc.get_dictionary(id) {
            if let Ok(x) = dict.get(b"XObject") {
                tables.push(x.clone());
            }
        }
    }

    let mut images = Vec::new();
    for table in tables {
        let Ok((_, object)) = doc.dereference(&table) else {
            continue;
        };
        let Ok(entries) = object.as_dict() else {
            continue;
        };
        for (_, value) in entries.iter() {
            let Ok((_, object)) = doc.dereference(value) else {
                continue;
            };
            let Ok(stream) = object.as_stream() else {
                continue;
            };
            let named = |key: &[u8], expected: &[u8]| {
                stream
                    .dict
                    .get(key)
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .is_some_and(|name| name == expected)
            };
            if named(b"Subtype", b"Image") && named(b"Filter", b"DCTDecode") {
                images.push(stream.content.clone());
            }
        }
    }
    images
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
    fn a_scanned_page_yields_its_embedded_image() {
        let doc = Document::load(fixture("scanne.pdf")).expect("la fixture doit se lire");
        let (_, page) = doc.get_pages().into_iter().next().expect("une page");
        let images = page_images(&doc, page);
        assert_eq!(images.len(), 1, "la page porte une image");
        assert_eq!(&images[0][..2], &[0xFF, 0xD8], "ce doit être un JPEG");
    }

    #[test]
    fn a_text_page_yields_no_image() {
        let doc = Document::load(fixture("spec.pdf")).expect("la fixture doit se lire");
        let (_, page) = doc.get_pages().into_iter().next().expect("une page");
        assert!(page_images(&doc, page).is_empty());
    }

    #[test]
    fn a_pdf_yields_one_entry_per_page() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        assert_eq!(pages.len(), 2, "le document a deux pages");
    }

    #[test]
    fn accents_survive_the_extraction() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        let first = pages[0].lines.join("\n");
        assert!(
            first.contains("Spécification"),
            "les accents ont été perdus : {first}"
        );
        assert!(
            first.contains("règlements différés"),
            "les accents ont été perdus : {first}"
        );
    }

    #[test]
    fn each_page_holds_only_its_own_text() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        let second = pages[1].lines.join("\n");
        assert!(second.contains("2. Contraintes"), "page 2 : {second}");
        assert!(
            !second.contains("1. Contexte"),
            "la page 1 a débordé sur la page 2 : {second}"
        );
    }

    #[test]
    fn lines_come_back_without_their_right_padding() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        for line in pages.iter().flat_map(|p| &p.lines) {
            assert_eq!(line.trim_end(), line, "reste une fin de ligne : {line:?}");
        }
    }

    #[test]
    fn a_broken_file_names_itself_in_the_error() {
        let err = pages(&fixture("tronque.pdf")).unwrap_err();
        assert!(
            err.to_string().contains("tronque.pdf"),
            "l'erreur doit nommer le fichier : {err}"
        );
    }
}
