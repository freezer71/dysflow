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
    let out = Command::new(env!("CARGO_BIN_EXE_reader"))
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
    let markdown = rs_reader::source::load(&fixture("spec.pdf")).expect("le PDF doit se lire");
    let blocks = rs_reader::input::split_blocks(&markdown);
    assert!(
        blocks.len() >= 2,
        "le document doit se découper en blocs navigables, obtenu {} : {blocks:?}",
        blocks.len()
    );
}

#[test]
fn a_pdf_produces_sentences_to_speak() {
    use rs_reader::doc::builder::{parse_document, CodePolicy};

    let markdown = rs_reader::source::load(&fixture("spec.pdf")).expect("le PDF doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    assert!(!doc.sentences.is_empty(), "aucune phrase à prononcer");
    let spoken: String = doc.sentences.iter().map(|s| s.text.as_str()).collect();
    assert!(
        spoken.contains("règlements"),
        "le texte du PDF n'a pas atteint la synthèse : {spoken}"
    );
}
