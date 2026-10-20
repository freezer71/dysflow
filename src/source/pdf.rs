//! Extraction du texte d'un PDF qui en porte une couche.
//!
//! Le texte est rendu page par page, et non d'un bloc : les règles de
//! reconstruction ont besoin de savoir où une page finit.

use anyhow::{bail, Context, Result};
use pdf_extract::{
    output_doc, output_doc_page, ConvertToFmt, Document, MediaBox, ObjectId, OutputDev,
    OutputError, PlainTextOutput, Transform,
};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Une page telle que le PDF la livre, avant toute reconstruction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub lines: Vec<String>,
}

/// Rend le texte du document, une entrée par page.
pub fn pages(path: &Path) -> Result<Vec<Page>> {
    let mut doc =
        Document::load(path).with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))?;
    // Un PDF « protégé » sans mot de passe s'ouvre avec une chaîne vide ; les
    // autres ne s'ouvriront pas, et mieux vaut le dire que rendre un document
    // vide.
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        bail!(crate::tr!(en = "{} is protected by a password", fr = "{} est protégé par un mot de passe", path.display()));
    }
    let numbered: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    if numbered.is_empty() {
        bail!(crate::tr!(en = "{} contains no readable page", fr = "{} ne contient aucune page lisible", path.display()));
    }
    let numbers: Vec<ObjectId> = numbered.iter().map(|(_, id)| *id).collect();
    // D'un seul passage, les polices ne se lisent qu'une fois pour tout le
    // document. Une page au flux invalide fait échouer ce passage : on
    // reprend alors page par page, où elle seule reste vide.
    let mut failures = Vec::new();
    let mut out = match whole_document(&doc, numbered.len()) {
        Some(pages) => pages,
        None => numbered
            .iter()
            .map(|(number, _)| {
                let mut text = String::new();
                {
                    let mut sink = PlainTextOutput::new(&mut text);
                    if let Err(err) = output_doc_page(&doc, &mut sink, *number) {
                        failures.push(crate::tr!(en = "page {number}: {err}", fr = "page {number} : {err}"));
                        text.clear();
                    }
                }
                page_of(&text)
            })
            .collect(),
    };
    if failures.len() == out.len() {
        bail!(crate::tr!(
            en = "{}: no page can be read ({})",
            fr = "{} : aucune page ne se lit ({})",
            path.display(),
            failures.join(crate::tr!("; ", " ; ")),
        ));
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

fn page_of(text: &str) -> Page {
    Page {
        lines: text.lines().map(|line| line.trim_end().to_string()).collect(),
    }
}

/// Toutes les pages d'un seul passage, ou `None` si une page échoue.
///
/// Chaque page repart d'une sortie neuve, comme rendue seule : le texte est
/// le même qu'en page par page, sans relire polices et arbre des pages à
/// chaque fois.
fn whole_document(doc: &Document, count: usize) -> Option<Vec<Page>> {
    let mut split = PageSplitter { pages: Vec::with_capacity(count), current: None };
    output_doc(doc, &mut split).ok()?;
    (split.pages.len() == count).then_some(split.pages)
}

/// Texte d'une page en cours d'écriture, partagé avec la sortie qui l'écrit.
#[derive(Clone, Default)]
struct PageText(Rc<RefCell<String>>);

impl std::fmt::Write for PageText {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.borrow_mut().push_str(s);
        Ok(())
    }
}

impl ConvertToFmt for PageText {
    type Writer = PageText;
    fn convert(self) -> PageText {
        self
    }
}

struct PageSplitter {
    pages: Vec<Page>,
    current: Option<(PageText, PlainTextOutput<PageText>)>,
}

impl PageSplitter {
    fn sink(&mut self) -> Result<&mut PlainTextOutput<PageText>, OutputError> {
        match self.current.as_mut() {
            Some((_, sink)) => Ok(sink),
            None => Err(OutputError::FormatError(std::fmt::Error)),
        }
    }
}

