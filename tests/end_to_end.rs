//! Tests de bout en bout : du markdown brut jusqu'à l'audio et à la position.
//!
//! Ils utilisent un moteur factice déterministe, afin de vérifier le câblage du
//! pipeline sans dépendre d'une synthèse réelle.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use sillage::doc::builder::{parse_document, CodePolicy};
use sillage::doc::BlockKind;
use sillage::tts::{PcmBuffer, Synthesizer, TtsEngine, TtsParams, TtsSession};

/// Moteur dont la durée produite est proportionnelle au texte.
struct StubEngine {
    concurrent: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

struct StubSession {
    concurrent: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl TtsEngine for StubEngine {
    fn name(&self) -> String {
        "stub".into()
    }
    fn open_session(&self) -> anyhow::Result<Box<dyn TtsSession>> {
        Ok(Box::new(StubSession {
            concurrent: Arc::clone(&self.concurrent),
            peak: Arc::clone(&self.peak),
        }))
    }
    fn max_sessions(&self) -> usize {
        8
    }
}

impl TtsSession for StubSession {
    fn synthesize(&mut self, text: &str, _params: &TtsParams) -> anyhow::Result<PcmBuffer> {
        let now = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(20));
        self.concurrent.fetch_sub(1, Ordering::SeqCst);

