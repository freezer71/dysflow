# Interface macOS — plan d'implémentation

> **Pour les agents :** sous-skill requis : superpowers:subagent-driven-development
> (recommandé) ou superpowers:executing-plans, tâche par tâche. Les étapes
> utilisent des cases à cocher (`- [ ]`).

**Objectif :** une app macOS, `Sillon.app`, qui lit à voix haute la sélection
ou le presse-papiers d'un raccourci global, dans un panneau type Spotlight qui
s'agrandit en lecteur, entièrement au clavier.

**Architecture :** le crate Rust `sillon` reste le moteur. Un nouveau `Reader`
garde la voix chargée d'un texte à l'autre et vit sur son propre fil
d'exécution (`ReaderHandle`). Un crate `sillon-ffi` l'expose à Swift par
UniFFI, en bibliothèque statique. L'app SwiftUI + AppKit (dossier `macos/`,
projet généré par XcodeGen) envoie des commandes et lit un instantané de
l'état à chaque image affichée.

**Pile :** Rust 1.80+, UniFFI 0.32, rodio, audio.cpp (shim dylib) ; Swift 6.4
en mode de langage 5, SwiftUI + AppKit, TextKit 2, macOS 14+, XcodeGen,
KeyboardShortcuts (sindresorhus) 2.x, XCTest / XCUITest.

**Spec :** `docs/superpowers/specs/2026-10-05-interface-macos-design.md`. La
lire avant toute tâche.

## Contraintes globales

- macOS 14 minimum, Apple Silicon (`arm64`) seul.
- Le terminal `sillon` ne change pas de comportement ; `cargo test` reste vert
  après chaque tâche Rust.
- Contrôles d'interface en police système **13 pt, fixe** ; seul le texte lu
  change de taille.
- Texte lu en **pleine largeur** ; jamais de colonne étroite par défaut.
- Voix par défaut : **Supertonic**. Voix gardée **10 minutes** après la
  dernière lecture (`keep_voice_minutes = 10`, `-1` = toujours, `0` = relâchée
  dès l'arrêt).
- Raccourcis par défaut : **Lire `⌃⌥Espace`**, **Afficher `⌃⌥S`**.
- Style « Papier » : fond `#fbf8f1`, texte `#2a2722`, lu `#9a948a`, mot fond
  `#cfe3ff` texte `#1b2a44`, ligne `#7aa7e8` ; nuit : fond `#1e1b17`, texte
  `#e8e1d4`, lu `#8a8276`, mot fond `#2c4a75` texte `#e6efff`, ligne `#7aa7e8`.
- Contraste élevé (noir, blanc, jaune `#ffe600`) dès que « Augmenter le
  contraste » est actif.
- Police de lecture par défaut **Literata** ; aussi Atkinson Hyperlegible et
  OpenDyslexic, embarquées.
- Seuils : raccourci → premier son **< 0,5 s** voix chargée, **< 0,9 s** à
  froid ; saut **< 0,3 s** ; aucune image perdue ; mémoire au repos voix
  relâchée **< 120 Mo**, stable sur dix cycles.
- Ne jamais ajouter `vendor/audio.cpp` à un commit (patch local non versionné) :
  `git add` des chemins explicites uniquement.
- Messages de commit en français, style du dépôt (`feat(app): …`), terminés par
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Commentaires et noms en français, comme le code existant.

## Points d'attention

Cinq situations que la spec implique sans qu'un test de tâche les couvre
d'office ; chacune a reçu son test dans la tâche indiquée.

1. **Lire pendant qu'une lecture est en cours** : la nouvelle capture remplace
   l'ancienne, sans que l'audio de l'ancien texte ne sorte. → Tâche 4
   (`player` vidé dans `into_parts`) et Tâche 5 (test « le second texte
   remplace le premier »).
2. **Une commande arrive pendant que la voix est au repos** : elle doit
   recharger la voix et reprendre à la bonne phrase, sans démarrer seule si la
   commande était un déplacement. → Tâche 5 (test de reprise après repos).
3. **Sélection très longue** (un article entier) : le panneau ne doit pas
   grandir au-delà de sa limite ni bloquer l'interface en calculant toute la
   mise en page. → Tâche 15 (test de hauteur bornée).
4. **Presse-papiers de l'utilisateur pendant la copie simulée** : il doit
   retrouver exactement son contenu, même si l'app cible ne copie rien. →
   Tâche 17 (tests de restauration et d'expiration).
5. **Saut de mot au bord du document** (`⌥←` sur le premier mot, `⌥→` sur le
   dernier) : rien ne bouge, un message le dit. → Tâche 3.

---

# Partie A — Mesurer d'abord

### Tâche 1 : la voix rend-elle sa mémoire ?

La spec fait du déchargement la base de la politique mémoire. Une fuite le
rendrait inutile : on mesure avant de construire dessus.

**Fichiers :**
- Créer : `examples/voice_cycles.rs`
- Modifier : `Cargo.toml` (déclarer l'exemple)
- Modifier : `docs/superpowers/specs/2026-10-05-interface-macos-design.md`
  (section « Mesures », résultats)

**Interfaces :** aucune ; banc autonome.

- [ ] **Étape 1 : écrire le banc**

```rust
//! Dix cycles chargement → une phrase → déchargement, mémoire relevée à
//! chaque cycle. Répond à une question : la voix rend-elle sa mémoire ?
//!
//! ```bash
//! cargo run --release --features audio-cpp --example voice_cycles -- <modele.gguf> <famille>
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use sillon::config::Config;
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{Synthesizer, TtsEngine};

/// Mémoire résidente du processus, en Mo.
fn rss_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / 1024
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut config, _) = Config::load();
    if let (Some(path), Some(family)) = (args.next(), args.next()) {
        config.engine.model_path = path;
        config.engine.family = family;
    }
    println!("modèle : {} ({})", config.engine.model_path, config.engine.family);
    println!("départ : {} Mo", rss_mb());

    for cycle in 1..=10 {
        let t = Instant::now();
        let engine: Arc<dyn TtsEngine> =
            Arc::new(AudioCppEngine::load(&config.engine_settings()).expect("chargement"));
        let load = t.elapsed();
        let synth = Synthesizer::new(
            Arc::clone(&engine),
            config.tts_params(),
            config.sessions() as usize,
            config.engine.prefetch as usize,
        );
        synth.push("Les monades enchaînent des calculs qui portent un contexte.".into(), 0);
        synth.set_cursor(0);
        while synth.get(0).is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let first = t.elapsed();
        let loaded = rss_mb();
        drop(synth);
        drop(engine);
        std::thread::sleep(Duration::from_millis(500));
        println!(
            "cycle {cycle:2} : chargement {:.2} s · première phrase {:.2} s · chargé {loaded} Mo · relâché {} Mo",
            load.as_secs_f32(),
            first.as_secs_f32(),
            rss_mb()
        );
    }
}
```

- [ ] **Étape 2 : déclarer l'exemple**

Dans `Cargo.toml`, après les autres `[[example]]` :

```toml
[[example]]
name = "voice_cycles"
required-features = ["audio-cpp"]
```

- [ ] **Étape 3 : mesurer les deux variantes de Supertonic**

```bash
M="$HOME/Library/Application Support/sillon/models/supertonic"
cargo run --release --features audio-cpp --example voice_cycles -- "$M/supertonic-3-orig.gguf" supertonic 2>/dev/null
cargo run --release --features audio-cpp --example voice_cycles -- "$M/supertonic-3-f16.gguf" supertonic 2>/dev/null
```

Attendu : dix lignes par modèle. Noter la colonne « relâché ».

- [ ] **Étape 4 : trancher**

- Si « relâché » **se stabilise** après deux ou trois cycles (écart < 20 Mo
  entre le cycle 3 et le cycle 10) : ce n'est pas une fuite, c'est un cache
  (Metal, allocateur). Le déchargement reste utile ; continuer.
- Si « relâché » **monte à chaque cycle** : c'est une fuite. **Arrêter le
  plan** et ouvrir une investigation avec superpowers:systematic-debugging
  (suspects : `AudioCppEngine::drop`, `AudioCppSessionHandle::drop` dans
  `src/tts/audio_cpp.rs`, puis le shim `native/audiocpp_shim.cpp`). Le
  déchargement n'a pas de sens tant qu'elle n'est pas corrigée.
- Si le palier stable dépasse 120 Mo, le noter : le seuil de la spec devra être
  revu avec l'utilisateur à la Tâche 21.

- [ ] **Étape 5 : écrire les résultats dans la spec**

Ajouter sous le tableau « Mesures » de la spec un tableau « Dix cycles » (une
ligne par modèle : chargé moyen, relâché au cycle 1, 3, 10, verdict), et choisir
`orig` ou `f16` comme modèle par défaut selon la mémoire chargée, à qualité
égale.

- [ ] **Étape 6 : commit**

```bash
git add examples/voice_cycles.rs Cargo.toml docs/superpowers/specs/2026-10-05-interface-macos-design.md
git commit -m "bench(voix): dix cycles de chargement, la mémoire rendue mesurée

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

# Partie B — Le cœur Rust

### Tâche 2 : commencer le son au début d'un mot

**Fichiers :**
- Modifier : `src/doc/mod.rs` (`Sentence::word_at`, nouvelle `word_start`)
- Modifier : `src/tts/mod.rs` (`PcmBuffer::skip`)
- Modifier : `src/audio/player.rs` (`Queued.offset`, `enqueue_from`)

**Interfaces :**
- Produit : `Sentence::word_start(&self, word: usize) -> Option<f32>` (fraction
  de la parole, 0..1) ; `PcmBuffer::skip(&self, offset: Duration) -> PcmBuffer` ;
  `Player::enqueue_from(&mut self, sentence_id: usize, audio: &Arc<PcmBuffer>, speech: Duration, offset: Duration)`.
  `Player::enqueue` garde sa signature.

- [ ] **Étape 1 : tests qui échouent**

Dans le module de tests de `src/doc/mod.rs` (le créer en bas du fichier s'il
n'existe pas, avec `use super::*;`) :

```rust
    fn sentence_of(words: &[&str]) -> Sentence {
        Sentence {
            id: 0,
            block_id: 0,
            section_id: 0,
            text: words.join(" "),
            words: words
                .iter()
                .enumerate()
                .map(|(index, text)| Word {
                    text: text.to_string(),
                    index,
                    punctuation: None,
                    spoken: None,
                    emphasis: Emphasis::default(),
                })
                .collect(),
            trailing_pause_ms: 0,
        }
    }

    #[test]
    fn a_word_starts_where_the_highlight_places_it() {
        let s = sentence_of(&["un", "deux", "trois", "quatre"]);
        for word in 0..4 {
            let start = s.word_start(word).unwrap();
            assert_eq!(s.word_at(start, 1.0), Some(word), "mot {word}");
        }
    }

    #[test]
    fn the_first_word_starts_at_zero_and_past_the_end_is_none() {
        let s = sentence_of(&["un", "deux"]);
        assert_eq!(s.word_start(0), Some(0.0));
        assert_eq!(s.word_start(2), None);
    }
```

Dans le module de tests de `src/tts/mod.rs` (même remarque) :

```rust
    #[test]
    fn skipping_drops_whole_frames_from_the_start() {
        // Stéréo, 10 trames par seconde : 0,3 s = 3 trames = 6 échantillons.
        let pcm = PcmBuffer::new((0..20).map(|i| i as f32).collect(), 10, 2);
        let rest = pcm.skip(Duration::from_millis(300));
        assert_eq!(rest.samples.len(), 14);
        assert_eq!(rest.samples[0], 6.0);
        assert_eq!(rest.channels, 2);
    }

    #[test]
    fn skipping_past_the_end_leaves_silence() {
        let pcm = PcmBuffer::new(vec![0.5; 10], 10, 1);
        assert!(pcm.skip(Duration::from_secs(5)).is_empty());
    }
```

- [ ] **Étape 2 : les voir échouer**

Run : `cargo test word_start skipping a_word_starts`
Attendu : erreurs de compilation (`word_start`, `skip` inconnues).

- [ ] **Étape 3 : implémenter**

Dans `src/doc/mod.rs`, dans `word_at`, remplacer la comparaison
`if target <= acc {` par `if target < acc {` — à la frontière exacte entre deux
mots, c'est le mot qui commence qui est prononcé. Puis ajouter à
`impl Sentence` :

```rust
    /// Instant où commence le mot `word`, en fraction de la parole.
    ///
    /// Réciproque de `word_at`, sur la même répartition par poids : un saut au
    /// mot tombe là où le surlignage le placerait.
    pub fn word_start(&self, word: usize) -> Option<f32> {
        if word >= self.words.len() {
            return None;
        }
        let sum: f32 = self.words.iter().map(Word::weight).sum();
        if sum <= 0.0 {
            return Some(0.0);
        }
        let before: f32 = self.words[..word].iter().map(Word::weight).sum();
        Some(before / sum)
    }
```

Dans `src/tts/mod.rs`, à `impl PcmBuffer` (ajouter `use std::time::Duration;`
s'il manque) :

```rust
    /// Le même son, privé de ses `offset` premières secondes.
    ///
    /// La coupe tombe sur une trame entière : couper entre deux canaux
    /// inverserait gauche et droite pour toute la suite.
    pub fn skip(&self, offset: Duration) -> PcmBuffer {
        let frames = (offset.as_secs_f64() * self.sample_rate as f64) as usize;
        let start = (frames * self.channels as usize).min(self.samples.len());
        PcmBuffer::new(self.samples[start..].to_vec(), self.sample_rate, self.channels)
    }
```

Dans `src/audio/player.rs` :

1. Ajouter à `struct Queued` le champ :

```rust
    /// Parole déjà passée au moment où la phrase entre en file : un saut au
    /// mot la fait commencer en son milieu.
    offset: Duration,
```

2. Remplacer `enqueue` par :

```rust
    /// Empile une phrase à jouer depuis son début.
    pub fn enqueue(&mut self, sentence_id: usize, audio: &Arc<PcmBuffer>, speech: Duration) {
        self.enqueue_from(sentence_id, audio, speech, Duration::ZERO);
    }

    /// Empile une phrase à jouer à partir de `offset` dans sa parole.
    ///
    /// La position rendue compte depuis le début de la phrase : le
    /// surlignage reste juste, comme si le début avait été entendu.
    pub fn enqueue_from(
        &mut self,
        sentence_id: usize,
        audio: &Arc<PcmBuffer>,
        speech: Duration,
        offset: Duration,
    ) {
        let Some(sink) = &self.sink else { return };
        let offset = offset.min(speech);
        let samples = if offset.is_zero() {
            audio.samples.to_vec()
        } else {
            audio.skip(offset).samples.to_vec()
        };
        sink.append(SamplesBuffer::new(audio.channels, audio.sample_rate, samples));
        self.queued.push_back(Queued {
            sentence_id,
            total: audio.duration,
            speech,
            offset,
        });
        if self.playing {
            sink.play();
        }
    }
```

3. Dans `reconcile`, à la création de `Active`, remplacer
`accumulated: Duration::ZERO,` par `accumulated: front.offset,`.

- [ ] **Étape 4 : tout faire passer**

Run : `cargo test`
Attendu : tout vert. Si un test existant de `word_at` vérifiait une frontière
exacte avec l'ancien `<=`, le lire : s'il attend le mot qui *finit*, corriger
l'attente (le mot qui commence est la règle désormais) et le dire dans le
message de commit.

- [ ] **Étape 5 : commit**

```bash
git add src/doc/mod.rs src/tts/mod.rs src/audio/player.rs
git commit -m "feat(audio): jouer une phrase à partir d'un mot

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 3 : sauter mot par mot

**Fichiers :**
- Modifier : `src/app/mod.rs`

**Interfaces :**
- Consomme : `Sentence::word_start`, `Player::enqueue_from` (Tâche 2).
- Produit : `App::seek_words(&mut self, delta: isize)` ;
  `App::reading_word(&self) -> Option<usize>` (index du mot **dans** la phrase
  courante, y compris pendant un saut en attente d'audio) ;
  `pub(crate) fn word_target(doc: &Document, sentence: usize, word: usize, delta: isize) -> Option<(usize, usize)>`.

- [ ] **Étape 1 : tests qui échouent**

Dans `mod tests` de `src/app/mod.rs` :

```rust
    #[test]
    fn word_target_crosses_sentences_both_ways() {
        let app = app_reading("Un deux trois quatre. Cinq six.");
        let doc = app.doc();
        assert_eq!(word_target(doc, 0, 1, 2), Some((0, 3)));
        assert_eq!(word_target(doc, 0, 2, 3), Some((1, 1)));
        assert_eq!(word_target(doc, 1, 0, -1), Some((0, 3)));
        assert_eq!(word_target(doc, 0, 0, -1), None);
        assert_eq!(word_target(doc, 1, 1, 1), None);
    }

    #[test]
    fn seeking_words_moves_the_reading_word_before_any_audio() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_words(2);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.reading_word(), Some(2));
        // Un second saut part du mot visé, pas du début de la phrase.
        app.seek_words(3);
        assert_eq!(app.current_sentence(), 1);
        assert_eq!(app.reading_word(), Some(1));
    }

    #[test]
    fn seeking_words_at_the_edge_says_so_and_stays() {
        let mut app = app_reading("Un deux.");
        app.seek_words(-1);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.status(), "Aucun mot de ce côté");
    }
```

- [ ] **Étape 2 : les voir échouer**

Run : `cargo test --lib app::tests::word_target app::tests::seeking_words`
Attendu : erreurs de compilation.

- [ ] **Étape 3 : implémenter**

1. Ajouter à `struct App`, après `pace` :

```rust
    /// Saut au mot en attente de son audio : la phrase, le mot, et la part
    /// de la parole à passer. Tombe dès que la phrase entre en file.
    start_at: Option<StartAt>,
```

et avant `impl App` :

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
struct StartAt {
    sentence: usize,
    word: usize,
    ratio: f32,
}
```

Initialiser `start_at: None,` dans le constructeur.

2. Au début de `seek_to`, après le `if total == 0 { return; }` :
`self.start_at = None;`

3. Ajouter, dans la section « commandes » :

```rust
    /// Mot lu dans la phrase courante, saut en attente compris.
    ///
    /// Entre un saut et l'arrivée de son audio, aucune position n'existe ;
    /// sans ce relais, le surlignage reviendrait au début de la phrase.
    pub fn reading_word(&self) -> Option<usize> {
        self.current_word().or_else(|| {
            self.start_at
                .filter(|s| s.sentence == self.current_sentence())
                .map(|s| s.word)
        })
    }

    /// Déplace la lecture de `delta` mots, en franchissant les phrases.
    pub fn seek_words(&mut self, delta: isize) {
        let sentence = self.current_sentence();
        let word = self.reading_word().unwrap_or(0);
        let Some((target, word)) = word_target(self.doc(), sentence, word, delta) else {
            self.notify("Aucun mot de ce côté");
            return;
        };
        let ratio = self
            .doc()
            .sentence(target)
            .and_then(|s| s.word_start(word))
            .unwrap_or(0.0);
        self.seek_to(target);
        self.start_at = Some(StartAt { sentence: target, word, ratio });
    }
```

4. Dans `fill_output`, remplacer l'appel
`self.player.enqueue(self.queue_head, &rendered.audio, rendered.speech);` par :

```rust
                let offset = match self.start_at {
                    Some(start) if start.sentence == self.queue_head => {
                        self.start_at = None;
                        rendered.speech.mul_f32(start.ratio)
                    }
                    _ => Duration::ZERO,
                };
                self.player.enqueue_from(
                    self.queue_head,
                    &rendered.audio,
                    rendered.speech,
                    offset,
                );
```

5. En bas du fichier, hors de `impl App` :

```rust
/// Phrase et mot atteints en avançant de `delta` mots depuis
/// (`sentence`, `word`). `None` au-delà du document.
pub(crate) fn word_target(
    doc: &Document,
    sentence: usize,
    word: usize,
    delta: isize,
) -> Option<(usize, usize)> {
    let mut s = sentence;
    let mut w = word as isize + delta;
    loop {
        let len = doc.sentence(s)?.words.len() as isize;
        if w < 0 {
            s = s.checked_sub(1)?;
            w += doc.sentence(s)?.words.len() as isize;
        } else if w >= len {
            w -= len;
            s += 1;
        } else {
            return Some((s, w as usize));
        }
    }
}
```

- [ ] **Étape 4 : tout faire passer**

Run : `cargo test`
Attendu : tout vert.

- [ ] **Étape 5 : commit**