impl OutputDev for PageSplitter {
    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &MediaBox,
        art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        let text = PageText::default();
        let mut sink = PlainTextOutput::new(text.clone());
        sink.begin_page(page_num, media_box, art_box)?;
        self.current = Some((text, sink));
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        self.sink()?.end_page()?;
        if let Some((text, _)) = self.current.take() {
            self.pages.push(page_of(&text.0.borrow()));
        }
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        spacing: f64,
        font_size: f64,
        char: &str,
    ) -> Result<(), OutputError> {
        self.sink()?.output_character(trm, width, spacing, font_size, char)
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        self.sink()?.begin_word()
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        self.sink()?.end_word()
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        self.sink()?.end_line()
    }
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
    // reconnaissances sur la même page se recouvriraient. Les octets restent
    // ceux du document, sans copie : un livre scanné en pèse des centaines
    // de mégaoctets.
    let work: Vec<(usize, &[u8])> = blank
        .iter()
        .filter_map(|&i| Some((i, page_image(doc, *numbers.get(i)?)?)))
        .collect();
    if work.is_empty() {
        return Ok(());
    }

    // Un fil par cœur, moins un laissé à la lecture et à l'écran, et pas plus
    // de quatre : chaque fil tient un Tesseract et une page décodée (des
    // dizaines de mégaoctets), et une petite machine n'a pas la mémoire de
    // huit. Jamais plus que de pages.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(1)
        .clamp(1, 4)
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
                        .with_context(|| crate::tr!(en = "image of page {}", fr = "image de la page {}", index + 1))?;
                    let page = engine.read(&image)?;
                    done.lock()
                        .map_err(|_| anyhow::anyhow!(crate::tr!("recognition thread interrupted", "fil de reconnaissance interrompu")))?
                        .push((*index, page));
                }
            }));
        }
        for handle in handles {
            handle.join().map_err(|_| {
                anyhow::anyhow!(crate::tr!(
                    en = "recognition of {} interrupted",
                    fr = "reconnaissance de {} interrompue",
                    path.display(),
                ))
            })??;
        }
        Ok(())
    })?;

    for (index, page) in done
        .into_inner()
        .map_err(|_| anyhow::anyhow!(crate::tr!("recognition thread interrupted", "fil de reconnaissance interrompu")))?
    {
        out[index] = page;
    }
    Ok(())
}

/// Sans l'option `ocr`, une page scannée le dit plutôt que de rester vide.
///
/// Un document entièrement scanné est refusé : il n'y a rien à lire. Une page
/// image parmi des pages de texte — une couverture photographiée — ne condamne
/// pas le reste : elle porte une ligne qui dit ce qui manque, et la lecture
/// continue.
#[cfg(not(feature = "ocr"))]
fn recognise_scanned(
    doc: &Document,
    numbers: &[ObjectId],
    blank: &[usize],
    out: &mut [Page],
    path: &Path,
) -> Result<()> {
    let scanned: Vec<usize> = blank
        .iter()
        .copied()
        .filter(|&i| numbers.get(i).is_some_and(|id| page_image(doc, *id).is_some()))
        .collect();
    if scanned.is_empty() {
        return Ok(());
    }
    if scanned.len() == out.len() {
        bail!(crate::tr!(
            en = "{} is a scanned document: this binary was built without the `ocr` option",
            fr = "{} est un document scanné : ce binaire a été compilé sans l'option `ocr`",
            path.display(),
        ));
    }
    for i in scanned {
        out[i].lines.push(format!(
            "(page {} : image sans couche texte — sa lecture demande l'option `ocr`)",
            i + 1
        ));
    }
    Ok(())
}

/// Première image embarquée d'une page, dans sa forme d'origine, empruntée
/// au document.
///
/// Seul `/DCTDecode` est rendu : ses octets sont un JPEG complet. Les
/// télécopies en CCITT ou JBIG2 demanderaient un décodeur que le pur Rust
/// n'offre pas — mieux vaut une page vide qu'un texte inventé.
fn page_image(doc: &Document, page: ObjectId) -> Option<&[u8]> {
    let (own, inherited) = doc.get_page_resources(page).ok()?;
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

    for table in tables {
        let Ok((_, object)) = doc.dereference(&table) else {
            continue;
        };
        let Ok(entries) = object.as_dict() else {
            continue;
        };
        for (_, value) in entries.iter() {
            // Un flux est toujours un objet indirect : le lire par son
            // numéro le rattache au document, pas à la table copiée.
            let Ok(id) = value.as_reference() else {
                continue;
            };
            let Some(stream) = doc
                .get_object(id)
                .and_then(|object| doc.dereference(object))
                .ok()
                .and_then(|(_, object)| object.as_stream().ok())
            else {
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
                return Some(&stream.content);
            }
        }
    }
    None
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
        let image = page_image(&doc, page).expect("la page porte une image");
        assert_eq!(&image[..2], &[0xFF, 0xD8], "ce doit être un JPEG");
    }

    #[test]
    fn a_text_page_yields_no_image() {
        let doc = Document::load(fixture("spec.pdf")).expect("la fixture doit se lire");
        let (_, page) = doc.get_pages().into_iter().next().expect("une page");
        assert!(page_image(&doc, page).is_none());
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


    #[cfg(not(feature = "ocr"))]
    #[test]
    fn one_image_page_does_not_forbid_the_rest_of_the_document() {
        // Une couverture photographiée dans un rapport textuel est le cas
        // courant : refuser tout le document pour elle, c'est ne rien lire.
        let read = pages(&fixture("mixte.pdf")).expect("le texte doit se lire");
        assert_eq!(read.len(), 2);
        assert!(read[0].lines.join("\n").contains("Contexte"));
        let note = read[1].lines.join("\n");
        assert!(note.contains("ocr"), "la page image doit dire ce qui manque : {note:?}");
    }

    #[cfg(feature = "ocr")]
    #[test]
    fn an_image_page_among_text_pages_is_recognised() {
        let read = pages(&fixture("mixte.pdf")).expect("la fixture doit se lire");
        assert!(read[0].lines.join("\n").contains("Contexte"));
        assert!(read[1].lines.join("\n").contains("Spécification"), "{:?}", read[1]);
    }
}