        let chars = text.chars().count().max(1);
        let samples = chars * 100;
        Ok(PcmBuffer::new(vec![0.05; samples], 16_000, 1))
    }
}

fn wait_for(mut predicate: impl FnMut() -> bool) -> bool {
    for _ in 0..250 {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

const DOCUMENT: &str = "\
# Guide de démarrage

Le lecteur accepte un fichier ou un flux. Il coûte 3.14 euros environ.

## Contrôles

- Espace met en pause.
- Les flèches déplacent la lecture.

```rust
let secret = 42;
```

| Touche | Effet |
|---|---|
| b | signet |

> Une citation pour finir.
";

#[test]
fn full_document_pipeline_preserves_every_kind_of_content() {
    let doc = parse_document(DOCUMENT, CodePolicy::Announce);

    // Aucun contenu perdu : titres, liste, tableau, citation.
    let kinds: Vec<&BlockKind> = doc.blocks.iter().map(|b| &b.kind).collect();
    assert!(kinds.iter().any(|k| matches!(k, BlockKind::Heading { level: 1 })));
    assert!(kinds.iter().any(|k| matches!(k, BlockKind::Heading { level: 2 })));
    assert_eq!(
        kinds.iter().filter(|k| matches!(k, BlockKind::ListItem { .. })).count(),
        2
    );
    assert!(kinds.iter().any(|k| matches!(k, BlockKind::TableRow { .. })));
    assert!(kinds.iter().any(|k| matches!(k, BlockKind::Quote)));

    let spoken = doc
        .sentences
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    // Rien du formatage n'est prononcé…
    for marker in ["#", "|", "```", "---", ">", "- "] {
        assert!(!spoken.contains(marker), "formatage lu : {marker:?} dans {spoken:?}");
    }
    // …et le contenu du code n'est pas récité.
    assert!(!spoken.contains("secret"));
    assert!(spoken.contains("Bloc de code rust"));
    // Le contenu utile est bien là.
    assert!(spoken.contains("Espace met en pause"));
    assert!(spoken.contains("signet"));
    assert!(spoken.contains("citation"));
    // Une décimale n'a pas été coupée en deux phrases.
    assert!(spoken.contains("3.14 euros"));
}

#[test]
fn streamed_input_matches_a_single_pass() {
    use sillage::doc::DocumentBuilder;
    use sillage::doc::blocks::BlockAssembler;

    let mut assembler = BlockAssembler::new();
    let mut builder = DocumentBuilder::new(CodePolicy::Announce);
    // Ligne à ligne, comme le ferait un générateur branché sur stdin.
    for line in DOCUMENT.lines() {
        if let Some(block) = assembler.push(line) {
            builder.push_block(&block);
        }
    }
    if let Some(block) = assembler.finish() {
        builder.push_block(&block);
    }
    builder.finish();

    let bulk = parse_document(DOCUMENT, CodePolicy::Announce);
    assert_eq!(builder.document().blocks, bulk.blocks);
    assert_eq!(builder.document().sentences, bulk.sentences);
}

#[test]
fn synthesis_feeds_playback_in_document_order() {
    let doc = parse_document(DOCUMENT, CodePolicy::Announce);
    let peak = Arc::new(AtomicUsize::new(0));
    let engine = Arc::new(StubEngine {
        concurrent: Arc::new(AtomicUsize::new(0)),
        peak: Arc::clone(&peak),
    });

    let synth = Synthesizer::new(engine, TtsParams::default(), 4, 6);
    for sentence in &doc.sentences {
        synth.push(sentence.text.clone(), sentence.trailing_pause_ms);
    }

    assert!(wait_for(|| synth.get(0).is_some() && synth.get(2).is_some()));
    assert!(peak.load(Ordering::SeqCst) > 1, "la synthèse n'était pas parallèle");

    // Le silence de fin de bloc allonge l'audio sans allonger la parole.
    let rendered = synth.get(0).unwrap();
    assert!(rendered.audio.duration > rendered.speech);
}

#[test]
fn word_cursor_follows_the_measured_duration() {
    let doc = parse_document("Un deux trois quatre cinq six.", CodePolicy::Announce);
    let sentence = &doc.sentences[0];
    let total = 6.0;

    let first = sentence.word_at(0.0, total).unwrap();
    let middle = sentence.word_at(total / 2.0, total).unwrap();
    let last = sentence.word_at(total, total).unwrap();

    assert_eq!(first, 0);
    assert!(middle > first && middle < last, "{first} {middle} {last}");
    assert_eq!(last, sentence.words.len() - 1);

    // Le curseur est monotone : il ne recule jamais au fil du temps.
    let mut previous = 0;
    for step in 0..=60 {
        let index = sentence.word_at(step as f32 * total / 60.0, total).unwrap();
        assert!(index >= previous, "recul du curseur à l'étape {step}");
        previous = index;
    }
}

#[test]
fn navigation_targets_are_reachable_in_both_directions() {
    let doc = parse_document(DOCUMENT, CodePolicy::Announce);
    let last = doc.sentences.len() - 1;

    // Depuis la fin, on remonte de section en section jusqu'à la première.
    let mut cursor = last;
    let mut visited = 0;
    while let Some(previous) = doc.sentence_in_neighbour_section(cursor, -1) {
        assert!(previous < cursor);
        cursor = previous;
        visited += 1;
        assert!(visited < 50, "boucle dans la navigation par section");
    }
    assert!(visited >= 2, "les sections ne sont pas atteignables");

    // Et on redescend bloc par bloc jusqu'au dernier.
    let mut cursor = 0;
    let mut steps = 0;
    while let Some(next) = doc.sentence_in_neighbour_block(cursor, 1) {
        assert!(next > cursor);
        cursor = next;
        steps += 1;
        assert!(steps < 100, "boucle dans la navigation par bloc");
    }
    assert!(steps >= 5);
}

#[test]
fn a_speed_change_keeps_previously_rendered_audio() {
    // La vitesse s'applique au son à la lecture : l'audio synthétisé à 1×
    // sert à toutes les vitesses, rien n'est jeté ni refait.
    let doc = parse_document("Une phrase de test suffisamment longue.", CodePolicy::Announce);
    let engine = Arc::new(StubEngine {
        concurrent: Arc::new(AtomicUsize::new(0)),
        peak: Arc::new(AtomicUsize::new(0)),
    });
    let mut config = sillage::config::Config::default();
    let synth = Synthesizer::new(engine, config.tts_params(), 2, 4);
    synth.push(doc.sentences[0].text.clone(), 0);

    assert!(wait_for(|| synth.get(0).is_some()));
    let before = synth.get(0).unwrap().audio;

    config.voice.speed = 2.0;
    synth.set_params(config.tts_params());

    let after = synth.get(0).expect("l'audio a été jeté");
    assert!(Arc::ptr_eq(&before, &after.audio), "l'audio a été refait");
}