```bash
git add src/app/mod.rs
git commit -m "feat(app): sauter mot par mot, d'une phrase à l'autre

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 4 : garder synthèse et sortie d'un texte à l'autre

Ouvrir une session de synthèse coûte ; un nouveau texte ne doit pas le
repayer.

**Fichiers :**
- Modifier : `src/tts/pool.rs` (`Synthesizer::reset`)
- Modifier : `src/app/mod.rs` (`App::from_parts`, `App::into_parts`)

**Interfaces :**
- Produit : `Synthesizer::reset(&self)` ;
  `App::from_parts(builder: DocumentBuilder, input: Option<Receiver<InputEvent>>, synth: Synthesizer, config: Config, source: String, notices: Vec<String>, player: Player) -> App` ;
  `App::into_parts(self) -> (Synthesizer, Player)` (sortie vidée).

- [ ] **Étape 1 : tests qui échouent**

Dans `mod tests` de `src/tts/pool.rs` :

```rust
    /// Compte les sessions ouvertes, et rend un son proportionnel au texte.
    struct OpeningCounter {
        opened: Arc<std::sync::atomic::AtomicUsize>,
    }
    struct LengthSession;

    impl TtsEngine for OpeningCounter {
        fn name(&self) -> String {
            "compteur".into()
        }
        fn open_session(&self) -> Result<Box<dyn super::super::TtsSession>> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(LengthSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl super::super::TtsSession for LengthSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    fn wait_for(synth: &Synthesizer, id: usize) -> Rendered {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = synth.get(id) {
                return r;
            }
            assert!(std::time::Instant::now() < deadline, "phrase {id} jamais prête");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn reset_keeps_the_session_and_forgets_the_text() {
        let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let engine = Arc::new(OpeningCounter { opened: Arc::clone(&opened) });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        assert_eq!(wait_for(&synth, 0).audio.samples.len(), 40);

        synth.reset();
        assert_eq!(synth.stats().total, 0);
        synth.push("bb".into(), 0);
        assert_eq!(wait_for(&synth, 0).audio.samples.len(), 20);
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }
```

Adapter les chemins `super::super::TtsSession` aux `use` déjà présents dans ce
module de tests (les faux moteurs existants `FakeEngine` / `InstantEngine`
montrent la forme exacte). Si `TtsParams` n'implémente pas `Default`, construire
les paramètres comme le font ces tests existants.

Dans `mod tests` de `src/app/mod.rs` :

```rust
    #[test]
    fn parts_carry_over_to_a_new_text() {
        let app = app_reading("Premier texte.");
        let (synth, player) = app.into_parts();
        synth.reset();
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("Second texte.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::from_parts(builder, None, synth, config, "b".into(), Vec::new(), player);
        assert_eq!(app.doc().sentences.len(), 1);
        assert_eq!(app.doc().sentences[0].written(), "Second texte.");
        assert_eq!(app.player.queue_len(), 0);
    }
```

- [ ] **Étape 2 : les voir échouer**

Run : `cargo test reset_keeps parts_carry_over`
Attendu : erreurs de compilation.

- [ ] **Étape 3 : implémenter**

Dans `src/tts/pool.rs`, à `impl Synthesizer` :

```rust
    /// Repart d'un document vide, threads et sessions gardés ouverts.
    ///
    /// Les synthèses encore en cours portent l'ancienne génération : elles
    /// seront jetées à leur arrivée, et libéreront alors leur place.
    pub fn reset(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.clear();
        state.cache.clear();
        state.failures.clear();
        state.cursor = 0;
        state.generation += 1;
        drop(state);
        self.shared.wake.notify_all();
    }
```

`in_flight` n'est **pas** vidé : un worker qui rend une ancienne phrase retire
lui-même son identifiant (`finish_generation`), et la nouvelle phrase du même
numéro attend jusque-là au lieu d'être synthétisée deux fois.

Dans `src/app/mod.rs` :

1. Remplacer le début de `App::new` jusqu'à `let previewer = …;` par une
   délégation :

```rust
    pub fn new(
        builder: DocumentBuilder,
        input: Option<Receiver<InputEvent>>,
        engine: Arc<dyn TtsEngine>,
        config: Config,
        source: String,
        notices: Vec<String>,
        player: Player,
    ) -> Self {
        let synth = Synthesizer::new(
            engine,
            config.tts_params(),
            config.sessions() as usize,
            config.engine.prefetch as usize,
        );
        Self::from_parts(builder, input, synth, config, source, notices, player)
    }

    /// Un lecteur bâti sur une synthèse et une sortie déjà ouvertes.
    ///
    /// C'est ce qui rend un nouveau texte immédiat : ni session de synthèse
    /// ni flux audio à rouvrir.
    pub fn from_parts(
        builder: DocumentBuilder,
        input: Option<Receiver<InputEvent>>,
        synth: Synthesizer,
        config: Config,
        source: String,
        notices: Vec<String>,
        mut player: Player,
    ) -> Self {
        synth.set_params(config.tts_params());
        let previewer = Previewer::new(synth.engine());
        player.set_volume(config.voice.volume);
```

   La suite du corps actuel (`BookmarkStore::load()` … `app`) reste telle
   quelle. Supprimer de la suite la ligne `player.set_volume(...)` en double si
   elle y figure encore. Adapter le nom `Player` à l'import existant
   (`crate::audio::Player`).

2. Ajouter :

```rust
    /// Rend synthèse et sortie pour le texte suivant.
    ///
    /// La sortie est vidée : l'audio de ce texte ne doit pas déborder sur
    /// le suivant.
    pub fn into_parts(mut self) -> (Synthesizer, Player) {
        self.player.flush();
        (self.synth, self.player)
    }
```

- [ ] **Étape 4 : tout faire passer**

Run : `cargo test`
Attendu : tout vert.

- [ ] **Étape 5 : commit**

```bash
git add src/tts/pool.rs src/app/mod.rs
git commit -m "feat(app): un texte neuf réutilise synthèse et sortie audio

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 5 : `Reader`, la voix gardée et mise au repos

**Fichiers :**
- Créer : `src/reader/mod.rs`
- Créer : `src/tts/silent.rs`
- Modifier : `src/lib.rs` (`pub mod reader;`)
- Modifier : `src/tts/mod.rs` (`pub mod silent;`, `Readiness`, méthode
  `readiness`)
- Modifier : `src/tts/lazy.rs` (`readiness`)
- Modifier : `src/audio/player.rs` (`silent` public)
- Modifier : `src/config.rs` (section `[app]`)

**Interfaces :**
- Consomme : `App::from_parts`, `App::into_parts`, `Synthesizer::reset`.
- Produit :
  - `pub enum Readiness { Ready, Loading, Fallback(String) }` et
    `TtsEngine::readiness(&self) -> Readiness` (défaut `Ready`).
  - `pub struct SilentEngine;` (`crate::tts::silent`) : silence de 50 ms par
    caractère à 16 kHz.
  - `Player::silent() -> Player`, désormais public hors tests.
  - `Config.app: AppConfig { keep_voice_minutes: i64 }`, défaut 10.
  - `pub type VoiceFactory = Box<dyn Fn(&Config) -> Arc<dyn TtsEngine>>;`
  - `pub type PlayerFactory = Box<dyn Fn() -> Player>;`
  - `Reader::new(config: Config, voices: VoiceFactory, players: PlayerFactory, now: Instant) -> Reader`
  - `Reader::open_text(&mut self, markdown: &str, source: &str, now: Instant)`
  - `Reader::open_file(&mut self, path: &Path, now: Instant) -> anyhow::Result<()>`
  - `Reader::prepare_voice(&mut self, now: Instant)`
  - `Reader::with_session(&mut self, now: Instant, f: impl FnOnce(&mut App))`
  - `Reader::tick(&mut self, now: Instant)`
  - `Reader::rest(&mut self)` (mise au repos immédiate)
  - `Reader::close(&mut self)` (position et configuration écrites)
  - lecture : `session() -> Option<&App>`, `config() -> &Config`,
    `revision() -> u64`, `voice_loaded() -> bool`, `is_resting() -> bool`,
    `engine() -> Option<&Arc<dyn TtsEngine>>`, `text_markdown() -> Option<&str>`.

- [ ] **Étape 1 : préparatifs sans test (types et moteur muet)**

`src/tts/mod.rs`, près du trait :

```rust
/// Où en est un moteur, vu de l'interface.
#[derive(Debug, Clone, PartialEq)]
pub enum Readiness {
    Ready,
    Loading,
    /// Le moteur voulu a échoué ; la voix de repli parle à sa place.
    Fallback(String),
}
```

et dans `pub trait TtsEngine` :

```rust
    /// État du moteur, pour l'annoncer. Un moteur ordinaire est prêt.
    fn readiness(&self) -> Readiness {
        Readiness::Ready
    }
```

Ajouter `pub mod silent;` à côté des autres `pub mod`.

`src/tts/lazy.rs`, dans `impl TtsEngine for LazyEngine` :

```rust
    fn readiness(&self) -> super::Readiness {
        match &*self.shared.state.lock().unwrap() {
            State::Loading => super::Readiness::Loading,
            State::Ready(_) => super::Readiness::Ready,
            State::Failed(err) => super::Readiness::Fallback(err.clone()),
        }
    }
```

`src/tts/silent.rs` :

```rust
//! Moteur muet : du silence, à la durée d'une vraie phrase.
//!
//! Sert aux essais et au mode de test de l'app : la lecture avance, le
//! surlignage bouge, sans modèle ni carte son.

use anyhow::Result;

use super::{PcmBuffer, TtsEngine, TtsParams, TtsSession};

pub struct SilentEngine;
struct SilentSession;

const RATE: u32 = 16_000;
/// Durée d'un caractère : de l'ordre d'une lecture à voix haute.
const PER_CHAR_MS: usize = 50;

impl TtsEngine for SilentEngine {
    fn name(&self) -> String {
        "silencieux".into()
    }
    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(SilentSession))
    }
    fn max_sessions(&self) -> usize {
        1
    }
}

impl TtsSession for SilentSession {
    fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
        let samples = text.chars().count().max(1) * PER_CHAR_MS * RATE as usize / 1000;
        Ok(PcmBuffer::new(vec![0.0; samples], RATE, 1))
    }
}
```

`src/audio/player.rs` : retirer `#[cfg(test)]` au-dessus de `pub fn silent()`
(garder celui de `simulate`) et compléter son commentaire : « Sert aussi au
mode de test de l'app ».

`src/config.rs` : ajouter à `struct Config` le champ `pub app: AppConfig,` et :

```rust
/// Réglages propres à l'app macOS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    /// Minutes pendant lesquelles la voix reste chargée après la dernière
    /// lecture. `-1` : toujours ; `0` : relâchée dès l'arrêt.
    pub keep_voice_minutes: i64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { keep_voice_minutes: 10 }
    }
}
```

Run : `cargo test` — attendu : vert (rien ne se sert encore des ajouts).

- [ ] **Étape 2 : tests de `Reader` qui échouent**

`src/reader/mod.rs`, en bas :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn reader(keep: i64) -> (Reader, Arc<AtomicUsize>, Instant) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.app.keep_voice_minutes = keep;
        let t0 = Instant::now();
        let reader = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            t0,
        );
        (reader, loads, t0)
    }

    fn written(reader: &Reader) -> String {
        reader.session().unwrap().doc().sentences[0].written()
    }

    const MIN: Duration = Duration::from_secs(60);

    #[test]
    fn two_texts_share_one_voice() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Premier texte.", "a", t0);
        r.open_text("Second texte.", "b", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(r.revision(), 2);
        assert_eq!(written(&r), "Second texte.");
    }

    #[test]
    fn the_second_text_replaces_the_first() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.open_text("Autre.", "b", t0);
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.player.queue_len(), 0);
    }

    #[test]
    fn the_voice_rests_after_the_delay_and_comes_back_in_place() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.tick(t0 + 9 * MIN);
        assert!(r.voice_loaded());
        r.tick(t0 + 11 * MIN);
        assert!(!r.voice_loaded());
        assert!(r.is_resting());

        // Un déplacement réveille la voix, sans lancer la lecture.
        r.with_session(t0 + 12 * MIN, |app| app.seek_sentences(-1));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(r.revision(), 1, "même document : pas de nouvelle révision");
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 1);
        assert!(!app.player.is_playing());
    }

    #[test]
    fn keeping_forever_never_rests() {
        let (mut r, _, t0) = reader(-1);
        r.open_text("Un.", "a", t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
    }

    #[test]
    fn speed_carries_over_to_the_next_text() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        r.with_session(t0, |app| app.set_speed(1.5));
        r.open_text("Deux.", "b", t0);
        assert_eq!(r.session().unwrap().config.voice.speed, 1.5);
    }

    #[test]
    fn preparing_the_voice_loads_it_once() {
        let (mut r, loads, t0) = reader(10);
        r.prepare_voice(t0);
        r.prepare_voice(t0);
        r.open_text("Un.", "a", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
```

Run : `cargo test --lib reader` — attendu : erreur de compilation (module
absent). Ajouter `pub mod reader;` à `src/lib.rs` (après `pub mod position;`)
pour que l'erreur porte sur `Reader`.

- [ ] **Étape 3 : implémenter `Reader`**

En tête de `src/reader/mod.rs`, au-dessus des tests :

```rust
//! Le lecteur qui dure : une voix gardée d'un texte à l'autre.
//!
//! `App` lit un document. `Reader` en ouvre un par texte sans recharger la
//! voix, et la relâche quand rien n'a été lu depuis un moment. Il ne quitte
//! jamais le fil qui l'a créé : la sortie audio n'est pas transférable d'un
//! fil à l'autre.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::audio::Player;
use crate::config::Config;
use crate::doc::DocumentBuilder;
use crate::tts::{Synthesizer, TtsEngine};

pub type VoiceFactory = Box<dyn Fn(&Config) -> Arc<dyn TtsEngine>>;
pub type PlayerFactory = Box<dyn Fn() -> Player>;

/// Le texte en cours, gardé pour le rouvrir après une mise au repos.
#[derive(Clone)]
struct Text {
    markdown: String,
    source: String,
    /// Un fichier : sa position se retient d'une ouverture à l'autre.
    identity: bool,
}

pub struct Reader {
    config: Config,
    voices: VoiceFactory,
    players: PlayerFactory,
    engine: Option<Arc<dyn TtsEngine>>,
    session: Option<App>,
    text: Option<Text>,
    /// Phrase où reprendre un texte mis au repos.
    resting_at: Option<usize>,
    revision: u64,
    last_activity: Instant,
}

impl Reader {
    pub fn new(config: Config, voices: VoiceFactory, players: PlayerFactory, now: Instant) -> Self {
        Self {
            config,
            voices,
            players,
            engine: None,
            session: None,
            text: None,
            resting_at: None,
            revision: 0,
            last_activity: now,
        }
    }

    pub fn session(&self) -> Option<&App> {
        self.session.as_ref()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Change à chaque nouveau document — pas à chaque réveil de la voix.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn voice_loaded(&self) -> bool {
        self.engine.is_some()
    }

    pub fn is_resting(&self) -> bool {
        self.resting_at.is_some()
    }

    pub fn engine(&self) -> Option<&Arc<dyn TtsEngine>> {
        self.engine.as_ref()
    }

    pub fn text_markdown(&self) -> Option<&str> {
        self.text.as_ref().map(|t| t.markdown.as_str())
    }

    /// Charge la voix sans attendre de texte : le raccourci la demande
    /// pendant que la sélection se lit.
    pub fn prepare_voice(&mut self, now: Instant) {
        self.last_activity = now;
        self.ensure_engine();
    }

    pub fn open_text(&mut self, markdown: &str, source: &str, now: Instant) {
        self.open(
            Text { markdown: markdown.to_string(), source: source.to_string(), identity: false },
            now,
        );
        self.revision += 1;
    }

    pub fn open_file(&mut self, path: &Path, now: Instant) -> anyhow::Result<()> {
        let markdown = crate::source::load(path)?;
        let source = crate::launch::document_identity(path);
        self.open(Text { markdown, source, identity: true }, now);
        if let Some(app) = self.session.as_mut() {
            if let Some(name) = path.file_name() {
                app.adopt_former_identity(&name.to_string_lossy());
            }
            app.resume_where_left();
        }
        self.revision += 1;
        Ok(())
    }

    /// Applique une commande au document en cours, en réveillant la voix
    /// si elle était au repos.
    pub fn with_session(&mut self, now: Instant, f: impl FnOnce(&mut App)) {
        self.last_activity = now;
        self.wake(now);
        if let Some(app) = self.session.as_mut() {
            f(app);
        }
    }

    pub fn tick(&mut self, now: Instant) {
        let Some(app) = self.session.as_mut() else { return };
        app.tick();
        if app.player.is_playing() && !app.is_finished() {
            self.last_activity = now;
        }
        if self.idle_expired(now) {
            self.rest();
        }
    }

    /// Relâche la voix, la synthèse et la sortie audio ; garde le texte et
    /// la phrase pour reprendre.
    pub fn rest(&mut self) {
        let Some(mut app) = self.session.take() else {
            self.engine = None;
            return;
        };
        app.remember_position();
        app.flush_config();
        self.config = app.config.clone();
        self.resting_at = Some(app.current_sentence());
        drop(app);
        self.engine = None;
    }

    /// Fermeture de l'app : rien de ce qui a été lu ou réglé ne se perd.
    pub fn close(&mut self) {
        if let Some(app) = self.session.as_mut() {
            app.remember_position();
            app.flush_config();
        }
    }

    fn idle_expired(&self, now: Instant) -> bool {
        let minutes = self.config.app.keep_voice_minutes;
        minutes >= 0
            && now.saturating_duration_since(self.last_activity)
                >= Duration::from_secs(minutes as u64 * 60)
    }

    fn ensure_engine(&mut self) -> Arc<dyn TtsEngine> {
        if let Some(engine) = &self.engine {
            return Arc::clone(engine);
        }
        let engine = (self.voices)(&self.config);
        self.engine = Some(Arc::clone(&engine));
        engine
    }

    fn open(&mut self, text: Text, now: Instant) {
        self.last_activity = now;
        self.resting_at = None;
        let engine = self.ensure_engine();
        let (synth, player) = match self.session.take() {
            Some(mut old) => {
                old.remember_position();
                self.config = old.config.clone();
                old.into_parts()
            }
            None => (
                Synthesizer::new(
                    engine,
                    self.config.tts_params(),
                    self.config.sessions() as usize,
                    self.config.engine.prefetch as usize,
                ),
                (self.players)(),
            ),
        };
        synth.reset();
        let mut builder = DocumentBuilder::new(self.config.code_policy());
        for block in crate::input::split_blocks(&text.markdown) {
            builder.push_block(&block);
        }
        let app = App::from_parts(
            builder,
            None,
            synth,
            self.config.clone(),
            text.source.clone(),
            Vec::new(),
            player,
        );
        self.session = Some(app);
        self.text = Some(text);
    }

    /// Rouvre le texte mis au repos, à sa phrase, lecture arrêtée : la
    /// commande qui a réveillé la voix décide de la suite.
    fn wake(&mut self, now: Instant) {
        let Some(at) = self.resting_at else { return };
        let Some(text) = self.text.clone() else { return };
        let identity = text.identity;
        self.open(text, now);
        if let Some(app) = self.session.as_mut() {
            app.player.pause();
            if identity {
                app.resume_where_left();
            }
            app.seek_to(at);
        }
    }
}
```

`App::player` et `App::config` sont déjà publics ; `seek_to`,
`remember_position`, `flush_config`, `adopt_former_identity`,
`resume_where_left` aussi. Si `App::from_parts` démarre la lecture en
autolecture, `wake` la remet en pause aussitôt : c'est voulu.

- [ ] **Étape 4 : tout faire passer**

Run : `cargo test`
Attendu : tout vert, dont les six tests `reader::tests`.

- [ ] **Étape 5 : commit**

```bash
git add src/reader/mod.rs src/tts/silent.rs src/tts/mod.rs src/tts/lazy.rs src/audio/player.rs src/config.rs src/lib.rs
git commit -m "feat(reader): une voix gardée d'un texte à l'autre, mise au repos après un délai

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 6 : ce que l'interface voit — instantané et document

**Fichiers :**
- Créer : `src/reader/view.rs`
- Modifier : `src/reader/mod.rs` (`pub mod view;`)

**Interfaces :**
- Consomme : `Reader`, `App::reading_word`, `TtsEngine::readiness`.
- Produit (tous `#[derive(Debug, Clone, PartialEq, Default)]` sauf mention) :

```rust
pub enum VoiceState { #[default] Resting, Loading, Ready, Fallback { reason: String } }
pub struct SearchState { pub query: String, pub position: usize, pub count: usize }
pub struct Snapshot {
    pub revision: u64, pub has_document: bool,
    pub sentence: usize, pub word: Option<usize>,   // index global du mot
    pub section: Option<usize>,
    pub playing: bool, pub audible: bool, pub finished: bool,
    pub progress: f32, pub elapsed: Option<f64>, pub total: Option<f64>,
    pub speed: f32, pub volume: f32, pub muted: bool,
    pub voice: VoiceState, pub status: String,
    pub search: Option<SearchState>, pub bookmarked: bool, pub skipped: usize,
}
pub struct WordView { pub index: usize, pub text: String, pub strong: bool, pub italic: bool, pub code: bool, pub link: bool, pub struck: bool }
pub struct SentenceView { pub id: usize, pub words: Vec<WordView> }
pub enum BlockKindView { #[default] Paragraph, Heading { level: u8 }, ListItem { depth: u8, ordinal: Option<u64> }, Quote, Code { language: Option<String> }, TableRow { header: bool } }
pub struct BlockView { pub id: usize, pub kind: BlockKindView, pub section_id: usize, pub display_lines: Vec<String>, pub sentences: Vec<SentenceView> }
pub struct SectionView { pub id: usize, pub level: u8, pub title: String, pub first_sentence: Option<usize> }
pub struct DocView { pub title: String, pub complete: bool, pub sentence_count: usize, pub sections: Vec<SectionView>, pub blocks: Vec<BlockView> }
pub fn snapshot(reader: &Reader) -> Snapshot
pub fn document(reader: &Reader) -> DocView
```

- [ ] **Étape 1 : tests qui échouent**

En bas de `src/reader/view.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn reader_with(markdown: &str) -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut r = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        r.open_text(markdown, "test", Instant::now());
        r
    }

    #[test]
    fn an_empty_reader_has_no_document_and_a_resting_voice() {
        let r = Reader::new(
            Config::default(),
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        let s = snapshot(&r);
        assert!(!s.has_document);
        assert_eq!(s.voice, VoiceState::Resting);
        assert!(document(&r).blocks.is_empty());
    }

    #[test]
    fn the_document_keeps_structure_and_global_word_indices() {
        let r = reader_with("# Titre\n\nUn **deux** trois. Quatre.\n\n- item");
        let d = document(&r);
        assert_eq!(d.sections[0].title, "Titre");
        assert_eq!(d.blocks[0].kind, BlockKindView::Heading { level: 1 });
        let para = &d.blocks[1];
        assert_eq!(para.kind, BlockKindView::Paragraph);
        assert_eq!(para.sentences.len(), 2);
        let deux = &para.sentences[0].words[1];
        assert_eq!(deux.text, "deux");
        assert!(deux.strong);
        // Les index sont globaux : le titre a pris le mot 0.
        assert_eq!(para.sentences[0].words[0].index, 1);
        assert!(matches!(d.blocks[2].kind, BlockKindView::ListItem { .. }));
        assert_eq!(d.sentence_count, 4);
    }

    #[test]
    fn a_word_jump_shows_in_the_snapshot_as_a_global_index() {
        let mut r = reader_with("# Titre\n\nUn deux trois.");
        r.with_session(Instant::now(), |app| app.seek_sentences(1));
        r.with_session(Instant::now(), |app| app.seek_words(2));
        let s = snapshot(&r);
        assert!(s.has_document);
        assert_eq!(s.revision, 1);
        assert_eq!(s.sentence, 1);
        assert_eq!(s.word, Some(3));
        assert_eq!(s.voice, VoiceState::Ready);
        assert!(!s.playing);
    }

    #[test]
    fn a_resting_reader_still_shows_its_document() {
        let mut r = reader_with("Un. Deux.");
        r.rest();
        let s = snapshot(&r);
        assert!(s.has_document);
        assert_eq!(s.voice, VoiceState::Resting);
        assert_eq!(document(&r).sentence_count, 2);
    }
}
```

Run : `cargo test --lib reader::view` après avoir ajouté `pub mod view;` à
`src/reader/mod.rs` — attendu : erreurs de compilation.

- [ ] **Étape 2 : implémenter**

Haut de `src/reader/view.rs` : les types de la section **Interfaces**
ci-dessus, chacun avec un commentaire d'une ligne, puis :

```rust
//! Ce que l'interface voit du lecteur : un instantané par image, et le
//! document une fois par révision. Des valeurs plates, copiables, sans
//! référence vers le lecteur.

use crate::doc::{BlockKind, Document};
use crate::tts::Readiness;

use super::Reader;

pub fn snapshot(reader: &Reader) -> Snapshot {
    let voice = match (reader.is_resting(), reader.engine()) {
        (true, _) | (_, None) => VoiceState::Resting,
        (false, Some(engine)) => match engine.readiness() {
            Readiness::Ready => VoiceState::Ready,
            Readiness::Loading => VoiceState::Loading,
            Readiness::Fallback(reason) => VoiceState::Fallback { reason },
        },
    };
    let Some(app) = reader.session() else {
        return Snapshot {
            revision: reader.revision(),
            has_document: reader.text_markdown().is_some(),
            speed: reader.config().voice.speed,
            volume: reader.config().voice.volume,
            voice,
            ..Snapshot::default()
        };
    };
    let doc = app.doc();
    let sentence = app.current_sentence();
    let word = app
        .reading_word()
        .and_then(|w| doc.sentence(sentence)?.words.get(w))
        .map(|w| w.index);
    let total = doc.sentences.len().max(1) as f32;
    Snapshot {
        revision: reader.revision(),
        has_document: true,
        sentence,
        word,
        section: doc.section_of_sentence(sentence).map(|s| s.id),
        playing: app.player.is_playing(),
        audible: app.player.is_playing() && app.player.position().is_some(),
        finished: app.is_finished(),
        progress: ((sentence as f32 + app.sentence_progress()) / total).clamp(0.0, 1.0),
        elapsed: app.estimated_elapsed(),
        total: app.estimated_total(),
        speed: app.config.voice.speed,
        volume: app.player.volume(),
        muted: app.player.is_muted(),
        voice,
        status: app.status().to_string(),
        search: app.search().map(|s| SearchState {
            query: s.query.clone(),
            position: s.position(),
            count: s.count(),
        }),
        bookmarked: app.bookmarked_here(),
        skipped: app.skipped_count(),
    }
}

pub fn document(reader: &Reader) -> DocView {
    match (reader.session(), reader.text_markdown()) {
        (Some(app), _) => view_of(app.doc()),
        // Au repos, le document se reconstruit à l'identique depuis son
        // texte : mêmes blocs, mêmes index.
        (None, Some(markdown)) => {
            view_of(&crate::doc::builder::parse_document(markdown, reader.config().code_policy()))
        }
        (None, None) => DocView::default(),
    }
}

fn view_of(doc: &Document) -> DocView {
    DocView {
        title: doc.title.clone(),
        complete: doc.complete,
        sentence_count: doc.sentences.len(),
        sections: doc
            .sections
            .iter()
            .map(|s| SectionView {
                id: s.id,
                level: s.level,
                title: s.title.clone(),
                first_sentence: s.first_sentence,
            })
            .collect(),
        blocks: doc
            .blocks
            .iter()
            .map(|b| BlockView {
                id: b.id,
                kind: kind_of(&b.kind),
                section_id: b.section_id,
                display_lines: b.display_lines.clone(),
                sentences: doc.sentences[b.sentences.0..b.sentences.1]
                    .iter()
                    .map(|s| SentenceView {
                        id: s.id,
                        words: s
                            .words
                            .iter()
                            .map(|w| WordView {
                                index: w.index,
                                text: w.text.clone(),
                                strong: w.emphasis.strong,
                                italic: w.emphasis.italic,
                                code: w.emphasis.code,
                                link: w.emphasis.link,
                                struck: w.emphasis.struck,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn kind_of(kind: &BlockKind) -> BlockKindView {
    match kind {
        BlockKind::Heading { level } => BlockKindView::Heading { level: *level },
        BlockKind::Paragraph => BlockKindView::Paragraph,
        BlockKind::ListItem { depth, ordinal } => BlockKindView::ListItem { depth: *depth, ordinal: *ordinal },
        BlockKind::Quote => BlockKindView::Quote,
        BlockKind::Code { language } => BlockKindView::Code { language: language.clone() },
        BlockKind::TableRow { header } => BlockKindView::TableRow { header: *header },
    }
}
```

Vérifier que `parse_document` est bien atteignable sous ce chemin
(`src/doc/builder.rs:690`, `pub fn parse_document`) et que `doc::builder` est
public ; sinon ajuster le chemin d'import. Si l'index global du premier mot du
paragraphe n'est pas 1 (titre compté autrement), lire comment
`DocumentBuilder` numérote les mots et corriger **l'attente du test**, pas la
numérotation.

- [ ] **Étape 3 : tout faire passer**

Run : `cargo test`
Attendu : tout vert.

- [ ] **Étape 4 : commit**

```bash
git add src/reader/view.rs src/reader/mod.rs
git commit -m "feat(reader): l'instantané et le document vus par l'interface

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 7 : le fil du lecteur

`Reader` ne peut pas changer de fil (la sortie audio est attachée au sien).
`ReaderHandle` le fait vivre sur un fil dédié et lui parle par messages.

**Fichiers :**
- Créer : `src/reader/command.rs`
- Créer : `src/reader/handle.rs`
- Modifier : `src/reader/mod.rs` (`pub mod command; pub mod handle;`)

**Interfaces :**
- Consomme : `Reader`, `view::{snapshot, document}`.
- Produit :

```rust
// command.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    TogglePlay, Pause, Words(isize), Sentences(isize), Blocks(isize), Sections(isize),
    SeekTo(usize), RestartSentence, Start, End, SetSpeed(f32), ChangeSpeed(f32),
    ChangeVolume(f32), ToggleMute, ToggleBookmark, Search(String), SearchAdvance(isize), SearchCancel,
}
impl Command { pub fn apply(self, app: &mut App) }

// handle.rs
pub enum Request {
    OpenText { markdown: String, source: String },
    OpenFile { path: PathBuf, reply: Sender<Result<(), String>> },
    PrepareVoice, Rest, Command(Command),
    Ask(Box<dyn FnOnce(&mut Reader) + Send>),
    Shutdown,
}
pub struct ReaderHandle;
impl ReaderHandle {
    pub fn spawn<F: FnOnce() -> Reader + Send + 'static>(build: F) -> Self;
    pub fn send(&self, request: Request);
    pub fn snapshot(&self) -> Snapshot;
    pub fn document(&self) -> Arc<DocView>;
    /// Exécute `f` sur le fil du lecteur et rend son résultat (bloquant).
    pub fn ask<T: Send + 'static>(&self, f: impl FnOnce(&mut Reader) -> T + Send + 'static) -> Option<T>;
}
```

- [ ] **Étape 1 : tests qui échouent**

En bas de `src/reader/handle.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::time::Duration;

    fn silent_handle() -> ReaderHandle {
        ReaderHandle::spawn(|| {
            let mut config = Config::default();
            config.reading.autoplay = false;
            Reader::new(
                config,
                Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                Box::new(Player::silent),
                Instant::now(),
            )
        })
    }

    fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(Instant::now() < deadline, "{what} : jamais vu");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_opened_text_is_published_with_its_document() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un. Deux.".into(), source: "t".into() });
        eventually("révision 1", || h.snapshot().revision == 1);
        assert_eq!(h.document().sentence_count, 2);
    }

    #[test]
    fn commands_reach_the_reader() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un. Deux. Trois.".into(), source: "t".into() });
        h.send(Request::Command(Command::Sentences(2)));
        eventually("phrase 2", || h.snapshot().sentence == 2);
    }

    #[test]
    fn ask_returns_a_value_from_the_reader_thread() {
        let h = silent_handle();
        h.send(Request::OpenText { markdown: "Un.".into(), source: "t".into() });
        let revision = h.ask(|r| r.revision());
        assert_eq!(revision, Some(1));
    }

    #[test]
    fn dropping_the_handle_stops_the_thread() {
        let h = silent_handle();
        drop(h); // ne doit pas bloquer
    }
}
```

Run : `cargo test --lib reader::handle` — attendu : erreurs de compilation.

- [ ] **Étape 2 : implémenter `command.rs`**

```rust
//! Les gestes de l'interface, traduits en appels au lecteur.

