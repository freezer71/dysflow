//! Passe un document réel dans le pipeline et montre ce qui atteint le moteur.
//!
//! Diagnostic, pas régression : le chemin du document est passé en argument.
//!
//! ```sh
//! DOC=chemin.md cargo test --test document_probe -- --ignored --nocapture
//! ```

use sillon::doc::builder::CodePolicy;
use sillon::doc::DocumentBuilder;

#[test]
#[ignore = "sonde : demande un document"]
fn what_reaches_the_engine() {
    let path = std::env::var("DOC").expect("DOC=chemin.md");
    let text = std::fs::read_to_string(&path).expect("lecture");
    let mut builder = DocumentBuilder::new(CodePolicy::Announce);
    for block in sillon::doc::blocks::split_blocks(&text) {
        builder.push_block(&block);
    }
    builder.finish();
    let doc = builder.document();

    println!("{} phrases\n", doc.sentences.len());
    println!("=== phrases contenant encore un chiffre ===");
    let mut left = 0;
    for sentence in &doc.sentences {
        if sentence.text.chars().any(|c| c.is_ascii_digit()) {
            left += 1;
            println!("  {:?}", sentence.text);
        }
    }
    println!("\n{left} phrases avec des chiffres restants sur {}", doc.sentences.len());

    println!("\n=== mots suspects (collés ou isolés) ===");
    for sentence in &doc.sentences {
        for word in sentence.text.split_whitespace() {
            let letters = word.chars().filter(|c| c.is_alphabetic()).count();
            // Un mot d'une lettre hors élision, ou une casse recollée au milieu.
            if letters == 1 && !word.ends_with('\u{2019}') && !"aàyOoùÀÔ".contains(word.trim_matches(|c: char| !c.is_alphabetic())) {
                println!("  isolé {word:?} dans {:?}", sentence.text);
            }
        }
    }

    println!("\n=== écrit \u{2260} dit ===");
    for sentence in doc.sentences.iter().take(200) {
        let shown = sentence.written();
        if shown != sentence.text {
            println!("  œil  {shown:?}");
            println!("  voix {:?}\n", sentence.text);
        }
    }

    println!("\n=== titres ===");
    for section in doc.sections.iter().take(14) {
        println!("  {:?}", section.title);
    }

    println!("\n=== échantillon : ce que l'œil lit, puis ce que la voix dit ===");
    for sentence in doc.sentences.iter().skip(40).take(8) {
        println!("  œil  {:?}", sentence.written());
        println!("  voix {:?}\n", sentence.text);
    }
}
