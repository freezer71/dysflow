//! Phonétise les phrases du spike comme le moteur le fera (par
//! `sillon-espeak`, `scripts/build-espeak.sh`) : la
//! phrase parlée entière, puis chaque mot affiché seul. Écrit les fixtures
//! d'alignement (le test tourne ensuite sans espeak-ng). En anglais, des
//! phrases s'ajoutent pour les contractions, « Dr. » et les nombres.
//!
//!   SILLON_ESPEAK=target/espeak/bin/sillon-espeak SILLON_ESPEAK_DATA=target/espeak/share \
//!     cargo run --example kokoro_fixtures -- tests/fixtures/kokoro/align-fr.json fr
//!   (idem avec tests/fixtures/kokoro/align-en.json en)

use std::collections::BTreeMap;

use sillon::doc::builder::{parse_document_in, CodePolicy};
use sillon::tts::kokoro::phonemes::{EspeakProcess, Phonemizer};

/// Ce que le spike anglais ne couvre pas : contractions, abréviations,
/// nombres, heures et sommes.
const ENGLISH_EXTRAS: &[&str] = &[
    "Dr. Patel didn't think it'd rain, but it's pouring and we're soaked.",
    "I can't believe they'll arrive at 3:30; she's already made 24 sandwiches.",
    "He paid $12.50 for 2 books in 1999, and he wouldn't've paid more.",
    "Mr. and Mrs. O'Neill won't be here until the 21st of June.",
];

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "tests/fixtures/kokoro/align-fr.json".into());
    let language = std::env::args().nth(2).unwrap_or_else(|| "fr".into());
    let reference: BTreeMap<String, BTreeMap<String, String>> =
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/kokoro/misaki-phonemes.json")?)?;
    let mut espeak = EspeakProcess::locate()?;
    let mut fixtures = Vec::new();
    let mut texts: Vec<&str> = reference[&language].keys().map(String::as_str).collect();
    if language == "en" {
        texts.extend(ENGLISH_EXTRAS);
    }
    for text in texts {
        let doc = parse_document_in(text, CodePolicy::Announce, &language);
        for sentence in &doc.sentences {
            let words = sentence.spoken_words();
            let mut items = vec![sentence.text.clone()];
            items.extend(words.iter().cloned());
            let ps = espeak.phonemize(&language, &items)?;
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