use crate::app::App;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    TogglePlay,
    Pause,
    Words(isize),
    Sentences(isize),
    Blocks(isize),
    Sections(isize),
    SeekTo(usize),
    RestartSentence,
    Start,
    End,
    SetSpeed(f32),
    ChangeSpeed(f32),
    ChangeVolume(f32),
    ToggleMute,
    ToggleBookmark,
    Search(String),
    SearchAdvance(isize),
    SearchCancel,
}

impl Command {
    pub fn apply(self, app: &mut App) {
        match self {
            Command::TogglePlay => app.toggle_play(),
            Command::Pause => {
                if app.player.is_playing() {
                    app.toggle_play();
                }
            }
            Command::Words(delta) => app.seek_words(delta),
            Command::Sentences(delta) => app.seek_sentences(delta),
            Command::Blocks(delta) => app.seek_block(delta),
            Command::Sections(delta) => app.seek_section(delta),
            Command::SeekTo(sentence) => app.seek_to(sentence),
            Command::RestartSentence => app.restart_sentence(),
            Command::Start => app.seek_start(),
            Command::End => app.seek_end(),
            Command::SetSpeed(speed) => app.set_speed(speed),
            Command::ChangeSpeed(delta) => app.change_speed(delta),
            Command::ChangeVolume(delta) => app.change_volume(delta),
            Command::ToggleMute => app.toggle_mute(),
            Command::ToggleBookmark => app.toggle_bookmark(),
            // La recherche du terminal se tape touche par touche ; l'interface
            // la donne d'un bloc.
            Command::Search(query) => {
                app.search_open();
                for c in query.chars() {
                    app.search_push(c);
                }
                app.search_commit();
            }
            Command::SearchAdvance(delta) => app.search_advance(delta),
            Command::SearchCancel => app.search_cancel(),
        }
    }
}
```

- [ ] **Étape 3 : implémenter `handle.rs`**

```rust
//! Le lecteur sur son propre fil.
//!
//! Les commandes arrivent par un canal et sont appliquées aussitôt ; entre
//! deux, le fil fait tourner la lecture et publie un instantané toutes les
//! 16 ms. Le document n'est republié qu'à chaque nouvelle révision.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::command::Command;
use super::view::{self, DocView, Snapshot};
use super::Reader;

const TICK: Duration = Duration::from_millis(16);

pub enum Request {
    OpenText { markdown: String, source: String },
    OpenFile { path: PathBuf, reply: Sender<Result<(), String>> },
    PrepareVoice,
    Rest,
    Command(Command),
    Ask(Box<dyn FnOnce(&mut Reader) + Send>),
    Shutdown,
}

#[derive(Default)]
struct Published {
    snapshot: Snapshot,
    document: Arc<DocView>,
}

pub struct ReaderHandle {
    tx: Sender<Request>,
    published: Arc<Mutex<Published>>,
    thread: Option<JoinHandle<()>>,
}

impl ReaderHandle {
    pub fn spawn<F>(build: F) -> Self
    where
        F: FnOnce() -> Reader + Send + 'static,
    {
        let (tx, rx) = crossbeam_channel::unbounded();
        let published = Arc::new(Mutex::new(Published::default()));
        let out = Arc::clone(&published);
        let thread = thread::Builder::new()
            .name("sillon-lecteur".into())
            .spawn(move || run(build(), rx, out))
            .expect("fil du lecteur");
        Self { tx, published, thread: Some(thread) }
    }

    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.published.lock().unwrap().snapshot.clone()
    }

    pub fn document(&self) -> Arc<DocView> {
        Arc::clone(&self.published.lock().unwrap().document)
    }

    pub fn ask<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Reader) -> T + Send + 'static,
    ) -> Option<T> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.send(Request::Ask(Box::new(move |reader| {
            let _ = reply.send(f(reader));
        })));
        answer.recv_timeout(Duration::from_secs(5)).ok()
    }
}

impl Drop for ReaderHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(mut reader: Reader, rx: Receiver<Request>, out: Arc<Mutex<Published>>) {
    let mut revision = u64::MAX;
    'outer: loop {
        match rx.recv_timeout(TICK) {
            Ok(request) => {
                if !handle(&mut reader, request) {
                    break 'outer;
                }
                // Les touches répétées arrivent en rafale : tout appliquer
                // avant de publier.
                while let Ok(request) = rx.try_recv() {
                    if !handle(&mut reader, request) {
                        break 'outer;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        reader.tick(Instant::now());
        publish(&reader, &out, &mut revision);
    }
    reader.close();
}

/// Applique une requête ; `false` quand il faut s'arrêter.
fn handle(reader: &mut Reader, request: Request) -> bool {
    let now = Instant::now();
    match request {
        Request::OpenText { markdown, source } => reader.open_text(&markdown, &source, now),
        Request::OpenFile { path, reply } => {
            let _ = reply.send(reader.open_file(&path, now).map_err(|e| e.to_string()));
        }
        Request::PrepareVoice => reader.prepare_voice(now),
        Request::Rest => reader.rest(),
        Request::Command(command) => reader.with_session(now, |app| command.apply(app)),
        Request::Ask(f) => f(reader),
        Request::Shutdown => return false,
    }
    true
}

fn publish(reader: &Reader, out: &Mutex<Published>, revision: &mut u64) {
    let snapshot = view::snapshot(reader);
    let document = (snapshot.revision != *revision).then(|| Arc::new(view::document(reader)));
    let mut out = out.lock().unwrap();
    if let Some(document) = document {
        *revision = snapshot.revision;
        out.document = document;
    }
    out.snapshot = snapshot;
}
```

- [ ] **Étape 4 : tout faire passer**

Run : `cargo test`
Attendu : tout vert.

- [ ] **Étape 5 : commit**

```bash
git add src/reader/command.rs src/reader/handle.rs src/reader/mod.rs
git commit -m "feat(reader): le lecteur sur son fil, piloté par messages

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 8 : réglages et voix, vus par l'interface

**Fichiers :**
- Créer : `src/reader/settings.rs`
- Modifier : `src/reader/mod.rs` (`pub mod settings;`, champ `download`,
  méthodes, sondage dans `tick`)
- Modifier : `src/reader/view.rs` (`Snapshot.download`)

**Interfaces :**
- Consomme : `models::{available, install_path, is_installed, Download, Progress}`,
  `Config::{save, align_voice_with_family}`.
- Produit :

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView { pub speed: f32, pub line_spacing: u32, pub reading_ruler: bool, pub keep_voice_minutes: i64 }
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceModel { pub id: String, pub name: String, pub bytes: u64, pub installed: bool, pub active: bool, pub recommended: bool }
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadView { pub id: String, pub received: u64, pub total: u64, pub failed: Option<String> }
impl Reader {
    pub fn settings(&self) -> SettingsView;
    pub fn update_settings(&mut self, s: SettingsView) -> anyhow::Result<()>;
    pub fn voices(&self) -> Vec<VoiceModel>;
    pub fn install_voice(&mut self, id: &str) -> anyhow::Result<()>;
    pub fn use_voice(&mut self, id: &str) -> anyhow::Result<()>;
    pub fn download(&self) -> Option<DownloadView>;
}
// Snapshot reçoit : pub download: Option<DownloadView>
```

- [ ] **Étape 1 : tests qui échouent**

En bas de `src/reader/settings.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn reader() -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    #[test]
    fn settings_reach_the_open_text() {
        let mut r = reader();
        r.open_text("Un.", "a", Instant::now());
        let mut s = r.settings();
        s.speed = 1.4;
        s.reading_ruler = false;
        s.keep_voice_minutes = 30;
        r.apply_settings(s.clone());
        assert_eq!(r.settings(), s);
        let app = r.session().unwrap();
        assert_eq!(app.config.voice.speed, 1.4);
        assert!(!app.config.reading.reading_ruler);
    }

    #[test]
    fn the_catalog_lists_every_available_model_once() {
        let r = reader();
        let voices = r.voices();
        let mut ids: Vec<_> = voices.iter().map(|v| v.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), voices.len());
        assert_eq!(voices.len(), crate::models::available().len());
    }

    #[test]
    fn using_an_unknown_voice_is_an_error() {
        let mut r = reader();
        assert!(r.use_voice("n-existe-pas").is_err());
    }
}
```

`apply_settings` est la partie sans écriture disque de `update_settings`
(qui appelle `apply_settings` puis `Config::save`) : les tests n'écrivent pas
dans la configuration de l'utilisateur.

Run : `cargo test --lib reader::settings` après `pub mod settings;` — attendu :
erreurs de compilation.

- [ ] **Étape 2 : implémenter**

`src/reader/settings.rs` :

```rust
//! Réglages et voix, tels que l'interface les montre et les change.

use anyhow::{anyhow, Result};
use std::time::Instant;

use crate::models::{self, Download, Progress};

use super::Reader;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    pub speed: f32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadView {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub failed: Option<String>,
}

impl Reader {
    pub fn settings(&self) -> SettingsView {
        let c = self.current_config();
        SettingsView {
            speed: c.voice.speed,
            line_spacing: c.reading.line_spacing,
            reading_ruler: c.reading.reading_ruler,
            keep_voice_minutes: c.app.keep_voice_minutes,
        }
    }

    /// Applique sans écrire : le texte ouvert suit tout de suite.
    pub fn apply_settings(&mut self, s: SettingsView) {
        self.sync_config();
        self.config.reading.line_spacing = s.line_spacing;
        self.config.reading.reading_ruler = s.reading_ruler;
        self.config.app.keep_voice_minutes = s.keep_voice_minutes;
        if let Some(app) = self.session.as_mut() {
            app.config.reading.line_spacing = s.line_spacing;
            app.config.reading.reading_ruler = s.reading_ruler;
            app.config.app.keep_voice_minutes = s.keep_voice_minutes;
            app.set_speed(s.speed);
        }
        self.config.voice.speed = s.speed;
    }

    pub fn update_settings(&mut self, s: SettingsView) -> Result<()> {
        self.apply_settings(s);
        self.config.save()?;
        Ok(())
    }

    pub fn voices(&self) -> Vec<VoiceModel> {
        let active = &self.current_config().engine.model_path;
        models::available()
            .into_iter()
            .map(|p| VoiceModel {
                id: p.id.to_string(),
                name: p.display_name.to_string(),
                bytes: p.bytes,
                installed: models::is_installed(p),
                active: models::install_path(p)
                    .is_some_and(|path| path.to_string_lossy() == active.as_str()),
                recommended: p.recommended,
            })
            .collect()
    }

    pub fn install_voice(&mut self, id: &str) -> Result<()> {
        let package = package(id)?;
        self.download = Some(Download::start(package)?);
        Ok(())
    }

    /// Change de voix : la suivante se chargera à la prochaine commande, au
    /// même endroit du texte.
    pub fn use_voice(&mut self, id: &str) -> Result<()> {
        let package = package(id)?;
        let path = models::install_path(package)
            .filter(|_| models::is_installed(package))
            .ok_or_else(|| anyhow!("voix non installée : {id}"))?;
        self.sync_config();
        self.config.engine.model_path = path.to_string_lossy().into_owned();
        self.config.engine.family = package.family.to_string();
        self.config.align_voice_with_family();
        self.config.save()?;
        self.rest();
        Ok(())
    }

    pub fn download(&self) -> Option<DownloadView> {
        let d = self.download.as_ref()?;
        let (received, total, failed) = match d.progress() {
            Progress::Running { received, total } => (received, total, None),
            Progress::Done => (d.package.bytes, d.package.bytes, None),
            Progress::Failed(message) => (0, d.package.bytes, Some(message)),
            Progress::Cancelled => (0, d.package.bytes, Some("annulé".into())),
        };
        Some(DownloadView { id: d.package.id.to_string(), received, total, failed })
    }

    /// Appelé à chaque tour : un téléchargement terminé disparaît, et devient
    /// la voix active s'il n'y en avait aucune.
    pub(super) fn poll_download(&mut self, _now: Instant) {
        let done = self
            .download
            .as_ref()
            .is_some_and(|d| matches!(d.progress(), Progress::Done));
        if !done {
            return;
        }
        let id = self.download.take().map(|d| d.package.id.to_string());
        let none_active = !self.voices().iter().any(|v| v.active);
        if let (Some(id), true) = (id, none_active) {
            let _ = self.use_voice(&id);
        }
    }

    /// La configuration que le texte ouvert a pu modifier (vitesse…).
    fn current_config(&self) -> &crate::config::Config {
        self.session.as_ref().map(|a| &a.config).unwrap_or(&self.config)
    }

    fn sync_config(&mut self) {
        if let Some(app) = self.session.as_ref() {
            self.config = app.config.clone();
        }
    }
}

fn package(id: &str) -> Result<&'static models::ModelPackage> {
    models::available()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!("voix inconnue : {id}"))
}
```

Dans `src/reader/mod.rs` : ajouter `pub mod settings;`, le champ
`download: Option<crate::models::Download>,` à `Reader` (initialisé à `None`),
et en tête de `tick` (avant le `let Some(app)`) : `self.poll_download(now);`.
Vérifier dans `src/models.rs` les noms exacts de `available`, `is_installed`,
`install_path` et le type rendu par `available()` (`Vec<&'static ModelPackage>`) ;
ajuster sans changer `models`.

