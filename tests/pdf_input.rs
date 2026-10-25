//! Lecture d'un PDF : de la fixture jusqu'aux blocs que le lecteur navigue.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn the_binary_writes_the_extracted_markdown_and_stops() {
    let out = Command::new(env!("CARGO_BIN_EXE_dysflow"))
        .arg(fixture("spec.pdf"))
        .arg("--to-markdown")
        .output()
        .expect("le binaire doit se lancer");

    assert!(
        out.status.success(),
        "sortie en erreur : {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("sortie en UTF-8");
    assert!(text.contains("Spécification"), "accents perdus : {text}");
    assert!(text.contains("2. Contraintes"), "seconde page absente : {text}");
}

#[test]
fn a_pdf_becomes_markdown_the_reader_can_navigate() {
    let markdown = dysflow::source::load_with_cache(&fixture("spec.pdf"), false).expect("le PDF doit se lire");
    let blocks = dysflow::doc::blocks::split_blocks(&markdown);
    assert!(
        blocks.len() >= 2,
        "le document doit se découper en blocs navigables, obtenu {} : {blocks:?}",
        blocks.len()
    );
}

#[test]
fn a_pdf_produces_sentences_to_speak() {
    use dysflow::doc::builder::{parse_document, CodePolicy};

    let markdown = dysflow::source::load_with_cache(&fixture("spec.pdf"), false).expect("le PDF doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    assert!(!doc.sentences.is_empty(), "aucune phrase à prononcer");
    let spoken: String = doc.sentences.iter().map(|s| s.text.as_str()).collect();
    assert!(
        spoken.contains("règlements"),
        "le texte du PDF n'a pas atteint la synthèse : {spoken}"
    );
}

#[test]
fn a_laid_out_pdf_is_rebuilt_into_readable_markdown() {
    let markdown = dysflow::source::load_with_cache(&fixture("rapport.pdf"), false).expect("le PDF doit se lire");

    assert!(
        !markdown.contains("Confidentiel"),
        "l'en-tête courant subsiste : {markdown}"
    );
    assert!(
        !markdown.contains("Page 1 sur"),
        "le pied courant subsiste : {markdown}"
    );
    assert!(
        markdown.contains("règlements différés sur trente jours"),
        "la césure n'a pas été recollée : {markdown}"
    );
    assert!(
        markdown.contains("par le prestataire externe"),
        "la seconde césure n'a pas été recollée : {markdown}"
    );
    assert!(
        markdown.contains("## 1. Contexte"),
        "le titre n'est pas devenu un titre : {markdown}"
    );
    assert!(
        markdown.contains("### 2.1 Latence"),
        "le sous-titre n'a pas son niveau : {markdown}"
    );
    assert!(
        markdown.contains("- latence maximale de 250 ms"),
        "la puce n'est pas devenue une liste : {markdown}"
    );
    assert!(
        markdown.contains("aux opérations de remboursement"),
        "la phrase coupée par le saut de page n'a pas été refermée : {markdown}"
    );
}

#[test]
fn the_rebuilt_pdf_fills_the_table_of_contents() {
    use dysflow::doc::builder::{parse_document, CodePolicy};

    let markdown = dysflow::source::load_with_cache(&fixture("rapport.pdf"), false).expect("le PDF doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    assert!(
        doc.sections.len() >= 4,
        "le sommaire doit porter les quatre titres, obtenu {} : {:?}",
        doc.sections.len(),
        doc.sections.iter().map(|s| &s.title).collect::<Vec<_>>()
    );
}

#[cfg(feature = "ocr")]
#[test]
fn a_scanned_pdf_is_recognised_and_rebuilt() {
    // Le cache est écarté : un test doit mesurer la chaîne, pas un souvenir.
    let markdown = dysflow::source::load_with_cache(&fixture("scanne.pdf"), false)
        .expect("le scan doit se lire");
    assert!(
        markdown.contains("Spécification"),
        "le scan est resté vide : {markdown}"
    );
    assert!(
        markdown.contains("## 1. Contexte"),
        "la reconstruction ne s'est pas appliquée : {markdown}"
    );
}
