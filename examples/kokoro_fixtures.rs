//! Phonétise les phrases françaises du spike comme le moteur le fera (par
//! `sillon-espeak`, `scripts/build-espeak.sh`) : la
//! phrase parlée entière, puis chaque mot affiché seul. Écrit les fixtures
//! d'alignement (le test tourne ensuite sans espeak-ng).
//!
//!   SILLON_ESPEAK=target/espeak/bin/sillon-espeak SILLON_ESPEAK_DATA=target/espeak/share \
//!     cargo run --example kokoro_fixtures -- tests/fixtures/kokoro/align-fr.json

use std::collections::BTreeMap;

use sillon::doc::builder::{parse_document_in, CodePolicy};
use sillon::tts::kokoro::phonemes::{EspeakProcess, Phonemizer};

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "tests/fixtures/kokoro/align-fr.json".into());
    let reference: BTreeMap<String, BTreeMap<String, String>> =
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/kokoro/misaki-phonemes.json")?)?;
    let mut espeak = EspeakProcess::locate()?;
    let mut fixtures = Vec::new();
    for text in reference["fr"].keys() {
        let doc = parse_document_in(text, CodePolicy::Announce, "fr");
        for sentence in &doc.sentences {
            let words = sentence.spoken_words();
            let mut items = vec![sentence.text.clone()];
            items.extend(words.iter().cloned());
            let ps = espeak.phonemize("fr", &items)?;
            fixtures.push(serde_json::json!({
                "text": sentence.written(),
                "sentence": ps[0],
                "words": words,
                "word_phonemes": ps[1..],
            }));
        }
    }
    std::fs::write(&out, serde_json::to_string_pretty(&fixtures)?)?;
    println!("{} phrases écrites dans {out}", fixtures.len());
    Ok(())
}