Dans `src/reader/view.rs`, ajouter à `Snapshot` le champ
`pub download: Option<super::settings::DownloadView>,` et le remplir par
`download: reader.download(),` dans les deux branches de `snapshot`
(`DownloadView` doit dériver `Default` n'est pas nécessaire : `Option`).

- [ ] **Étape 3 : tout faire passer**

Run : `cargo test`
Attendu : tout vert.

- [ ] **Étape 4 : commit**

```bash
git add src/reader/settings.rs src/reader/mod.rs src/reader/view.rs
git commit -m "feat(reader): réglages et voix accessibles à l'interface

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

# Partie C — La liaison

### Tâche 9 : le crate `sillon-ffi` et sa liaison Swift

**Fichiers :**
- Modifier : `Cargo.toml` (section `[workspace]`)
- Créer : `ffi/Cargo.toml`
- Créer : `ffi/src/lib.rs`
- Créer : `ffi/src/bin/uniffi-bindgen.rs`
- Créer : `scripts/build-ffi.sh`
- Modifier : `.gitignore` (`macos/Generated/`, `macos/Libraries/`,
  `macos/*.xcodeproj`)

**Interfaces :**
- Consomme : `ReaderHandle`, `Request`, `Command`, `view::*`, `settings::*`,
  `build_engine`, `Player`.
- Produit (Swift, après génération) : classe `SillonReader` avec
  `init()`, `static func silent() -> SillonReader`,
  `openText(markdown:source:)`, `openFile(path:) throws`, `openClipboard() throws`,
  `prepareVoice()`, `restVoice()`, `command(command: ReaderCommand)`,
  `snapshot() -> Snapshot`, `document() -> DocView`, `bookmarks() -> [BookmarkView]`,
  `settings() -> SettingsView`, `updateSettings(settings:) throws`,
  `voices() -> [VoiceModel]`, `installVoice(id:) throws`, `useVoice(id:) throws`.
  Enum `ReaderCommand` (cas : `togglePlay`, `pause`, `words(delta:)`,
  `sentences(delta:)`, `blocks(delta:)`, `sections(delta:)`, `seekTo(sentence:)`,
  `restartSentence`, `start`, `end`, `setSpeed(speed:)`, `changeSpeed(delta:)`,
  `changeVolume(delta:)`, `toggleMute`, `toggleBookmark`, `search(query:)`,
  `searchAdvance(delta:)`, `searchCancel`). Records `Snapshot`, `DocView`,
  `BlockView`, `SentenceView`, `WordView`, `SectionView`, `SearchState`,
  `SettingsView`, `VoiceModel`, `DownloadView`, `BookmarkView` ; enums
  `VoiceState`, `BlockKindView`. Erreur `SillonError.Failed(message:)`.
  Les entiers sont `UInt32` côté Swift (sauf `revision`, `bytes`, `received`,
  `total` en `UInt64`, et `keepVoiceMinutes` en `Int64`).

- [ ] **Étape 1 : espace de travail et manifeste**

Au **début** de `Cargo.toml` (avant `[package]`) :

```toml
[workspace]
members = [".", "ffi"]
```

`ffi/Cargo.toml` :

```toml
[package]
name = "sillon-ffi"
version = "0.1.0"
edition = "2021"
publish = false
description = "Liaison UniFFI du lecteur sillon pour l'app macOS"

[lib]
name = "sillon_ffi"
# staticlib pour l'app, cdylib pour que uniffi-bindgen lise les métadonnées.
crate-type = ["staticlib", "cdylib", "lib"]

[[bin]]
name = "uniffi-bindgen"
path = "src/bin/uniffi-bindgen.rs"
required-features = ["bindgen"]

[dependencies]
sillon = { path = "..", default-features = false, features = ["clipboard"] }
uniffi = "0.32"
thiserror = "2"

[features]
audio-cpp = ["sillon/audio-cpp"]
ocr = ["sillon/ocr"]
bindgen = ["uniffi/cli"]
```

`ffi/src/bin/uniffi-bindgen.rs` :

```rust
fn main() {
    uniffi::uniffi_bindgen_main()
}
```

Run : `cargo build -p sillon-ffi` — attendu : échec, `src/lib.rs` manquant.

- [ ] **Étape 2 : test qui échoue**

En bas de `ffi/src/lib.rs` (créer le fichier avec ce seul module pour
commencer) :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_silent_reader_opens_text_and_follows_commands() {
        let reader = SillonReader::silent();
        reader.open_text("# Titre\n\nUn. Deux.".into(), "test".into());
        let deadline = Instant::now() + Duration::from_secs(5);
        while reader.snapshot().revision != 1 {
            assert!(Instant::now() < deadline, "texte jamais publié");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(reader.document().sentence_count, 3);
        reader.command(ReaderCommand::Sentences { delta: 2 });
        while reader.snapshot().sentence != 2 {
            assert!(Instant::now() < deadline, "commande jamais appliquée");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(reader.snapshot().voice, VoiceState::Ready));
    }

    #[test]
    fn settings_round_trip_without_touching_disk() {
        let reader = SillonReader::silent();
        let mut s = reader.settings();
        s.keep_voice_minutes = 5;
        reader.apply_settings(s.clone());
        assert_eq!(reader.settings().keep_voice_minutes, 5);
    }
}
```

Run : `cargo test -p sillon-ffi` — attendu : erreurs de compilation.

- [ ] **Étape 3 : implémenter la liaison**

En tête de `ffi/src/lib.rs` :

```rust
//! Le lecteur, vu de Swift.
//!
//! Des types miroirs, aux entiers de taille fixe que UniFFI sait passer, et
//! un objet `SillonReader` qui garde le fil du lecteur. Aucun état ici : tout
//! vit dans `sillon::reader`.

use std::sync::Arc;
use std::time::Instant;

use sillon::audio::Player;
use sillon::config::Config;
use sillon::reader::command::Command;
use sillon::reader::handle::{ReaderHandle, Request};
use sillon::reader::{settings, view, Reader};
use sillon::tts::{self, silent::SilentEngine, TtsEngine};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SillonError {
    #[error("{message}")]
    Failed { message: String },
}

fn failed(e: impl ToString) -> SillonError {
    SillonError::Failed { message: e.to_string() }
}

#[derive(uniffi::Enum)]
pub enum ReaderCommand {
    TogglePlay,
    Pause,
    Words { delta: i32 },
    Sentences { delta: i32 },
    Blocks { delta: i32 },
    Sections { delta: i32 },
    SeekTo { sentence: u32 },
    RestartSentence,
    Start,
    End,
    SetSpeed { speed: f32 },
    ChangeSpeed { delta: f32 },
    ChangeVolume { delta: f32 },
    ToggleMute,
    ToggleBookmark,
    Search { query: String },
    SearchAdvance { delta: i32 },
    SearchCancel,
}

impl From<ReaderCommand> for Command {
    fn from(c: ReaderCommand) -> Self {
        match c {
            ReaderCommand::TogglePlay => Command::TogglePlay,
            ReaderCommand::Pause => Command::Pause,
            ReaderCommand::Words { delta } => Command::Words(delta as isize),
            ReaderCommand::Sentences { delta } => Command::Sentences(delta as isize),
            ReaderCommand::Blocks { delta } => Command::Blocks(delta as isize),
            ReaderCommand::Sections { delta } => Command::Sections(delta as isize),
            ReaderCommand::SeekTo { sentence } => Command::SeekTo(sentence as usize),
            ReaderCommand::RestartSentence => Command::RestartSentence,
            ReaderCommand::Start => Command::Start,
            ReaderCommand::End => Command::End,
            ReaderCommand::SetSpeed { speed } => Command::SetSpeed(speed),
            ReaderCommand::ChangeSpeed { delta } => Command::ChangeSpeed(delta),
            ReaderCommand::ChangeVolume { delta } => Command::ChangeVolume(delta),
            ReaderCommand::ToggleMute => Command::ToggleMute,
            ReaderCommand::ToggleBookmark => Command::ToggleBookmark,
            ReaderCommand::Search { query } => Command::Search(query),
            ReaderCommand::SearchAdvance { delta } => Command::SearchAdvance(delta as isize),
            ReaderCommand::SearchCancel => Command::SearchCancel,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum VoiceState {
    Resting,
    Loading,
    Ready,
    Fallback { reason: String },
}

#[derive(uniffi::Record)]
pub struct SearchState {
    pub query: String,
    pub position: u32,
    pub count: u32,
}

#[derive(uniffi::Record)]
pub struct DownloadView {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub failed: Option<String>,
}

#[derive(uniffi::Record)]
pub struct Snapshot {
    pub revision: u64,
    pub has_document: bool,
    pub sentence: u32,
    pub word: Option<u32>,
    pub section: Option<u32>,
    pub playing: bool,
    pub audible: bool,
    pub finished: bool,
    pub progress: f32,
    pub elapsed: Option<f64>,
    pub total: Option<f64>,
    pub speed: f32,
    pub volume: f32,
    pub muted: bool,
    pub voice: VoiceState,
    pub status: String,
    pub search: Option<SearchState>,
    pub bookmarked: bool,
    pub skipped: u32,
    pub download: Option<DownloadView>,
}

#[derive(uniffi::Record)]
pub struct WordView {
    pub index: u32,
    pub text: String,
    pub strong: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
    pub struck: bool,
}

#[derive(uniffi::Record)]
pub struct SentenceView {
    pub id: u32,
    pub words: Vec<WordView>,
}

#[derive(uniffi::Enum)]
pub enum BlockKindView {
    Paragraph,
    Heading { level: u8 },
    ListItem { depth: u8, ordinal: Option<u64> },
    Quote,
    Code { language: Option<String> },
    TableRow { header: bool },
}

#[derive(uniffi::Record)]
pub struct BlockView {
    pub id: u32,
    pub kind: BlockKindView,
    pub section_id: u32,
    pub display_lines: Vec<String>,
    pub sentences: Vec<SentenceView>,
}

#[derive(uniffi::Record)]
pub struct SectionView {
    pub id: u32,
    pub level: u8,
    pub title: String,
    pub first_sentence: Option<u32>,
}

#[derive(uniffi::Record)]
pub struct DocView {
    pub title: String,
    pub complete: bool,
    pub sentence_count: u32,
    pub sections: Vec<SectionView>,
    pub blocks: Vec<BlockView>,
}

#[derive(uniffi::Record)]
pub struct BookmarkView {
    pub sentence: u32,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
}

#[derive(uniffi::Record, Clone)]
pub struct SettingsView {
    pub speed: f32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
}

#[derive(uniffi::Record)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
}

#[derive(uniffi::Object)]
pub struct SillonReader {
    handle: ReaderHandle,
}

#[uniffi::export]
impl SillonReader {
    /// Le lecteur réel : configuration de l'utilisateur, voix audio.cpp.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handle: ReaderHandle::spawn(|| {
                let (config, _) = Config::load();
                Reader::new(
                    config,
                    Box::new(|config: &Config| {
                        tts::build_engine(config.engine_preference(), &config.engine_settings()).0
                    }),
                    Box::new(Player::new),
                    Instant::now(),
                )
            }),
        })
    }

    /// Lecteur muet, sans modèle ni carte son : tests et mode de test.
    #[uniffi::constructor]
    pub fn silent() -> Arc<Self> {
        Arc::new(Self {
            handle: ReaderHandle::spawn(|| {
                let mut config = Config::default();
                config.reading.autoplay = false;
                config.app.keep_voice_minutes = -1;
                Reader::new(
                    config,
                    Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                    Box::new(Player::silent),
                    Instant::now(),
                )
            }),
        })
    }

    pub fn open_text(&self, markdown: String, source: String) {
        self.handle.send(Request::OpenText { markdown, source });
    }

    pub fn open_file(&self, path: String) -> Result<(), SillonError> {
        let (reply, answer) = crossbeam_channel_bounded();
        self.handle.send(Request::OpenFile { path: path.into(), reply });
        answer
            .recv()
            .map_err(failed)?
            .map_err(failed)
    }

    /// Le presse-papiers : son texte, ou son image reconnue (option `ocr`).
    pub fn open_clipboard(&self) -> Result<(), SillonError> {
        let markdown = sillon::source::clipboard().map_err(failed)?;
        if markdown.trim().is_empty() {
            return Err(failed("le presse-papiers est vide"));
        }
        self.open_text(markdown, "presse-papiers".into());
        Ok(())
    }

    pub fn prepare_voice(&self) {
        self.handle.send(Request::PrepareVoice);
    }

    pub fn rest_voice(&self) {
        self.handle.send(Request::Rest);
    }

    pub fn command(&self, command: ReaderCommand) {
        self.handle.send(Request::Command(command.into()));
    }

    pub fn snapshot(&self) -> Snapshot {
        snapshot_of(self.handle.snapshot())
    }

    pub fn document(&self) -> DocView {
        doc_of(&self.handle.document())
    }

    pub fn bookmarks(&self) -> Vec<BookmarkView> {
        self.handle
            .ask(|reader| {
                let Some(app) = reader.session() else { return Vec::new() };
                app.bookmarks
                    .for_document(&app.source)
                    .map(|b| BookmarkView {
                        sentence: b.sentence_id as u32,
                        section_title: b.section_title.clone(),
                        snippet: b.snippet.clone(),
                        when: b.timestamp(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn settings(&self) -> SettingsView {
        self.handle
            .ask(|reader| settings_of(reader.settings()))
            .unwrap_or(SettingsView { speed: 1.0, line_spacing: 1, reading_ruler: true, keep_voice_minutes: 10 })
    }

    /// Applique sans écrire sur disque.
    pub fn apply_settings(&self, settings: SettingsView) {
        let s = settings_from(settings);
        let _ = self.handle.ask(move |reader| reader.apply_settings(s));
    }

    pub fn update_settings(&self, settings: SettingsView) -> Result<(), SillonError> {
        let s = settings_from(settings);
        self.handle
            .ask(move |reader| reader.update_settings(s).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn voices(&self) -> Vec<VoiceModel> {
        self.handle
            .ask(|reader| {
                reader
                    .voices()
                    .into_iter()
                    .map(|v| VoiceModel {
                        id: v.id,
                        name: v.name,
                        bytes: v.bytes,
                        installed: v.installed,
                        active: v.active,
                        recommended: v.recommended,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn install_voice(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.install_voice(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn use_voice(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.use_voice(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
}

fn crossbeam_channel_bounded() -> (
    sillon::reader::handle::ReplySender,
    sillon::reader::handle::ReplyReceiver,
) {
    sillon::reader::handle::reply_channel()
}

fn settings_of(s: settings::SettingsView) -> SettingsView {
    SettingsView {
        speed: s.speed,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
    }
}

fn settings_from(s: SettingsView) -> settings::SettingsView {
    settings::SettingsView {
        speed: s.speed,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
    }
}

fn snapshot_of(s: view::Snapshot) -> Snapshot {
    Snapshot {
        revision: s.revision,
        has_document: s.has_document,
        sentence: s.sentence as u32,
        word: s.word.map(|w| w as u32),
        section: s.section.map(|w| w as u32),
        playing: s.playing,
        audible: s.audible,
        finished: s.finished,
        progress: s.progress,
        elapsed: s.elapsed,
        total: s.total,
        speed: s.speed,
        volume: s.volume,
        muted: s.muted,
        voice: match s.voice {
            view::VoiceState::Resting => VoiceState::Resting,
            view::VoiceState::Loading => VoiceState::Loading,
            view::VoiceState::Ready => VoiceState::Ready,
            view::VoiceState::Fallback { reason } => VoiceState::Fallback { reason },
        },
        status: s.status,
        search: s.search.map(|q| SearchState {
            query: q.query,
            position: q.position as u32,
            count: q.count as u32,
        }),
        bookmarked: s.bookmarked,
        skipped: s.skipped as u32,
        download: s.download.map(|d| DownloadView {
            id: d.id,
            received: d.received,
            total: d.total,
            failed: d.failed,
        }),
    }
}

fn doc_of(d: &view::DocView) -> DocView {
    DocView {
        title: d.title.clone(),
        complete: d.complete,
        sentence_count: d.sentence_count as u32,
        sections: d
            .sections
            .iter()
            .map(|s| SectionView {
                id: s.id as u32,
                level: s.level,
                title: s.title.clone(),
                first_sentence: s.first_sentence.map(|f| f as u32),
            })
            .collect(),
        blocks: d
            .blocks
            .iter()
            .map(|b| BlockView {
                id: b.id as u32,
                kind: match &b.kind {
                    view::BlockKindView::Paragraph => BlockKindView::Paragraph,
                    view::BlockKindView::Heading { level } => BlockKindView::Heading { level: *level },
                    view::BlockKindView::ListItem { depth, ordinal } => {
                        BlockKindView::ListItem { depth: *depth, ordinal: *ordinal }
                    }
                    view::BlockKindView::Quote => BlockKindView::Quote,
                    view::BlockKindView::Code { language } => {
                        BlockKindView::Code { language: language.clone() }
                    }
                    view::BlockKindView::TableRow { header } => BlockKindView::TableRow { header: *header },
                },
                section_id: b.section_id as u32,
                display_lines: b.display_lines.clone(),
                sentences: b
                    .sentences
                    .iter()
                    .map(|s| SentenceView {
                        id: s.id as u32,
                        words: s
                            .words
                            .iter()
                            .map(|w| WordView {
                                index: w.index as u32,
                                text: w.text.clone(),
                                strong: w.strong,
                                italic: w.italic,
                                code: w.code,
                                link: w.link,
                                struck: w.struck,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}
```

Le crate `ffi` ne dépend pas directement de `crossbeam-channel` : ajouter à
`src/reader/handle.rs` (cœur) un petit relais public, utilisé ci-dessus :

```rust
pub type ReplySender = Sender<Result<(), String>>;
pub type ReplyReceiver = Receiver<Result<(), String>>;

/// Canal de réponse d'un `OpenFile`, pour qui n'a pas `crossbeam` sous la main.
pub fn reply_channel() -> (ReplySender, ReplyReceiver) {
    crossbeam_channel::bounded(1)
}
```

Vérifier les noms exacts `config.engine_preference()` et
`config.engine_settings()` (utilisés dans `src/launch.rs:166`) et
`app.bookmarks` / `app.source` (publics dans `App`).

- [ ] **Étape 4 : tests verts**

Run : `cargo test -p sillon-ffi && cargo test`
Attendu : tout vert, dans les deux crates.

- [ ] **Étape 5 : script de construction**

`scripts/build-ffi.sh` (exécutable, `chmod +x`) :

```bash
#!/usr/bin/env bash
# Compile le moteur pour l'app macOS et génère sa liaison Swift.
#
#   ./scripts/build-ffi.sh                       # audio-cpp + ocr
#   FEATURES= ./scripts/build-ffi.sh             # voix du système seule
#
# Produit macos/Generated (Swift, en-tête, module, Link.xcconfig) et
# macos/Libraries (libsillon_ffi.a, et le shim audio.cpp s'il est compilé).
set -euo pipefail
cd "$(dirname "$0")/.."

FEATURES=${FEATURES-audio-cpp,ocr}
GEN=macos/Generated
LIB=macos/Libraries
mkdir -p "$GEN" "$LIB"

# La liaison d'abord : le binaire bindgen recompile le crate sans les options
# lourdes, et écraserait sinon la bibliothèque finale.
cargo build --release -p sillon-ffi
cargo run --release -p sillon-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/release/libsillon_ffi.dylib --language swift --out-dir "$GEN"
mv "$GEN/sillon_ffiFFI.modulemap" "$GEN/module.modulemap"

cargo build --release -p sillon-ffi ${FEATURES:+--features "$FEATURES"}
cp target/release/libsillon_ffi.a "$LIB/"

FLAGS="-lsillon_ffi -lc++ -framework CoreAudio -framework AudioToolbox -framework AudioUnit -framework CoreFoundation -framework Security -framework Metal -framework Accelerate"
rm -f "$LIB/libaudiocpp_shim.dylib"
shim=$(ls -t target/release/build/sillon-*/out/lib/libaudiocpp_shim.dylib 2>/dev/null | head -1 || true)
if [ -n "$shim" ]; then
    cp "$shim" "$LIB/"
    # Le binaire de l'app le cherchera dans Contents/Frameworks.
    install_name_tool -id @rpath/libaudiocpp_shim.dylib "$LIB/libaudiocpp_shim.dylib"
    FLAGS="$FLAGS -laudiocpp_shim"
    echo "Shim joint : $shim"
