//! Lecture d'une image : de la capture jusqu'aux phrases prononcées.
#![cfg(feature = "ocr")]

use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn a_capture_becomes_markdown_the_reader_can_navigate() {
    let markdown = sillon::source::load_with_cache(&fixture("capture.png"), false).expect("l'image doit se lire");
    assert!(markdown.contains("Spécification"), "accents perdus : {markdown}");
    assert!(
        markdown.contains("## 1. Contexte"),
        "la reconstruction ne s'est pas appliquée au texte reconnu : {markdown}"
    );
}

#[test]
fn a_capture_produces_sentences_to_speak() {
    use sillon::doc::builder::{parse_document, CodePolicy};

    let markdown = sillon::source::load_with_cache(&fixture("capture.png"), false).expect("l'image doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    let spoken: String = doc.sentences.iter().map(|s| s.text.as_str()).collect();
    assert!(
        spoken.contains("règlements"),
        "le texte reconnu n'a pas atteint la synthèse : {spoken}"
    );
}
