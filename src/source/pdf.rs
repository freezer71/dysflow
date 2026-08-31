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
    let numbered: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let numbers: Vec<ObjectId> = numbered.iter().map(|(_, id)| *id).collect();
    let mut out = Vec::new();
    for (number, _) in &numbered {
        let mut text = String::new();
        {
            let mut sink = PlainTextOutput::new(&mut text);
            output_doc_page(&doc, &mut sink, *number)
                .with_context(|| format!("page {number} de {}", path.display()))?;
        }
        out.push(Page {
            lines: text.lines().map(|line| line.trim_end().to_string()).collect(),
        });
    }

    // Une page sans couche texte n'est pas vide : son contenu est une image.
    let blank: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, page)| looks_scanned(page))
        .map(|(i, _)| i)
        .collect();
    if !blank.is_empty() {
        recognise_scanned(&doc, &numbers, &blank, &mut out, path)?;
    }
    Ok(out)
}

/// Caractères imprimables en deçà desquels une page est réputée scannée.
///
/// Sous ce seuil, ce qui reste est un artefact — un numéro de page isolé, une
/// légende — et non du contenu lisible.
const SCANNED_BELOW: usize = 30;

/// Vrai si la page ne porte pas de couche texte exploitable.
fn looks_scanned(page: &Page) -> bool {
    page.lines
        .iter()
        .flat_map(|l| l.chars())
        .filter(|c| !c.is_whitespace())
        .count()
        < SCANNED_BELOW
}

/// Reconnaît les pages sans couche texte, en parallèle.
///
/// Le parallélisme remplace ici la lecture en flux : `layout` a besoin du
/// document entier pour reconnaître les en-têtes courants, mais rien n'oblige
/// à lire les pages l'une après l'autre.
#[cfg(feature = "ocr")]
fn recognise_scanned(
    doc: &Document,
    numbers: &[ObjectId],
    blank: &[usize],
    out: &mut [Page],
    path: &Path,
) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    // Une image par page suffit : un scan n'en porte qu'une, et deux
    // reconnaissances sur la même page se recouvriraient.
    let work: Vec<(usize, Vec<u8>)> = blank
        .iter()
        .filter_map(|&i| {
            let image = page_images(doc, *numbers.get(i)?).into_iter().next()?;
            Some((i, image))
        })
        .collect();
    if work.is_empty() {
        return Ok(());
    }

    // Autant de fils que de cœurs, sans dépasser le nombre de pages : ouvrir un
    // moteur coûte moins que de lire une page, mais pas moins que rien.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(work.len());
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Page)>> = Mutex::new(Vec::new());

    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            handles.push(scope.spawn(|| -> Result<()> {
                let engine = super::ocr::Ocr::new()?;
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((index, bytes)) = work.get(i) else {
                        return Ok(());
                    };
                    let image = image::load_from_memory(bytes)
                        .with_context(|| format!("image de la page {}", index + 1))?;
                    let page = engine.read(&image)?;
                    done.lock()
                        .map_err(|_| anyhow::anyhow!("fil de reconnaissance interrompu"))?
                        .push((*index, page));
                }
            }));
        }
        for handle in handles {
            handle.join().map_err(|_| {
                anyhow::anyhow!("reconnaissance de {} interrompue", path.display())
            })??;
        }
        Ok(())
    })?;

    for (index, page) in done
        .into_inner()
        .map_err(|_| anyhow::anyhow!("fil de reconnaissance interrompu"))?
    {
        out[index] = page;
    }
    Ok(())
}

/// Sans l'option `ocr`, une page scannée le dit plutôt que de rester vide.
#[cfg(not(feature = "ocr"))]
fn recognise_scanned(
    doc: &Document,
    numbers: &[ObjectId],
    blank: &[usize],
    _out: &mut [Page],
    path: &Path,
) -> Result<()> {
    let has_image = blank
        .iter()
        .filter_map(|&i| numbers.get(i))
        .any(|id| !page_images(doc, *id).is_empty());
    if has_image {
        bail!(
            "{} est un document scanné : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        );
    }
    Ok(())
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

    #[cfg(feature = "ocr")]
    #[test]
    fn a_scanned_page_is_read_by_the_recogniser() {
        let read = pages(&fixture("scanne.pdf")).expect("la fixture doit se lire");
        let text = read[0].lines.join("\n");
        assert!(
            text.contains("Spécification"),
            "la page scannée est restée vide : {text:?}"
        );
    }

    #[cfg(not(feature = "ocr"))]
    #[test]
    fn a_scanned_page_says_what_is_missing() {
        let err = pages(&fixture("scanne.pdf")).unwrap_err();
        assert!(
            err.to_string().contains("ocr"),
            "l'erreur doit nommer l'option manquante : {err}"
        );
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