fi
echo "OTHER_LDFLAGS = $FLAGS" > "$GEN/Link.xcconfig"
echo "Liaison prête dans $GEN et $LIB"
```

`.gitignore`, ajouter :

```
# App macOS : fichiers générés
macos/Generated/
macos/Libraries/
macos/*.xcodeproj
```

Run : `./scripts/build-ffi.sh`
Attendu : `macos/Generated/sillon_ffi.swift`, `sillon_ffiFFI.h`,
`module.modulemap`, `Link.xcconfig` ; `macos/Libraries/libsillon_ffi.a` et
`libaudiocpp_shim.dylib`. Vérifier :
`grep -c "public func openText" macos/Generated/sillon_ffi.swift` → `1` ;
`otool -D macos/Libraries/libaudiocpp_shim.dylib` → `@rpath/libaudiocpp_shim.dylib`.

- [ ] **Étape 6 : commit**

```bash
git add Cargo.toml Cargo.lock ffi/Cargo.toml ffi/src/lib.rs ffi/src/bin/uniffi-bindgen.rs src/reader/handle.rs scripts/build-ffi.sh .gitignore
git commit -m "feat(ffi): le lecteur exposé à Swift par UniFFI

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

# Partie D — L'app macOS

Toutes les commandes Swift de cette partie supposent `./scripts/build-ffi.sh`
déjà lancé, et se font depuis `macos/` :

```bash
cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test
```

Abrégée plus bas en **`TEST`**. Pour une seule classe :
`… test -only-testing:SillonTests/<Classe>`.

### Tâche 10 : le squelette de l'app

**Fichiers :**
- Créer : `macos/project.yml`
- Créer : `macos/Sources/SillonApp.swift`
- Créer : `macos/Sources/AppDelegate.swift`
- Créer : `macos/Sources/MenuContent.swift`
- Créer : `macos/Tests/ReaderBridgeTests.swift`
- Créer : `macos/Tests/Waiting.swift`
- Créer : `macos/UITests/LaunchUITests.swift`
- Créer : `scripts/build-macos.sh`

**Interfaces :**
- Consomme : `SillonReader` (Tâche 9).
- Produit : `AppDelegate.shared` ; `AppDelegate.reader: SillonReader` ;
  `AppDelegate.isUITesting: Bool` (argument `--ui-testing`) ; aide de test
  `waitUntil(_ timeout: TimeInterval = 5, _ condition: () -> Bool) -> Bool`.

- [ ] **Étape 1 : le projet XcodeGen**

`macos/project.yml` :

```yaml
name: Sillon
options:
  bundleIdPrefix: fr.sillon
  deploymentTarget:
    macOS: "14.0"
packages:
  KeyboardShortcuts:
    url: https://github.com/sindresorhus/KeyboardShortcuts
    from: 2.2.0
configFiles:
  Debug: Generated/Link.xcconfig
  Release: Generated/Link.xcconfig
settings:
  base:
    SWIFT_VERSION: "5.0"
    ARCHS: arm64
    MACOSX_DEPLOYMENT_TARGET: "14.0"
    CODE_SIGN_IDENTITY: "-"
    ENABLE_HARDENED_RUNTIME: NO
targets:
  Sillon:
    type: application
    platform: macOS
    sources:
      - path: Sources
      - path: Generated/sillon_ffi.swift
      - path: Resources
        optional: true
    dependencies:
      - package: KeyboardShortcuts
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: fr.sillon.app
        PRODUCT_NAME: Sillon
        SWIFT_INCLUDE_PATHS: $(SRCROOT)/Generated
        LIBRARY_SEARCH_PATHS: $(SRCROOT)/Libraries
        LD_RUNPATH_SEARCH_PATHS: "@executable_path/../Frameworks"
        GENERATE_INFOPLIST_FILE: YES
        INFOPLIST_KEY_LSUIElement: YES
        INFOPLIST_KEY_CFBundleDisplayName: sillon
        INFOPLIST_KEY_NSHumanReadableCopyright: ""
    postBuildScripts:
      - name: Joindre le shim audio.cpp
        basedOnDependencyAnalysis: false
        script: |
          SHIM="$SRCROOT/Libraries/libaudiocpp_shim.dylib"
          if [ -f "$SHIM" ]; then
            DEST="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH"
            mkdir -p "$DEST"
            cp "$SHIM" "$DEST/"
            codesign --force --sign - "$DEST/libaudiocpp_shim.dylib"
          fi
  SillonTests:
    type: bundle.unit-test
    platform: macOS
    sources: [Tests]
    dependencies:
      - target: Sillon
    settings:
      base:
        SWIFT_INCLUDE_PATHS: $(SRCROOT)/Generated
        TEST_HOST: $(BUILT_PRODUCTS_DIR)/Sillon.app/Contents/MacOS/Sillon
        BUNDLE_LOADER: $(TEST_HOST)
        GENERATE_INFOPLIST_FILE: YES
  SillonUITests:
    type: bundle.ui-testing
    platform: macOS
    sources: [UITests]
    dependencies:
      - target: Sillon
    settings:
      base:
        TEST_TARGET_NAME: Sillon
        GENERATE_INFOPLIST_FILE: YES
schemes:
  Sillon:
    build:
      targets:
        Sillon: all
    test:
      targets: [SillonTests, SillonUITests]
```

- [ ] **Étape 2 : test qui échoue**

`macos/Tests/Waiting.swift` :

```swift
import Foundation

/// Fait tourner la boucle principale jusqu'à ce que `condition` soit vraie.
@MainActor
@discardableResult
func waitUntil(_ timeout: TimeInterval = 5, _ condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while !condition() {
        if Date() > deadline { return false }
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }
    return true
}
```

`macos/Tests/ReaderBridgeTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class ReaderBridgeTests: XCTestCase {
    func testSilentReaderOpensTextAndMoves() {
        let reader = SillonReader.silent()
        reader.openText(markdown: "# Titre\n\nUn. Deux.", source: "test")
        XCTAssertTrue(waitUntil { reader.snapshot().revision == 1 })
        XCTAssertEqual(reader.document().sentenceCount, 3)
        reader.command(command: .sentences(delta: 2))
        XCTAssertTrue(waitUntil { reader.snapshot().sentence == 2 })
    }
}
```

`macos/UITests/LaunchUITests.swift` :

```swift
import XCTest

final class LaunchUITests: XCTestCase {
    func testAppLaunchesInTestMode() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5)
                      || app.wait(for: .runningBackground, timeout: 5))
    }
}
```

Run : `cd macos && xcodegen generate && xcodebuild … test`
Attendu : échec de compilation (aucune source dans `Sources/`).

- [ ] **Étape 3 : l'app minimale**

`macos/Sources/SillonApp.swift` :

```swift
import SwiftUI

@main
struct SillonApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra("sillon", systemImage: "text.line.first.and.arrowtriangle.forward") {
            MenuContent()
        }
    }
}
```

`macos/Sources/AppDelegate.swift` :

```swift
import AppKit

/// Point d'entrée AppKit : le lecteur vit aussi longtemps que l'app.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static private(set) var shared: AppDelegate!

    /// Lancée par les tests d'interface : lecteur muet, texte d'exemple.
    let isUITesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    private(set) var reader: SillonReader!

    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.shared = self
        reader = isUITesting ? SillonReader.silent() : SillonReader()
    }
}
```

`macos/Sources/MenuContent.swift` :

```swift
import SwiftUI

struct MenuContent: View {
    var body: some View {
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
```

- [ ] **Étape 4 : construire et tester**

Run : **`TEST`**
Attendu : `ReaderBridgeTests` et `LaunchUITests` passent. Si l'édition de liens
échoue sur des symboles manquants, les lire (`Undefined symbols … referenced
from libsillon_ffi.a`) : chacun nomme une bibliothèque système à ajouter à
`FLAGS` dans `scripts/build-ffi.sh` (par exemple `_SecRandomCopyBytes` →
`-framework Security`, `_deflate` → `-lz`). Ajouter, relancer
`build-ffi.sh`, recommencer.

- [ ] **Étape 5 : script de bout en bout**

`scripts/build-macos.sh` (exécutable) :

```bash
#!/usr/bin/env bash
# Construit Sillon.app : moteur, liaison, projet, application.
#
#   ./scripts/build-macos.sh            # dans macos/build/Release/Sillon.app
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build-ffi.sh
cd macos
xcodegen generate
xcodebuild -project Sillon.xcodeproj -scheme Sillon -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build
APP=$(find build/derived -name Sillon.app -path "*Release*" -maxdepth 6 | head -1)
rm -rf build/Release && mkdir -p build/Release && cp -R "$APP" build/Release/
# L'app doit trouver son shim hors de l'arbre de compilation.
otool -L build/Release/Sillon.app/Contents/MacOS/Sillon | grep -q "@rpath/libaudiocpp_shim" \
    && test -f build/Release/Sillon.app/Contents/Frameworks/libaudiocpp_shim.dylib \
    || echo "Attention : shim absent — app compilée sans audio-cpp ?"
echo "Prête : macos/build/Release/Sillon.app"
```

Ajouter `macos/build/` au `.gitignore`.

Run : `./scripts/build-macos.sh && open macos/build/Release/Sillon.app`
Attendu : une icône sillon dans la barre des menus, pas d'icône dans le Dock ;
« Quitter sillon » ferme l'app.

- [ ] **Étape 6 : commit**

```bash
git add macos/project.yml macos/Sources macos/Tests macos/UITests scripts/build-macos.sh .gitignore
git commit -m "feat(macos): le squelette de Sillon.app, lié au moteur

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 11 : `ReaderModel`, l'état suivi à chaque image

**Fichiers :**
- Créer : `macos/Sources/Model/ReaderModel.swift`
- Créer : `macos/Sources/Model/DisplayTicker.swift`
- Créer : `macos/Tests/ReaderModelTests.swift`
- Modifier : `macos/Sources/AppDelegate.swift` (créer le modèle)

**Interfaces :**
- Consomme : `SillonReader`.
- Produit :

```swift
@MainActor @Observable final class ReaderModel {
    init(reader: SillonReader)
    private(set) var snapshot: Snapshot
    private(set) var document: DocView
    var message: String?            // ligne d'information du panneau
    func start()                    // suivi à la cadence de l'écran
    func stop()
    func refresh()
    func send(_ command: ReaderCommand)
    func open(text: String, source: String)
    func openClipboard() throws
    func prepareVoice()
    func restVoice()
    func bookmarks() -> [BookmarkView]
    func settings() -> SettingsView
    func updateSettings(_ s: SettingsView) throws
    func voices() -> [VoiceModel]
    func installVoice(_ id: String) throws
    func useVoice(_ id: String) throws
    var currentSentenceText: String { get }   // pour ⌘C
}
// AppDelegate.model: ReaderModel
```

- [ ] **Étape 1 : test qui échoue**

`macos/Tests/ReaderModelTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class ReaderModelTests: XCTestCase {
    private func model(_ text: String) -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        return m
    }

    func testDocumentFollowsRevision() {
        let m = model("Un. Deux. Trois.")
        XCTAssertEqual(m.document.sentenceCount, 3)
        m.open(text: "Autre.", source: "b")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 2 })
        XCTAssertEqual(m.document.sentenceCount, 1)
    }

    func testSendRefreshesSnapshot() {
        let m = model("Un. Deux. Trois.")
        m.send(.sentences(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.sentence == 1 })
        XCTAssertEqual(m.currentSentenceText, "Deux.")
    }

    func testClearingMessageOnOpen() {
        let m = model("Un.")
        m.message = "Sélectionnez ou copiez du texte"
        m.open(text: "Deux.", source: "b")
        XCTAssertNil(m.message)
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/ReaderModelTests`
Attendu : échec de compilation (`ReaderModel` inconnu).

- [ ] **Étape 2 : implémenter**

`macos/Sources/Model/DisplayTicker.swift` :

```swift
import AppKit
import QuartzCore

/// Appelle `tick` à chaque rafraîchissement de l'écran principal.
@MainActor
final class DisplayTicker: NSObject {
    private var link: CADisplayLink?
    private let tick: () -> Void

    init(tick: @escaping () -> Void) {
        self.tick = tick
    }

    func start() {
        guard link == nil, let screen = NSScreen.main else { return }
        let link = screen.displayLink(target: self, selector: #selector(step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func step(_ link: CADisplayLink) {
        tick()
    }
}
```

`macos/Sources/Model/ReaderModel.swift` :

```swift
import Foundation
import Observation

/// L'état du lecteur, relu à chaque image affichée.
///
/// Swift ne garde rien de propre : l'instantané vient de Rust, le document
/// n'est relu que lorsque sa révision change.
@MainActor
@Observable
final class ReaderModel {
    @ObservationIgnored private let reader: SillonReader
    @ObservationIgnored private var ticker: DisplayTicker?

    private(set) var snapshot: Snapshot
    private(set) var document: DocView
    var message: String?

    init(reader: SillonReader) {
        self.reader = reader
        snapshot = reader.snapshot()
        document = reader.document()
    }

    func start() {
        if ticker == nil {
            ticker = DisplayTicker { [weak self] in self?.refresh() }
        }
        ticker?.start()
    }

    func stop() {
        ticker?.stop()
    }

    func refresh() {
        let next = reader.snapshot()
        if next.revision != snapshot.revision {
            document = reader.document()
        }
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if next != snapshot {
            snapshot = next
        }
    }

    func send(_ command: ReaderCommand) {
        reader.command(command: command)
    }

    func open(text: String, source: String) {
        message = nil
        reader.openText(markdown: text, source: source)
    }

    func openClipboard() throws {
        message = nil
        try reader.openClipboard()
    }

    func prepareVoice() { reader.prepareVoice() }
    func restVoice() { reader.restVoice() }
    func bookmarks() -> [BookmarkView] { reader.bookmarks() }
    func settings() -> SettingsView { reader.settings() }
    func updateSettings(_ s: SettingsView) throws { try reader.updateSettings(settings: s) }
    func voices() -> [VoiceModel] { reader.voices() }
    func installVoice(_ id: String) throws { try reader.installVoice(id: id) }
    func useVoice(_ id: String) throws { try reader.useVoice(id: id) }

    /// La phrase en cours telle qu'elle est écrite.
    var currentSentenceText: String {
        for block in document.blocks {
            if let s = block.sentences.first(where: { $0.id == snapshot.sentence }) {
                return s.words.map(\.text).joined(separator: " ")
            }
        }
        return ""
    }
}
```

`AppDelegate` : ajouter `private(set) var model: ReaderModel!` et, après la
création du lecteur : `model = ReaderModel(reader: reader); model.start()`.

Les records UniFFI Swift sont `Equatable` : si `next != snapshot` ne compile
pas, vérifier dans `Generated/sillon_ffi.swift` que `Snapshot` se conforme à
`Equatable` (c'est le cas par défaut en 0.32) ; sinon comparer
`revision`, `sentence`, `word`, `playing`, `progress`, `voice`, `status`.

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** `-only-testing:SillonTests/ReaderModelTests`
Attendu : PASS.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/Model macos/Sources/AppDelegate.swift macos/Tests/ReaderModelTests.swift
git commit -m "feat(macos): l'état du lecteur suivi à chaque image

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 12 : le clavier

**Fichiers :**
- Créer : `macos/Sources/Input/KeyMap.swift`
- Créer : `macos/Tests/KeyMapTests.swift`

**Interfaces :**
- Produit :

```swift
struct KeyPress: Equatable {
    var characters: String      // charactersIgnoringModifiers, casse conservée
    var keyCode: UInt16
    var option = false, shift = false, command = false
    init(_ event: NSEvent)
    init(characters: String, keyCode: UInt16 = 0, option: Bool = false, shift: Bool = false, command: Bool = false)
}
enum ReaderAction: Equatable {
    case command(ReaderCommand)
    case expand, collapseOrHide, copySentence, openSearch
    case toggleSidebar, toggleBookmarks, showHelp, openSettings
}
enum KeyMap { static func action(for key: KeyPress, seekStep: Int32 = 5) -> ReaderAction? }
enum KeyCode { static let left: UInt16 = 123, right: UInt16 = 124, down: UInt16 = 125, up: UInt16 = 126, enter: UInt16 = 36, escape: UInt16 = 53, space: UInt16 = 49, home: UInt16 = 115, end: UInt16 = 119 }
```

`ReaderCommand` doit être `Equatable` (il l'est, généré par UniFFI) ; s'il ne
l'est pas, ajouter dans ce fichier `extension ReaderCommand: Equatable {}`.

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/KeyMapTests.swift` :

```swift
import XCTest
@testable import Sillon

final class KeyMapTests: XCTestCase {
    private func act(_ c: String, _ code: UInt16 = 0, option: Bool = false,
                     shift: Bool = false, command: Bool = false) -> ReaderAction? {
        KeyMap.action(for: KeyPress(characters: c, keyCode: code, option: option,
                                    shift: shift, command: command))
    }

    func testPlayAndWords() {
        XCTAssertEqual(act(" ", KeyCode.space), .command(.togglePlay))
        XCTAssertEqual(act("", KeyCode.left, option: true), .command(.words(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right, option: true), .command(.words(delta: 1)))
    }

    func testSentencesBlocksSections() {
        XCTAssertEqual(act("", KeyCode.left), .command(.sentences(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right), .command(.sentences(delta: 1)))
        XCTAssertEqual(act("", KeyCode.right, shift: true), .command(.sentences(delta: 5)))
        XCTAssertEqual(act("", KeyCode.up), .command(.blocks(delta: -1)))
        XCTAssertEqual(act("", KeyCode.down), .command(.blocks(delta: 1)))
        XCTAssertEqual(act("n"), .command(.sections(delta: 1)))
        XCTAssertEqual(act("p"), .command(.sections(delta: -1)))
    }

    func testSpeedVolumeAndMisc() {
        XCTAssertEqual(act("+"), .command(.changeSpeed(delta: 0.1)))
        XCTAssertEqual(act("-"), .command(.changeSpeed(delta: -0.1)))
        XCTAssertEqual(act("3"), .command(.setSpeed(speed: 1.5)))
        XCTAssertEqual(act("]"), .command(.changeVolume(delta: 0.05)))
        XCTAssertEqual(act("m"), .command(.toggleMute))
        XCTAssertEqual(act("r"), .command(.restartSentence))
        XCTAssertEqual(act("b"), .command(.toggleBookmark))
        XCTAssertEqual(act("g"), .command(.start))
        XCTAssertEqual(act("", KeyCode.end), .command(.end))
    }

    func testCaseMattersForSearchAndBookmarks() {
        XCTAssertEqual(act("N"), .command(.searchAdvance(delta: 1)))
        XCTAssertEqual(act("P"), .command(.searchAdvance(delta: -1)))
        XCTAssertEqual(act("B"), .toggleBookmarks)
        XCTAssertEqual(act("t"), .toggleSidebar)
        XCTAssertEqual(act("/"), .openSearch)
    }

    func testWindowGestures() {
        XCTAssertEqual(act("\r", KeyCode.enter), .expand)
        XCTAssertEqual(act("", KeyCode.escape), .collapseOrHide)
        XCTAssertEqual(act("c", command: true), .copySentence)
        XCTAssertEqual(act(",", command: true), .openSettings)
        XCTAssertEqual(act("?"), .showHelp)
    }

    func testUnknownKeyIsIgnored() {
        XCTAssertNil(act("z"))
        XCTAssertNil(act("x", command: true))
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/KeyMapTests` — attendu : échec de
compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/Input/KeyMap.swift` :

```swift
import AppKit

/// Codes matériels des touches sans caractère.
enum KeyCode {
    static let left: UInt16 = 123, right: UInt16 = 124, down: UInt16 = 125, up: UInt16 = 126
    static let enter: UInt16 = 36, escape: UInt16 = 53, space: UInt16 = 49
    static let home: UInt16 = 115, end: UInt16 = 119
}

struct KeyPress: Equatable {
    var characters: String
    var keyCode: UInt16
    var option = false
    var shift = false
    var command = false

    init(characters: String, keyCode: UInt16 = 0, option: Bool = false,
         shift: Bool = false, command: Bool = false) {
        self.characters = characters
        self.keyCode = keyCode
        self.option = option
        self.shift = shift
        self.command = command
    }

    init(_ event: NSEvent) {
        let flags = event.modifierFlags
        self.init(characters: event.charactersIgnoringModifiers ?? "",
                  keyCode: event.keyCode,
                  option: flags.contains(.option),
                  shift: flags.contains(.shift),
                  command: flags.contains(.command))
    }
}

enum ReaderAction: Equatable {
    case command(ReaderCommand)
    case expand, collapseOrHide, copySentence, openSearch
    case toggleSidebar, toggleBookmarks, showHelp, openSettings
}

/// Les touches du terminal, plus les gestes propres à la fenêtre.
enum KeyMap {
    static func action(for key: KeyPress, seekStep: Int32 = 5) -> ReaderAction? {
        if key.command {
            switch key.characters {
            case "c": return .copySentence
            case ",": return .openSettings
            default: return nil
            }
        }
        switch key.keyCode {
        case KeyCode.space: return .command(.togglePlay)
        case KeyCode.enter: return .expand
        case KeyCode.escape: return .collapseOrHide
        case KeyCode.home: return .command(.start)
        case KeyCode.end: return .command(.end)
        case KeyCode.left, KeyCode.right:
            let sign: Int32 = key.keyCode == KeyCode.left ? -1 : 1
            if key.option { return .command(.words(delta: sign)) }
            if key.shift { return .command(.sentences(delta: sign * seekStep)) }
            return .command(.sentences(delta: sign))
        case KeyCode.up: return .command(.blocks(delta: -1))
        case KeyCode.down: return .command(.blocks(delta: 1))
        default: break
        }
        switch key.characters {
        case "n": return .command(.sections(delta: 1))
        case "p": return .command(.sections(delta: -1))
        case "N": return .command(.searchAdvance(delta: 1))
        case "P": return .command(.searchAdvance(delta: -1))
        case "+", "=": return .command(.changeSpeed(delta: 0.1))
        case "-", "_": return .command(.changeSpeed(delta: -0.1))
        case "1": return .command(.setSpeed(speed: 0.75))
        case "2": return .command(.setSpeed(speed: 1.0))
        case "3": return .command(.setSpeed(speed: 1.5))
        case "4": return .command(.setSpeed(speed: 2.0))
        case "[": return .command(.changeVolume(delta: -0.05))
        case "]": return .command(.changeVolume(delta: 0.05))
        case "m": return .command(.toggleMute)
        case "r": return .command(.restartSentence)
        case "b": return .command(.toggleBookmark)
        case "g": return .command(.start)
        case "B": return .toggleBookmarks
        case "t": return .toggleSidebar
        case "/": return .openSearch
        case "?", "h": return .showHelp
        default: return nil
        }
    }
}
```

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** `-only-testing:SillonTests/KeyMapTests` — attendu : PASS.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/Input/KeyMap.swift macos/Tests/KeyMapTests.swift
git commit -m "feat(macos): les touches du lecteur, celles du terminal et trois de plus

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 13 : le style Papier et les polices

**Fichiers :**
- Créer : `macos/Resources/Fonts/` (Literata, Atkinson Hyperlegible,
  OpenDyslexic, et leurs licences OFL)
- Créer : `macos/Sources/Style/Theme.swift`
- Créer : `macos/Sources/Style/ReadingFont.swift`
- Créer : `macos/Tests/ThemeTests.swift`

**Interfaces :**
- Produit :

```swift
struct Theme: Equatable {
    var background, text, past, wordFill, wordText, line, chrome, border: NSColor
    static let paper: Theme, night: Theme, contrast: Theme
    static func resolve(dark: Bool, increaseContrast: Bool) -> Theme
    static var current: Theme   // lit l'apparence et NSWorkspace
}
enum ReadingFont: String, CaseIterable, Identifiable {
    case literata, atkinson, openDyslexic
    var label: String
    var familyName: String
    func font(size: CGFloat, bold: Bool = false, italic: Bool = false) -> NSFont
    static func registerBundled()
}
enum DisplayDefaults { static let fontKey = "readingFont", sizeKey = "readingSize"; static let defaultSize: Double = 22 }
```

- [ ] **Étape 1 : rapatrier les polices**

```bash
mkdir -p macos/Resources/Fonts && cd macos/Resources/Fonts
for f in AtkinsonHyperlegible-Regular.ttf AtkinsonHyperlegible-Italic.ttf AtkinsonHyperlegible-BoldItalic.ttf \
         OpenDyslexic-Regular.otf OpenDyslexic-Bold.otf OpenDyslexic-Italic.otf OpenDyslexic-BoldItalic.otf OFL-OpenDyslexic.txt; do
    git show "5c2a0fe^:assets/fonts/$f" > "$f"
done
cp ../../../assets/fonts/AtkinsonHyperlegible-Bold.ttf .
curl -fsSLo "Literata[opsz,wght].ttf" "https://github.com/google/fonts/raw/main/ofl/literata/Literata%5Bopsz,wght%5D.ttf"
curl -fsSLo "Literata-Italic[opsz,wght].ttf" "https://github.com/google/fonts/raw/main/ofl/literata/Literata-Italic%5Bopsz,wght%5D.ttf"
curl -fsSLo OFL-Literata.txt "https://github.com/google/fonts/raw/main/ofl/literata/OFL.txt"
ls
```

Attendu : douze fichiers, aucun vide (`find . -size 0` ne rend rien). Vérifier
l'existence de `assets/fonts/AtkinsonHyperlegible-Bold.ttf` et de la licence
Atkinson dans `assets/fonts/` ; copier aussi cette licence si elle s'y trouve.

- [ ] **Étape 2 : tests qui échouent**

`macos/Tests/ThemeTests.swift` :

```swift
import XCTest
@testable import Sillon

final class ThemeTests: XCTestCase {
    func testResolution() {
        XCTAssertEqual(Theme.resolve(dark: false, increaseContrast: false), .paper)
        XCTAssertEqual(Theme.resolve(dark: true, increaseContrast: false), .night)
        XCTAssertEqual(Theme.resolve(dark: false, increaseContrast: true), .contrast)
        XCTAssertEqual(Theme.resolve(dark: true, increaseContrast: true), .contrast)
    }

    func testPaperColorsMatchTheSpec() {
        XCTAssertEqual(Theme.paper.background.hex, "#FBF8F1")
        XCTAssertEqual(Theme.paper.wordFill.hex, "#CFE3FF")
        XCTAssertEqual(Theme.night.background.hex, "#1E1B17")
        XCTAssertEqual(Theme.contrast.wordFill.hex, "#FFE600")
    }

    func testBundledFontsResolve() {
        ReadingFont.registerBundled()
        for font in ReadingFont.allCases {
            let nsFont = font.font(size: 20)
            XCTAssertEqual(nsFont.familyName, font.familyName, "\(font)")
        }
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/ThemeTests` — attendu : échec de
compilation.

- [ ] **Étape 3 : implémenter**

`macos/Sources/Style/Theme.swift` :

```swift
import AppKit

extension NSColor {
    convenience init(hex: UInt32) {
        self.init(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                  green: CGFloat((hex >> 8) & 0xFF) / 255,
                  blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }

    var hex: String {
        let c = usingColorSpace(.sRGB) ?? self
        return String(format: "#%02X%02X%02X",
                      Int((c.redComponent * 255).rounded()),
                      Int((c.greenComponent * 255).rounded()),
                      Int((c.blueComponent * 255).rounded()))
    }
}

/// Les couleurs du lecteur. « Papier » le jour, sa version de nuit le soir,
/// le contraste élevé dès que macOS le demande.
struct Theme: Equatable {
    var background: NSColor
    var text: NSColor
    var past: NSColor
    var wordFill: NSColor
    var wordText: NSColor
    var line: NSColor
    /// Texte des contrôles.
    var chrome: NSColor
    var border: NSColor

    static let paper = Theme(
        background: NSColor(hex: 0xFBF8F1), text: NSColor(hex: 0x2A2722),
        past: NSColor(hex: 0x9A948A), wordFill: NSColor(hex: 0xCFE3FF),
        wordText: NSColor(hex: 0x1B2A44), line: NSColor(hex: 0x7AA7E8),
        chrome: NSColor(hex: 0x8A847A), border: NSColor(hex: 0xE6E0D3))

    static let night = Theme(
        background: NSColor(hex: 0x1E1B17), text: NSColor(hex: 0xE8E1D4),
        past: NSColor(hex: 0x8A8276), wordFill: NSColor(hex: 0x2C4A75),
        wordText: NSColor(hex: 0xE6EFFF), line: NSColor(hex: 0x7AA7E8),
        chrome: NSColor(hex: 0x9A9286), border: NSColor(hex: 0x3A342C))

    static let contrast = Theme(
        background: .black, text: .white, past: NSColor(hex: 0xB0B0B0),
        wordFill: NSColor(hex: 0xFFE600), wordText: .black,
        line: NSColor(hex: 0xFFE600), chrome: .white, border: .white)

    static func resolve(dark: Bool, increaseContrast: Bool) -> Theme {
        if increaseContrast { return .contrast }
        return dark ? .night : .paper
    }

    @MainActor
    static var current: Theme {
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        return resolve(dark: dark,
                       increaseContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast)
    }
}
```

`macos/Sources/Style/ReadingFont.swift` :

```swift
import AppKit
import CoreText

enum DisplayDefaults {
    static let fontKey = "readingFont"
    static let sizeKey = "readingSize"
    static let defaultSize: Double = 22
}

/// Les polices de lecture, embarquées dans l'app.
enum ReadingFont: String, CaseIterable, Identifiable {
    case literata, atkinson, openDyslexic

    var id: String { rawValue }

    var label: String {
        switch self {
        case .literata: "Literata"
        case .atkinson: "Atkinson Hyperlegible"
        case .openDyslexic: "OpenDyslexic"
        }
    }

    var familyName: String { label }

    func font(size: CGFloat, bold: Bool = false, italic: Bool = false) -> NSFont {
        var traits: NSFontTraitMask = []
        if bold { traits.insert(.boldFontMask) }
        if italic { traits.insert(.italicFontMask) }
        return NSFontManager.shared.font(withFamily: familyName, traits: traits,
                                         weight: bold ? 9 : 5, size: size)
            ?? .systemFont(ofSize: size, weight: bold ? .semibold : .regular)
    }

    /// Rend les polices du paquet utilisables par leur nom de famille.
    static func registerBundled() {
        guard let urls = Bundle.main.urls(forResourcesWithExtension: nil, subdirectory: "Fonts")
        else { return }
        let fonts = urls.filter { ["ttf", "otf"].contains($0.pathExtension.lowercased()) }
        CTFontManagerRegisterFontURLs(fonts as CFArray, .process, true, nil)
    }
}
```

Dans `project.yml`, la source `Resources` doit garder l'arborescence `Fonts/` :
remplacer l'entrée par

```yaml
      - path: Resources/Fonts
        type: folder
        buildPhase: resources
```

Appeler `ReadingFont.registerBundled()` en tête de
`applicationDidFinishLaunching`.

Si `testBundledFontsResolve` échoue pour Literata parce que la famille
s'annonce autrement, lire le nom réel :
`mdls -name com_apple_ats_name_family "macos/Resources/Fonts/Literata[opsz,wght].ttf"`
et corriger `familyName` pour ce cas (garder `label` lisible).

- [ ] **Étape 4 : tests verts**

Run : **`TEST`** `-only-testing:SillonTests/ThemeTests` — attendu : PASS.

- [ ] **Étape 5 : commit**

```bash
git add macos/Resources/Fonts macos/Sources/Style macos/Tests/ThemeTests.swift macos/project.yml macos/Sources/AppDelegate.swift
git commit -m "feat(macos): le style Papier, sa nuit, le contraste élevé et trois polices

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 14 : la vue de lecture (TextKit 2)

**Fichiers :**
- Créer : `macos/Sources/Reading/DocumentLayout.swift`
- Créer : `macos/Sources/Reading/ReadingTextView.swift`
- Créer : `macos/Sources/Reading/ReadingView.swift`
- Créer : `macos/Tests/DocumentLayoutTests.swift`

**Interfaces :**
- Consomme : `DocView`, `Snapshot`, `Theme`, `ReadingFont`, `ReaderModel`.
- Produit :

```swift
struct ReadingStyle: Equatable { var font: ReadingFont; var size: CGFloat; var lineSpacing: UInt32; var ruler: Bool; var theme: Theme }
struct DocumentLayout {
    let text: NSAttributedString
    let wordRanges: [UInt32: NSRange]        // index global → plage
    let sentenceRanges: [UInt32: NSRange]    // phrase → plage (mots seuls)
    static func build(_ doc: DocView, style: ReadingStyle) -> DocumentLayout
}
final class ReadingTextView: NSTextView {
    func show(word: NSRange?, sentence: NSRange?, pastUpTo: Int, follow: Bool)
    var contentHeight: CGFloat { get }        // borné, pour le panneau
    var onContentHeight: ((CGFloat) -> Void)?
}
struct ReadingView: NSViewRepresentable { init(model: ReaderModel, style: ReadingStyle, onContentHeight: @escaping (CGFloat) -> Void = { _ in }) }
```

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/DocumentLayoutTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class DocumentLayoutTests: XCTestCase {
    private func doc(_ markdown: String) -> DocView {
        let r = SillonReader.silent()
        r.openText(markdown: markdown, source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return r.document()
    }

    private let style = ReadingStyle(font: .literata, size: 20, lineSpacing: 1,
                                     ruler: true, theme: .paper)

    func testEveryWordMapsToItsOwnText() {
        let d = doc("# Titre\n\nUn **deux** trois. Quatre.")
        let layout = DocumentLayout.build(d, style: style)
        let s = layout.text.string as NSString
        for block in d.blocks {
            for sentence in block.sentences {
                for word in sentence.words {
                    let range = try! XCTUnwrap(layout.wordRanges[word.index])
                    XCTAssertEqual(s.substring(with: range), word.text)
                }
            }
        }
    }

    func testSentenceRangeCoversItsWords() {
        let d = doc("Un deux. Trois quatre.")
        let layout = DocumentLayout.build(d, style: style)
        let s = layout.text.string as NSString
        XCTAssertEqual(s.substring(with: layout.sentenceRanges[1]!), "Trois quatre.")
    }

    func testCodeBlockShowsItsLinesAndMapsNoWord() {
        let d = doc("Avant.\n\n```rust\nfn main() {}\n```\n\nAprès.")
        let layout = DocumentLayout.build(d, style: style)
        XCTAssertTrue(layout.text.string.contains("fn main() {}"))
        let code = d.blocks.first { if case .code = $0.kind { true } else { false } }!
        for w in code.sentences.flatMap(\.words) {
            XCTAssertNil(layout.wordRanges[w.index])
        }
    }

    func testStrongWordIsBold() {
        let d = doc("Un **deux** trois.")
        let layout = DocumentLayout.build(d, style: style)
        let deux = d.blocks[0].sentences[0].words[1]
        let font = layout.text.attribute(.font, at: layout.wordRanges[deux.index]!.location,
                                         effectiveRange: nil) as! NSFont
        XCTAssertTrue(font.fontDescriptor.symbolicTraits.contains(.bold))
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/DocumentLayoutTests` — attendu :
échec de compilation.

- [ ] **Étape 2 : implémenter `DocumentLayout`**

`macos/Sources/Reading/DocumentLayout.swift` :

```swift
import AppKit

struct ReadingStyle: Equatable {
    var font: ReadingFont
    var size: CGFloat
    var lineSpacing: UInt32
    var ruler: Bool
    var theme: Theme
}

/// Le document mis en texte, avec la plage de chaque mot et de chaque phrase.
///
/// Les mots sont séparés d'une espace, les blocs d'un retour à la ligne ; un
/// bloc qui a des lignes à montrer telles quelles (code, tableau) les montre
/// en chasse fixe, et ses mots — une annonce — ne sont pas surlignés.
struct DocumentLayout {
    let text: NSAttributedString
    let wordRanges: [UInt32: NSRange]
    let sentenceRanges: [UInt32: NSRange]

    static func build(_ doc: DocView, style: ReadingStyle) -> DocumentLayout {
        let out = NSMutableAttributedString()
        var words: [UInt32: NSRange] = [:]
        var sentences: [UInt32: NSRange] = [:]
        let mono = NSFont.monospacedSystemFont(ofSize: style.size * 0.85, weight: .regular)

        for (i, block) in doc.blocks.enumerated() {
            if i > 0 { out.append(NSAttributedString(string: "\n")) }
            let paragraph = paragraphStyle(for: block.kind, style: style)
            let base = baseFont(for: block.kind, style: style)
            var attrs: [NSAttributedString.Key: Any] = [
                .font: base, .foregroundColor: color(for: block.kind, style: style),
                .paragraphStyle: paragraph,
            ]

            if !block.displayLines.isEmpty {
                attrs[.font] = mono
                out.append(NSAttributedString(string: block.displayLines.joined(separator: "\n"),
                                              attributes: attrs))
                continue
            }
            if let prefix = prefix(for: block.kind) {
                out.append(NSAttributedString(string: prefix, attributes: attrs))
            }
            for (s, sentence) in block.sentences.enumerated() {
                if s > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                let start = out.length
                for (w, word) in sentence.words.enumerated() {
                    if w > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                    var wordAttrs = attrs
                    wordAttrs[.font] = word.code ? mono
                        : style.font.font(size: base.pointSize,
                                          bold: word.strong || isHeading(block.kind),
                                          italic: word.italic)
                    if word.link {
                        wordAttrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                        wordAttrs[.underlineColor] = style.theme.line
                    }
                    if word.struck {
                        wordAttrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                    }
                    words[word.index] = NSRange(location: out.length,
                                                length: (word.text as NSString).length)
                    out.append(NSAttributedString(string: word.text, attributes: wordAttrs))
                }
                sentences[sentence.id] = NSRange(location: start, length: out.length - start)
            }
        }
        return DocumentLayout(text: out, wordRanges: words, sentenceRanges: sentences)
    }

    private static func isHeading(_ kind: BlockKindView) -> Bool {
        if case .heading = kind { return true }
        return false
    }

    private static func baseFont(for kind: BlockKindView, style: ReadingStyle) -> NSFont {
        switch kind {
        case .heading(let level):
            let scale: CGFloat = level == 1 ? 1.6 : level == 2 ? 1.35 : 1.15
            return style.font.font(size: style.size * scale, bold: true)
        default:
            return style.font.font(size: style.size)
        }
    }

    private static func color(for kind: BlockKindView, style: ReadingStyle) -> NSColor {
        if case .quote = kind { return style.theme.chrome }
        return style.theme.text
    }

    private static func prefix(for kind: BlockKindView) -> String? {
        if case .listItem(_, let ordinal) = kind {
            return ordinal.map { "\($0). " } ?? "• "
        }
        return nil
    }

    private static func paragraphStyle(for kind: BlockKindView, style: ReadingStyle) -> NSParagraphStyle {
        let p = NSMutableParagraphStyle()
        // `line_spacing` compte des lignes vides en terminal ; ici, une fraction
        // de ligne par cran.
        p.lineHeightMultiple = 1.35 + 0.25 * CGFloat(style.lineSpacing)
        p.paragraphSpacing = style.size * 0.6
        switch kind {
        case .heading:
            p.paragraphSpacingBefore = style.size * 0.8
        case .listItem(let depth, _):
            let indent = style.size * 1.2 * CGFloat(depth + 1)
            p.headIndent = indent
            p.firstLineHeadIndent = indent - style.size * 0.9
        case .quote:
            p.headIndent = style.size * 1.2
            p.firstLineHeadIndent = style.size * 1.2
        default:
            break
        }
        return p
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/DocumentLayoutTests` — attendu :
PASS. Si le cas d'énumération généré s'appelle autrement (`.listItem(depth:ordinal:)`),
lire `Generated/sillon_ffi.swift` et ajuster les motifs.

- [ ] **Étape 3 : implémenter `ReadingTextView`**

`macos/Sources/Reading/ReadingTextView.swift` :

```swift
import AppKit
import QuartzCore

/// Le texte lu, avec son mot surligné qui glisse, sa ligne soulignée, et le
/// déjà-lu atténué. TextKit 2 ne met en page que ce qui est visible.
final class ReadingTextView: NSTextView {
    var theme: Theme = .paper { didSet { needsDisplay = true } }
    var ruler = true
    var onContentHeight: ((CGFloat) -> Void)?

    private var shown: CGRect?
    private var from: CGRect?
    private var target: CGRect?
    private var lineBand: CGRect?
    private var animationStart: CFTimeInterval = 0
    private var link: CADisplayLink?
    private var dimmedUpTo = 0
    private var underlined: NSRange?
    private var lastLineY: CGFloat?
    private static let glide: CFTimeInterval = 0.08

    convenience init() {
        self.init(usingTextLayoutManager: true)
        isEditable = false
        isSelectable = false
        drawsBackground = false
        textContainerInset = NSSize(width: 4, height: 8)
        isVerticallyResizable = true
        isHorizontallyResizable = false
        autoresizingMask = [.width]
        textContainer?.widthTracksTextView = true
        setAccessibilityRole(.staticText)
        setAccessibilityLabel("Texte lu")
    }

    private var reduceMotion: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }

    func setDocument(_ text: NSAttributedString) {
        textStorage?.setAttributedString(text)
        shown = nil; target = nil; lineBand = nil; dimmedUpTo = 0; underlined = nil; lastLineY = nil
        needsDisplay = true
        onContentHeight?(contentHeight)
    }

    /// Hauteur du texte, sans mettre en page au-delà de ce qu'un panneau
    /// peut montrer.
    var contentHeight: CGFloat {
        guard let tlm = textLayoutManager else { return 0 }
        let length = textStorage?.length ?? 0
        if length > 4000 { return .greatestFiniteMagnitude }
        tlm.ensureLayout(for: tlm.documentRange)
        return tlm.usageBoundsForTextContainer.height + textContainerInset.height * 2
    }

    func show(word: NSRange?, sentence: NSRange?, pastUpTo: Int, follow: Bool) {
        applyPast(upTo: pastUpTo)
        applyUnderline(sentence)
        guard let word, let rect = rect(for: word) else {
            target = nil; shown = nil; needsDisplay = true
            return
        }
        lineBand = ruler ? CGRect(x: 0, y: rect.minY - 2, width: bounds.width, height: rect.height + 4) : nil
        if shown == nil || reduceMotion {
            shown = rect; target = rect
            needsDisplay = true
        } else if rect != target {
            from = shown; target = rect
            animationStart = CACurrentMediaTime()
            startLink()
        }
        if follow, lastLineY != rect.minY {
            lastLineY = rect.minY
            keepLineInPlace(rect)
        }
        setAccessibilityValue(sentence.flatMap { range in
            (textStorage?.string as NSString?)?.substring(with: range)
        })
    }

    override func drawBackground(in dirtyRect: NSRect) {
        super.drawBackground(in: dirtyRect)
        if let band = lineBand {
            theme.wordFill.withAlphaComponent(0.25).setFill()
            band.fill()
        }
        if let r = shown {
            theme.wordFill.setFill()
            NSBezierPath(roundedRect: r.insetBy(dx: -3, dy: -1), xRadius: 4, yRadius: 4).fill()
        }
    }

    // MARK: - Glissement du surlignage

    private func startLink() {
        guard link == nil else { return }
        let link = displayLink(target: self, selector: #selector(step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func step(_ link: CADisplayLink) {
        guard let from, let target else { stopLink(); return }
        let t = min(1, (CACurrentMediaTime() - animationStart) / Self.glide)
        let e = 1 - pow(1 - t, 3)
        shown = CGRect(x: from.minX + (target.minX - from.minX) * e,
                       y: from.minY + (target.minY - from.minY) * e,
                       width: from.width + (target.width - from.width) * e,
                       height: from.height + (target.height - from.height) * e)
        needsDisplay = true
        if t >= 1 { stopLink() }
    }

    private func stopLink() {
        link?.invalidate()
        link = nil
    }

    // MARK: - Attributs de rendu

    private func applyPast(upTo location: Int) {
        guard location != dimmedUpTo, let tlm = textLayoutManager,
              let all = textRange(NSRange(location: 0, length: textStorage?.length ?? 0)) else { return }
        tlm.removeRenderingAttribute(.foregroundColor, for: all)
        if location > 0, let past = textRange(NSRange(location: 0, length: location)) {
            tlm.addRenderingAttribute(.foregroundColor, value: theme.past, for: past)
        }
        dimmedUpTo = location
    }

    private func applyUnderline(_ sentence: NSRange?) {
        guard sentence != underlined, let tlm = textLayoutManager else { return }
        if let old = underlined, let r = textRange(old) {
            tlm.removeRenderingAttribute(.underlineStyle, for: r)
            tlm.removeRenderingAttribute(.underlineColor, for: r)
        }
        if let new = sentence, let r = textRange(new) {
            tlm.addRenderingAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, for: r)
            tlm.addRenderingAttribute(.underlineColor, value: theme.line, for: r)
        }
        underlined = sentence
    }

    // MARK: - Géométrie

    private func textRange(_ range: NSRange) -> NSTextRange? {
        guard let tcm = textLayoutManager?.textContentManager,
              let start = tcm.location(tcm.documentRange.location, offsetBy: range.location),
              let end = tcm.location(start, offsetBy: range.length) else { return nil }
        return NSTextRange(location: start, end: end)
    }

    private func rect(for range: NSRange) -> CGRect? {
        guard let tlm = textLayoutManager, let tr = textRange(range) else { return nil }
        tlm.ensureLayout(for: tr)
        var result: CGRect?
        tlm.enumerateTextSegments(in: tr, type: .standard, options: []) { _, frame, _, _ in
            result = result.map { $0.union(frame) } ?? frame
            return true
        }
        return result?.offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
    }

    /// La ligne lue reste au même endroit : c'est le texte qui monte.
    private func keepLineInPlace(_ rect: CGRect) {
        guard let clip = enclosingScrollView?.contentView else { return }
        let y = max(0, rect.midY - clip.bounds.height * 0.35)
        let origin = NSPoint(x: 0, y: min(y, max(0, frame.height - clip.bounds.height)))
        if reduceMotion {
            clip.setBoundsOrigin(origin)
        } else {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.2
                clip.animator().setBoundsOrigin(origin)
            }
        }
        enclosingScrollView?.reflectScrolledClipView(clip)
    }
}
```

- [ ] **Étape 4 : implémenter `ReadingView`**

`macos/Sources/Reading/ReadingView.swift` :

```swift
import SwiftUI

/// Le texte lu, dans SwiftUI. Le document n'est remis en page qu'à chaque
/// révision ou changement de style ; le reste n'est que surlignage.
struct ReadingView: NSViewRepresentable {
    let model: ReaderModel
    let style: ReadingStyle
    var onContentHeight: (CGFloat) -> Void = { _ in }

    final class Coordinator {
        var revision: UInt64 = .max
        var style: ReadingStyle?
        var layout: DocumentLayout?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        let text = ReadingTextView()
        text.onContentHeight = onContentHeight
        scroll.documentView = text
        scroll.setAccessibilityIdentifier("texte-lu")
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let text = scroll.documentView as? ReadingTextView else { return }
        let c = context.coordinator
        let snapshot = model.snapshot
        text.theme = style.theme
        text.ruler = style.ruler
        if c.revision != snapshot.revision || c.style != style {
            let layout = DocumentLayout.build(model.document, style: style)
            text.setDocument(layout.text)
            c.layout = layout
            c.revision = snapshot.revision
            c.style = style
        }
        guard let layout = c.layout else { return }
        let sentence = layout.sentenceRanges[snapshot.sentence]
        let word = snapshot.word.flatMap { layout.wordRanges[$0] }
        text.show(word: word, sentence: sentence,
                  pastUpTo: sentence?.location ?? 0, follow: true)
    }
}
```

- [ ] **Étape 5 : vérifier**

Run : **`TEST`** — attendu : tout vert (la vue n'a pas de test d'interface
propre ; elle est éprouvée par les Tâches 15 et 16).

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/Reading macos/Tests/DocumentLayoutTests.swift
git commit -m "feat(macos): la vue de lecture — mot qui glisse, ligne tenue, déjà-lu atténué

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 15 : le panneau flottant

**Fichiers :**
- Créer : `macos/Sources/Panel/ReadingPanel.swift`
- Créer : `macos/Sources/Panel/PanelController.swift`
- Créer : `macos/Sources/Panel/PanelContent.swift`
- Créer : `macos/Sources/Panel/PlayerBar.swift`
- Créer : `macos/Tests/PanelGeometryTests.swift`
- Créer : `macos/UITests/PanelUITests.swift`
- Modifier : `macos/Sources/AppDelegate.swift`

**Interfaces :**
- Consomme : `ReaderModel`, `KeyMap`, `ReadingView`, `Theme`, `ReadingFont`.
- Produit :

```swift
enum PanelMode: Equatable { case hidden, panel, expanded }
enum PanelGeometry {
    static func panelFrame(contentHeight: CGFloat, screen: CGRect) -> CGRect
    static func expandedFrame(screen: CGRect) -> CGRect
}
@MainActor @Observable final class PanelController {
    init(model: ReaderModel)
    private(set) var mode: PanelMode
    var reading: SettingsView
    var sidebarVisible: Bool
    var searchActive: Bool
    var helpVisible: Bool
    var bookmarksVisible: Bool
    func show()
    func expand()
    func collapseOrHide()
    func perform(_ action: ReaderAction)
    func contentHeightChanged(_ height: CGFloat)
}
// AppDelegate.panel: PanelController
```

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/PanelGeometryTests.swift` :

```swift
import XCTest
@testable import Sillon

final class PanelGeometryTests: XCTestCase {
    let screen = CGRect(x: 0, y: 0, width: 1800, height: 1100)

    func testPanelIsCenteredAndFollowsShortText() {
        let f = PanelGeometry.panelFrame(contentHeight: 80, screen: screen)
        XCTAssertEqual(f.midX, screen.midX, accuracy: 1)
        XCTAssertEqual(f.height, 80 + PanelGeometry.chrome, accuracy: 1)
        XCTAssertLessThanOrEqual(f.width, 760)
    }

    func testPanelHeightIsBoundedForLongText() {
        let f = PanelGeometry.panelFrame(contentHeight: .greatestFiniteMagnitude, screen: screen)
        XCTAssertEqual(f.height, screen.height * 0.45, accuracy: 1)
    }

    func testPanelNeverShrinksBelowMinimum() {
        let f = PanelGeometry.panelFrame(contentHeight: 0, screen: screen)
        XCTAssertEqual(f.height, PanelGeometry.minimumHeight, accuracy: 1)
    }

    func testExpandedFillsTheScreenWithAMargin() {
        let f = PanelGeometry.expandedFrame(screen: screen)
        XCTAssertEqual(f, screen.insetBy(dx: 24, dy: 24))
    }
}
```

`macos/UITests/PanelUITests.swift` :

```swift
import XCTest

final class PanelUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
    }

    func testKeyboardOnlyFlow() {
        let panel = app.otherElements["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))

        let play = app.buttons["lecture"]
        XCTAssertEqual(play.label, "Lecture")
        panel.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))

        panel.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(app.otherElements["lecteur-agrandi"].waitForExistence(timeout: 2))

        panel.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.otherElements["panneau-lecture"].waitForExistence(timeout: 2))

        panel.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.otherElements["panneau-lecture"].waitForNonExistence(timeout: 2))
    }
}

extension XCUIElement {
    func waitForLabel(_ label: String, timeout: TimeInterval = 2) -> Bool {
        let predicate = NSPredicate(format: "label == %@", label)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [exp], timeout: timeout) == .completed
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/PanelGeometryTests` — attendu :
échec de compilation.

- [ ] **Étape 2 : géométrie et fenêtre**

`macos/Sources/Panel/ReadingPanel.swift` :

```swift
import AppKit

enum PanelMode: Equatable { case hidden, panel, expanded }

/// Tailles du panneau, calculées sans fenêtre pour être éprouvées.
enum PanelGeometry {
    /// Barre de lecture, aides et marges autour du texte.
    static let chrome: CGFloat = 96
    static let minimumHeight: CGFloat = 150
    static let maximumWidth: CGFloat = 760

    static func panelFrame(contentHeight: CGFloat, screen: CGRect) -> CGRect {
        let width = min(maximumWidth, screen.width * 0.6)
        let wanted = contentHeight >= .greatestFiniteMagnitude / 2
            ? .greatestFiniteMagnitude : contentHeight + chrome
        let height = min(max(wanted, minimumHeight), screen.height * 0.45)
        // Le haut du panneau à un quart de l'écran, comme Spotlight.
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - width / 2, y: top - height, width: width, height: height)
    }

    static func expandedFrame(screen: CGRect) -> CGRect {
        screen.insetBy(dx: 24, dy: 24)
    }
}

/// Une fenêtre qui prend le clavier sans activer l'app : l'application en
/// cours reste au premier plan derrière.
final class ReadingPanel: NSPanel {
    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 720, height: 200),
                   styleMask: [.nonactivatingPanel, .titled, .fullSizeContentView, .resizable],
                   backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        isMovableByWindowBackground = true
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        for button in [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton] {
            standardWindowButton(button)?.isHidden = true
        }
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
```

Run : **`TEST`** `-only-testing:SillonTests/PanelGeometryTests` — attendu :
PASS.

- [ ] **Étape 3 : le contrôleur**

`macos/Sources/Panel/PanelController.swift` :

```swift
import AppKit
import SwiftUI

/// Ouvre, agrandit, réduit et masque le panneau ; route le clavier.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored private let window = ReadingPanel()
    @ObservationIgnored private var keyMonitor: Any?
    @ObservationIgnored private var contentHeight: CGFloat = 0

    private(set) var mode: PanelMode = .hidden
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture : un changement dans les réglages s'applique au suivant.
    var reading: SettingsView
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    var helpVisible = false

    init(model: ReaderModel) {
        self.model = model
        reading = model.settings()
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
    }

    private var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    func show() {
        reading = model.settings()
        if mode == .hidden {
            window.setFrame(PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame),
                            display: false)
            mode = .panel
        }
        window.makeKeyAndOrderFront(nil)
        installKeyMonitor()
    }

    func expand() {
        guard mode == .panel else { return }
        mode = .expanded
        animate(to: PanelGeometry.expandedFrame(screen: screenFrame))
        updateIdentifier()
    }

    func collapseOrHide() {
        switch mode {
        case .expanded:
            mode = .panel
            animate(to: PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame))
            updateIdentifier()
        case .panel:
            model.send(.pause)
            window.orderOut(nil)
            mode = .hidden
            removeKeyMonitor()
        case .hidden:
            break
        }
    }

    func contentHeightChanged(_ height: CGFloat) {
        contentHeight = height
        guard mode == .panel else { return }
        animate(to: PanelGeometry.panelFrame(contentHeight: height, screen: screenFrame))
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .expand: expand()
        case .collapseOrHide:
            if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else { collapseOrHide() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            model.message = "Phrase copiée"
        case .openSearch:
            if mode == .panel { expand() }
            searchActive = true
        case .toggleSidebar:
            if mode == .expanded { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode == .panel { expand() }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp: helpVisible.toggle()
        case .openSettings:
            NSApp.activate()
            NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
        }
    }

    private func animate(to frame: CGRect) {
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true)
        } else {
            window.setFrame(frame, display: true, animate: true)
        }
    }

    private func updateIdentifier() {
        window.contentView?.setAccessibilityIdentifier(mode == .expanded ? "lecteur-agrandi" : "panneau-lecture")
    }

    private func installKeyMonitor() {
        guard keyMonitor == nil else { return }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            // Un champ de saisie garde ses touches, sauf Échap.
            if self.window.firstResponder is NSTextView,
               !(self.window.firstResponder is ReadingTextView),
               event.keyCode != KeyCode.escape {
                return event
            }
            guard let action = KeyMap.action(for: KeyPress(event)) else { return event }
            self.perform(action)
            return nil
        }
    }

    private func removeKeyMonitor() {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
    }
}
```

- [ ] **Étape 4 : le contenu**

`macos/Sources/Panel/PlayerBar.swift` :

```swift
import SwiftUI

/// Lecture, progression, temps, vitesse. Contrôles fixes à 13 pt.
struct PlayerBar: View {
    let model: ReaderModel
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let s = model.snapshot
        HStack(spacing: 12) {
            Button {
                model.send(.togglePlay)
            } label: {
                Image(systemName: s.playing ? "pause.fill" : "play.fill")
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("lecture")
            .accessibilityLabel(s.playing ? "Pause" : "Lecture")

            Timeline(progress: Double(s.progress), markers: markers, theme: theme)
                .frame(height: 9)
                .accessibilityElement()
                .accessibilityLabel("Progression")
                .accessibilityValue("\(Int(s.progress * 100)) %")

            Text(Self.time(s.elapsed) + " / ~" + Self.time(s.total))
                .monospacedDigit()
            Text(String(format: "%.1f×", s.speed).replacingOccurrences(of: ".", with: ","))
                .accessibilityLabel("Vitesse")
                .accessibilityValue(String(format: "%.1f fois", s.speed))
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }

    static func time(_ seconds: Double?) -> String {
        guard let seconds else { return "–:––" }
        let t = Int(seconds.rounded())
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}

/// La ligne de temps : un trait, la part lue, un repère par section.
struct Timeline: View {
    let progress: Double
    let markers: [Double]
    let theme: Theme

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(nsColor: theme.border)).frame(height: 3)
                Capsule().fill(Color(nsColor: theme.line))
                    .frame(width: geo.size.width * progress, height: 3)
                ForEach(markers, id: \.self) { m in
                    Rectangle().fill(Color(nsColor: theme.chrome))
                        .frame(width: 1, height: 9)
                        .offset(x: geo.size.width * m)
                }
            }
            .frame(maxHeight: .infinity)
        }
    }
}
```

`macos/Sources/Panel/PanelContent.swift` :

```swift
import SwiftUI

struct PanelContent: View {
    let controller: PanelController
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize
    @Environment(\.colorScheme) private var colorScheme

    private var theme: Theme {
        Theme.resolve(dark: colorScheme == .dark,
                      increaseContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast)
    }

    private var style: ReadingStyle {
        ReadingStyle(font: ReadingFont(rawValue: fontName) ?? .literata,
                     size: size,
                     lineSpacing: controller.reading.lineSpacing,
                     ruler: controller.reading.readingRuler,
                     theme: theme)
    }

    var body: some View {
        let model = controller.model
        VStack(alignment: .leading, spacing: 14) {
            if controller.mode == .expanded {
                ExpandedReader(controller: controller, style: style)
            } else {
                if let message = model.message ?? emptyMessage {
                    Text(message)
                        .font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: theme.chrome))
                }
                ReadingView(model: model, style: style) { height in
                    controller.contentHeightChanged(height)
                }
                PlayerBar(model: model, theme: theme)
                HStack(spacing: 14) {
                    hint("⏎", "agrandir")
                    hint("esc", "masquer")
                    hint("?", "raccourcis")
                }
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 18)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(Color(nsColor: theme.background))
                .overlay(RoundedRectangle(cornerRadius: 14)
                    .stroke(Color(nsColor: theme.border), lineWidth: 1))
        )
        .overlay { if controller.helpVisible { HelpOverlay(theme: theme) } }
    }

    private var emptyMessage: String? {
        controller.model.snapshot.hasDocument ? nil : "Sélectionnez ou copiez du texte, puis ⌃⌥Espace"
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key)
                .font(.system(size: 11, design: .monospaced))
                .padding(.horizontal, 5).padding(.vertical, 1)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color(nsColor: theme.border)))
            Text(label)
        }
        .font(.system(size: 11))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .accessibilityElement(children: .combine)
    }
}

/// Les raccourcis, en surimpression ; n'importe quelle touche la ferme.
struct HelpOverlay: View {
    let theme: Theme
    static let rows: [(String, String)] = [
        ("Espace", "Lecture / pause"), ("⌥← ⌥→", "Mot précédent / suivant"),
        ("← →", "Phrase précédente / suivante"), ("⇧← ⇧→", "Recul / avance rapide"),
        ("↑ ↓", "Paragraphe précédent / suivant"), ("n p", "Titre suivant / précédent"),
        ("+ − · 1…4", "Vitesse"), ("r", "Reprendre la phrase"),
        ("/ · N P", "Chercher · résultat suivant / précédent"),
        ("t · B · b", "Sommaire · signets · poser un signet"),
        ("⏎", "Agrandir"), ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),
        ("Échap", "Revenir · masquer"),
    ]

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 6) {
            ForEach(Self.rows, id: \.0) { row in
                GridRow {
                    Text(row.0).font(.system(size: 13, design: .monospaced))
                    Text(row.1).font(.system(size: 13))
                }
            }
        }
        .padding(20)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: theme.background)))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color(nsColor: theme.border)))
        .foregroundStyle(Color(nsColor: theme.text))
        .accessibilityIdentifier("aide-raccourcis")
    }
}
```

`ExpandedReader` est l'objet de la Tâche 16 ; pour compiler maintenant, créer
`macos/Sources/Panel/ExpandedReader.swift` avec un premier corps minimal qui
sera remplacé :

```swift
import SwiftUI

struct ExpandedReader: View {
    let controller: PanelController
    let style: ReadingStyle

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ReadingView(model: controller.model, style: style)
            PlayerBar(model: controller.model, theme: style.theme)
        }
    }
}
```

- [ ] **Étape 5 : brancher sur l'app**

Dans `AppDelegate` : `private(set) var panel: PanelController!` ; après le
modèle : `panel = PanelController(model: model)`. Puis, en mode test :

```swift
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.show() }
        }
```

avec

```swift
    static let sample = """
    # Les monades

    Une monade est une structure qui encapsule une valeur et un contexte. \
    Les monades permettent d'enchaîner des calculs qui portent un contexte, \
    sans le manipuler à la main à chaque étape.

    ## Exemples

    On les retrouve partout : options, listes, promesses.
    """
```

Dans `MenuContent`, ajouter en tête :
`Button("Afficher sillon") { AppDelegate.shared.panel.show() }`.

- [ ] **Étape 6 : tests verts**

Run : **`TEST`**
Attendu : tout vert, dont `PanelUITests.testKeyboardOnlyFlow`.

- [ ] **Étape 7 : commit**

```bash
git add macos/Sources/Panel macos/Sources/AppDelegate.swift macos/Sources/MenuContent.swift macos/Tests/PanelGeometryTests.swift macos/UITests/PanelUITests.swift
git commit -m "feat(macos): le panneau flottant, piloté au clavier

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 16 : le lecteur agrandi

**Fichiers :**
- Modifier : `macos/Sources/Panel/ExpandedReader.swift` (remplacer le corps
  minimal)
- Créer : `macos/Sources/Panel/Sidebar.swift`
- Créer : `macos/Sources/Panel/SectionMarkers.swift`
- Créer : `macos/Tests/SectionMarkersTests.swift`
- Créer : `macos/UITests/ExpandedUITests.swift`

**Interfaces :**
- Consomme : `PanelController` (`sidebarVisible`, `bookmarksVisible`,
  `searchActive`), `ReaderModel.bookmarks()`, `DocView.sections`.
- Produit : `enum SectionMarkers { static func positions(_ doc: DocView) -> [Double]; static func breadcrumb(_ doc: DocView, section: UInt32?) -> String }`.

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/SectionMarkersTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class SectionMarkersTests: XCTestCase {
    private func doc(_ markdown: String) -> DocView {
        let r = SillonReader.silent()
        r.openText(markdown: markdown, source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return r.document()
    }

    func testOneMarkerPerSectionAtItsFirstSentence() {
        let d = doc("# A\n\nUn. Deux.\n\n# B\n\nTrois.")
        let m = SectionMarkers.positions(d)
        XCTAssertEqual(m.count, 2)
        XCTAssertEqual(m[0], 0, accuracy: 0.001)
        XCTAssertEqual(m[1], 3.0 / 5.0, accuracy: 0.001)
    }

    func testBreadcrumbJoinsParentAndChild() {
        let d = doc("# Monades\n\n## Exemples\n\nTexte.")
        let child = d.sections.first { $0.title == "Exemples" }!
        XCTAssertEqual(SectionMarkers.breadcrumb(d, section: child.id), "Monades ▸ Exemples")
    }
}
```

`macos/UITests/ExpandedUITests.swift` :

```swift
import XCTest

final class ExpandedUITests: XCTestCase {
    func testSidebarSearchAndBack() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        let panel = app.otherElements["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey(.return, modifierFlags: [])
        let reader = app.otherElements["lecteur-agrandi"]
        XCTAssertTrue(reader.waitForExistence(timeout: 2))
        XCTAssertTrue(app.outlines["sommaire"].exists || app.tables["sommaire"].exists)

        reader.typeKey("t", modifierFlags: [])
        XCTAssertTrue(app.outlines["sommaire"].waitForNonExistence(timeout: 2))

        reader.typeKey("/", modifierFlags: [])
        let field = app.textFields["recherche"]
        XCTAssertTrue(field.waitForExistence(timeout: 2))
        field.typeText("promesses\r")
        XCTAssertTrue(app.staticTexts["resultats-recherche"].waitForExistence(timeout: 2))

        reader.typeKey(.escape, modifierFlags: [])
        reader.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.otherElements["panneau-lecture"].waitForExistence(timeout: 2))
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/SectionMarkersTests` — attendu :
échec de compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/Panel/SectionMarkers.swift` :

```swift
/// Repères de section sur la ligne de temps, et fil d'Ariane.
enum SectionMarkers {
    static func positions(_ doc: DocView) -> [Double] {
        let total = Double(max(doc.sentenceCount, 1))
        return doc.sections.compactMap { s in s.firstSentence.map { Double($0) / total } }
    }

    static func breadcrumb(_ doc: DocView, section: UInt32?) -> String {
        guard let section, let index = doc.sections.firstIndex(where: { $0.id == section })
        else { return doc.title }
        var trail = [doc.sections[index]]
        var level = doc.sections[index].level
        for s in doc.sections[..<index].reversed() where s.level < level {
            trail.insert(s, at: 0)
            level = s.level
        }
        return trail.map(\.title).joined(separator: " ▸ ")
    }
}
```

`macos/Sources/Panel/Sidebar.swift` :

```swift
import SwiftUI

/// Sommaire et signets. Flèches pour parcourir, Entrée pour y aller.
struct Sidebar: View {
    let controller: PanelController
    let theme: Theme
    @State private var selection: UInt32?

    var body: some View {
        let model = controller.model
        let items: [(id: UInt32, title: String, indent: Int, sentence: UInt32?)] =
            controller.bookmarksVisible
            ? model.bookmarks().map { ($0.sentence, "« \($0.snippet) » · \($0.when)", 0, $0.sentence) }
            : model.document.sections.map { ($0.id, $0.title, Int($0.level) - 1, $0.firstSentence) }

        VStack(alignment: .leading, spacing: 6) {
            Text(controller.bookmarksVisible ? "Signets" : "Sommaire")
                .font(.system(size: 13, weight: .semibold))
            List(items, id: \.id, selection: $selection) { item in
                Text(item.title)
                    .font(.system(size: 13))
                    .padding(.leading, CGFloat(item.indent) * 12)
                    .lineLimit(1)
                    .tag(item.id)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .accessibilityIdentifier("sommaire")
            .onKeyPress(.return) {
                if let id = selection, let s = items.first(where: { $0.id == id })?.sentence {
                    model.send(.seekTo(sentence: s))
                }
                return .handled
            }
        }
        .foregroundStyle(Color(nsColor: theme.text))
        .frame(width: 220)
        .onAppear { selection = model.snapshot.section }
    }
}
```

`macos/Sources/Panel/ExpandedReader.swift` (corps complet) :

```swift
import SwiftUI

/// Le panneau agrandi : sommaire, fil d'Ariane, recherche, texte pleine
/// largeur, ligne de temps à repères.
struct ExpandedReader: View {
    let controller: PanelController
    let style: ReadingStyle
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        let model = controller.model
        HStack(alignment: .top, spacing: 18) {
            if controller.sidebarVisible {
                Sidebar(controller: controller, theme: style.theme)
                Divider()
            }
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text(SectionMarkers.breadcrumb(model.document, section: model.snapshot.section))
                        .lineLimit(1)
                    Spacer()
                    if controller.searchActive {
                        TextField("Chercher", text: $query)
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 220)
                            .focused($searchFocused)
                            .accessibilityIdentifier("recherche")
                            .onSubmit {
                                model.send(.search(query: query))
                                searchFocused = false
                            }
                            .onAppear { searchFocused = true }
                    }
                    if let search = model.snapshot.search {
                        Text(search.count == 0 ? "Aucun résultat"
                             : "\(search.position + 1) / \(search.count)")
                            .accessibilityIdentifier("resultats-recherche")
                    }
                }
                .font(.system(size: 13))
                .foregroundStyle(Color(nsColor: style.theme.chrome))

                ReadingView(model: model, style: style)
                PlayerBar(model: model, theme: style.theme,
                          markers: SectionMarkers.positions(model.document))
            }
        }
    }
}
```

Le moniteur clavier de `PanelController` laisse déjà ses touches au champ de
recherche (premier répondant `NSTextView` autre que `ReadingTextView`) ; Échap
y passe et ferme la recherche.

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** — attendu : tout vert, dont `ExpandedUITests`. Si le sommaire
SwiftUI ne s'expose ni comme `outline` ni comme `table`, lire l'arbre
d'accessibilité réel (`print(app.debugDescription)` dans le test) et viser le
type qu'il donne.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/Panel macos/Tests/SectionMarkersTests.swift macos/UITests/ExpandedUITests.swift
git commit -m "feat(macos): le lecteur agrandi — sommaire, signets, recherche, repères

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 17 : capturer le texte

**Fichiers :**
- Créer : `macos/Sources/Capture/TextCapture.swift`
- Créer : `macos/Sources/Capture/SystemPasteboard.swift`
- Créer : `macos/Tests/TextCaptureTests.swift`

**Interfaces :**
- Produit :

```swift
protocol TextSource { func text() async -> String? }
struct SelectionSource: TextSource { init(trusted: @escaping () -> Bool = { AXIsProcessTrusted() }) }
protocol PasteboardAccess: AnyObject {
    var changeCount: Int { get }
    func save() -> [[String: Data]]
    func restore(_ items: [[String: Data]])
    func string() -> String?
}
protocol KeystrokeSender { func sendCopy() }
struct CopySource: TextSource { init(pasteboard: PasteboardAccess, keys: KeystrokeSender, trusted: @escaping () -> Bool = { AXIsProcessTrusted() }, timeout: Duration = .milliseconds(250)) }
struct CaptureChain { let sources: [TextSource]; func capture() async -> String? }
final class SystemPasteboard: PasteboardAccess
struct SystemKeystrokes: KeystrokeSender
```

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/TextCaptureTests.swift` :

```swift
import XCTest
@testable import Sillon

final class FakePasteboard: PasteboardAccess {
    var changeCount = 0
    var items: [[String: Data]] = [["public.utf8-plain-text": Data("ancien".utf8)]]
    func save() -> [[String: Data]] { items }
    func restore(_ saved: [[String: Data]]) { items = saved; changeCount += 1 }
    func string() -> String? {
        items.first?["public.utf8-plain-text"].flatMap { String(data: $0, encoding: .utf8) }
    }
}

struct FakeKeys: KeystrokeSender {
    let pasteboard: FakePasteboard
    let copied: String?
    func sendCopy() {
        guard let copied else { return }
        pasteboard.items = [["public.utf8-plain-text": Data(copied.utf8)]]
        pasteboard.changeCount += 1
    }
}

struct Stub: TextSource {
    let value: String?
    func text() async -> String? { value }
}

final class TextCaptureTests: XCTestCase {
    func testCopyReadsThenRestoresThePasteboard() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "sélection"),
                                trusted: { true })
        let text = await source.text()
        XCTAssertEqual(text, "sélection")
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testCopyThatChangesNothingTimesOutAndTouchesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: nil),
                                trusted: { true }, timeout: .milliseconds(50))
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.string(), "ancien")
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testCopyWithoutPermissionDoesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "x"),
                                trusted: { false })
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testBlankCopyCountsAsNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "  \n"),
                                trusted: { true })
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testChainTakesTheFirstSourceThatAnswers() async {
        let chain = CaptureChain(sources: [Stub(value: nil), Stub(value: "deux"), Stub(value: "trois")])
        let text = await chain.capture()
        XCTAssertEqual(text, "deux")
        let empty = await CaptureChain(sources: [Stub(value: nil)]).capture()
        XCTAssertNil(empty)
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/TextCaptureTests` — attendu : échec
de compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/Capture/TextCapture.swift` :

```swift
import AppKit
import ApplicationServices

/// Une façon d'obtenir le texte que l'utilisateur veut entendre.
protocol TextSource {
    func text() async -> String?
}

private func nonBlank(_ s: String?) -> String? {
    guard let s, !s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
    return s
}

/// La sélection, lue directement dans l'application au premier plan.
/// Instantané, et le presse-papiers n'est pas touché.
struct SelectionSource: TextSource {
    var trusted: () -> Bool = { AXIsProcessTrusted() }

    func text() async -> String? {
        guard trusted() else { return nil }
        let system = AXUIElementCreateSystemWide()
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString,
                                            &focused) == .success,
              let element = focused, CFGetTypeID(element) == AXUIElementGetTypeID()
        else { return nil }
        var selected: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element as! AXUIElement,
                                            kAXSelectedTextAttribute as CFString,
                                            &selected) == .success
        else { return nil }
        return nonBlank(selected as? String)
    }
}

protocol PasteboardAccess: AnyObject {
    var changeCount: Int { get }
    func save() -> [[String: Data]]
    func restore(_ items: [[String: Data]])
    func string() -> String?
}

protocol KeystrokeSender {
    func sendCopy()
}

/// Pour les applications qui ne donnent pas leur sélection (Chrome,
/// Electron…) : un ⌘C simulé, puis le presse-papiers remis comme avant.
struct CopySource: TextSource {
    let pasteboard: PasteboardAccess
    let keys: KeystrokeSender
    var trusted: () -> Bool = { AXIsProcessTrusted() }
    var timeout: Duration = .milliseconds(250)

    func text() async -> String? {
        guard trusted() else { return nil }
        let saved = pasteboard.save()
        let before = pasteboard.changeCount
        keys.sendCopy()
        let deadline = ContinuousClock.now + timeout
        while pasteboard.changeCount == before {
            if ContinuousClock.now >= deadline { return nil }
            try? await Task.sleep(for: .milliseconds(10))
        }
        let copied = pasteboard.string()
        pasteboard.restore(saved)
        return nonBlank(copied)
    }
}

/// Essaie chaque source dans l'ordre ; la première qui répond l'emporte.
struct CaptureChain {
    let sources: [TextSource]

    func capture() async -> String? {
        for source in sources {
            if let text = await source.text() { return text }
        }
        return nil
    }
}
```

`macos/Sources/Capture/SystemPasteboard.swift` :

```swift
import AppKit
import CoreGraphics

final class SystemPasteboard: PasteboardAccess {
    private let board = NSPasteboard.general

    var changeCount: Int { board.changeCount }

    /// Tous les éléments, sous tous leurs types : une image copiée revient
    /// intacte, pas seulement le texte.
    func save() -> [[String: Data]] {
        (board.pasteboardItems ?? []).map { item in
            var out: [String: Data] = [:]
            for type in item.types {
                if let data = item.data(forType: type) { out[type.rawValue] = data }
            }
            return out
        }
    }

    func restore(_ items: [[String: Data]]) {
        board.clearContents()
        let restored = items.map { saved -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in saved { item.setData(data, forType: NSPasteboard.PasteboardType(type)) }
            return item
        }
        if !restored.isEmpty { board.writeObjects(restored) }
    }

    func string() -> String? { board.string(forType: .string) }
}

/// ⌘C envoyé à l'application au premier plan. Les modificateurs du raccourci
/// encore enfoncés (⌃⌥) sont écrasés : l'événement porte ⌘ seul.
struct SystemKeystrokes: KeystrokeSender {
    func sendCopy() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let c: CGKeyCode = 8
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: c, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cghidEventTap)
        }
    }
}
```

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** `-only-testing:SillonTests/TextCaptureTests` — attendu : PASS.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/Capture macos/Tests/TextCaptureTests.swift
git commit -m "feat(macos): capturer la sélection, sinon la copier sans perdre le presse-papiers

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 18 : raccourcis globaux, autorisation, connexion, menu

**Fichiers :**
- Créer : `macos/Sources/App/Shortcuts.swift`
- Créer : `macos/Sources/App/ReadFlow.swift`
- Créer : `macos/Sources/App/Onboarding.swift`
- Créer : `macos/Tests/ReadFlowTests.swift`
- Modifier : `macos/Sources/AppDelegate.swift`, `macos/Sources/MenuContent.swift`

**Interfaces :**
- Consomme : `CaptureChain`, `SelectionSource`, `CopySource`,
  `SystemPasteboard`, `SystemKeystrokes`, `ReaderModel`, `PanelController`.
- Produit :

```swift
extension KeyboardShortcuts.Name { static let readSelection: Self; static let showPanel: Self }
@MainActor struct ReadFlow {
    let prepare: () -> Void
    let capture: () async -> String?
    let open: (String) -> Void
    let openClipboard: () throws -> Void
    let say: (String) -> Void
    let show: () -> Void
    func run() async
}
enum Onboarding { static func runIfNeeded(); static var needsReminder: Bool }
enum LoginItem { static func enableOnFirstLaunch(); static var isEnabled: Bool { get }; static func set(_ on: Bool) }
```

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/ReadFlowTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class ReadFlowTests: XCTestCase {
    private var log: [String] = []

    private func flow(capture: String?, clipboard: Bool) -> ReadFlow {
        ReadFlow(
            prepare: { self.log.append("voix") },
            capture: { self.log.append("capture"); return capture },
            open: { self.log.append("ouvre:\($0)") },
            openClipboard: {
                self.log.append("presse-papiers")
                if !clipboard { throw SillonError.Failed(message: "vide") }
            },
            say: { self.log.append("dit:\($0)") },
            show: { self.log.append("montre") })
    }

    func testSelectionWinsAndVoiceStartsFirst() async {
        await flow(capture: "texte", clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "ouvre:texte", "montre"])
    }

    func testFallsBackToClipboard() async {
        await flow(capture: nil, clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "montre"])
    }

    func testNothingToReadSaysSoAndStillShows() async {
        await flow(capture: nil, clipboard: false).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers",
                             "dit:Sélectionnez ou copiez du texte", "montre"])
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/ReadFlowTests` — attendu : échec de
compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/App/ReadFlow.swift` :

```swift
/// Ce que fait le raccourci « Lire », dans l'ordre de la spec : la voix se
/// charge pendant que le texte se capture ; la capture se fait avant que le
/// panneau ne prenne le clavier, sinon ⌘C le viserait lui.
@MainActor
struct ReadFlow {
    let prepare: () -> Void
    let capture: () async -> String?
    let open: (String) -> Void
    let openClipboard: () throws -> Void
    let say: (String) -> Void
    let show: () -> Void

    func run() async {
        prepare()
        if let text = await capture() {
            open(text)
        } else {
            do { try openClipboard() } catch { say("Sélectionnez ou copiez du texte") }
        }
        show()
    }
}
```

`macos/Sources/App/Shortcuts.swift` :

```swift
import KeyboardShortcuts
import ServiceManagement

extension KeyboardShortcuts.Name {
    static let readSelection = Self("readSelection", default: .init(.space, modifiers: [.control, .option]))
    static let showPanel = Self("showPanel", default: .init(.s, modifiers: [.control, .option]))
}

/// « Ouvrir à la connexion », activé au premier lancement.
enum LoginItem {
    private static let firstLaunchKey = "loginItemOffered"

    static var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    static func set(_ on: Bool) {
        try? on ? SMAppService.mainApp.register() : SMAppService.mainApp.unregister()
    }

    static func enableOnFirstLaunch() {
        guard !UserDefaults.standard.bool(forKey: firstLaunchKey) else { return }
        UserDefaults.standard.set(true, forKey: firstLaunchKey)
        set(true)
    }
}
```

`macos/Sources/App/Onboarding.swift` :

```swift
import AppKit
import ApplicationServices

/// Au premier lancement, dire pourquoi sillon demande l'accessibilité, et
/// mener au bon réglage. Sans elle, le presse-papiers seul.
@MainActor
enum Onboarding {
    private static let doneKey = "accessibilityExplained"
    private static let remindedKey = "accessibilityReminded"

    static func runIfNeeded() {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: doneKey) else { return }
        UserDefaults.standard.set(true, forKey: doneKey)
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = "Lire ce que vous sélectionnez"
        alert.informativeText = """
        Pour lire le texte sélectionné dans n'importe quelle application, \
        sillon a besoin de l'autorisation Accessibilité. Sans elle, il lit \
        seulement ce que vous copiez.
        """
        alert.addButton(withTitle: "Ouvrir les réglages")
        alert.addButton(withTitle: "Plus tard")
        if alert.runModal() == .alertFirstButtonReturn {
            let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
        }
    }

    /// Le menu le rappelle une fois, discrètement.
    static var needsReminder: Bool {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: remindedKey) else { return false }
        return true
    }

    static func reminded() { UserDefaults.standard.set(true, forKey: remindedKey) }
}
```

Dans `AppDelegate.applicationDidFinishLaunching`, après la création du panneau
et hors mode test :

```swift
        guard !isUITesting else { return }
        LoginItem.enableOnFirstLaunch()
        Onboarding.runIfNeeded()
        let pasteboard = SystemPasteboard()
        let chain = CaptureChain(sources: [
            SelectionSource(),
            CopySource(pasteboard: pasteboard, keys: SystemKeystrokes()),
        ])
        KeyboardShortcuts.onKeyUp(for: .readSelection) { [unowned self] in
            Task { @MainActor in
                await ReadFlow(
                    prepare: { self.model.prepareVoice() },
                    capture: { await chain.capture() },
                    open: { self.model.open(text: $0, source: "sélection") },
                    openClipboard: { try self.model.openClipboard() },
                    say: { self.model.message = $0 },
                    show: { self.panel.show() }
                ).run()
            }
        }
        KeyboardShortcuts.onKeyUp(for: .showPanel) { [unowned self] in self.panel.show() }
```

(ajouter `import KeyboardShortcuts` en tête du fichier).

`macos/Sources/MenuContent.swift` (corps complet) :

```swift
import SwiftUI

struct MenuContent: View {
    private var model: ReaderModel? { AppDelegate.shared?.model }

    var body: some View {
        // Un clic dans le menu a déjà pris le focus à l'application : seule
        // la copie déjà faite est atteignable d'ici.
        Button("Lire le presse-papiers") {
            Task { @MainActor in
                AppDelegate.shared.model.prepareVoice()
                try? AppDelegate.shared.model.openClipboard()
                AppDelegate.shared.panel.show()
            }
        }
        Button("Afficher sillon") { AppDelegate.shared.panel.show() }
        Divider()
        Text(voiceLine)
        if Onboarding.needsReminder {
            Button("Autoriser la lecture de la sélection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Button("Mettre la voix au repos") { model?.restVoice() }
        Divider()
        SettingsLink { Text("Réglages…") }
            .keyboardShortcut(",")
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var voiceLine: String {
        switch model?.snapshot.voice {
        case .ready: "Voix prête"
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système (\(reason))"
        default: "Voix au repos"
        }
    }
}
```

`SettingsLink` exige une scène `Settings` : elle arrive à la Tâche 19 ; d'ici
là, remplacer temporairement `SettingsLink { … }` par rien (la ligne sera
ajoutée à la Tâche 19, étape 3). Le cas `.fallback(let reason)` suit le nom
généré (`.fallback(reason:)`) : ajuster au besoin.

- [ ] **Étape 3 : tests verts, puis essai réel**

Run : **`TEST`** — attendu : tout vert.

Essai manuel (ce comportement dépend d'une autorisation système que les tests
n'ont pas) : `./scripts/build-macos.sh && open macos/build/Release/Sillon.app`,
accorder l'accessibilité, sélectionner une phrase dans Safari, puis
`⌃⌥Espace` → la lecture démarre dans le panneau ; dans Chrome, idem ; copier
du texte dans TextEdit puis vérifier que `⌘V` colle toujours **ce** texte
après un `⌃⌥Espace` sur une sélection Chrome.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/App macos/Sources/AppDelegate.swift macos/Sources/MenuContent.swift macos/Tests/ReadFlowTests.swift
git commit -m "feat(macos): ⌃⌥Espace lit la sélection, l'autorisation expliquée, ouverture à la connexion

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 19 : les réglages

**Fichiers :**
- Créer : `macos/Sources/Settings/SettingsView.swift`
- Créer : `macos/Sources/Settings/KeepVoiceChoice.swift`
- Créer : `macos/Tests/KeepVoiceChoiceTests.swift`
- Créer : `macos/UITests/SettingsUITests.swift`
- Modifier : `macos/Sources/SillonApp.swift`, `macos/Sources/MenuContent.swift`

**Interfaces :**
- Consomme : `ReaderModel.{settings, updateSettings, voices, installVoice, useVoice}`,
  `KeyboardShortcuts.Recorder`, `LoginItem`, `ReadingFont`, `DisplayDefaults`.
- Produit : `enum KeepVoiceChoice: Hashable, CaseIterable { case end, minutes(Int64), always … }`
  avec `init(minutes: Int64)`, `var minutes: Int64`, `var label: String`,
  `static var allCases: [KeepVoiceChoice]` = dès la fin, 2, 5, 10, 30 min,
  toujours.

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/KeepVoiceChoiceTests.swift` :

```swift
import XCTest
@testable import Sillon

final class KeepVoiceChoiceTests: XCTestCase {
    func testRoundTrip() {
        for choice in KeepVoiceChoice.allCases {
            XCTAssertEqual(KeepVoiceChoice(minutes: choice.minutes), choice)
        }
    }

    func testSpecialValues() {
        XCTAssertEqual(KeepVoiceChoice(minutes: -1), .always)
        XCTAssertEqual(KeepVoiceChoice(minutes: 0), .end)
        XCTAssertEqual(KeepVoiceChoice(minutes: 10).label, "10 minutes")
    }

    func testUnlistedValueSnapsToNearest() {
        XCTAssertEqual(KeepVoiceChoice(minutes: 7), .minutes(5))
        XCTAssertEqual(KeepVoiceChoice(minutes: 300), .minutes(30))
    }
}
```

`macos/UITests/SettingsUITests.swift` :

```swift
import XCTest

final class SettingsUITests: XCTestCase {
    func testSettingsReachableAndKeyboardNavigable() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.popUpButtons["police-lecture"].exists)
        XCTAssertTrue(window.popUpButtons["garder-voix"].exists)
        window.typeKey("\t", modifierFlags: [])
        try app.performAccessibilityAudit()
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/KeepVoiceChoiceTests` — attendu :
échec de compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/Settings/KeepVoiceChoice.swift` :

```swift
/// Combien de temps la voix reste chargée après la dernière lecture.
enum KeepVoiceChoice: Hashable {
    case end
    case minutes(Int64)
    case always

    static let steps: [Int64] = [2, 5, 10, 30]
    static var allCases: [KeepVoiceChoice] { [.end] + steps.map { .minutes($0) } + [.always] }

    init(minutes: Int64) {
        switch minutes {
        case ..<0: self = .always
        case 0: self = .end
        default:
            let nearest = Self.steps.min { abs($0 - minutes) < abs($1 - minutes) } ?? 10
            self = .minutes(nearest)
        }
    }

    var minutes: Int64 {
        switch self {
        case .end: 0
        case .minutes(let m): m
        case .always: -1
        }
    }

    var label: String {
        switch self {
        case .end: "Relâchée dès l'arrêt"
        case .minutes(let m): "\(m) minutes"
        case .always: "Toujours chargée"
        }
    }
}
```

`macos/Sources/Settings/SettingsView.swift` :

```swift
import KeyboardShortcuts
import SwiftUI

struct SettingsView: View {
    let model: ReaderModel

    var body: some View {
        TabView {
            ReadingSettings(model: model).tabItem { Label("Lecture", systemImage: "text.alignleft") }
            ShortcutSettings().tabItem { Label("Raccourcis", systemImage: "keyboard") }
            VoiceSettings(model: model).tabItem { Label("Voix", systemImage: "waveform") }
            GeneralSettings().tabItem { Label("Général", systemImage: "gearshape") }
        }
        .frame(width: 520)
        .font(.system(size: 13))
    }
}

private struct ReadingSettings: View {
    let model: ReaderModel
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize
    @State private var settings: SettingsView_ = .init()

    var body: some View {
        Form {
            Picker("Police", selection: $fontName) {
                ForEach(ReadingFont.allCases) { Text($0.label).tag($0.rawValue) }
            }
            .accessibilityIdentifier("police-lecture")
            Slider(value: $size, in: 14...40, step: 1) { Text("Taille : \(Int(size)) pt") }
            Stepper("Interlignage : \(settings.lineSpacing)", value: $settings.lineSpacing, in: 0...4)
            Toggle("Règle de lecture", isOn: $settings.ruler)
            Slider(value: $settings.speed, in: 0.5...3, step: 0.1) {
                Text(String(format: "Vitesse : %.1f×", settings.speed))
            }
        }
        .formStyle(.grouped)
        .onAppear { settings = .init(model.settings()) }
        .onChange(of: settings) { _, new in
            var s = model.settings()
            s.lineSpacing = UInt32(new.lineSpacing)
            s.readingRuler = new.ruler
            s.speed = Float(new.speed)
            try? model.updateSettings(s)
        }
    }
}

/// Les réglages de lecture, en types que SwiftUI lie directement.
struct SettingsView_: Equatable {
    var lineSpacing = 1
    var ruler = true
    var speed = 1.0

    init() {}
    init(_ s: SettingsView) {
        lineSpacing = Int(s.lineSpacing)
        ruler = s.readingRuler
        speed = Double(s.speed)
    }
}

private struct ShortcutSettings: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
        }
        .formStyle(.grouped)
    }
}

private struct VoiceSettings: View {
    let model: ReaderModel
    @State private var voices: [VoiceModel] = []
    @State private var keep: KeepVoiceChoice = .minutes(10)
    @State private var error: String?

    var body: some View {
        Form {
            Section("Voix") {
                ForEach(voices, id: \.id) { voice in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(voice.name)
                            Text(ByteCountFormatter.string(fromByteCount: Int64(voice.bytes), countStyle: .file))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if voice.active {
                            Text("Active").foregroundStyle(.secondary)
                        } else if voice.installed {
                            Button("Utiliser") { act { try model.useVoice(voice.id) } }
                        } else if let d = model.snapshot.download, d.id == voice.id {
                            ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                                .frame(width: 120)
                        } else {
                            Button("Installer") { act { try model.installVoice(voice.id) } }
                        }
                    }
                }
            }
            Picker("Garder la voix chargée", selection: $keep) {
                ForEach(KeepVoiceChoice.allCases, id: \.self) { Text($0.label).tag($0) }
            }
            .accessibilityIdentifier("garder-voix")
            if let error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .onAppear {
            voices = model.voices()
            keep = KeepVoiceChoice(minutes: model.settings().keepVoiceMinutes)
        }
        .onChange(of: keep) { _, new in
            var s = model.settings()
            s.keepVoiceMinutes = new.minutes
            act { try model.updateSettings(s) }
        }
        .onChange(of: model.snapshot.download == nil) { _, _ in voices = model.voices() }
    }

    private func act(_ f: () throws -> Void) {
        do { try f(); error = nil; voices = model.voices() } catch { self.error = "\(error)" }
    }
}

private struct GeneralSettings: View {
    @State private var login = LoginItem.isEnabled

    var body: some View {
        Form {
            Toggle("Ouvrir sillon à la connexion", isOn: $login)
                .onChange(of: login) { _, on in LoginItem.set(on) }
        }
        .formStyle(.grouped)
    }
}
```

`macos/Sources/SillonApp.swift`, ajouter la scène après `MenuBarExtra` :

```swift
        Settings {
            if let model = AppDelegate.shared?.model {
                SettingsView(model: model)
            }
        }
```

et rétablir dans `MenuContent` la ligne `SettingsLink { Text("Réglages…") }`
avec `.keyboardShortcut(",")`. Dans `AppDelegate`, en mode test, si
`--show-settings` est passé :
`NSApp.activate(); NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)`.

La fenêtre de réglages SwiftUI porte le titre de l'onglet courant ; si le test
ne trouve pas `windows["Réglages"]`, poser
`.navigationTitle("Réglages")` sur le `TabView` ou viser `app.windows.firstMatch`.

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** — attendu : tout vert. Si `performAccessibilityAudit` signale
un problème, le corriger dans la vue concernée (libellé manquant, contraste) —
ne pas le filtrer.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/Settings macos/Sources/SillonApp.swift macos/Sources/MenuContent.swift macos/Sources/AppDelegate.swift macos/Tests/KeepVoiceChoiceTests.swift macos/UITests/SettingsUITests.swift
git commit -m "feat(macos): les réglages — lecture, raccourcis, voix, connexion

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 20 : accessibilité de bout en bout

**Fichiers :**
- Créer : `macos/Sources/App/Announcer.swift`
- Créer : `macos/Tests/AnnouncerTests.swift`
- Créer : `macos/UITests/AccessibilityAuditUITests.swift`
- Modifier : `macos/Sources/Panel/PanelController.swift` (annonces),
  `macos/Sources/Panel/PanelContent.swift` (thème suivi en direct)

**Interfaces :**
- Consomme : `Snapshot`, `DocView`, `SectionMarkers.breadcrumb`.
- Produit : `struct Announcer { mutating func announcement(for s: Snapshot, doc: DocView) -> String? }`
  — rend une phrase à annoncer quand l'état **change** (lecture, pause,
  nouvelle section), jamais pour un mot.

- [ ] **Étape 1 : tests qui échouent**

`macos/Tests/AnnouncerTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class AnnouncerTests: XCTestCase {
    private func setup() -> (SillonReader, DocView) {
        let r = SillonReader.silent()
        r.openText(markdown: "# A\n\nUn. Deux.\n\n# B\n\nTrois.", source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return (r, r.document())
    }

    func testPlayPauseAndSectionAreAnnouncedOnce() {
        let (r, doc) = setup()
        var a = Announcer()
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc), "état initial : rien")

        r.command(command: .togglePlay)
        XCTAssertTrue(waitUntil { r.snapshot().playing })
        XCTAssertEqual(a.announcement(for: r.snapshot(), doc: doc), "Lecture")
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc), "pas deux fois")

        r.command(command: .sections(delta: 1))
        XCTAssertTrue(waitUntil { r.snapshot().sentence == 2 })
        XCTAssertEqual(a.announcement(for: r.snapshot(), doc: doc), "Section : B")
    }

    func testWordChangesAreNeverAnnounced() {
        let (r, doc) = setup()
        var a = Announcer()
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.command(command: .words(delta: 1))
        XCTAssertTrue(waitUntil { r.snapshot().word != nil })
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc))
    }
}
```

`macos/UITests/AccessibilityAuditUITests.swift` :

```swift
import XCTest

final class AccessibilityAuditUITests: XCTestCase {
    func testPanelAndExpandedPassTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        let panel = app.otherElements["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        try app.performAccessibilityAudit()
        panel.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(app.otherElements["lecteur-agrandi"].waitForExistence(timeout: 2))
        try app.performAccessibilityAudit()
    }
}
```

Run : **`TEST`** `-only-testing:SillonTests/AnnouncerTests` — attendu : échec
de compilation.

- [ ] **Étape 2 : implémenter**

`macos/Sources/App/Announcer.swift` :

```swift
/// Ce que VoiceOver doit dire : les changements d'état, jamais les mots —
/// la voix de sillon et celle de VoiceOver ne doivent pas se chevaucher.
struct Announcer {
    private var playing: Bool?
    private var section: UInt32??

    mutating func announcement(for s: Snapshot, doc: DocView) -> String? {
        defer { playing = s.playing; section = .some(s.section) }
        guard let wasPlaying = playing, let wasSection = section else { return nil }
        if s.playing != wasPlaying { return s.playing ? "Lecture" : "Pause" }
        if s.section != wasSection, let id = s.section,
           let title = doc.sections.first(where: { $0.id == id })?.title {
            return "Section : \(title)"
        }
        return nil
    }
}
```

Dans `PanelController`, ajouter `@ObservationIgnored private var announcer = Announcer()`
et une méthode appelée par `ReaderModel` à chaque rafraîchissement — le plus
simple : dans `PanelContent.body`, `.onChange(of: controller.model.snapshot) { _, s in controller.announce(s) }` avec :

```swift
    func announce(_ s: Snapshot) {
        guard NSWorkspace.shared.isVoiceOverEnabled,
              let text = announcer.announcement(for: s, doc: model.document) else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }
```

Thème suivi en direct : dans `PanelContent`, remplacer la lecture ponctuelle de
`accessibilityDisplayShouldIncreaseContrast` par un état mis à jour sur
`NSWorkspace.accessibilityDisplayOptionsDidChangeNotification` :

```swift
    @State private var increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
    // …
        .onReceive(NSWorkspace.shared.notificationCenter.publisher(
            for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
            increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        }
```

et utiliser `increaseContrast` dans le calcul de `theme`.

- [ ] **Étape 3 : tests verts**

Run : **`TEST`** — attendu : tout vert. Corriger dans les vues tout ce que
l'audit signale (libellés, cibles trop petites, contraste) ; ne jamais
l'exclure du test.

Vérification manuelle VoiceOver (`⌘F5`) : ouvrir le panneau, `Espace` →
VoiceOver dit « Lecture » une fois puis se tait pendant que sillon lit ;
`n` → « Section : … ». Activer « Accès complet au clavier » (Réglages
système → Clavier) et parcourir les réglages à la tabulation : chaque
contrôle prend un focus visible.

- [ ] **Étape 4 : commit**

```bash
git add macos/Sources/App/Announcer.swift macos/Sources/Panel macos/Tests/AnnouncerTests.swift macos/UITests/AccessibilityAuditUITests.swift
git commit -m "feat(macos): VoiceOver annonce l'état sans couvrir la lecture ; audit d'accessibilité

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Tâche 21 : mesurer les critères d'acceptation

**Fichiers :**
- Créer : `macos/Sources/App/Measure.swift`
- Créer : `scripts/measure-macos.sh`
- Modifier : `macos/Sources/AppDelegate.swift` (argument `--measure`)
- Modifier : `docs/superpowers/specs/2026-10-05-interface-macos-design.md`
  (résultats)

**Interfaces :**
- Consomme : `SillonReader`, `ReaderCommand`.
- Produit : mode `--measure` qui écrit sur la sortie standard une ligne
  `MESURE <nom> <valeur> <unité>` par mesure, puis quitte.

- [ ] **Étape 1 : le mode mesure**

`macos/Sources/App/Measure.swift` :

```swift
import Foundation

/// Mesures des seuils de la spec, sur le vrai moteur. Lancé par
/// `scripts/measure-macos.sh` ; écrit `MESURE nom valeur unité`.
@MainActor
enum Measure {
    static let text = """
    Les monades permettent d'enchaîner des calculs qui portent un contexte. \
    On les retrouve partout : options, listes, promesses. \
    Une seconde phrase pour avoir de quoi sauter. Et une troisième.
    """

    static func run(reader: SillonReader) async {
        // À froid : voix au repos, puis le geste du raccourci.
        for i in 1...5 {
            reader.restVoice()
            try? await Task.sleep(for: .seconds(1))
            let t = ContinuousClock.now
            reader.prepareVoice()
            reader.openText(markdown: text, source: "mesure-\(i)")
            reader.command(command: .togglePlay)
            await until { reader.snapshot().audible }
            emit("froid_premier_son", t.duration(to: .now))
        }
        // Voix chargée.
        for i in 1...5 {
            reader.command(command: .pause)
            let t = ContinuousClock.now
            reader.openText(markdown: text, source: "chaud-\(i)")
            reader.command(command: .togglePlay)
            await until { reader.snapshot().audible }
            emit("chaud_premier_son", t.duration(to: .now))
        }
        // Sauts : phrase, puis mot.
        for _ in 1...5 {
            await until { reader.snapshot().audible }
            let target = reader.snapshot().sentence + 1
            let t = ContinuousClock.now
            reader.command(command: .sentences(delta: 1))
            await until { let s = reader.snapshot(); return s.sentence == target && s.audible }
            emit("saut_phrase", t.duration(to: .now))
            reader.command(command: .sentences(delta: -1))
        }
        for _ in 1...5 {
            await until { reader.snapshot().audible }
            let t = ContinuousClock.now
            reader.command(command: .words(delta: 3))
            try? await Task.sleep(for: .milliseconds(20))
            await until { reader.snapshot().audible }
            emit("saut_mot", t.duration(to: .now))
        }
        // Mémoire au repos, sur dix cycles.
        for cycle in 1...10 {
            reader.prepareVoice()
            reader.openText(markdown: text, source: "cycle-\(cycle)")
            reader.command(command: .togglePlay)
            await until { reader.snapshot().audible }
            reader.restVoice()
            try? await Task.sleep(for: .seconds(1))
            print("MESURE memoire_repos_\(cycle) \(residentMB()) Mo")
        }
    }

    private static func until(_ condition: () -> Bool) async {
        let deadline = ContinuousClock.now + .seconds(10)
        while !condition(), ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(2))
        }
    }

    private static func emit(_ name: String, _ d: Duration) {
        let ms = Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15
        print(String(format: "MESURE %@ %.0f ms", name, ms))
    }

    private static func residentMB() -> Int {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? Int(info.phys_footprint / 1_048_576) : -1
    }
}
```

Dans `AppDelegate.applicationDidFinishLaunching`, juste après la création du
lecteur :

```swift
        if ProcessInfo.processInfo.arguments.contains("--measure") {
            Task { @MainActor in
                await Measure.run(reader: reader)
                NSApp.terminate(nil)
            }
            return
        }
```

- [ ] **Étape 2 : le script**

`scripts/measure-macos.sh` (exécutable) :

```bash
#!/usr/bin/env bash
# Mesure les seuils de la spec sur Sillon.app construite en Release.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=macos/build/Release/Sillon.app
[ -d "$APP" ] || ./scripts/build-macos.sh
"$APP/Contents/MacOS/Sillon" --measure 2>/dev/null | grep '^MESURE' | tee /tmp/sillon-mesures.txt
echo "--- médianes ---"
for name in froid_premier_son chaud_premier_son saut_phrase saut_mot; do
    grep " $name " /tmp/sillon-mesures.txt | awk '{print $3}' | sort -n \
        | awk -v n="$name" '{a[NR]=$1} END {print n, a[int((NR+1)/2)], "ms"}'
done
grep memoire_repos /tmp/sillon-mesures.txt | awk '{print $2, $3, $4}'
```

- [ ] **Étape 3 : mesurer**

Run : `./scripts/measure-macos.sh`
Attendu, d'après la spec : `froid_premier_son` < 900 ms, `chaud_premier_son`
< 500 ms, `saut_phrase` et `saut_mot` < 300 ms, `memoire_repos_*` < 120 Mo et
sans montée d'un cycle à l'autre.

Images perdues : `xcrun xctrace record --template 'Animation Hitches' --launch -- macos/build/Release/Sillon.app/Contents/MacOS/Sillon --ui-testing --show-panel --time-limit 20s`,
puis pendant l'enregistrement lancer la lecture (`Espace`) et sauter (`⌥→`
répété). Ouvrir la trace : aucune saccade (« hitch ») pendant la lecture.

- [ ] **Étape 4 : consigner, et ne rien maquiller**

Écrire les médianes et la série mémoire dans la spec, section « Critères
d'acceptation mesurés », colonne « Mesuré (date) ». Un seuil manqué **reste
manqué** dans le tableau : ne pas changer la mesure, ne pas relâcher le seuil
sans l'accord de l'utilisateur. Pour chaque seuil manqué, écrire en dessous ce
que la mesure montre (où passe le temps) et le signaler dans le compte rendu de
la tâche.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/App/Measure.swift macos/Sources/AppDelegate.swift scripts/measure-macos.sh docs/superpowers/specs/2026-10-05-interface-macos-design.md
git commit -m "bench(macos): les seuils de la spec mesurés sur l'app

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Couverture de la spec

| Exigence de la spec | Tâche |
|---|---|
| Anomalie mémoire à mesurer d'abord, orig contre f16 | 1 |
| Saut mot par mot, reprise en milieu de phrase | 2, 3 |
| Voix gardée entre deux textes ; synthèse et sortie réutilisées | 4, 5 |
| Déchargement après délai réglable, rechargement à la demande | 5, 19 |
| Instantané lu à chaque image, document par révision | 6, 7, 11 |
| Voix : catalogue, installation, choix ; repli système | 8, 19 |
| Liaison UniFFI, bibliothèque statique, shim en `@rpath` | 9, 10 |
| App sans Dock, menu dans la barre, état de la voix | 10, 18 |
| Clavier complet (terminal + `⌥←→`, `⏎`, `⌘C`, `⌘,`) | 12, 15 |
| Style Papier, nuit, contraste élevé ; trois polices | 13, 20 |
| Mot qui glisse < 80 ms, ligne tenue, déjà-lu atténué, réduire les animations | 14 |
| Panneau type Spotlight, hauteur bornée, Échap qui recule et met en pause | 15 |
| Lecteur agrandi : sommaire, signets, recherche, repères | 16 |
| Capture : sélection AX → ⌘C simulé + restauration → presse-papiers (OCR) → message | 17, 18 |
| Raccourcis `⌃⌥Espace` / `⌃⌥S` modifiables | 18, 19 |
| Autorisation expliquée, rappel unique ; ouverture à la connexion | 18, 19 |
| VoiceOver : état annoncé, mots jamais ; audit automatique | 20 |
| Critères d'acceptation mesurés | 21 |
| Construction par script, signature ad hoc | 9, 10 |
