# Réglages complets dans l'app macOS — plan d'implémentation

> **Pour les agents :** sous-skill requis : superpowers:subagent-driven-development
> (recommandé) ou superpowers:executing-plans, tâche par tâche. Les étapes
> utilisent des cases à cocher (`- [ ]`).

**Objectif :** la fenêtre de réglages de `Sillon.app` expose tout ce que le
moteur sait régler — six onglets (Voix, Lecture, Modèles, Raccourcis, Avancé,
Général), un écrou ⚙ dans le panneau qui ouvre l'onglet Voix — et chaque
changement s'applique tout de suite avec l'effet de sa nature.

**Architecture :** côté Rust, `Reader` gagne une vue complète des réglages
(`SettingsView`), des modifications par patch (`SettingsPatch`, seuls les
champs présents changent) et déclenche l'effet le plus fort de ce qui a
changé (`SettingsEffect` : affichage, volume, resynthèse, remise en forme,
rechargement). Il expose aussi les capacités du modèle actif, un aperçu qui
marche sans texte ouvert, et les actions sur les modèles et les données. Le
crate `sillon-ffi` les passe à Swift. Côté Swift, un magasin unique
(`SettingsStore`) garde réglages, capacités et l'état « afficher les
ignorés », calcule le patch par différence et l'écrit après une courte pause
(`Committer`) ; un fichier par onglet.

**Pile :** Rust 1.80+, UniFFI 0.32 ; Swift 6.x en mode de langage 5, SwiftUI
+ AppKit, macOS 14+, XcodeGen, KeyboardShortcuts 2.x, XCTest / XCUITest.

**Spec :** `docs/superpowers/specs/2026-10-06-reglages-complets-design.md`
(elle prolonge `2026-10-05-interface-macos-design.md`). La lire avant toute
tâche.

## Contraintes globales

- macOS 14 minimum, Apple Silicon (`arm64`) seul.
- Swift en **mode de langage 5** (`SWIFT_VERSION: "5.0"` dans `macos/project.yml`).
- Texte d'interface en police système **13 pt, fixe** ; seul le texte lu change de taille.
- Textes d'interface, commentaires et noms en **français**, comme le code existant.
- Un appel qui attend le fil du lecteur (`ReaderHandle::ask`, toute méthode
  FFI qui passe par lui) **ne part jamais du fil principal** : motif
  `Task.detached { reader.x() }.value` de `ReaderModel`.
- `config.toml` n'est écrit qu'**après une courte pause** (`Committer`, 150 ms)
  et **par patch** : seuls les champs touchés partent ; une valeur changée
  ailleurs entre la lecture et l'écriture n'est pas écrasée.
- Bornes = **`Config::normalised`**, partout : interlignage **0 à 3**, pas
  d'avance rapide **1 à 50**, vitesse 0,5 à 3, volume 0 à 2, fils 1 à 64,
  sessions 0 à 16, phrases d'avance 2 à 64.
- Effets (spec, « Comportement ») : voix, langue, vitesse, échantillonnage,
  options libres → resynthèse depuis la phrase en cours ; blocs de code →
  remise en forme à la même position ; moteur, calcul, fils, sessions,
  phrases d'avance, modèle actif → rechargement de la voix, lecture reprise
  seule à la même phrase ; volume → immédiat ; police, taille, interlignage,
  règle → affichage seulement.
- L'écrou ⚙ : bout de la barre de lecture, 13 pt, libellé VoiceOver
  **« Réglages de la voix »**, ouvre l'onglet **Voix** ; `⌘,` ouvre le
  **dernier onglet utilisé**.
- Ne jamais indexer `vendor/audio.cpp`, `macos/Generated`, `macos/Libraries`,
  `macos/*.xcodeproj`, `macos/build` : `git add` de chemins explicites
  uniquement.
- Après tout changement de `ffi/src/lib.rs` : `./scripts/build-ffi.sh`
  (régénère `macos/Generated` et `macos/Libraries`, jamais commités).
- Tests : `cargo test` (crate `sillon`), `cargo test -p sillon-ffi`, et Swift :
  `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`.
- Tests d'interface **seulement avec le feu vert de l'utilisateur** : ils sont
  écrits pour compiler (`build-for-testing`) ; leur exécution est une étape
  finale à part, soumise à son accord.
- Les tests ne touchent jamais aux fichiers de l'utilisateur : côté Rust,
  `apply_patch` (sans disque) ou un `Reader` mis hors disque
  (`set_persistent(false)`) ; côté Swift, des faux moteurs
  (`FakeSettingsBackend`, `FakeModelsBackend`).
- Messages de commit en français, style du dépôt (`feat(reader): …`,
  `feat(macos): …`), terminés par
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Points d'attention

Cinq situations que la spec implique sans les nommer, les plus susceptibles
de mordre un utilisateur ; chacune a reçu son test dans la tâche indiquée.

1. **« Écouter » sans texte ouvert** : rien ne tourne, le fil du lecteur dort
   (aucune cadence) ; l'aperçu synthétisé n'est alors jamais sondé, donc
   jamais joué. Attendu : il sonne dès qu'il est prêt. → Tâche 3, test
   `a_preview_keeps_the_reader_thread_awake` (cadence vive pendant un aperçu).
2. **Valeurs écrites à la main, hors liste** (`code_blocks = "lire"`,
   `language = "ja"` sur MagpieTTS, `compute = "cuda"`,
   `keep_voice_minutes = 7`) : ouvrir un onglet ou toucher un autre réglage
   ne doit ni les remplacer ni les réécrire. → Tâche 1,
   `hand_written_values_survive_other_changes` ; Tâche 6,
   `testOpeningTheTabRewritesNothing` et `Choices` ; Tâche 9 (7 minutes).
3. **Blocs de code changés en pleine lecture** : « sauter » retire des
   phrases, « lire » en ajoute ; garder le *numéro* de phrase ferait reprendre
   ailleurs. Attendu : la même phrase de texte. → Tâche 1,
   `changing_code_blocks_keeps_the_sentence_being_read`.
4. **Supprimer un signet du document ouvert** : la session garde son magasin
   en mémoire et le réécrit au prochain signet posé, ce qui ferait revenir le
   signet supprimé. → Tâche 4,
   `removing_a_bookmark_also_forgets_it_in_the_open_session`.
5. **Écouter juste après avoir choisi une voix** : le choix attend encore sa
   pause de 150 ms ; l'aperçu partirait avec l'ancienne voix. Attendu : la
   voix choisie. → Tâche 6, `testListeningSendsThePendingChangeFirst`
   (`Committer.flush()` avant l'aperçu).

## Structure des fichiers

Rust :

| Fichier | Rôle |
|---|---|
| `src/config.rs` | `normalised()` borne aussi l'échantillonnage |
| `src/reader/settings.rs` | `SettingsView` complet, `SettingsPatch`, `SettingsEffect`, `apply_patch` / `update_settings`, rechargement et remise en forme, catalogue enrichi, capacités |
| `src/reader/preview.rs` (nouveau) | aperçu sans texte (`LonePreview`), `PreviewView` |
| `src/reader/data.rs` (nouveau) | suppression de modèle, annulation de téléchargement, cache, signets |
| `src/reader/mod.rs` | champs `persist`, `lone_preview`, `preview_error` ; cadence |
| `src/reader/handle.rs` | cadence vive pendant un aperçu |
| `src/reader/view.rs` | `Snapshot.preview` |
| `src/models.rs` | `languages_for_family` |
| `src/tts/preview.rs` | `refusal()` partagé |
| `src/app/mod.rs` | `rescale_pace`, `request_preview`, `preview_error` |
| `src/app/config.rs` | bornes du terminal alignées, langues du modèle |
| `src/source/cache.rs` | `clear()` |
| `ffi/src/lib.rs` | tous les types et actions vers Swift |

Swift (`macos/Sources/Settings/` sauf mention) :

| Fichier | Rôle |
|---|---|
| `SettingsDefaults.swift` | `SettingsView.defaults`, `SettingsPatch.empty` |
| `Committer.swift` | valeur appliquée rendue, `flush()` |
| `SettingsStore.swift` | magasin des réglages, protocole `SettingsBackend` |
| `SettingsDiff.swift` | patch par différence |
| `SettingKey.swift` | réglages ignorables, filtre par capacités |
| `Choices.swift` | menus qui gardent la valeur hors liste |
| `SettingControls.swift` | `IgnorableNote`, `SettingFormat`, `LanguageName`, contrôles optionnels |
| `SettingsTab.swift` | onglets, `SettingsNavigation` (dernier onglet) |
| `SettingsOpener.swift` | `open(tab:)` |
| `ReglagesView.swift` | la fenêtre à six onglets |
| `VoiceTab.swift`, `ReadingTab.swift`, `ModelsTab.swift`, `ShortcutsTab.swift`, `AdvancedTab.swift`, `GeneralTab.swift` | un fichier par onglet |
| `ModelsStore.swift` | catalogue, installation, suppression confirmée |
| `KeepVoiceChoice.swift` | valeur hors liste gardée |
| `SettingsView.swift` | **supprimé** à la Tâche 10 (anciens onglets) |
| `macos/Sources/Model/ReaderModel.swift` | appels hors fil principal, `settingsRevision` |
| `macos/Sources/Panel/ReadingDisplay.swift` (nouveau) | ce que le panneau lit des réglages |
| `macos/Sources/Panel/{PlayerBar,PanelController,PanelContent}.swift`, `macos/Sources/Input/KeyMap.swift` | écrou, touche `v`, ligne « Voix en chargement… », pas d'avance rapide réel |

---

# Partie A — Le moteur

### Tâche 1 : tous les réglages, modifiés par patch, avec l'effet de leur nature

Le lecteur montre aujourd'hui quatre réglages et les remplace d'un bloc. Il
doit les montrer tous, n'appliquer que ceux qu'un patch touche, et déclencher
l'effet le plus fort de ce qui a réellement changé. Les bornes du terminal
sont alignées sur `Config::normalised`.

**Fichiers :**
- Modifier : `src/reader/settings.rs` (réécrit en entier, ci-dessous)
- Modifier : `src/reader/mod.rs:35-68` (champ `persist`, `set_persistent`)
- Modifier : `src/config.rs:215-232` (bornes de l'échantillonnage) et ses tests
- Modifier : `src/app/mod.rs:654` (ajout de `rescale_pace` après `toggle_mute`)
- Modifier : `src/app/config.rs:513-516` et `:532-536` (bornes), test `line_spacing_stays_between_none_and_two`
- Modifier : `ffi/src/lib.rs` (adaptation minimale : l'app garde ses quatre réglages jusqu'à la Tâche 5)

**Interfaces :**
- Consomme : `Config::normalised()`, `App::apply_effect(Effect::Resynthesize)`,
  `Reader::{rest, wake, open, ensure_engine, sync_config}` (privés, visibles
  depuis le sous-module `settings`).
- Produit :
  - `pub struct SettingsView { voice: String, language: String, speed: f32, pitch: f32, energy: f32, emotion: String, instruction: String, volume: f32, engine: String, model_path: String, family: String, compute: String, threads: u32, sessions: u32, prefetch: u32, temperature: Option<f32>, top_p: Option<f32>, top_k: Option<u32>, guidance_scale: Option<f32>, repetition_penalty: Option<f32>, inference_steps: Option<u32>, seed: Option<u64>, code_blocks: String, autoplay: bool, seek_step: u32, line_width: u32, line_spacing: u32, reading_ruler: bool, keep_voice_minutes: i64, extra: BTreeMap<String, String> }` avec `SettingsView::of(&Config) -> SettingsView`
  - `pub struct SettingsPatch` : mêmes noms, chaque champ `Option<T>` (échantillonnage `Option<Option<T>>`, `Some(None)` = valeur du modèle), plus `extra_set: Vec<(String, String)>` et `extra_remove: Vec<String>` ; `SettingsPatch::write_into(&self, &mut Config)` ; `#[derive(Default)]`
  - `pub enum SettingsEffect { Nothing, Display, Volume, Resynthesize, Reparse, Reload }` (ordonné), `SettingsEffect::between(&Config, &Config) -> SettingsEffect`
  - `Reader::apply_patch(&mut self, &SettingsPatch, Instant) -> SettingsView` (sans disque)
  - `Reader::update_settings(&mut self, &SettingsPatch, Instant) -> anyhow::Result<SettingsView>` (écrit si `persist`)
  - `Reader::use_voice(&mut self, id: &str, now: Instant) -> Result<()>` (recharge en place)
  - `Reader::set_persistent(&mut self, bool)`
  - `App::rescale_pace(&mut self, from: f32, to: f32)`
  - Disparaissent : `Reader::apply_settings`, l'ancien `update_settings(SettingsView)`.

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/config.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn sampling_values_are_brought_back_within_the_screen_bounds() {
        let mut config = Config::default();
        config.sampling.temperature = Some(f32::NAN);
        config.sampling.top_p = Some(4.0);
        config.sampling.top_k = Some(900);
        config.sampling.guidance_scale = Some(-3.0);
        config.sampling.repetition_penalty = Some(9.0);
        config.sampling.inference_steps = Some(0);
        let c = config.normalised();
        assert_eq!(c.sampling.temperature, None, "NaN rend la main au modèle");
        assert_eq!(c.sampling.top_p, Some(1.0));
        assert_eq!(c.sampling.top_k, Some(200));
        assert_eq!(c.sampling.guidance_scale, Some(0.0));
        assert_eq!(c.sampling.repetition_penalty, Some(2.0));
        assert_eq!(c.sampling.inference_steps, Some(1));
    }
```

Dans `src/app/config.rs`, remplacer le test `line_spacing_stays_between_none_and_two` par :

```rust
    #[test]
    fn line_spacing_stays_between_none_and_three() {
        // Les bornes de `Config::normalised`, partout : trois au plus.
        let mut config = Config::default();
        assert_eq!(config.reading.line_spacing, 1);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.line_spacing, 3);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, -1);
        }
        assert_eq!(config.reading.line_spacing, 0);
        assert_eq!(Field::LineSpacing.value(&config, 1.0), "aucun");
    }

    #[test]
    fn the_seek_step_stops_at_fifty() {
        let mut config = Config::default();
        config.reading.seek_step = 48;
        for _ in 0..5 {
            Field::SeekStep.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.seek_step, 50);
    }
```

Dans `src/reader/settings.rs`, remplacer tout le module `tests` par celui de
l'étape 3 (il est donné avec le fichier entier) ; à ce stade, il ne compile
pas.

- [ ] **Étape 2 : vérifier l'échec**

Run: `cargo test --lib -- config:: app::config:: reader::settings::`
Expected: FAIL — erreurs de compilation (`no method named apply_patch`,
`cannot find type SettingsPatch`) ; une fois `settings.rs` mis de côté, les
deux tests de bornes échouent (`left: 2, right: 3`, `left: 52, right: 50`,
`left: Some(NaN)`).

- [ ] **Étape 3 : écrire l'implémentation**

`src/config.rs`, dans `normalised()`, juste avant
`self.reading.seek_step = …`, ajouter :

```rust
        // L'échantillonnage suit les bornes de l'écran ; un nombre sans
        // valeur rend la main au modèle plutôt que d'atteindre le moteur.
        let bounded = |value: Option<f32>, low: f32, high: f32| {
            value.filter(|v| v.is_finite()).map(|v| v.clamp(low, high))
        };
        self.sampling.temperature = bounded(self.sampling.temperature, 0.0, 2.0);
        self.sampling.top_p = bounded(self.sampling.top_p, 0.0, 1.0);
        self.sampling.guidance_scale = bounded(self.sampling.guidance_scale, 0.0, 20.0);
        self.sampling.repetition_penalty = bounded(self.sampling.repetition_penalty, 0.5, 2.0);
        self.sampling.top_k = self.sampling.top_k.map(|v| v.clamp(1, 200));
        self.sampling.inference_steps = self.sampling.inference_steps.map(|v| v.clamp(1, 200));
```

`src/app/config.rs` : dans `Field::SeekStep`, remplacer `.clamp(1, 100)` par
`.clamp(1, 50)` ; dans `Field::LineSpacing`, remplacer `.min(2)` par `.min(3)`.

`src/app/mod.rs`, juste après `pub fn toggle_mute(&mut self) { … }` :

```rust
    /// Transpose la cadence mesurée à un débit posé ailleurs (réglages de
    /// l'app) : les temps affichés suivent sans attendre une nouvelle mesure.
    pub fn rescale_pace(&mut self, from: f32, to: f32) {
        self.pace.rescale(from, to);
    }
```

`src/reader/mod.rs` : dans `pub struct Reader`, après `resting_doc`, ajouter

```rust
    /// Faux pour le lecteur muet de l'app de test : rien n'est écrit ni
    /// effacé sur le disque de l'utilisateur.
    persist: bool,
```

dans `Reader::new`, après `resting_doc: None,` ajouter `persist: true,`, et
après `pub fn config(&self)` ajouter :

```rust
    /// Hors disque : les réglages s'appliquent sans s'écrire, et rien ne
    /// s'efface (modèles, cache, signets).
    pub fn set_persistent(&mut self, on: bool) {
        self.persist = on;
    }
```

`src/reader/settings.rs`, en entier :

```rust
//! Réglages et voix, tels que l'interface les montre et les change.
//!
//! L'interface lit tous les réglages d'un coup (`SettingsView`) et ne
//! renvoie que ceux qu'elle touche (`SettingsPatch`) : rien n'écrase une
//! valeur changée ailleurs — au clavier, au terminal — entre la lecture et
//! l'écriture. Chaque modification déclenche l'effet le plus fort de ce qui
//! a réellement changé, comme l'écran du terminal (`app::config::Effect`).

use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::time::Instant;

use crate::app::config::Effect;
use crate::config::Config;
use crate::doc::{BlockKind, Document};
use crate::models::{self, Download, Progress};

use super::Reader;

/// Tous les réglages du moteur, rangés comme dans `config.toml`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    // [voice]
    pub voice: String,
    pub language: String,
    pub speed: f32,
    pub pitch: f32,
    pub energy: f32,
    pub emotion: String,
    pub instruction: String,
    pub volume: f32,
    // [engine]
    pub engine: String,
    pub model_path: String,
    pub family: String,
    pub compute: String,
    pub threads: u32,
    pub sessions: u32,
    pub prefetch: u32,
    // [sampling] — `None` : la valeur du modèle ; pour la graine, un tirage.
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
    // [reading]
    pub code_blocks: String,
    pub autoplay: bool,
    pub seek_step: u32,
    pub line_width: u32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    // [app]
    pub keep_voice_minutes: i64,
    // [extra]
    pub extra: BTreeMap<String, String>,
}

impl SettingsView {
    pub fn of(c: &Config) -> Self {
        Self {
            voice: c.voice.voice.clone(),
            language: c.voice.language.clone(),
            speed: c.voice.speed,
            pitch: c.voice.pitch,
            energy: c.voice.energy,
            emotion: c.voice.emotion.clone(),
            instruction: c.voice.instruction.clone(),
            volume: c.voice.volume,
            engine: c.engine.engine.clone(),
            model_path: c.engine.model_path.clone(),
            family: c.engine.family.clone(),
            compute: c.engine.compute.clone(),
            threads: c.engine.threads,
            sessions: c.engine.sessions,
            prefetch: c.engine.prefetch,
            temperature: c.sampling.temperature,
            top_p: c.sampling.top_p,
            top_k: c.sampling.top_k,
            guidance_scale: c.sampling.guidance_scale,
            repetition_penalty: c.sampling.repetition_penalty,
            inference_steps: c.sampling.inference_steps,
            seed: c.sampling.seed,
            code_blocks: c.reading.code_blocks.clone(),
            autoplay: c.reading.autoplay,
            seek_step: c.reading.seek_step,
            line_width: c.reading.line_width,
            line_spacing: c.reading.line_spacing,
            reading_ruler: c.reading.reading_ruler,
            keep_voice_minutes: c.app.keep_voice_minutes,
            extra: c.extra.clone(),
        }
    }
}

/// Une modification : un champ par réglage, `None` quand il n'est pas touché.
///
/// L'échantillonnage a deux niveaux : `Some(None)` rend la main au modèle
/// (à un tirage au hasard pour la graine). Le chemin et la famille du modèle
/// n'y sont pas : ils changent avec `use_voice`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsPatch {
    pub voice: Option<String>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub energy: Option<f32>,
    pub emotion: Option<String>,
    pub instruction: Option<String>,
    pub volume: Option<f32>,
    pub engine: Option<String>,
    pub compute: Option<String>,
    pub threads: Option<u32>,
    pub sessions: Option<u32>,
    pub prefetch: Option<u32>,
    pub temperature: Option<Option<f32>>,
    pub top_p: Option<Option<f32>>,
    pub top_k: Option<Option<u32>>,
    pub guidance_scale: Option<Option<f32>>,
    pub repetition_penalty: Option<Option<f32>>,
    pub inference_steps: Option<Option<u32>>,
    pub seed: Option<Option<u64>>,
    pub code_blocks: Option<String>,
    pub autoplay: Option<bool>,
    pub seek_step: Option<u32>,
    pub line_width: Option<u32>,
    pub line_spacing: Option<u32>,
    pub reading_ruler: Option<bool>,
    pub keep_voice_minutes: Option<i64>,
    /// Options libres posées ou remplacées.
    pub extra_set: Vec<(String, String)>,
    /// Options libres retirées.
    pub extra_remove: Vec<String>,
}

fn set<T: Clone>(slot: &mut T, value: &Option<T>) {
    if let Some(v) = value {
        *slot = v.clone();
    }
}

impl SettingsPatch {
    /// Reporte sur `c` les seuls champs présents.
    pub fn write_into(&self, c: &mut Config) {
        set(&mut c.voice.voice, &self.voice);
        set(&mut c.voice.language, &self.language);
        set(&mut c.voice.speed, &self.speed);
        set(&mut c.voice.pitch, &self.pitch);
        set(&mut c.voice.energy, &self.energy);
        set(&mut c.voice.emotion, &self.emotion);
        set(&mut c.voice.instruction, &self.instruction);
        set(&mut c.voice.volume, &self.volume);
        set(&mut c.engine.engine, &self.engine);
        set(&mut c.engine.compute, &self.compute);
        set(&mut c.engine.threads, &self.threads);
        set(&mut c.engine.sessions, &self.sessions);
        set(&mut c.engine.prefetch, &self.prefetch);
        set(&mut c.sampling.temperature, &self.temperature);
        set(&mut c.sampling.top_p, &self.top_p);
        set(&mut c.sampling.top_k, &self.top_k);
        set(&mut c.sampling.guidance_scale, &self.guidance_scale);
        set(&mut c.sampling.repetition_penalty, &self.repetition_penalty);
        set(&mut c.sampling.inference_steps, &self.inference_steps);
        set(&mut c.sampling.seed, &self.seed);
        set(&mut c.reading.code_blocks, &self.code_blocks);
        set(&mut c.reading.autoplay, &self.autoplay);
        set(&mut c.reading.seek_step, &self.seek_step);
        set(&mut c.reading.line_width, &self.line_width);
        set(&mut c.reading.line_spacing, &self.line_spacing);
        set(&mut c.reading.reading_ruler, &self.reading_ruler);
        set(&mut c.app.keep_voice_minutes, &self.keep_voice_minutes);
        for (key, value) in &self.extra_set {
            c.extra.insert(key.clone(), value.clone());
        }
        for key in &self.extra_remove {
            c.extra.remove(key);
        }
    }
}

/// Ce qu'un changement de réglage demande, du plus léger au plus lourd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEffect {
    /// Rien n'a changé.
    Nothing,
    /// Affichage, ou valeur lue au prochain usage (pas d'avance, délai…).
    Display,
    /// Volume de sortie : immédiat, sans resynthèse.
    Volume,
    /// Paramètres de synthèse : resynthèse depuis la phrase en cours.
    Resynthesize,
    /// Découpage du document : remise en forme, à la même phrase.
    Reparse,
    /// Moteur : rechargement de la voix, lecture reprise à la même phrase.
    Reload,
}

impl SettingsEffect {
    /// L'effet le plus fort de ce qui change de `before` à `after`.
    ///
    /// Mesuré sur la configuration, pas sur les champs présents : renvoyer
    /// une valeur déjà en place ne relance rien.
    pub fn between(before: &Config, after: &Config) -> Self {
        if before.engine != after.engine {
            return Self::Reload;
        }
        if before.reading.code_blocks != after.reading.code_blocks {
            return Self::Reparse;
        }
        let mut voice = before.voice.clone();
        voice.volume = after.voice.volume;
        if voice != after.voice || before.sampling != after.sampling || before.extra != after.extra {
            return Self::Resynthesize;
        }
        if before.voice.volume != after.voice.volume {
            return Self::Volume;
        }
        if before.reading != after.reading || before.app != after.app {
            return Self::Display;
        }
        Self::Nothing
    }
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
        SettingsView::of(self.current_config())
    }

    /// Applique un patch sans écrire : seuls les champs présents changent,
    /// ramenés dans les bornes, puis l'effet le plus fort de ce qui a changé
    /// est déclenché. Rend les réglages tels qu'appliqués.
    pub fn apply_patch(&mut self, patch: &SettingsPatch, now: Instant) -> SettingsView {
        self.sync_config();
        let before = self.config.clone();
        let mut next = before.clone();
        patch.write_into(&mut next);
        let next = next.normalised();
        let effect = SettingsEffect::between(&before, &next);
        if let Some(app) = self.session.as_mut() {
            let old_speed = app.config.voice.speed;
            if (old_speed - next.voice.speed).abs() >= f32::EPSILON {
                app.rescale_pace(old_speed, next.voice.speed);
            }
            app.config = next.clone();
            if (app.player.volume() - next.voice.volume).abs() >= f32::EPSILON {
                app.player.set_volume(next.voice.volume);
            }
            if effect == SettingsEffect::Resynthesize {
                app.apply_effect(Effect::Resynthesize);
            }
        }
        self.config = next;
        match effect {
            SettingsEffect::Reload => self.reload_voice(now),
            SettingsEffect::Reparse => self.reparse(now),
            _ => {}
        }
        self.settings()
    }

    /// Applique, puis écrit `config.toml` (sauf lecteur hors disque). Une
    /// écriture impossible laisse le réglage appliqué : l'erreur le dit.
    pub fn update_settings(&mut self, patch: &SettingsPatch, now: Instant) -> Result<SettingsView> {
        let applied = self.apply_patch(patch, now);
        if self.persist {
            self.config.save()?;
        }
        Ok(applied)
    }

    /// Recharge la voix avec la configuration courante. Un texte ouvert est
    /// rouvert à sa phrase, et la lecture reprend seule si elle tournait ; au
    /// repos, rien ne se charge avant le prochain réveil.
    pub(super) fn reload_voice(&mut self, now: Instant) {
        if self.session.is_some() {
            let playing = self
                .session
                .as_ref()
                .is_some_and(|app| app.player.is_playing() && !app.is_finished());
            self.rest();
            self.wake(now);
            if playing {
                if let Some(app) = self.session.as_mut() {
                    app.player.play();
                }
            }
            return;
        }
        let prepared = self.engine.is_some();
        self.engine = None;
        if prepared && !self.is_resting() {
            self.ensure_engine();
        }
    }

    /// Reconstruit le document avec la politique des blocs de code courante,
    /// à la même phrase de texte : « sauter » retire des phrases, « lire » en
    /// ajoute, le numéro seul ne suffit donc pas.
    fn reparse(&mut self, now: Instant) {
        if let Some(app) = self.session.as_ref() {
            let playing = app.player.is_playing() && !app.is_finished();
            let rank = prose_rank(app.doc(), app.current_sentence());
            let Some(text) = self.text.clone() else { return };
            self.open(text, now);
            if let Some(app) = self.session.as_mut() {
                app.player.pause();
                let target = sentence_at_prose_rank(app.doc(), rank);
                app.seek_to(target);
                if playing {
                    app.player.play();
                }
            }
            self.revision += 1;
            return;
        }
        let (Some(at), Some(text)) = (self.resting_at, self.text.as_ref()) else { return };
        let rank = self.resting_doc.as_ref().map_or(0, |doc| prose_rank(doc, at));
        let doc = crate::doc::builder::parse_document(&text.markdown, self.config.code_policy());
        self.resting_at = Some(sentence_at_prose_rank(&doc, rank));
        self.resting_doc = Some(doc);
        self.revision += 1;
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
        if self.download.as_ref().is_some_and(|d| !d.is_finished()) {
            return Err(anyhow!("téléchargement déjà en cours"));
        }
        self.download = Some(Download::start(package)?);
        Ok(())
    }

    /// Change de modèle : la voix se recharge en place, au même endroit du
    /// texte, et la lecture reprend si elle tournait.
    pub fn use_voice(&mut self, id: &str, now: Instant) -> Result<()> {
        let package = package(id)?;
        let path = models::install_path(package)
            .filter(|_| models::is_installed(package))
            .ok_or_else(|| anyhow!("voix non installée : {id}"))?;
        self.sync_config();
        self.switch_voice_config(&path.to_string_lossy(), package.family);
        // Le texte ouvert recopie sa configuration à la mise au repos : il
        // doit porter le nouveau modèle avant le rechargement.
        if let Some(app) = self.session.as_mut() {
            app.config = self.config.clone();
        }
        self.reload_voice(now);
        if self.persist {
            self.config.save()?;
        }
        Ok(())
    }

    /// La partie sans disque du changement de voix.
    fn switch_voice_config(&mut self, model_path: &str, family: &str) {
        self.config.engine.model_path = model_path.to_string();
        self.config.engine.family = family.to_string();
        self.config.align_voice_with_family();
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
    pub(super) fn poll_download(&mut self, now: Instant) {
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
            let _ = self.use_voice(&id, now);
        }
    }

    /// La configuration que le texte ouvert a pu modifier (vitesse…).
    fn current_config(&self) -> &Config {
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

fn in_code(doc: &Document, block: usize) -> bool {
    doc.block(block).is_some_and(|b| matches!(b.kind, BlockKind::Code { .. }))
}

/// Rang d'une phrase parmi celles qui ne sont pas du code : il ne bouge pas
/// quand la politique des blocs de code change.
fn prose_rank(doc: &Document, sentence: usize) -> usize {
    doc.sentences
        .iter()
        .take(sentence)
        .filter(|s| !in_code(doc, s.block_id))
        .count()
}

/// La phrase de texte de ce rang ; la dernière si le document est plus court.
fn sentence_at_prose_rank(doc: &Document, rank: usize) -> usize {
    doc.sentences
        .iter()
        .filter(|s| !in_code(doc, s.block_id))
        .nth(rank)
        .map(|s| s.id)
        .unwrap_or_else(|| doc.sentences.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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

    /// Un lecteur qui compte les voix chargées.
    fn counting_reader() -> (Reader, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let r = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            Instant::now(),
        );
        (r, loads)
    }

    fn effect_of(change: impl FnOnce(&mut Config)) -> SettingsEffect {
        let before = Config::default();
        let mut after = before.clone();
        change(&mut after);
        SettingsEffect::between(&before, &after)
    }

    #[test]
    fn every_setting_round_trips_through_a_patch() {
        let mut r = reader();
        let patch = SettingsPatch {
            voice: Some("F2".into()),
            language: Some("en".into()),
            speed: Some(1.4),
            pitch: Some(0.2),
            energy: Some(1.3),
            emotion: Some("calme".into()),
            instruction: Some("posé".into()),
            volume: Some(0.6),
            engine: Some("system".into()),
            compute: Some("cpu".into()),
            threads: Some(3),
            sessions: Some(2),
            prefetch: Some(9),
            temperature: Some(Some(0.7)),
            top_p: Some(Some(0.9)),
            top_k: Some(Some(40)),
            guidance_scale: Some(Some(3.5)),
            repetition_penalty: Some(Some(1.2)),
            inference_steps: Some(Some(12)),
            seed: Some(Some(42)),
            code_blocks: Some("skip".into()),
            autoplay: Some(true),
            seek_step: Some(7),
            line_width: Some(80),
            line_spacing: Some(2),
            reading_ruler: Some(false),
            keep_voice_minutes: Some(30),
            extra_set: vec![("cfg_scale".into(), "3".into())],
            extra_remove: vec![],
        };
        let expected = SettingsView {
            voice: "F2".into(),
            language: "en".into(),
            speed: 1.4,
            pitch: 0.2,
            energy: 1.3,
            emotion: "calme".into(),
            instruction: "posé".into(),
            volume: 0.6,
            engine: "system".into(),
            model_path: String::new(),
            family: String::new(),
            compute: "cpu".into(),
            threads: 3,
            sessions: 2,
            prefetch: 9,
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            guidance_scale: Some(3.5),
            repetition_penalty: Some(1.2),
            inference_steps: Some(12),
            seed: Some(42),
            code_blocks: "skip".into(),
            autoplay: true,
            seek_step: 7,
            line_width: 80,
            line_spacing: 2,
            reading_ruler: false,
            keep_voice_minutes: 30,
            extra: BTreeMap::from([("cfg_scale".to_string(), "3".to_string())]),
        };
        assert_eq!(r.apply_patch(&patch, Instant::now()), expected);
        assert_eq!(r.settings(), expected);
        // Retirer une option libre, rendre la main au modèle.
        let back = SettingsPatch {
            temperature: Some(None),
            extra_remove: vec!["cfg_scale".into()],
            ..Default::default()
        };
        let applied = r.apply_patch(&back, Instant::now());
        assert_eq!(applied.temperature, None);
        assert!(applied.extra.is_empty());
    }

    #[test]
    fn a_patch_reaches_the_open_text() {
        let mut r = reader();
        r.open_text("Un.", "a", Instant::now());
        let patch = SettingsPatch {
            speed: Some(1.4),
            reading_ruler: Some(false),
            keep_voice_minutes: Some(30),
            ..Default::default()
        };
        let applied = r.apply_patch(&patch, Instant::now());
        assert_eq!(r.settings(), applied);
        let app = r.session().unwrap();
        assert_eq!(app.config.voice.speed, 1.4);
        assert!(!app.config.reading.reading_ruler);
        assert_eq!(app.config.app.keep_voice_minutes, 30);
    }

    #[test]
    fn each_setting_triggers_the_effect_of_its_kind() {
        use SettingsEffect::*;
        assert_eq!(effect_of(|_| {}), Nothing);
        assert_eq!(effect_of(|c| c.reading.line_spacing = 2), Display);
        assert_eq!(effect_of(|c| c.reading.reading_ruler = false), Display);
        assert_eq!(effect_of(|c| c.reading.seek_step = 9), Display);
        assert_eq!(effect_of(|c| c.reading.autoplay = false), Display);
        assert_eq!(effect_of(|c| c.app.keep_voice_minutes = 2), Display);
        assert_eq!(effect_of(|c| c.voice.volume = 0.5), Volume);
        assert_eq!(effect_of(|c| c.voice.voice = "F1".into()), Resynthesize);
        assert_eq!(effect_of(|c| c.voice.language = "en".into()), Resynthesize);
        assert_eq!(effect_of(|c| c.voice.speed = 1.5), Resynthesize);
        assert_eq!(effect_of(|c| c.sampling.temperature = Some(0.5)), Resynthesize);
        assert_eq!(effect_of(|c| c.sampling.seed = Some(1)), Resynthesize);
        assert_eq!(effect_of(|c| { c.extra.insert("k".into(), "v".into()); }), Resynthesize);
        assert_eq!(effect_of(|c| c.reading.code_blocks = "skip".into()), Reparse);
        assert_eq!(effect_of(|c| c.engine.engine = "system".into()), Reload);
        assert_eq!(effect_of(|c| c.engine.compute = "cpu".into()), Reload);
        assert_eq!(effect_of(|c| c.engine.threads = 2), Reload);
        assert_eq!(effect_of(|c| c.engine.sessions = 3), Reload);
        assert_eq!(effect_of(|c| c.engine.prefetch = 9), Reload);
        assert_eq!(effect_of(|c| c.engine.model_path = "/m.gguf".into()), Reload);
        // Le plus fort l'emporte.
        assert_eq!(effect_of(|c| { c.voice.volume = 0.5; c.reading.code_blocks = "read".into(); }), Reparse);
        assert_eq!(effect_of(|c| { c.voice.voice = "F1".into(); c.engine.threads = 2; }), Reload);
    }

    #[test]
    fn hand_written_values_survive_other_changes() {
        let mut config = Config::default();
        config.reading.code_blocks = "lire".into();
        config.voice.language = "ja".into();
        config.engine.compute = "cuda".into();
        config.app.keep_voice_minutes = 7;
        let mut r = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        let before = r.settings();
        r.apply_patch(&SettingsPatch::default(), Instant::now());
        assert_eq!(r.settings(), before, "un patch vide ne touche à rien");
        r.apply_patch(&SettingsPatch { line_spacing: Some(2), ..Default::default() }, Instant::now());
        let after = r.settings();
        assert_eq!(after.code_blocks, "lire");
        assert_eq!(after.language, "ja");
        assert_eq!(after.compute, "cuda");
        assert_eq!(after.keep_voice_minutes, 7);
        assert_eq!(after.line_spacing, 2);
    }

    #[test]
    fn values_out_of_bounds_come_back_as_applied() {
        let mut r = reader();
        let applied = r.apply_patch(
            &SettingsPatch {
                line_spacing: Some(4),
                seek_step: Some(80),
                threads: Some(0),
                speed: Some(9.0),
                volume: Some(-1.0),
                temperature: Some(Some(f32::NAN)),
                top_k: Some(Some(900)),
                ..Default::default()
            },
            Instant::now(),
        );
        assert_eq!(applied.line_spacing, 3);
        assert_eq!(applied.seek_step, 50);
        assert_eq!(applied.threads, 1);
        assert_eq!(applied.speed, 3.0);
        assert_eq!(applied.volume, 0.0);
        assert_eq!(applied.temperature, None);
        assert_eq!(applied.top_k, Some(200));
    }

    #[test]
    fn the_volume_changes_without_resynthesis() {
        let mut r = reader();
        r.open_text("Un. Deux.", "a", Instant::now());
        r.apply_patch(&SettingsPatch { volume: Some(0.5), ..Default::default() }, Instant::now());
        let app = r.session().unwrap();
        assert_eq!(app.player.volume(), 0.5);
        assert_eq!(app.config.voice.volume, 0.5);
    }

    #[test]
    fn an_engine_change_reloads_the_voice_and_resumes_at_the_same_sentence() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| {
            app.seek_sentences(1);
            app.toggle_play();
        });
        r.apply_patch(&SettingsPatch { threads: Some(3), ..Default::default() }, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 2, "une nouvelle voix");
        let app = r.session().expect("texte rouvert");
        assert_eq!(app.current_sentence(), 1);
        assert!(app.player.is_playing(), "la lecture reprend seule");
        assert_eq!(app.config.engine.threads, 3);
    }

    #[test]
    fn an_engine_change_while_resting_waits_for_the_next_wake() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply_patch(&SettingsPatch { compute: Some("cpu".into()), ..Default::default() }, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "rien ne se charge au repos");
        assert!(r.is_resting());
        assert_eq!(r.config().engine.compute, "cpu");
    }

    #[test]
    fn a_prepared_voice_is_reloaded_at_once() {
        let (mut r, loads) = counting_reader();
        r.prepare_voice(Instant::now());
        r.apply_patch(&SettingsPatch { prefetch: Some(8), ..Default::default() }, Instant::now());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert!(r.voice_loaded());
    }

    const WITH_CODE: &str = "Avant.\n\n```\nx = 1\ny = 2\n```\n\nAprès un. Après deux.\n";

    #[test]
    fn changing_code_blocks_keeps_the_sentence_being_read() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        // Annoncé : Avant. · Bloc de code. · Après un. · Après deux.
        r.with_session(t0, |app| app.seek_to(3));
        let revision = r.revision();
        r.apply_patch(&SettingsPatch { code_blocks: Some("read".into()), ..Default::default() }, t0);
        let app = r.session().unwrap();
        let current = app.current_sentence();
        assert_eq!(app.doc().sentences[current].written(), "Après deux.");
        assert_eq!(current, 4, "deux lignes de code lues au lieu d'une annonce");
        assert_eq!(r.revision(), revision + 1, "le document est republié");
    }

    #[test]
    fn changing_code_blocks_at_rest_keeps_the_resting_sentence() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        r.with_session(t0, |app| app.seek_to(3));
        r.rest();
        r.apply_patch(&SettingsPatch { code_blocks: Some("skip".into()), ..Default::default() }, t0);
        // Sauté : Avant. · Après un. · Après deux.
        assert_eq!(crate::reader::view::snapshot(&r).sentence, 2);
        assert_eq!(crate::reader::view::document(&r).sentence_count, 3);
        assert!(r.is_resting());
    }

    #[test]
    fn a_reader_off_disk_applies_without_writing() {
        let mut r = reader();
        r.set_persistent(false);
        let applied = r
            .update_settings(&SettingsPatch { keep_voice_minutes: Some(5), ..Default::default() }, Instant::now())
            .expect("rien à écrire");
        assert_eq!(applied.keep_voice_minutes, 5);
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
        assert!(r.use_voice("n-existe-pas", Instant::now()).is_err());
    }

    #[test]
    fn a_voice_switch_survives_the_rest_and_reaches_the_next_engine() {
        use std::sync::Mutex;
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut r = Reader::new(
            config,
            Box::new(move |c| {
                log.lock().unwrap().push(c.engine.model_path.clone());
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            Instant::now(),
        );
        r.open_text("Un. Deux.", "a", Instant::now());
        r.rest();
        r.switch_voice_config("/nouvelle/voix.onnx", "supertonic");
        r.with_session(Instant::now(), |_| {});
        assert_eq!(seen.lock().unwrap().last().unwrap(), "/nouvelle/voix.onnx");
        assert_eq!(r.config().engine.model_path, "/nouvelle/voix.onnx");
    }

    #[test]
    fn the_resting_document_is_kept_once_and_dropped_on_open() {
        let mut r = reader();
        r.open_text("Un. Deux.", "a", Instant::now());
        assert!(r.resting_document().is_none());
        r.rest();
        assert!(r.resting_document().is_some());
        assert_eq!(crate::reader::view::document(&r).sentence_count, 2);
        r.open_text("Trois.", "b", Instant::now());
        assert!(r.resting_document().is_none());
    }
}
```

`ffi/src/lib.rs` — adaptation minimale (l'app garde ses quatre réglages
jusqu'à la Tâche 5) : remplacer les méthodes `apply_settings`,
`update_settings`, `use_voice` et la fonction `settings_from` par :

```rust
    /// Applique sans écrire sur disque.
    pub fn apply_settings(&self, settings: SettingsView) {
        let patch = patch_of(settings);
        let _ = self.handle.ask(move |reader| {
            reader.apply_patch(&patch, Instant::now());
        });
    }

    pub fn update_settings(&self, settings: SettingsView) -> Result<(), SillonError> {
        let patch = patch_of(settings);
        self.handle
            .ask(move |reader| {
                reader
                    .update_settings(&patch, Instant::now())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
```

```rust
    pub fn use_voice(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.use_voice(&id, Instant::now()).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
```

```rust
/// Les quatre réglages que l'app montre encore, en patch.
fn patch_of(s: SettingsView) -> settings::SettingsPatch {
    settings::SettingsPatch {
        speed: Some(s.speed),
        line_spacing: Some(s.line_spacing),
        reading_ruler: Some(s.reading_ruler),
        keep_voice_minutes: Some(s.keep_voice_minutes),
        ..Default::default()
    }
}
```

Et dans le constructeur `silent()`, mettre le lecteur hors disque :

```rust
            handle: ReaderHandle::spawn(|| {
                let mut config = Config::default();
                config.reading.autoplay = false;
                config.app.keep_voice_minutes = -1;
                let mut reader = Reader::new(
                    config,
                    Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                    Box::new(Player::silent),
                    Instant::now(),
                );
                // L'app de test ne touche pas au disque de l'utilisateur.
                reader.set_persistent(false);
                reader
            }),
```

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cargo test && cargo test -p sillon-ffi`
Expected: PASS — `test result: ok.` pour chaque binaire de test, aucun échec.

- [ ] **Étape 5 : commit**

```bash
git add src/reader/settings.rs src/reader/mod.rs src/config.rs src/app/mod.rs src/app/config.rs ffi/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(reader): tous les réglages, modifiés par patch, avec l'effet de leur nature

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 2 : capacités du modèle actif et catalogue enrichi

L'onglet Voix doit savoir quelles voix, quelles langues et quels réglages le
modèle actif honore ; l'onglet Modèles, décrire chaque paquet. Le terminal
propose lui aussi les langues du modèle au lieu de sept langues fixes.

**Fichiers :**
- Modifier : `src/models.rs` (ajout de `FALLBACK_LANGUAGES` et `languages_for_family` après `honours`, tests)
- Modifier : `src/reader/settings.rs` (`VoiceModel` enrichi, `voice_model`, `ModelCapabilities`, `Reader::capabilities`, tests)
- Modifier : `src/app/config.rs:171` (`LANGUAGES` retiré) et `:457-460` (`Field::Language`), tests

**Interfaces :**
- Consomme : `models::{CATALOG, voices_for_family, honoured_options, available, install_path, is_installed}`, `TtsEngine::requires_reference`.
- Produit :
  - `pub const models::FALLBACK_LANGUAGES: &[&str]` (les sept du terminal), `pub fn models::languages_for_family(family: &str) -> Vec<&'static str>`
  - `pub struct VoiceModel { id, name, bytes: u64, installed: bool, active: bool, recommended: bool, summary: String, family: String, precision: String, languages_label: String, languages: Vec<String>, pace: String }`
  - `pub fn voice_model(p: &ModelPackage, active_path: &str) -> VoiceModel`
  - `pub struct VoiceChoice { pub id: String, pub label: String }`
  - `pub struct ModelCapabilities { pub family: String, pub voices: Vec<VoiceChoice>, pub languages: Vec<String>, pub honoured: Vec<String>, pub requires_reference: bool }` (`honoured` vide = famille inconnue, tout compte)
  - `Reader::capabilities(&self) -> ModelCapabilities`

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/models.rs`, module `tests` :

```rust
    #[test]
    fn languages_follow_the_family() {
        assert_eq!(languages_for_family("supertonic").len(), 31);
        assert_eq!(languages_for_family("magpie_tts").len(), 11);
        assert!(languages_for_family("magpie_tts").contains(&"zh"));
        // Famille inconnue : les sept que le terminal proposait.
        assert_eq!(languages_for_family(""), FALLBACK_LANGUAGES.to_vec());
    }
```

Dans `src/reader/settings.rs`, module `tests` :

```rust
    fn reader_on(family: &str) -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.engine.family = family.into();
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    #[test]
    fn capabilities_follow_the_active_family() {
        let supertonic = reader_on("supertonic").capabilities();
        assert_eq!(supertonic.family, "supertonic");
        assert_eq!(supertonic.voices.len(), 10);
        assert_eq!(supertonic.voices[0], VoiceChoice { id: "M1".into(), label: "M1 · masculine 1".into() });
        assert_eq!(supertonic.languages.len(), 31);
        assert!(supertonic.honoured.contains(&"speaking_rate".to_string()));
        assert!(!supertonic.honoured.contains(&"temperature".to_string()));
        assert!(!supertonic.requires_reference);

        // MagpieTTS : température, top-k et guidage apparaissent, la vitesse disparaît.
        let magpie = reader_on("magpie_tts").capabilities();
        assert_eq!(magpie.voices.len(), 5);
        assert_eq!(magpie.languages.len(), 11);
        for key in ["temperature", "top_k", "guidance_scale"] {
            assert!(magpie.honoured.contains(&key.to_string()), "{key}");
        }
        assert!(!magpie.honoured.contains(&"speaking_rate".to_string()));

        // Famille inconnue : rien n'est affirmé.
        let unknown = reader_on("").capabilities();
        assert!(unknown.voices.is_empty());
        assert!(unknown.honoured.is_empty());
        assert_eq!(unknown.languages.len(), 7);
    }

    #[test]
    fn every_catalog_entry_describes_itself() {
        for package in crate::models::CATALOG {
            let v = voice_model(package, "");
            assert_eq!(v.id, package.id);
            assert_eq!(v.summary, package.description);
            assert_eq!(v.family, package.family);
            assert_eq!(v.precision, package.precision);
            assert_eq!(v.languages_label, package.languages_label);
            assert_eq!(v.languages.len(), package.languages.len());
            assert_eq!(v.pace, package.pace);
            assert!(!v.active, "aucun chemin actif");
        }
        let first = &crate::models::CATALOG[0];
        if let Some(path) = crate::models::install_path(first) {
            assert!(voice_model(first, &path.to_string_lossy()).active);
        }
    }
```

Dans `src/app/config.rs`, module `tests` :

```rust
    #[test]
    fn languages_cycle_through_those_of_the_model() {
        let mut config = Config::default();
        config.engine.family = "supertonic".into();
        config.voice.language = "fr".into();
        for _ in 0..31 {
            Field::Language.adjust(&mut config, 1);
        }
        assert_eq!(config.voice.language, "fr", "trente et une langues, un tour complet");
        config.engine.family = "magpie_tts".into();
        for _ in 0..11 {
            Field::Language.adjust(&mut config, 1);
        }
        assert_eq!(config.voice.language, "fr", "onze langues chez MagpieTTS");
    }
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cargo test --lib -- models:: reader::settings:: app::config::`
Expected: FAIL — `cannot find function languages_for_family`,
`no method named capabilities`, `cannot find function voice_model`.

- [ ] **Étape 3 : écrire l'implémentation**

`src/models.rs`, après `pub fn honours(…)` :

```rust
/// Langues proposées quand la famille n'est pas au catalogue : les sept que
/// le terminal proposait.
pub const FALLBACK_LANGUAGES: &[&str] = &["fr", "en", "de", "es", "it", "pt", "nl"];

/// Langues qu'une famille sait parler : celles de son paquet au catalogue.
///
/// Toutes les variantes d'une famille partagent la même liste ; le catalogue
/// entier est consulté, pas seulement ce que ce binaire sait charger, pour
/// que l'écran dise juste même compilé sans audio.cpp.
pub fn languages_for_family(family: &str) -> Vec<&'static str> {
    CATALOG
        .iter()
        .find(|p| p.family == family)
        .map(|p| p.languages.to_vec())
        .unwrap_or_else(|| FALLBACK_LANGUAGES.to_vec())
}
```

`src/app/config.rs` : supprimer la ligne
`const LANGUAGES: &[&str] = &["fr", "en", "de", "es", "it", "pt", "nl"];` et
remplacer la branche `Field::Language` d'`adjust` par :

```rust
            Field::Language => {
                // Les langues du modèle, plus seulement sept langues fixes.
                let languages = models::languages_for_family(&config.engine.family);
                cycle_string(&mut config.voice.language, &languages, delta);
                Effect::Resynthesize
            }
```

`src/reader/settings.rs` : remplacer `pub struct VoiceModel { … }` par :

```rust
/// Un paquet du catalogue, tel que l'onglet Modèles le décrit.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
    pub summary: String,
    pub family: String,
    pub precision: String,
    pub languages_label: String,
    pub languages: Vec<String>,
    /// Le rythme mesuré : fluide, ou haché.
    pub pace: String,
}

/// Une voix intégrée du modèle actif.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceChoice {
    pub id: String,
    pub label: String,
}

/// Ce que le modèle actif sait faire : ses voix, ses langues, les réglages
/// qu'il lit. `honoured` vide : famille inconnue, rien n'est affirmé.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelCapabilities {
    pub family: String,
    pub voices: Vec<VoiceChoice>,
    pub languages: Vec<String>,
    pub honoured: Vec<String>,
    /// Le modèle ne parle qu'en clonant un WAV. Connu une fois la voix
    /// chargée seulement : faux avant.
    pub requires_reference: bool,
}

/// Décrit un paquet ; `active_path` est le chemin du modèle en place.
pub fn voice_model(p: &models::ModelPackage, active_path: &str) -> VoiceModel {
    VoiceModel {
        id: p.id.to_string(),
        name: p.display_name.to_string(),
        bytes: p.bytes,
        installed: models::is_installed(p),
        active: !active_path.is_empty()
            && models::install_path(p).is_some_and(|path| path.to_string_lossy() == active_path),
        recommended: p.recommended,
        summary: p.description.to_string(),
        family: p.family.to_string(),
        precision: p.precision.to_string(),
        languages_label: p.languages_label.to_string(),
        languages: p.languages.iter().map(|l| l.to_string()).collect(),
        pace: p.pace.to_string(),
    }
}
```

et remplacer `pub fn voices(&self)` dans `impl Reader` par :

```rust
    pub fn voices(&self) -> Vec<VoiceModel> {
        let active = self.current_config().engine.model_path.clone();
        models::available().into_iter().map(|p| voice_model(p, &active)).collect()
    }

    pub fn capabilities(&self) -> ModelCapabilities {
        let family = self.current_config().engine.family.clone();
        ModelCapabilities {
            voices: models::voices_for_family(&family)
                .iter()
                .map(|v| VoiceChoice { id: v.id.to_string(), label: v.label.to_string() })
                .collect(),
            languages: models::languages_for_family(&family).into_iter().map(String::from).collect(),
            honoured: models::honoured_options(&family).iter().map(|o| o.to_string()).collect(),
            requires_reference: self.engine.as_ref().is_some_and(|e| e.requires_reference()),
            family,
        }
    }
```

`ffi/src/lib.rs` compile tel quel (il ne lit que les six premiers champs de
`VoiceModel`).

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cargo test && cargo test -p sillon-ffi`
Expected: PASS — `test result: ok.` partout.

- [ ] **Étape 5 : commit**

```bash
git add src/models.rs src/reader/settings.rs src/app/config.rs
git commit -m "$(cat <<'EOF'
feat(reader): capacités du modèle actif et catalogue décrit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 3 : « Écouter », même sans texte ouvert

L'aperçu ne marche aujourd'hui que dans l'écran du terminal, avec un texte
ouvert. L'app doit pouvoir faire dire la phrase d'exemple à tout moment :
la voix se charge au besoin, un nouvel essai coupe le précédent, un refus
s'explique. Sans texte, le lecteur porte son propre aperçu (`LonePreview`) ;
le fil du lecteur reste vif tant qu'un aperçu se prépare ou sonne (Point
d'attention 1).

**Fichiers :**
- Modifier : `src/tts/preview.rs` (ajout de `refusal`, `REFERENCE_MISSING`, `AUDIO_MISSING`, tests)
- Modifier : `src/app/mod.rs` (champ `preview_error`, `request_preview`, `preview_pending`, `preview_playing`, fin de `preview_voice`, `poll_preview`)
- Créer : `src/reader/preview.rs`
- Modifier : `src/reader/mod.rs` (`pub mod preview;`, champs `lone_preview` et `preview_error`, `tick`, `rest`, `open`)
- Modifier : `src/reader/settings.rs` (`reload_voice` relâche l'aperçu seul)
- Modifier : `src/reader/view.rs` (`Snapshot.preview`)
- Modifier : `src/reader/handle.rs` (`Activity.previewing`, `cadence`, test)

**Interfaces :**
- Consomme : `Previewer::{new, request, cancel, poll, pending_for}`, `Player::{play_preview, stop_preview, preview_playing, is_available, set_volume}`, `Reader::{wake, ensure_engine}` (Tâche 1 pour `reload_voice`).
- Produit :
  - `pub fn tts::preview::refusal(requires_reference: bool, voice: &str, audio_available: bool) -> Option<String>`, `pub const REFERENCE_MISSING: &str`, `pub const AUDIO_MISSING: &str`
  - `App::request_preview(&mut self) -> Result<(), String>`, `App::preview_pending(&self) -> bool`, `App::preview_playing(&self) -> bool`, `pub App::preview_error: Option<String>`
  - `pub struct reader::preview::PreviewView { pub pending: bool, pub playing: bool, pub error: Option<String> }` (`Default`)
  - `Reader::preview_voice(&mut self, now: Instant) -> Result<(), String>`, `Reader::stop_preview(&mut self)`, `Reader::preview_view(&self) -> PreviewView`, `Reader::previewing(&self) -> bool`
  - `Snapshot.preview: PreviewView`

- [ ] **Étape 1 : écrire les tests qui échouent**

`src/tts/preview.rs`, module `tests` (le créer s'il n'existe pas, avec `use super::*;`) :

```rust
    #[test]
    fn a_cloning_model_without_a_wav_is_refused_before_the_audio_check() {
        assert_eq!(refusal(true, "", true).as_deref(), Some(REFERENCE_MISSING));
        assert_eq!(refusal(true, "/n/existe/pas.wav", false).as_deref(), Some(REFERENCE_MISSING));
        let present = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        assert_eq!(refusal(true, present, true), None);
        assert_eq!(refusal(false, "", false).as_deref(), Some(AUDIO_MISSING));
        assert_eq!(refusal(false, "", true), None);
    }
```

`src/reader/handle.rs`, module `tests` :

```rust
    #[test]
    fn a_preview_keeps_the_reader_thread_awake() {
        // Sans texte ni lecture, le fil dormirait : l'aperçu prêt ne serait
        // jamais sondé, donc jamais joué.
        let none = Activity::default();
        assert_eq!(cadence(Activity { previewing: true, ..none }), FAST);
        assert_eq!(cadence(Activity { previewing: true, engine: true, ..none }), FAST);
    }
```

`src/reader/preview.rs` (nouveau fichier ; ses tests figurent à l'étape 3).

- [ ] **Étape 2 : vérifier l'échec**

Run: `cargo test --lib -- tts::preview:: reader::handle::`
Expected: FAIL — `cannot find function refusal`, `struct Activity has no field named previewing`.

- [ ] **Étape 3 : écrire l'implémentation**

`src/tts/preview.rs`, après `sample_text` :

```rust
/// Refus d'un aperçu clonant sans WAV de référence.
pub const REFERENCE_MISSING: &str =
    "ce modèle ne parle qu'en clonant : choisissez d'abord un fichier WAV";
/// Refus d'un aperçu sans sortie audio.
pub const AUDIO_MISSING: &str = "sortie audio indisponible : aucun aperçu possible";

/// Pourquoi un aperçu ne peut pas partir, s'il ne le peut pas.
///
/// La voix clonée passe d'abord : c'est la seule à laquelle l'utilisateur
/// peut remédier depuis l'onglet.
pub fn refusal(requires_reference: bool, voice: &str, audio_available: bool) -> Option<String> {
    if requires_reference {
        let voice = crate::config::expand_tilde(voice.trim());
        if voice.is_empty() || !std::path::Path::new(&voice).is_file() {
            return Some(REFERENCE_MISSING.to_string());
        }
    }
    if !audio_available {
        return Some(AUDIO_MISSING.to_string());
    }
    None
}
```

`src/app/mod.rs` :
- dans `pub struct App`, après `previewer: Previewer,` :

```rust
    /// Dernier échec d'aperçu, pour l'app (le terminal le lit dans `screen`).
    pub preview_error: Option<String>,
```

- dans `from_parts`, après `previewer,` : `preview_error: None,`
- dans `preview_voice`, remplacer la fin, depuis `// L'aperçu parle seul.`, par :

```rust
        match self.request_preview() {
            Ok(()) => self.screen.notice.clear(),
            Err(err) => self.screen.notice = err,
        }
    }

    /// L'aperçu tel que l'app le demande : les réglages en cours sur la
    /// phrase d'exemple, la lecture mise en pause, le précédent coupé. Rend
    /// la raison d'un refus.
    pub fn request_preview(&mut self) -> Result<(), String> {
        self.stop_preview();
        self.preview_error = None;
        if let Some(reason) = preview::refusal(
            self.synth.requires_reference(),
            &self.config.voice.voice,
            self.player.is_available(),
        ) {
            return Err(reason);
        }
        // L'aperçu parle seul.
        self.player.pause();
        let text = preview::sample_text(&self.config.voice.language).to_string();
        let params = self.config.tts_params();
        let label = self.preview_label();
        self.previewer
            .request(text, params, label)
            .map_err(|err| format!("aperçu impossible : {err}"))
    }

    /// Un aperçu se synthétise.
    pub fn preview_pending(&self) -> bool {
        self.previewer.pending_for().is_some()
    }

    /// Un aperçu sonne.
    pub fn preview_playing(&self) -> bool {
        self.player.preview_playing()
    }
```

- dans `poll_preview`, remplacer la branche d'erreur par :

```rust
            Err(err) => {
                let message = format!("aperçu impossible : {err}");
                self.preview_error = Some(message.clone());
                self.screen.notice = message;
            }
```

`src/reader/preview.rs` :

```rust
//! « Écouter » depuis l'app : la phrase d'exemple avec la voix et les
//! réglages en cours, texte ouvert ou non.
//!
//! Avec un texte, l'aperçu passe par sa session (elle a déjà sa voix et sa
//! sortie). Sans texte, le lecteur charge la voix au besoin et garde un
//! aperçu à lui, relâché dès qu'un texte s'ouvre ou que la voix se repose.

use std::sync::Arc;
use std::time::Instant;

use crate::audio::Player;
use crate::tts::preview::{self as sample, Previewer};

use super::Reader;

/// L'aperçu d'un lecteur sans texte : sa fabrique et sa sortie.
pub(super) struct LonePreview {
    previewer: Previewer,
    player: Player,
}

/// Ce que l'aperçu fait, pour l'interface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreviewView {
    /// La phrase d'exemple se synthétise.
    pub pending: bool,
    /// Elle sonne.
    pub playing: bool,
    /// Le dernier échec de synthèse.
    pub error: Option<String>,
}

impl Reader {
    /// Fait dire la phrase d'exemple. Un texte au repos se réveille ; sans
    /// texte, la voix se charge. Rend la raison d'un refus.
    pub fn preview_voice(&mut self, now: Instant) -> Result<(), String> {
        self.last_activity = now;
        self.preview_error = None;
        self.wake(now);
        if let Some(app) = self.session.as_mut() {
            return app.request_preview();
        }
        let engine = self.ensure_engine();
        if self.lone_preview.is_none() {
            let player = (self.players)();
            self.lone_preview = Some(LonePreview { previewer: Previewer::new(Arc::clone(&engine)), player });
        }
        let lone = self.lone_preview.as_mut().expect("posé juste au-dessus");
        lone.previewer.cancel();
        lone.player.stop_preview();
        lone.player.set_volume(self.config.voice.volume);
        if let Some(reason) =
            sample::refusal(engine.requires_reference(), &self.config.voice.voice, lone.player.is_available())
        {
            return Err(reason);
        }
        let label = format!("{} · {:.2}×", self.config.voice.voice, self.config.voice.speed);
        lone.previewer
            .request(
                sample::sample_text(&self.config.voice.language).to_string(),
                self.config.tts_params(),
                label,
            )
            .map_err(|err| format!("aperçu impossible : {err}"))
    }

    /// Coupe l'aperçu, qu'il se prépare ou qu'il sonne.
    pub fn stop_preview(&mut self) {
        if let Some(app) = self.session.as_mut() {
            app.stop_preview();
        }
        if let Some(lone) = self.lone_preview.as_mut() {
            lone.previewer.cancel();
            lone.player.stop_preview();
        }
    }

    pub fn preview_view(&self) -> PreviewView {
        if let Some(app) = self.session.as_ref() {
            return PreviewView {
                pending: app.preview_pending(),
                playing: app.preview_playing(),
                error: app.preview_error.clone(),
            };
        }
        match &self.lone_preview {
            Some(lone) => PreviewView {
                pending: lone.previewer.pending_for().is_some(),
                playing: lone.player.preview_playing(),
                error: self.preview_error.clone(),
            },
            None => PreviewView { error: self.preview_error.clone(), ..PreviewView::default() },
        }
    }

    /// Un aperçu se prépare ou sonne : le fil doit rester vif.
    pub fn previewing(&self) -> bool {
        let view = self.preview_view();
        view.pending || view.playing
    }

    /// Joue l'aperçu sans texte dès qu'il est prêt.
    pub(super) fn poll_lone_preview(&mut self) {
        let Some(lone) = self.lone_preview.as_mut() else { return };
        match lone.previewer.poll() {
            Some(Ok(audio)) => lone.player.play_preview(&audio),
            Some(Err(err)) => self.preview_error = Some(format!("aperçu impossible : {err}")),
            None => {}
        }
    }

    /// Relâche l'aperçu sans texte. Son arrêt attend la synthèse en cours :
    /// hors de ce fil, comme `App::reload_engine`.
    pub(super) fn drop_lone_preview(&mut self) {
        if let Some(LonePreview { previewer, player }) = self.lone_preview.take() {
            std::thread::spawn(move || drop(previewer));
            drop(player);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::{TtsEngine, TtsSession};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Un moteur qui ne parle qu'en clonant.
    struct CloningEngine;
    impl TtsEngine for CloningEngine {
        fn name(&self) -> String {
            "clonage".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn TtsSession>> {
            SilentEngine.open_session()
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn requires_reference(&self) -> bool {
            true
        }
    }

    fn reader_with(engine: fn() -> Arc<dyn TtsEngine>) -> (Reader, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let r = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                engine()
            }),
            Box::new(Player::silent),
            Instant::now(),
        );
        (r, loads)
    }

    fn silent() -> Arc<dyn TtsEngine> {
        Arc::new(SilentEngine)
    }

    fn cloning() -> Arc<dyn TtsEngine> {
        Arc::new(CloningEngine)
    }

    #[test]
    fn a_preview_without_text_loads_the_voice_and_explains_a_missing_output() {
        let (mut r, loads) = reader_with(silent);
        let err = r.preview_voice(Instant::now()).unwrap_err();
        assert_eq!(err, sample::AUDIO_MISSING);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "la voix se charge au besoin");
        assert!(r.voice_loaded());
        assert!(r.session().is_none(), "aucun texte n'est ouvert pour autant");
        assert!(!r.previewing());
    }

    #[test]
    fn a_cloning_model_without_wav_refuses_the_preview() {
        let (mut r, _) = reader_with(cloning);
        let err = r.preview_voice(Instant::now()).unwrap_err();
        assert_eq!(err, sample::REFERENCE_MISSING);
    }

    #[test]
    fn a_preview_with_an_open_text_goes_through_the_session() {
        let (mut r, loads) = reader_with(silent);
        r.open_text("Un. Deux.", "a", Instant::now());
        let err = r.preview_voice(Instant::now()).unwrap_err();
        assert_eq!(err, sample::AUDIO_MISSING);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "la voix du texte sert");
        assert!(r.lone_preview.is_none(), "pas d'aperçu à part");
    }

    #[test]
    fn a_resting_text_wakes_for_a_preview() {
        let (mut r, loads) = reader_with(silent);
        r.open_text("Un. Deux.", "a", Instant::now());
        r.rest();
        let _ = r.preview_voice(Instant::now());
        assert!(!r.is_resting());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn opening_a_text_drops_the_lone_preview() {
        let (mut r, _) = reader_with(silent);
        let _ = r.preview_voice(Instant::now());
        assert!(r.lone_preview.is_some());
        r.open_text("Un.", "a", Instant::now());
        assert!(r.lone_preview.is_none());
    }

    #[test]
    fn stopping_without_a_preview_is_harmless() {
        let (mut r, _) = reader_with(silent);
        r.stop_preview();
        assert_eq!(r.preview_view(), PreviewView::default());
    }
}
```

`src/reader/mod.rs` :
- après `pub mod handle;` : `pub mod preview;`
- dans `pub struct Reader`, après `persist: bool,` :

```rust
    /// Aperçu de voix sans texte ouvert.
    lone_preview: Option<preview::LonePreview>,
    /// Dernier échec de cet aperçu.
    preview_error: Option<String>,
```

- dans `Reader::new`, après `persist: true,` : `lone_preview: None,` puis `preview_error: None,`
- dans `tick`, juste après `self.poll_download(now);` : `self.poll_lone_preview();`
- au début de `rest` et au début de `open` : `self.drop_lone_preview();`

`src/reader/settings.rs`, dans `reload_voice`, branche sans session, avant
`let prepared = …` : `self.drop_lone_preview();` (l'aperçu parlait avec
l'ancienne voix).

`src/reader/view.rs` :
- dans `pub struct Snapshot`, après `download` :

```rust
    /// L'aperçu de voix (« Écouter »).
    pub preview: super::preview::PreviewView,
```

- dans le `Snapshot { … }` du repos, après `download: reader.download(),` :
  `preview: reader.preview_view(),` ; dans celui de la session, après
  `download: reader.download(),` : `preview: reader.preview_view(),`

`src/reader/handle.rs` :
- dans `struct Activity`, après `engine: bool,` :

```rust
    /// Un aperçu se prépare ou sonne : il faut le sonder.
    previewing: bool,
```

- dans `Activity::of`, remplacer la dernière ligne par :

```rust
        Self {
            reading,
            loading,
            downloading,
            download_settled,
            engine: reader.engine().is_some(),
            previewing: reader.previewing(),
        }
```

- dans `cadence`, remplacer la première condition par
  `if a.reading || a.loading || a.downloading || a.previewing {`

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cargo test && cargo test -p sillon-ffi`
Expected: PASS — `test result: ok.` partout (le FFI ignore encore `preview`).

- [ ] **Étape 5 : commit**

```bash
git add src/tts/preview.rs src/app/mod.rs src/reader/preview.rs src/reader/mod.rs src/reader/settings.rs src/reader/view.rs src/reader/handle.rs
git commit -m "$(cat <<'EOF'
feat(reader): écouter une voix même sans texte ouvert

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 4 : modèles et données — supprimer, annuler, vider, gérer les signets

Les onglets Modèles et Général agissent sur le disque : supprimer un modèle
(jamais l'actif), annuler un téléchargement, vider le cache des documents,
lister et retirer des signets. Un lecteur hors disque (l'app de test) refuse
tout effacement.

**Fichiers :**
- Modifier : `src/source/cache.rs` (ajout de `clear` et `clear_in`, test)
- Créer : `src/reader/data.rs`
- Modifier : `src/reader/mod.rs` (`pub mod data;`)
- Modifier : `src/reader/settings.rs` (`current_config` et `package` passent en `pub(super)`)

**Interfaces :**
- Consomme : `models::{remove, install_path, ModelPackage}`, `Download::{cancel, is_finished}`, `BookmarkStore::{load, save, remove}`, `Reader::persist` (Tâche 1), `Reader::current_config`, `settings::package`.
- Produit :
  - `pub fn source::cache::clear() -> anyhow::Result<u64>` (octets libérés), `fn clear_in(dir: &Path) -> std::io::Result<u64>`
  - `pub const reader::data::TRIAL: &str`
  - `Reader::removal_refusal(&self, &ModelPackage) -> Option<String>`, `Reader::remove_model(&mut self, id: &str) -> Result<()>`
  - `Reader::cancel_download(&mut self) -> Result<()>`
  - `Reader::clear_document_cache(&self) -> Result<u64>`
  - `pub struct reader::data::StoredBookmark { pub document: String, pub label: String, pub sentence: usize, pub section_title: String, pub snippet: String, pub when: String }`
  - `Reader::all_bookmarks(&self) -> Vec<StoredBookmark>`, `Reader::remove_bookmark(&mut self, document: &str, sentence: usize) -> Result<()>`
  - `pub fn reader::data::document_label(identity: &str) -> String`

- [ ] **Étape 1 : écrire les tests qui échouent**

`src/source/cache.rs`, module `tests` :

```rust
    #[test]
    fn clearing_removes_only_the_cached_documents_and_counts_their_bytes() {
        let dir = scratch();
        write_in(&dir, "a", "12345");
        write_in(&dir, "b", "123");
        std::fs::write(dir.join("garde.txt"), b"x").unwrap();
        assert_eq!(clear_in(&dir).unwrap(), 8);
        assert_eq!(read_in(&dir, "a"), None);
        assert!(dir.join("garde.txt").exists(), "seuls les .md partent");
        assert_eq!(clear_in(&dir).unwrap(), 0, "déjà vide");
        std::fs::remove_dir_all(&dir).ok();
    }
```

`src/reader/data.rs` (nouveau ; ses tests figurent à l'étape 3).

- [ ] **Étape 2 : vérifier l'échec**

Run: `cargo test --lib -- source::cache::`
Expected: FAIL — `cannot find function clear_in`.

- [ ] **Étape 3 : écrire l'implémentation**

`src/source/cache.rs`, après `write` :

```rust
/// Vide le cache des documents ; rend les octets libérés.
///
/// Rien n'est perdu : un document se ré-extrait à sa prochaine ouverture.
pub fn clear() -> Result<u64> {
    let Some(dir) = crate::paths::cache_dir().map(|d| d.join("documents")) else {
        return Ok(0);
    };
    if !dir.exists() {
        return Ok(0);
    }
    Ok(clear_in(&dir)?)
}

fn clear_in(dir: &Path) -> std::io::Result<u64> {
    let mut freed = 0;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "md") {
            freed += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            std::fs::remove_file(&path)?;
        }
    }
    Ok(freed)
}
```

`src/reader/settings.rs` : `fn current_config(&self) -> &Config` devient
`pub(super) fn current_config(&self) -> &Config`, et
`fn package(id: &str) -> …` devient `pub(super) fn package(id: &str) -> …`.

`src/reader/mod.rs`, après `pub mod command;` : `pub mod data;`

`src/reader/data.rs` :

```rust
//! Ce que les onglets Modèles et Général effacent : modèles, téléchargement
//! en cours, cache des documents, signets.
//!
//! Un lecteur hors disque (`set_persistent(false)`, l'app de test) refuse
//! tout effacement : un test d'interface ne vide jamais les données de
//! l'utilisateur.

use anyhow::{anyhow, bail, Result};

use crate::bookmark::BookmarkStore;
use crate::models::{self, ModelPackage};

use super::settings::package;
use super::Reader;

/// Refus d'un lecteur hors disque.
pub const TRIAL: &str = "mode d'essai : rien n'est modifié sur le disque";

/// Un signet, quel que soit son document.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBookmark {
    /// Identité du document (chemin, ou source et empreinte).
    pub document: String,
    /// Nom lisible du document.
    pub label: String,
    pub sentence: usize,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
}

/// Le nom d'un document pour l'œil : le fichier d'un chemin, la source d'un
/// texte capturé (« sélection », « image »).
pub fn document_label(identity: &str) -> String {
    if identity.starts_with('/') {
        return std::path::Path::new(identity)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| identity.to_string());
    }
    identity.split(':').next().unwrap_or(identity).to_string()
}

impl Reader {
    /// Pourquoi ce modèle ne peut pas être supprimé maintenant, s'il ne le
    /// peut pas : il est actif, ou il se télécharge.
    pub fn removal_refusal(&self, package: &ModelPackage) -> Option<String> {
        let active_path = &self.current_config().engine.model_path;
        let active = !active_path.is_empty()
            && models::install_path(package).is_some_and(|p| p.to_string_lossy() == active_path.as_str());
        if active {
            return Some(format!(
                "{} est le modèle actif : choisissez-en un autre avant de le supprimer",
                package.display_name
            ));
        }
        if self.download.as_ref().is_some_and(|d| d.package.id == package.id && !d.is_finished()) {
            return Some(format!(
                "{} se télécharge : annulez d'abord le téléchargement",
                package.display_name
            ));
        }
        None
    }

    pub fn remove_model(&mut self, id: &str) -> Result<()> {
        let package = package(id)?;
        if let Some(reason) = self.removal_refusal(package) {
            bail!(reason);
        }
        if !self.persist {
            bail!(TRIAL);
        }
        models::remove(package)
    }

    /// Demande l'arrêt du téléchargement en cours. Le fichier partiel reste :
    /// une nouvelle installation reprendra de là.
    pub fn cancel_download(&mut self) -> Result<()> {
        match self.download.as_ref() {
            Some(d) if !d.is_finished() => {
                d.cancel();
                Ok(())
            }
            _ => Err(anyhow!("aucun téléchargement en cours")),
        }
    }

    pub fn clear_document_cache(&self) -> Result<u64> {
        if !self.persist {
            bail!(TRIAL);
        }
        crate::source::cache::clear()
    }

    /// Tous les signets, tous documents confondus. Un texte ouvert a les
    /// siens en mémoire : ils font foi.
    pub fn all_bookmarks(&self) -> Vec<StoredBookmark> {
        let items = match &self.session {
            Some(app) => app.bookmarks.items.clone(),
            None => BookmarkStore::load().0.items,
        };
        items
            .into_iter()
            .map(|b| StoredBookmark {
                label: document_label(&b.document),
                when: b.timestamp(),
                document: b.document,
                sentence: b.sentence_id,
                section_title: b.section_title,
                snippet: b.snippet,
            })
            .collect()
    }

    pub fn remove_bookmark(&mut self, document: &str, sentence: usize) -> Result<()> {
        if !self.persist {
            bail!(TRIAL);
        }
        let mut store = BookmarkStore::load().0;
        self.remove_bookmark_in(&mut store, document, sentence);
        store.save()
    }

    /// Retire le signet du magasin lu sur disque et de celui du texte ouvert :
    /// sans le second, le prochain signet posé réécrirait l'ancien.
    fn remove_bookmark_in(&mut self, store: &mut BookmarkStore, document: &str, sentence: usize) {
        store.remove(document, sentence);
        if let Some(app) = self.session.as_mut() {
            app.bookmarks.remove(document, sentence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn reader_with(config: Config) -> Reader {
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    fn quiet() -> Config {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config
    }

    #[test]
    fn the_active_model_cannot_be_removed() {
        let first = &models::CATALOG[0];
        let second = &models::CATALOG[1];
        let Some(path) = models::install_path(first) else { return };
        let mut config = quiet();
        config.engine.model_path = path.to_string_lossy().into_owned();
        let r = reader_with(config);
        let reason = r.removal_refusal(first).expect("refusé");
        assert!(reason.contains("modèle actif"), "{reason}");
        assert_eq!(r.removal_refusal(second), None);
    }

    #[test]
    fn no_model_is_active_without_a_path() {
        let r = reader_with(quiet());
        for package in models::CATALOG {
            assert_eq!(r.removal_refusal(package), None);
        }
    }

    #[test]
    fn a_reader_off_disk_erases_nothing() {
        let mut r = reader_with(quiet());
        r.set_persistent(false);
        assert_eq!(r.clear_document_cache().unwrap_err().to_string(), TRIAL);
        assert_eq!(r.remove_bookmark("a", 0).unwrap_err().to_string(), TRIAL);
    }

    #[test]
    fn cancelling_without_a_download_is_an_error() {
        let mut r = reader_with(quiet());
        assert!(r.cancel_download().is_err());
    }

    #[test]
    fn removing_a_bookmark_also_forgets_it_in_the_open_session() {
        let mut r = reader_with(quiet());
        r.open_text("Un. Deux.", "sélection", Instant::now());
        let identity = r.document_identity().unwrap().to_string();
        // En mémoire seulement : rien n'est écrit chez l'utilisateur.
        r.session.as_mut().unwrap().bookmarks.toggle(&identity, 1, 0, "Titre", "Deux.");
        let mut store = BookmarkStore::default();
        store.toggle(&identity, 1, 0, "Titre", "Deux.");
        r.remove_bookmark_in(&mut store, &identity, 1);
        assert!(!store.contains(&identity, 1));
        assert!(
            !r.session().unwrap().bookmarks.contains(&identity, 1),
            "la session ne le réécrira pas au prochain signet"
        );
        assert!(r.all_bookmarks().is_empty());
    }

    #[test]
    fn the_open_session_lists_its_bookmarks_with_a_readable_name() {
        let mut r = reader_with(quiet());
        r.open_text("Un. Deux.", "sélection", Instant::now());
        let identity = r.document_identity().unwrap().to_string();
        r.session.as_mut().unwrap().bookmarks.toggle(&identity, 0, 0, "Titre", "Un.");
        let all = r.all_bookmarks();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].label, "sélection");
        assert_eq!(all[0].snippet, "Un.");
        assert_eq!(all[0].sentence, 0);
    }

    #[test]
    fn documents_are_named_for_the_eye() {
        assert_eq!(document_label("/Users/x/notes/cours.md"), "cours.md");
        assert_eq!(document_label("sélection:00ab12"), "sélection");
        assert_eq!(document_label("image:ff"), "image");
        assert_eq!(document_label("presse-papiers"), "presse-papiers");
    }
}
```

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cargo test && cargo test -p sillon-ffi`
Expected: PASS — `test result: ok.` partout.

- [ ] **Étape 5 : commit**

```bash
git add src/source/cache.rs src/reader/data.rs src/reader/mod.rs src/reader/settings.rs
git commit -m "$(cat <<'EOF'
feat(reader): supprimer un modèle, annuler un téléchargement, vider le cache, retirer un signet

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 5 : le pont vers Swift

Tout ce que les Tâches 1 à 4 ont ajouté passe dans `ffi/src/lib.rs`, et la
liaison Swift est régénérée. Côté Swift, `ReaderModel` reçoit les appels
correspondants (hors fil principal) et le code existant est adapté juste
assez pour compiler : les anciens onglets disparaîtront aux Tâches 8 à 10.

**Fichiers :**
- Modifier : `ffi/src/lib.rs`
- Créer : `macos/Sources/Settings/SettingsDefaults.swift`
- Créer : `macos/Sources/Panel/ReadingDisplay.swift`
- Modifier : `macos/Sources/Model/ReaderModel.swift:202-229`
- Modifier : `macos/Sources/Panel/PanelController.swift:21` et `:47`
- Modifier : `macos/Sources/Settings/SettingsView.swift` (anciens onglets : `ReadingValues.patch`, appels par patch, interlignage 0…3)
- Modifier : `macos/Tests/CommitterTests.swift:98-123` (`ReadingValuesPatchTests`)
- Créer : `macos/Tests/ReadingDisplayTests.swift`

**Interfaces :**
- Consomme : `Reader::{settings, apply_patch, update_settings, capabilities, voices, preview_voice, stop_preview, cancel_download, remove_model, clear_document_cache, all_bookmarks, remove_bookmark}`, `models::free_space`, `paths::config_dir`, `Snapshot.preview`.
- Produit (Swift, noms générés par UniFFI) :
  - `struct ExtraOption { key: String; value: String }`
  - `struct SettingsView` : `voice, language: String; speed, pitch, energy: Float; emotion, instruction: String; volume: Float; engine, modelPath, family, compute: String; threads, sessions, prefetch: UInt32; temperature, topP: Float?; topK: UInt32?; guidanceScale, repetitionPenalty: Float?; inferenceSteps: UInt32?; seed: UInt64?; codeBlocks: String; autoplay: Bool; seekStep, lineWidth, lineSpacing: UInt32; readingRuler: Bool; keepVoiceMinutes: Int64; extra: [ExtraOption]`
  - `enum SamplingFloat { case model, value(value: Float) }`, `enum SamplingInt { case model, value(value: UInt32) }`, `enum SamplingSeed { case random, value(value: UInt64) }`
  - `struct SettingsPatch` : mêmes noms en optionnels (`speed: Float?`…), échantillonnage en `SamplingFloat?` / `SamplingInt?` / `SamplingSeed?`, plus `extraSet: [ExtraOption]`, `extraRemove: [String]`
  - `struct VoiceChoice { id, label: String }`, `struct ModelCapabilities { family: String; voices: [VoiceChoice]; languages: [String]; honoured: [String]; requiresReference: Bool }`
  - `struct VoiceModel { id, name: String; bytes: UInt64; installed, active, recommended: Bool; summary, family, precision, languagesLabel: String; languages: [String]; pace: String }`
  - `struct PreviewView { pending, playing: Bool; error: String? }`, `Snapshot.preview: PreviewView`
  - `struct StoredBookmarkView { document, label: String; sentence: UInt32; sectionTitle, snippet, when: String }`
  - `SillonReader` : `settings() -> SettingsView`, `applyPatch(patch:) -> SettingsView`, `updateSettings(patch:) throws -> SettingsView`, `capabilities() -> ModelCapabilities`, `previewVoice() throws`, `stopPreview()`, `cancelDownload() throws`, `removeModel(id:) throws`, `freeSpace() -> UInt64?`, `configDir() -> String?`, `clearDocumentCache() throws -> UInt64`, `allBookmarks() -> [StoredBookmarkView]`, `removeBookmark(document:sentence:) throws` ; `applySettings` disparaît.
  - `extension SettingsView { static let defaults }`, `extension SettingsPatch { static var empty; var isEmpty: Bool }`
  - `struct ReadingDisplay: Equatable { var lineSpacing: UInt32; var readingRuler: Bool; var seekStep: UInt32; init(); init(_ s: SettingsView) }`
  - `ReaderModel` : `updateSettings(_ patch: SettingsPatch) async throws -> SettingsView`, `private(set) var settingsRevision: Int`, `loadCapabilities() async -> ModelCapabilities`, `previewVoice() async throws`, `stopPreview() async`, `cancelDownloadAsync() async throws`, `removeModelAsync(_ id: String) async throws`, `freeSpace() async -> UInt64?`, `clearDocumentCache() async throws -> UInt64`, `loadAllBookmarks() async -> [StoredBookmarkView]`, `removeBookmarkAsync(document: String, sentence: UInt32) async throws`, `var configDirectory: String?` ; `changeSettings` disparaît ; `useVoiceAsync` relance le suivi.

- [ ] **Étape 1 : écrire les tests Rust qui échouent**

`ffi/src/lib.rs`, remplacer le test `settings_round_trip_without_touching_disk` par :

```rust
    #[test]
    fn settings_round_trip_through_a_patch_without_touching_disk() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.keep_voice_minutes = Some(5);
        patch.line_spacing = Some(9);
        patch.temperature = Some(SamplingFloat::Value { value: 0.7 });
        patch.extra_set = vec![ExtraOption { key: "cfg_scale".into(), value: "3".into() }];
        let applied = reader.apply_patch(patch);
        assert_eq!(applied.keep_voice_minutes, 5);
        assert_eq!(applied.line_spacing, 3, "ramené dans les bornes");
        assert_eq!(applied.temperature, Some(0.7));
        assert_eq!(applied.extra.len(), 1);
        let mut back = SettingsPatch::default();
        back.temperature = Some(SamplingFloat::Model);
        back.extra_remove = vec!["cfg_scale".into()];
        let applied = reader.apply_patch(back);
        assert_eq!(applied.temperature, None);
        assert!(applied.extra.is_empty());
        assert_eq!(reader.settings().keep_voice_minutes, 5);
    }

    #[test]
    fn the_silent_reader_writes_and_erases_nothing() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.seek_step = Some(7);
        assert_eq!(reader.update_settings(patch).expect("rien à écrire").seek_step, 7);
        let err = reader.clear_document_cache().unwrap_err().to_string();
        assert!(err.contains("mode d'essai"), "{err}");
        assert!(reader.remove_bookmark("x".into(), 0).is_err());
    }

    #[test]
    fn capabilities_and_preview_answer_without_a_model() {
        let reader = SillonReader::silent();
        let caps = reader.capabilities();
        assert!(caps.voices.is_empty());
        assert_eq!(caps.languages.len(), 7);
        let err = reader.preview_voice().unwrap_err().to_string();
        assert!(err.contains("sortie audio"), "{err}");
        reader.stop_preview();
        assert!(!reader.snapshot().preview.pending);
    }
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cargo test -p sillon-ffi`
Expected: FAIL — `cannot find struct SettingsPatch`, `no method named apply_patch`.

- [ ] **Étape 3 : écrire le pont Rust**

`ffi/src/lib.rs` — en tête, remplacer
`use sillon::reader::{settings, view, Reader};` par
`use sillon::reader::{data, preview, settings, view, Reader};`.

Ajouter après `pub struct DownloadView { … }` :

```rust
#[derive(uniffi::Record)]
pub struct PreviewView {
    pub pending: bool,
    pub playing: bool,
    pub error: Option<String>,
}
```

Dans `pub struct Snapshot`, après `pub download: Option<DownloadView>,` :
`pub preview: PreviewView,` ; dans `snapshot_of`, après le champ `download` :

```rust
        preview: preview_of(s.preview),
```

Remplacer les enregistrements `SettingsView` et `VoiceModel` par :

```rust
#[derive(uniffi::Record, Clone)]
pub struct ExtraOption {
    pub key: String,
    pub value: String,
}

#[derive(uniffi::Record, Clone)]
pub struct SettingsView {
    pub voice: String,
    pub language: String,
    pub speed: f32,
    pub pitch: f32,
    pub energy: f32,
    pub emotion: String,
    pub instruction: String,
    pub volume: f32,
    pub engine: String,
    pub model_path: String,
    pub family: String,
    pub compute: String,
    pub threads: u32,
    pub sessions: u32,
    pub prefetch: u32,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
    pub code_blocks: String,
    pub autoplay: bool,
    pub seek_step: u32,
    pub line_width: u32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
    pub extra: Vec<ExtraOption>,
}

/// Un réglage d'échantillonnage : la valeur du modèle, ou une valeur posée.
#[derive(uniffi::Enum, Clone)]
pub enum SamplingFloat {
    Model,
    Value { value: f32 },
}

#[derive(uniffi::Enum, Clone)]
pub enum SamplingInt {
    Model,
    Value { value: u32 },
}

/// La graine : un tirage au hasard, ou une valeur fixe.
#[derive(uniffi::Enum, Clone)]
pub enum SamplingSeed {
    Random,
    Value { value: u64 },
}

/// Une modification : `None` pour un réglage non touché.
#[derive(uniffi::Record, Clone, Default)]
pub struct SettingsPatch {
    pub voice: Option<String>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub energy: Option<f32>,
    pub emotion: Option<String>,
    pub instruction: Option<String>,
    pub volume: Option<f32>,
    pub engine: Option<String>,
    pub compute: Option<String>,
    pub threads: Option<u32>,
    pub sessions: Option<u32>,
    pub prefetch: Option<u32>,
    pub temperature: Option<SamplingFloat>,
    pub top_p: Option<SamplingFloat>,
    pub top_k: Option<SamplingInt>,
    pub guidance_scale: Option<SamplingFloat>,
    pub repetition_penalty: Option<SamplingFloat>,
    pub inference_steps: Option<SamplingInt>,
    pub seed: Option<SamplingSeed>,
    pub code_blocks: Option<String>,
    pub autoplay: Option<bool>,
    pub seek_step: Option<u32>,
    pub line_width: Option<u32>,
    pub line_spacing: Option<u32>,
    pub reading_ruler: Option<bool>,
    pub keep_voice_minutes: Option<i64>,
    pub extra_set: Vec<ExtraOption>,
    pub extra_remove: Vec<String>,
}

#[derive(uniffi::Record)]
pub struct VoiceChoice {
    pub id: String,
    pub label: String,
}

#[derive(uniffi::Record)]
pub struct ModelCapabilities {
    pub family: String,
    pub voices: Vec<VoiceChoice>,
    pub languages: Vec<String>,
    pub honoured: Vec<String>,
    pub requires_reference: bool,
}

#[derive(uniffi::Record)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
    pub summary: String,
    pub family: String,
    pub precision: String,
    pub languages_label: String,
    pub languages: Vec<String>,
    pub pace: String,
}

#[derive(uniffi::Record)]
pub struct StoredBookmarkView {
    pub document: String,
    pub label: String,
    pub sentence: u32,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
}
```

Dans `impl SillonReader`, remplacer tout le bloc de `pub fn settings` à
`pub fn use_voice` inclus par :

```rust
    pub fn settings(&self) -> SettingsView {
        self.handle
            .ask(|reader| settings_of(reader.settings()))
            .unwrap_or_else(|| settings_of(settings::SettingsView::of(&Config::default())))
    }

    /// Applique sans écrire sur disque ; rend les valeurs appliquées.
    pub fn apply_patch(&self, patch: SettingsPatch) -> SettingsView {
        let patch = patch_from(patch);
        self.handle
            .ask(move |reader| settings_of(reader.apply_patch(&patch, Instant::now())))
            .unwrap_or_else(|| self.settings())
    }

    /// Applique et écrit `config.toml` ; rend les valeurs appliquées.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsView, SillonError> {
        let patch = patch_from(patch);
        self.handle
            .ask(move |reader| {
                reader
                    .update_settings(&patch, Instant::now())
                    .map(settings_of)
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn capabilities(&self) -> ModelCapabilities {
        self.handle
            .ask(|reader| {
                let c = reader.capabilities();
                ModelCapabilities {
                    family: c.family,
                    voices: c
                        .voices
                        .into_iter()
                        .map(|v| VoiceChoice { id: v.id, label: v.label })
                        .collect(),
                    languages: c.languages,
                    honoured: c.honoured,
                    requires_reference: c.requires_reference,
                }
            })
            .unwrap_or(ModelCapabilities {
                family: String::new(),
                voices: Vec::new(),
                languages: Vec::new(),
                honoured: Vec::new(),
                requires_reference: false,
            })
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
                        summary: v.summary,
                        family: v.family,
                        precision: v.precision,
                        languages_label: v.languages_label,
                        languages: v.languages,
                        pace: v.pace,
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
            .ask(move |reader| reader.use_voice(&id, Instant::now()).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// La phrase d'exemple avec les réglages en cours ; la raison d'un refus.
    pub fn preview_voice(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.preview_voice(Instant::now()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn stop_preview(&self) {
        let _ = self.handle.ask(|reader| reader.stop_preview());
    }

    pub fn cancel_download(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.cancel_download().map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn remove_model(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.remove_model(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// Espace libre du volume des modèles. Ne passe pas par le fil du lecteur.
    pub fn free_space(&self) -> Option<u64> {
        sillon::models::free_space()
    }

    /// Le dossier de `config.toml`. Ne passe pas par le fil du lecteur.
    pub fn config_dir(&self) -> Option<String> {
        sillon::paths::config_dir().map(|d| d.to_string_lossy().into_owned())
    }

    /// Vide le cache des documents ; rend les octets libérés.
    pub fn clear_document_cache(&self) -> Result<u64, SillonError> {
        self.handle
            .ask(|reader| reader.clear_document_cache().map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn all_bookmarks(&self) -> Vec<StoredBookmarkView> {
        self.handle
            .ask(|reader| reader.all_bookmarks().into_iter().map(stored_bookmark_of).collect())
            .unwrap_or_default()
    }

    pub fn remove_bookmark(&self, document: String, sentence: u32) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| {
                reader
                    .remove_bookmark(&document, sentence as usize)
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
```

Remplacer les fonctions `settings_of` et `patch_of` (posée à la Tâche 1) par :

```rust
fn settings_of(s: settings::SettingsView) -> SettingsView {
    SettingsView {
        voice: s.voice,
        language: s.language,
        speed: s.speed,
        pitch: s.pitch,
        energy: s.energy,
        emotion: s.emotion,
        instruction: s.instruction,
        volume: s.volume,
        engine: s.engine,
        model_path: s.model_path,
        family: s.family,
        compute: s.compute,
        threads: s.threads,
        sessions: s.sessions,
        prefetch: s.prefetch,
        temperature: s.temperature,
        top_p: s.top_p,
        top_k: s.top_k,
        guidance_scale: s.guidance_scale,
        repetition_penalty: s.repetition_penalty,
        inference_steps: s.inference_steps,
        seed: s.seed,
        code_blocks: s.code_blocks,
        autoplay: s.autoplay,
        seek_step: s.seek_step,
        line_width: s.line_width,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
        extra: s
            .extra
            .into_iter()
            .map(|(key, value)| ExtraOption { key, value })
            .collect(),
    }
}

fn float(v: Option<SamplingFloat>) -> Option<Option<f32>> {
    v.map(|s| match s {
        SamplingFloat::Model => None,
        SamplingFloat::Value { value } => Some(value),
    })
}

fn int(v: Option<SamplingInt>) -> Option<Option<u32>> {
    v.map(|s| match s {
        SamplingInt::Model => None,
        SamplingInt::Value { value } => Some(value),
    })
}

fn patch_from(p: SettingsPatch) -> settings::SettingsPatch {
    settings::SettingsPatch {
        voice: p.voice,
        language: p.language,
        speed: p.speed,
        pitch: p.pitch,
        energy: p.energy,
        emotion: p.emotion,
        instruction: p.instruction,
        volume: p.volume,
        engine: p.engine,
        compute: p.compute,
        threads: p.threads,
        sessions: p.sessions,
        prefetch: p.prefetch,
        temperature: float(p.temperature),
        top_p: float(p.top_p),
        top_k: int(p.top_k),
        guidance_scale: float(p.guidance_scale),
        repetition_penalty: float(p.repetition_penalty),
        inference_steps: int(p.inference_steps),
        seed: p.seed.map(|s| match s {
            SamplingSeed::Random => None,
            SamplingSeed::Value { value } => Some(value),
        }),
        code_blocks: p.code_blocks,
        autoplay: p.autoplay,
        seek_step: p.seek_step,
        line_width: p.line_width,
        line_spacing: p.line_spacing,
        reading_ruler: p.reading_ruler,
        keep_voice_minutes: p.keep_voice_minutes,
        extra_set: p.extra_set.into_iter().map(|o| (o.key, o.value)).collect(),
        extra_remove: p.extra_remove,
    }
}

fn preview_of(p: preview::PreviewView) -> PreviewView {
    PreviewView { pending: p.pending, playing: p.playing, error: p.error }
}

fn stored_bookmark_of(b: data::StoredBookmark) -> StoredBookmarkView {
    StoredBookmarkView {
        document: b.document,
        label: b.label,
        sentence: b.sentence as u32,
        section_title: b.section_title,
        snippet: b.snippet,
        when: b.when,
    }
}
```

- [ ] **Étape 4 : vérifier les tests Rust**

Run: `cargo test -p sillon-ffi && cargo test`
Expected: PASS — `test result: ok.` partout.

- [ ] **Étape 5 : régénérer la liaison**

Run: `./scripts/build-ffi.sh`
Expected: dernière ligne `Liaison prête dans macos/Generated et macos/Libraries` ;
`grep -c "public func applyPatch" macos/Generated/sillon_ffi.swift` affiche `1`.

- [ ] **Étape 6 : écrire le test Swift qui échoue**

`macos/Tests/ReadingDisplayTests.swift` :

```swift
import XCTest
@testable import Sillon

/// Le panneau lit des réglages du moteur l'interlignage, la règle et le pas
/// de l'avance rapide.
final class ReadingDisplayTests: XCTestCase {
    func testTakesWhatThePanelNeedsFromTheSettings() {
        var s = SettingsView.defaults
        s.lineSpacing = 3; s.readingRuler = false; s.seekStep = 12
        let d = ReadingDisplay(s)
        XCTAssertEqual(d.lineSpacing, 3)
        XCTAssertFalse(d.readingRuler)
        XCTAssertEqual(d.seekStep, 12)
        XCTAssertEqual(ReadingDisplay(), ReadingDisplay(SettingsView.defaults))
    }

    func testAnEmptyPatchTouchesNothing() {
        XCTAssertTrue(SettingsPatch.empty.isEmpty)
        var p = SettingsPatch.empty
        p.speed = 1.2
        XCTAssertFalse(p.isEmpty)
    }
}
```

Dans `macos/Tests/CommitterTests.swift`, remplacer `ReadingValuesPatchTests`
par :

```swift
/// L'ancien onglet Lecture n'écrit que le réglage touché.
final class ReadingValuesPatchTests: XCTestCase {
    func testOnlyTheChangedFieldIsWritten() {
        var old = ReadingValues(); old.speed = 1.0; old.lineSpacing = 2; old.ruler = true
        var new = old; new.ruler = false
        var expected = SettingsPatch.empty
        expected.readingRuler = false
        XCTAssertEqual(ReadingValues.patch(new, over: old), expected)
    }

    func testEachChangedFieldIsWritten() {
        let old = ReadingValues()
        var new = old; new.speed = 2.0; new.lineSpacing = 3
        let p = ReadingValues.patch(new, over: old)
        XCTAssertEqual(p.speed, 2.0)
        XCTAssertEqual(p.lineSpacing, 3)
        XCTAssertNil(p.readingRuler)
        XCTAssertNil(p.keepVoiceMinutes)
    }
}
```

- [ ] **Étape 7 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: FAIL — compilation : `cannot find 'ReadingDisplay' in scope`,
`type 'SettingsView' has no member 'defaults'`, et dans les sources
`extra argument 'speed' in call` (`PanelController`), `value of type 'SillonReader' has no member 'updateSettings(settings:)'`.

- [ ] **Étape 8 : écrire le code Swift**

`macos/Sources/Settings/SettingsDefaults.swift` :

```swift
/// Les défauts du moteur (`Config::default`) : ce qu'un onglet montre avant
/// le premier chargement, et la base des tests.
extension SettingsView {
    static let defaults = SettingsView(
        voice: "", language: "fr", speed: 1, pitch: 0, energy: 1, emotion: "", instruction: "",
        volume: 1, engine: "auto", modelPath: "", family: "", compute: "metal", threads: 8,
        sessions: 0, prefetch: 6, temperature: nil, topP: nil, topK: nil, guidanceScale: nil,
        repetitionPenalty: nil, inferenceSteps: nil, seed: nil, codeBlocks: "announce",
        autoplay: true, seekStep: 5, lineWidth: 0, lineSpacing: 1, readingRuler: true,
        keepVoiceMinutes: 10, extra: [])
}

extension SettingsPatch {
    /// Un patch qui ne touche rien : on n'y pose que ce qui change.
    static var empty: SettingsPatch {
        SettingsPatch(
            voice: nil, language: nil, speed: nil, pitch: nil, energy: nil, emotion: nil,
            instruction: nil, volume: nil, engine: nil, compute: nil, threads: nil, sessions: nil,
            prefetch: nil, temperature: nil, topP: nil, topK: nil, guidanceScale: nil,
            repetitionPenalty: nil, inferenceSteps: nil, seed: nil, codeBlocks: nil,
            autoplay: nil, seekStep: nil, lineWidth: nil, lineSpacing: nil, readingRuler: nil,
            keepVoiceMinutes: nil, extraSet: [], extraRemove: [])
    }

    var isEmpty: Bool { self == .empty }
}
```

`macos/Sources/Panel/ReadingDisplay.swift` :

```swift
/// Ce que le panneau lit des réglages du moteur, relu à chaque ouverture et
/// à chaque réglage écrit.
struct ReadingDisplay: Equatable {
    var lineSpacing: UInt32 = 1
    var readingRuler = true
    /// Phrases sautées par ⇧← ⇧→.
    var seekStep: UInt32 = 5

    init() {}

    init(_ s: SettingsView) {
        lineSpacing = s.lineSpacing
        readingRuler = s.readingRuler
        seekStep = s.seekStep
    }
}
```

`macos/Sources/Panel/PanelController.swift` : la ligne 21 devient
`var reading = ReadingDisplay()` ; dans `show()`, la ligne 47 devient
`Task { reading = ReadingDisplay(await model.loadSettings()) }`.

`macos/Sources/Model/ReaderModel.swift` : remplacer le bloc de
`func changeSettings` à la fin de `func useVoiceAsync` par :

```swift
    /// Change à chaque réglage écrit : le panneau relit son affichage.
    private(set) var settingsRevision = 0

    /// Applique un patch et l'écrit, hors du fil principal ; rend les
    /// valeurs telles qu'appliquées (bornes comprises).
    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView {
        let reader = reader
        let applied = try await Task.detached { try reader.updateSettings(patch: patch) }.value
        settingsRevision &+= 1
        wake()
        return applied
    }
    /// Voix, langues et réglages honorés par le modèle actif.
    func loadCapabilities() async -> ModelCapabilities {
        let reader = reader
        return await Task.detached { reader.capabilities() }.value
    }
    /// Les voix du catalogue, lues hors du fil principal.
    func loadVoices() async -> [VoiceModel] {
        let reader = reader
        return await Task.detached { reader.voices() }.value
    }
    func installVoiceAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.installVoice(id: id) }.value
        wake()
    }
    /// Change de modèle : la voix se recharge, le suivi montre le chargement.
    func useVoiceAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.useVoice(id: id) }.value
        wake()
    }
    func cancelDownloadAsync() async throws {
        let reader = reader
        try await Task.detached { try reader.cancelDownload() }.value
        wake()
    }
    func removeModelAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.removeModel(id: id) }.value
    }
    func freeSpace() async -> UInt64? {
        let reader = reader
        return await Task.detached { reader.freeSpace() }.value
    }
    /// « Écouter » : la phrase d'exemple avec les réglages en cours.
    func previewVoice() async throws {
        let reader = reader
        try await Task.detached { try reader.previewVoice() }.value
        wake()
    }
    func stopPreview() async {
        let reader = reader
        await Task.detached { reader.stopPreview() }.value
        wake()
    }
    /// Vide le cache des documents ; rend les octets libérés.
    func clearDocumentCache() async throws -> UInt64 {
        let reader = reader
        return try await Task.detached { try reader.clearDocumentCache() }.value
    }
    /// Tous les signets, tous documents confondus.
    func loadAllBookmarks() async -> [StoredBookmarkView] {
        let reader = reader
        return await Task.detached { reader.allBookmarks() }.value
    }
    func removeBookmarkAsync(document: String, sentence: UInt32) async throws {
        let reader = reader
        try await Task.detached { try reader.removeBookmark(document: document, sentence: sentence) }.value
    }
    /// Le dossier de `config.toml` (lecture directe, sans le fil du lecteur).
    var configDirectory: String? { reader.configDir() }
```

`macos/Sources/Settings/SettingsView.swift` (anciens onglets, provisoires) :
- dans `ReadingSettings`, remplacer la création du `Committer` par :

```swift
            let c = Committer<ReadingValues>(saved: values, writeChange: { [model] new, old in
                _ = try await model.updateSettings(ReadingValues.patch(new, over: old))
            }, onResult: { error = $0.map(message(of:)) })
```

- et le `Stepper` d'interlignage : `in: 0...4` devient `in: 0...3` ;
- dans `ReadingValues`, remplacer `static func write(…)` par :

```swift
    /// Le patch des seuls champs que `new` change par rapport à `old`.
    static func patch(_ new: ReadingValues, over old: ReadingValues) -> SettingsPatch {
        var p = SettingsPatch.empty
        if new.lineSpacing != old.lineSpacing { p.lineSpacing = UInt32(new.lineSpacing) }
        if new.ruler != old.ruler { p.readingRuler = new.ruler }
        if new.speed != old.speed { p.speed = Float(new.speed) }
        return p
    }
```

- dans `VoiceSettings`, `.onChange(of: keep)`, remplacer l'appel par :

```swift
                await act {
                    var p = SettingsPatch.empty
                    p.keepVoiceMinutes = new.minutes
                    _ = try await model.updateSettings(p)
                }
```

- [ ] **Étape 9 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 10 : commit**

```bash
git add ffi/src/lib.rs macos/Sources/Settings/SettingsDefaults.swift macos/Sources/Panel/ReadingDisplay.swift macos/Sources/Model/ReaderModel.swift macos/Sources/Panel/PanelController.swift macos/Sources/Settings/SettingsView.swift macos/Tests/CommitterTests.swift macos/Tests/ReadingDisplayTests.swift
git commit -m "$(cat <<'EOF'
feat(ffi): tous les réglages, capacités, aperçu, modèles et données vers Swift

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

# Partie B — L'app

### Tâche 6 : le magasin des réglages

Un seul magasin pour les six onglets : il charge réglages et capacités hors
du fil principal, applique un changement à l'écran tout de suite, l'écrit en
patch après la pause, montre la valeur réellement appliquée, filtre les
réglages que le modèle ignore, et vide la file avant « Écouter » (Point
d'attention 5).

**Fichiers :**
- Modifier : `macos/Sources/Settings/Committer.swift` (valeur appliquée, `flush()`)
- Créer : `macos/Sources/Settings/SettingsDiff.swift`
- Créer : `macos/Sources/Settings/SettingKey.swift`
- Créer : `macos/Sources/Settings/Choices.swift`
- Créer : `macos/Sources/Settings/SettingsStore.swift`
- Modifier : `macos/Tests/Waiting.swift` (ajout de `eventuallyTrue`)
- Modifier : `macos/Tests/CommitterTests.swift` (deux tests)
- Créer : `macos/Tests/FakeSettingsBackend.swift`
- Créer : `macos/Tests/SettingsStoreTests.swift`
- Créer : `macos/Tests/SettingsDiffTests.swift`

**Interfaces :**
- Consomme (Tâche 5) : `SettingsView`, `SettingsPatch` (`.empty`, `isEmpty`), `SamplingFloat`, `SamplingInt`, `SamplingSeed`, `ExtraOption`, `ModelCapabilities`, `VoiceChoice`, `SillonError.Failed(message:)`, `ReaderModel.{loadSettings, loadCapabilities, updateSettings(_:), previewVoice, stopPreview}`.
- Produit :
  - `Committer.init(delay:saved:writeApplied: (Value, Value) async throws -> Value, onApplied: (_ sent: Value, _ applied: Value) -> Void, onResult:)`, `Committer.flush() async`
  - `enum SettingsDiff { static func patch(from: SettingsView, to: SettingsView) -> SettingsPatch }`
  - `enum SettingKey: String { voice, language, speed, pitch, energy, emotion, instruction, temperature, topP, topK, guidance, repetition, steps, seed; func isIgnored(honoured: [String]) -> Bool }`
  - `enum Choices { static func including<T: Hashable>(_ options: [(title: String, value: T)], current: T, title: (T) -> String) -> [(title: String, value: T)] }`
  - `@MainActor protocol SettingsBackend: AnyObject { loadSettings() async -> SettingsView; loadCapabilities() async -> ModelCapabilities; updateSettings(_:) async throws -> SettingsView; previewVoice() async throws; stopPreview() async }` ; `ReaderModel` s'y conforme.
  - `@MainActor @Observable final class SettingsStore` : `init(backend:delay:)`, `values: SettingsView` (lecture), `capabilities: ModelCapabilities`, `loaded: Bool`, `showIgnored: Bool`, `error: String?`, `previewNote: String?`, `load() async`, `reloadCapabilities() async`, `set<T>(_ key: WritableKeyPath<SettingsView, T>, _ value: T)`, `binding<T>(_ key:) -> Binding<T>`, `dragChanged(_ editing: Bool)`, `isIgnored(_: SettingKey) -> Bool`, `isShown(_: SettingKey) -> Bool`, `listen() async`, `stopListening() async`, `static func message(of: Error) -> String`
  - Tests : `func eventuallyTrue(_ timeout: Duration = .seconds(5), _ condition: () -> Bool) async -> Bool`, `final class FakeSettingsBackend`, `enum Capabilities { static let supertonic, magpie }`

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/Waiting.swift`, ajouter à la fin :

```swift
/// Attend, en laissant tourner les tâches Swift, que `condition` soit vraie.
@MainActor
func eventuallyTrue(_ timeout: Duration = .seconds(5), _ condition: () -> Bool) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: timeout)
    while !condition() {
        if clock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return true
}
```

`macos/Tests/CommitterTests.swift`, dans `CommitterTests` :

```swift
    /// La valeur que le moteur a réellement appliquée devient la référence,
    /// et revient à l'appelant avec celle qui avait été envoyée.
    func testTheAppliedValueBecomesTheSavedOne() async {
        var reported: [(sent: Int, applied: Int)] = []
        let c = Committer<Int>(delay: .zero, saved: 0,
                               writeApplied: { new, _ in min(new, 3) },
                               onApplied: { reported.append((sent: $0, applied: $1)) })
        c.change(9)
        let ok = await eventually { !reported.isEmpty }
        XCTAssertTrue(ok)
        XCTAssertEqual(reported.first?.sent, 9)
        XCTAssertEqual(reported.first?.applied, 3)
        XCTAssertEqual(c.saved, 3)
    }

    /// `flush` n'attend pas la pause : ce qui attend part tout de suite.
    func testFlushWritesThePendingValueAtOnce() async {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .seconds(30), saved: 0, write: { writes.append($0) })
        c.change(4)
        await c.flush()
        XCTAssertEqual(writes, [4])
        await c.flush()
        XCTAssertEqual(writes, [4], "rien de plus à écrire")
    }
```

`macos/Tests/FakeSettingsBackend.swift` :

```swift
@testable import Sillon

/// Capacités relevées sur `models::honoured_options`.
enum Capabilities {
    static let supertonic = ModelCapabilities(
        family: "supertonic",
        voices: [VoiceChoice(id: "M1", label: "M1 · masculine 1"),
                 VoiceChoice(id: "F2", label: "F2 · féminine 2")],
        languages: ["fr", "en"],
        honoured: ["voice", "language", "speaking_rate", "seed", "num_inference_steps"],
        requiresReference: false)
    static let magpie = ModelCapabilities(
        family: "magpie_tts",
        voices: [VoiceChoice(id: "Aria", label: "Aria")],
        languages: ["fr", "en", "de"],
        honoured: ["voice", "voice_id", "speaker", "language", "temperature", "top_k",
                   "guidance_scale", "seed"],
        requiresReference: false)
}

/// Un moteur pour les tests : il garde les patchs, borne l'interlignage
/// comme le vrai, et n'écrit rien nulle part.
@MainActor
final class FakeSettingsBackend: SettingsBackend {
    var stored = SettingsView.defaults
    var capabilities = Capabilities.supertonic
    private(set) var patches: [SettingsPatch] = []
    private(set) var log: [String] = []
    /// Message de l'erreur rendue par la prochaine écriture.
    var failNext: String?

    func loadSettings() async -> SettingsView { stored }
    func loadCapabilities() async -> ModelCapabilities { capabilities }

    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView {
        patches.append(patch)
        log.append("patch")
        if let failNext {
            self.failNext = nil
            throw SillonError.Failed(message: failNext)
        }
        if let v = patch.voice { stored.voice = v }
        if let v = patch.speed { stored.speed = v }
        if let v = patch.lineSpacing { stored.lineSpacing = min(v, 3) }
        if let v = patch.codeBlocks { stored.codeBlocks = v }
        return stored
    }

    func previewVoice() async throws { log.append("écoute") }
    func stopPreview() async { log.append("arrêt") }
}
```

`macos/Tests/SettingsStoreTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class SettingsStoreTests: XCTestCase {
    private func loadedStore(_ backend: FakeSettingsBackend,
                             delay: Duration = .milliseconds(20)) async -> SettingsStore {
        let store = SettingsStore(backend: backend, delay: delay)
        await store.load()
        return store
    }

    func testOnlyTheTouchedFieldIsSent() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.set(\.speed, 1.4)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        var expected = SettingsPatch.empty
        expected.speed = 1.4
        XCTAssertEqual(backend.patches, [expected])
    }

    /// Point d'attention 2 : une valeur écrite à la main ne repart jamais
    /// tant qu'on n'y touche pas.
    func testOpeningTheTabRewritesNothing() async throws {
        let backend = FakeSettingsBackend()
        backend.stored.codeBlocks = "lire"
        backend.stored.language = "ja"
        backend.stored.keepVoiceMinutes = 7
        let store = await loadedStore(backend)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
        XCTAssertEqual(store.values.codeBlocks, "lire")
        store.set(\.speed, 1.2)
        _ = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertNil(backend.patches.first?.codeBlocks)
        XCTAssertNil(backend.patches.first?.language)
        XCTAssertNil(backend.patches.first?.keepVoiceMinutes)
    }

    func testNothingIsSentBeforeLoading() async throws {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10))
        store.set(\.speed, 2)
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertTrue(backend.patches.isEmpty)
    }

    /// Supertonic → MagpieTTS : température apparaît, la vitesse disparaît.
    func testIgnoredSettingsFollowTheActiveModel() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        XCTAssertTrue(store.isShown(.speed))
        XCTAssertFalse(store.isShown(.temperature))
        XCTAssertTrue(store.isIgnored(.pitch))
        store.showIgnored = true
        XCTAssertTrue(store.isShown(.temperature), "montré sur demande")
        store.showIgnored = false
        backend.capabilities = Capabilities.magpie
        await store.reloadCapabilities()
        XCTAssertTrue(store.isShown(.temperature))
        XCTAssertTrue(store.isShown(.topK))
        XCTAssertTrue(store.isShown(.guidance))
        XCTAssertFalse(store.isShown(.speed))
    }

    func testTheValueShownIsTheOneApplied() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.set(\.lineSpacing, 4)
        let clamped = await eventuallyTrue { store.values.lineSpacing == 3 }
        XCTAssertTrue(clamped, "ramené dans les bornes et affiché tel qu'appliqué")
    }

    /// Point d'attention 5 : la voix choisie part avant l'aperçu.
    func testListeningSendsThePendingChangeFirst() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend, delay: .seconds(30))
        store.set(\.voice, "F2")
        await store.listen()
        XCTAssertEqual(backend.log, ["patch", "écoute"])
        XCTAssertEqual(backend.patches.last?.voice, "F2")
    }

    func testAFailedWriteKeepsTheValueAndSaysWhy() async {
        let backend = FakeSettingsBackend()
        backend.failNext = "écriture de config.toml impossible"
        let store = await loadedStore(backend)
        store.set(\.speed, 1.4)
        let said = await eventuallyTrue { store.error != nil }
        XCTAssertTrue(said)
        XCTAssertEqual(store.error, "écriture de config.toml impossible")
        XCTAssertEqual(store.values.speed, 1.4, "gardé à l'écran")
    }

    func testNothingLeavesWhileASliderIsDragged() async throws {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.dragChanged(true)
        store.set(\.speed, 1.1)
        store.set(\.speed, 1.3)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
        store.dragChanged(false)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        XCTAssertEqual(backend.patches.first?.speed, 1.3)
    }
}
```

`macos/Tests/SettingsDiffTests.swift` :

```swift
import XCTest
@testable import Sillon

final class SettingsDiffTests: XCTestCase {
    func testNoChangeMakesAnEmptyPatch() {
        XCTAssertTrue(SettingsDiff.patch(from: .defaults, to: .defaults).isEmpty)
    }

    func testOnlyWhatChangesIsInThePatch() {
        var old = SettingsView.defaults
        old.seed = 42
        old.temperature = 0.7
        old.extra = [ExtraOption(key: "a", value: "1"), ExtraOption(key: "b", value: "2")]
        var new = old
        new.topK = 40
        new.seed = nil
        new.temperature = nil
        new.codeBlocks = "skip"
        new.extra = [ExtraOption(key: "a", value: "9"), ExtraOption(key: "c", value: "3")]
        let p = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(p.topK, .value(value: 40))
        XCTAssertEqual(p.seed, .random)
        XCTAssertEqual(p.temperature, .model, "rendu au modèle")
        XCTAssertEqual(p.codeBlocks, "skip")
        XCTAssertEqual(p.extraSet, [ExtraOption(key: "a", value: "9"), ExtraOption(key: "c", value: "3")])
        XCTAssertEqual(p.extraRemove, ["b"])
        XCTAssertNil(p.speed)
        XCTAssertNil(p.voice)
        XCTAssertNil(p.guidanceScale)
    }

    func testEveryScalarFieldIsCovered() {
        var new = SettingsView.defaults
        new.voice = "F1"; new.language = "en"; new.speed = 2; new.pitch = 0.5; new.energy = 1.5
        new.emotion = "calme"; new.instruction = "posé"; new.volume = 0.5; new.engine = "system"
        new.compute = "cpu"; new.threads = 2; new.sessions = 3; new.prefetch = 9
        new.topP = 0.9; new.guidanceScale = 3; new.repetitionPenalty = 1.2; new.inferenceSteps = 12
        new.autoplay = false; new.seekStep = 9; new.lineWidth = 80; new.lineSpacing = 2
        new.readingRuler = false; new.keepVoiceMinutes = 30
        let p = SettingsDiff.patch(from: .defaults, to: new)
        XCTAssertEqual(p.voice, "F1"); XCTAssertEqual(p.language, "en"); XCTAssertEqual(p.speed, 2)
        XCTAssertEqual(p.pitch, 0.5); XCTAssertEqual(p.energy, 1.5); XCTAssertEqual(p.emotion, "calme")
        XCTAssertEqual(p.instruction, "posé"); XCTAssertEqual(p.volume, 0.5); XCTAssertEqual(p.engine, "system")
        XCTAssertEqual(p.compute, "cpu"); XCTAssertEqual(p.threads, 2); XCTAssertEqual(p.sessions, 3)
        XCTAssertEqual(p.prefetch, 9); XCTAssertEqual(p.topP, .value(value: 0.9))
        XCTAssertEqual(p.guidanceScale, .value(value: 3)); XCTAssertEqual(p.repetitionPenalty, .value(value: 1.2))
        XCTAssertEqual(p.inferenceSteps, .value(value: 12)); XCTAssertEqual(p.autoplay, false)
        XCTAssertEqual(p.seekStep, 9); XCTAssertEqual(p.lineWidth, 80); XCTAssertEqual(p.lineSpacing, 2)
        XCTAssertEqual(p.readingRuler, false); XCTAssertEqual(p.keepVoiceMinutes, 30)
    }

    func testAnUnlistedValueStaysInTheMenu() {
        let options: [(title: String, value: String)] = [("Annoncer", "announce"), ("Sauter", "skip")]
        let kept = Choices.including(options, current: "lire") { "\($0) (écrit à la main)" }
        XCTAssertEqual(kept.map(\.value), ["announce", "skip", "lire"])
        XCTAssertEqual(kept.last?.title, "lire (écrit à la main)")
        XCTAssertEqual(Choices.including(options, current: "skip") { $0 }.map(\.value), ["announce", "skip"])
    }

    func testAnUnknownFamilyIgnoresNothing() {
        XCTAssertFalse(SettingKey.temperature.isIgnored(honoured: []))
        XCTAssertTrue(SettingKey.temperature.isIgnored(honoured: ["voice", "speaking_rate"]))
        XCTAssertFalse(SettingKey.speed.isIgnored(honoured: ["voice", "speaking_rate"]))
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: FAIL — compilation : `cannot find type 'SettingsBackend'`,
`cannot find 'SettingsStore'`, `cannot find 'SettingsDiff'`, `extra argument 'writeApplied'`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/Committer.swift`, en entier :

```swift
import Foundation

/// Écrit la dernière valeur d'une rafale de changements, une écriture à la
/// fois : un curseur qu'on glisse ne produit pas une écriture par pas.
@MainActor
final class Committer<Value: Equatable> {
    private let delay: Duration
    /// Reçoit la valeur à écrire et celle que le moteur a déjà ; rend celle
    /// qu'il a réellement appliquée (bornes comprises).
    private let write: (Value, Value) async throws -> Value
    private let onApplied: (_ sent: Value, _ applied: Value) -> Void
    private let onResult: (Error?) -> Void
    private var pending: Task<Void, Never>?
    private var inFlight: Task<Value, Error>?
    /// La dernière valeur demandée, pour `flush`.
    private var latest: Value?
    /// Ce que le moteur a déjà : une valeur identique n'est pas renvoyée.
    private(set) var saved: Value?

    convenience init(delay: Duration = .milliseconds(150), saved: Value? = nil,
                     write: @escaping (Value) async throws -> Void,
                     onResult: @escaping (Error?) -> Void = { _ in }) {
        self.init(delay: delay, saved: saved,
                  writeApplied: { new, _ in try await write(new); return new },
                  onResult: onResult)
    }

    /// `writeChange` reçoit aussi la dernière valeur écrite : il peut
    /// n'envoyer que ce qui a changé.
    convenience init(delay: Duration = .milliseconds(150), saved: Value? = nil,
                     writeChange: @escaping (_ new: Value, _ previous: Value) async throws -> Void,
                     onResult: @escaping (Error?) -> Void = { _ in }) {
        self.init(delay: delay, saved: saved,
                  writeApplied: { new, previous in try await writeChange(new, previous); return new },
                  onResult: onResult)
    }

    init(delay: Duration = .milliseconds(150), saved: Value? = nil,
         writeApplied: @escaping (_ new: Value, _ previous: Value) async throws -> Value,
         onApplied: @escaping (_ sent: Value, _ applied: Value) -> Void = { _, _ in },
         onResult: @escaping (Error?) -> Void = { _ in }) {
        self.delay = delay
        self.saved = saved
        self.write = writeApplied
        self.onApplied = onApplied
        self.onResult = onResult
    }

    /// Fixe la valeur de départ (chargée du moteur) sans rien écrire.
    func prime(_ value: Value) {
        saved = value
        latest = value
    }

    /// `immediate` : sans pause (relâchement d'un curseur).
    func change(_ value: Value, immediate: Bool = false) {
        guard let saved else { return }   // pas encore chargé
        latest = value
        pending?.cancel()
        if value == saved && inFlight == nil { pending = nil; return }
        let wait = immediate ? .zero : delay
        pending = Task {
            if wait > .zero {
                do { try await Task.sleep(for: wait) } catch { return }
            }
            if let inFlight { _ = try? await inFlight.value }
            if Task.isCancelled { return }
            let previous = self.saved ?? value
            let job = Task { try await write(value, previous) }
            inFlight = job
            do {
                let applied = try await job.value
                self.saved = applied
                onApplied(value, applied)
                onResult(nil)
            } catch {
                onResult(error)
            }
            inFlight = nil
        }
    }

    /// Écrit tout de suite ce qui attend sa pause, et rend la main une fois
    /// l'écriture faite : « Écouter » ne doit pas partir avant la voix choisie.
    func flush() async {
        if let latest, let saved, latest != saved {
            change(latest, immediate: true)
        }
        await pending?.value
    }
}
```

`macos/Sources/Settings/SettingsDiff.swift` :

```swift
/// Le patch qui mène d'une valeur des réglages à une autre : seuls les
/// champs changés y figurent. C'est ce qui garantit qu'une valeur écrite à
/// la main, ou changée au clavier entre-temps, n'est pas réécrite.
enum SettingsDiff {
    static func patch(from old: SettingsView, to new: SettingsView) -> SettingsPatch {
        var p = SettingsPatch.empty
        if new.voice != old.voice { p.voice = new.voice }
        if new.language != old.language { p.language = new.language }
        if new.speed != old.speed { p.speed = new.speed }
        if new.pitch != old.pitch { p.pitch = new.pitch }
        if new.energy != old.energy { p.energy = new.energy }
        if new.emotion != old.emotion { p.emotion = new.emotion }
        if new.instruction != old.instruction { p.instruction = new.instruction }
        if new.volume != old.volume { p.volume = new.volume }
        if new.engine != old.engine { p.engine = new.engine }
        if new.compute != old.compute { p.compute = new.compute }
        if new.threads != old.threads { p.threads = new.threads }
        if new.sessions != old.sessions { p.sessions = new.sessions }
        if new.prefetch != old.prefetch { p.prefetch = new.prefetch }
        if new.temperature != old.temperature { p.temperature = float(new.temperature) }
        if new.topP != old.topP { p.topP = float(new.topP) }
        if new.topK != old.topK { p.topK = int(new.topK) }
        if new.guidanceScale != old.guidanceScale { p.guidanceScale = float(new.guidanceScale) }
        if new.repetitionPenalty != old.repetitionPenalty {
            p.repetitionPenalty = float(new.repetitionPenalty)
        }
        if new.inferenceSteps != old.inferenceSteps { p.inferenceSteps = int(new.inferenceSteps) }
        if new.seed != old.seed { p.seed = new.seed.map { .value(value: $0) } ?? .random }
        if new.codeBlocks != old.codeBlocks { p.codeBlocks = new.codeBlocks }
        if new.autoplay != old.autoplay { p.autoplay = new.autoplay }
        if new.seekStep != old.seekStep { p.seekStep = new.seekStep }
        if new.lineWidth != old.lineWidth { p.lineWidth = new.lineWidth }
        if new.lineSpacing != old.lineSpacing { p.lineSpacing = new.lineSpacing }
        if new.readingRuler != old.readingRuler { p.readingRuler = new.readingRuler }
        if new.keepVoiceMinutes != old.keepVoiceMinutes { p.keepVoiceMinutes = new.keepVoiceMinutes }
        let before = Dictionary(old.extra.map { ($0.key, $0.value) }, uniquingKeysWith: { $1 })
        let after = Dictionary(new.extra.map { ($0.key, $0.value) }, uniquingKeysWith: { $1 })
        p.extraSet = new.extra.filter { before[$0.key] != $0.value }
        p.extraRemove = old.extra.map(\.key).filter { after[$0] == nil }
        return p
    }

    private static func float(_ v: Float?) -> SamplingFloat { v.map { .value(value: $0) } ?? .model }
    private static func int(_ v: UInt32?) -> SamplingInt { v.map { .value(value: $0) } ?? .model }
}
```

`macos/Sources/Settings/SettingKey.swift` :

```swift
/// Les réglages qu'un modèle peut ignorer, sous le nom que leur donne le
/// moteur (`models::honoured_options`).
enum SettingKey: String, CaseIterable {
    case voice, language
    case speed = "speaking_rate"
    case pitch = "pitch_shift"
    case energy, emotion, instruction, temperature
    case topP = "top_p"
    case topK = "top_k"
    case guidance = "guidance_scale"
    case repetition = "repetition_penalty"
    case steps = "num_inference_steps"
    case seed

    /// Une liste vide : famille inconnue, rien n'est affirmé, tout compte.
    func isIgnored(honoured: [String]) -> Bool {
        !honoured.isEmpty && !honoured.contains(rawValue)
    }
}
```

`macos/Sources/Settings/Choices.swift` :

```swift
/// Les options d'un menu, plus la valeur en place si elle n'y est pas.
///
/// Une valeur écrite à la main dans `config.toml` (« lire », « ja » sur
/// MagpieTTS, « cuda ») s'affiche telle quelle : le menu ne la remplace pas
/// par sa première option, et rien n'est réécrit tant qu'on n'y touche pas.
enum Choices {
    static func including<T: Hashable>(_ options: [(title: String, value: T)], current: T,
                                      title: (T) -> String) -> [(title: String, value: T)] {
        options.contains { $0.value == current } ? options : options + [(title: title(current), value: current)]
    }
}
```

`macos/Sources/Settings/SettingsStore.swift` :

```swift
import SwiftUI

/// Ce que les onglets demandent au moteur. `ReaderModel` le fournit ; les
/// tests le simulent sans toucher au disque.
@MainActor
protocol SettingsBackend: AnyObject {
    func loadSettings() async -> SettingsView
    func loadCapabilities() async -> ModelCapabilities
    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView
    func previewVoice() async throws
    func stopPreview() async
}

extension ReaderModel: SettingsBackend {}

/// Les réglages tels que les onglets les montrent et les changent.
///
/// Un seul magasin pour les six onglets. Un changement s'affiche tout de
/// suite, part en patch (le seul champ touché) après une courte pause, et la
/// valeur réellement appliquée — bornes comprises — revient à l'écran.
@MainActor
@Observable
final class SettingsStore {
    @ObservationIgnored private let backend: SettingsBackend
    @ObservationIgnored private var committer: Committer<SettingsView>!
    @ObservationIgnored private var dragging = false

    private(set) var values = SettingsView.defaults
    private(set) var capabilities = ModelCapabilities(family: "", voices: [], languages: [],
                                                      honoured: [], requiresReference: false)
    private(set) var loaded = false
    /// Afficher aussi les réglages que le modèle actif ignore.
    var showIgnored = false
    /// La dernière écriture refusée ; le réglage reste affiché.
    private(set) var error: String?
    /// Le refus du dernier « Écouter ».
    private(set) var previewNote: String?

    init(backend: SettingsBackend, delay: Duration = .milliseconds(150)) {
        self.backend = backend
        committer = Committer(delay: delay, writeApplied: { [backend] new, old in
            try await backend.updateSettings(SettingsDiff.patch(from: old, to: new))
        }, onApplied: { [weak self] sent, applied in
            // Une valeur changée depuis l'envoi reste : elle partira à son tour.
            guard let self, self.values == sent else { return }
            self.values = applied
        }, onResult: { [weak self] error in
            self?.error = error.map(SettingsStore.message(of:))
        })
    }

    /// Réglages et capacités, lus hors du fil principal. Rien n'est écrit.
    func load() async {
        let loadedValues = await backend.loadSettings()
        capabilities = await backend.loadCapabilities()
        committer.prime(loadedValues)
        values = loadedValues
        loaded = true
    }

    func reloadCapabilities() async {
        capabilities = await backend.loadCapabilities()
    }

    /// Change un réglage : l'écran suit tout de suite, l'écriture après la pause.
    func set<T>(_ key: WritableKeyPath<SettingsView, T>, _ value: T) {
        values[keyPath: key] = value
        guard loaded, !dragging else { return }
        committer.change(values)
    }

    func binding<T>(_ key: WritableKeyPath<SettingsView, T>) -> Binding<T> {
        Binding(get: { self.values[keyPath: key] }, set: { self.set(key, $0) })
    }

    /// Un curseur qu'on glisse relance la synthèse : rien ne part avant le
    /// relâchement, puis tout de suite.
    func dragChanged(_ editing: Bool) {
        dragging = editing
        if !editing, loaded { committer.change(values, immediate: true) }
    }

    func isIgnored(_ key: SettingKey) -> Bool { key.isIgnored(honoured: capabilities.honoured) }

    func isShown(_ key: SettingKey) -> Bool { showIgnored || !isIgnored(key) }

    /// La phrase d'exemple, avec ce qui vient d'être choisi.
    func listen() async {
        previewNote = nil
        await committer.flush()
        do { try await backend.previewVoice() } catch { previewNote = Self.message(of: error) }
    }

    func stopListening() async { await backend.stopPreview() }

    /// Le message d'une erreur du moteur, sans l'habillage de l'enum.
    static func message(of error: Error) -> String {
        if case SillonError.Failed(let message) = error { return message }
        return error.localizedDescription
    }
}
```

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Settings/Committer.swift macos/Sources/Settings/SettingsDiff.swift macos/Sources/Settings/SettingKey.swift macos/Sources/Settings/Choices.swift macos/Sources/Settings/SettingsStore.swift macos/Tests/Waiting.swift macos/Tests/CommitterTests.swift macos/Tests/FakeSettingsBackend.swift macos/Tests/SettingsStoreTests.swift macos/Tests/SettingsDiffTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): un magasin des réglages, écrit par patch, filtré par modèle

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 7 : la fenêtre à onglets et l'onglet Voix

La fenêtre passe à une `TabView` liée à l'onglet courant, retenu d'une
ouverture à l'autre (`⌘,` rouvre le dernier) ; `SettingsOpener` sait ouvrir
un onglet précis. L'onglet Voix arrive : modèle actif, voix du modèle avec
« Écouter » (et `Espace` sur la liste), langue du modèle, vitesse, volume,
voix clonée quand il le faut, réglages ignorés montrés sur demande. Les
anciens onglets restent, provisoirement, jusqu'aux Tâches 8 à 10.

**Fichiers :**
- Créer : `macos/Sources/Settings/SettingsTab.swift`
- Modifier : `macos/Sources/Settings/SettingsOpener.swift`
- Créer : `macos/Sources/Settings/ReglagesView.swift`
- Modifier : `macos/Sources/Settings/SettingsView.swift` (retirer `ReglagesView`, rendre les anciens onglets internes)
- Créer : `macos/Sources/Settings/SettingControls.swift`
- Créer : `macos/Sources/Settings/VoiceTab.swift`
- Modifier : `macos/Tests/SettingsOpenerTests.swift` (deux tests)
- Créer : `macos/Tests/Samples.swift`
- Créer : `macos/Tests/VoiceTabTests.swift`

**Interfaces :**
- Consomme : `SettingsStore` (Tâche 6), `ReaderModel.{loadVoices, useVoiceAsync, refresh, snapshot}` (Tâche 5), `Snapshot.preview`, `VoiceState`, `PopUpPicker`, `Choices`.
- Produit :
  - `enum SettingsTab: String, CaseIterable { voice, reading, models, shortcuts, advanced, general; var label: String; var symbol: String }`
  - `@MainActor @Observable final class SettingsNavigation { static let shared; static let key = "reglages.onglet"; init(defaults: UserDefaults = .standard); private(set) var tab: SettingsTab; func select(_:); var binding: Binding<SettingsTab> }`
  - `extension View { func settingsTab(_ tab: SettingsTab) -> some View }`
  - `SettingsOpener.init(activate:navigation: SettingsNavigation? = nil)`, `SettingsOpener.open(tab: SettingsTab? = nil)`
  - `struct ReglagesView` (fenêtre ; même initialiseur `init(model:)`)
  - `struct IgnorableNote<Content: View>`, `enum SettingFormat { static func decimal(_ v: Float) -> String; static func percent(_ v: Float) -> String }`, `enum LanguageName { static func display(_ code: String) -> String }`
  - `struct VoiceTab: View` (`model`, `store`) avec `static func modelOptions(_:) -> [(title: String, value: String)]`, `static func languageOptions(_:current:)`, `static func voiceLine(_: VoiceState) -> String?`, `static func previewLine(_: PreviewView, refusal: String?) -> String?`, `static func unlistedVoice(_:_:) -> String?`, `static func needsReference(_: ModelCapabilities) -> Bool`, `static let emotions`
  - Tests : `extension VoiceModel { static func sample(id:name:bytes:installed:active:) -> VoiceModel }`

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/SettingsOpenerTests.swift`, dans `SettingsOpenerTests` (les
tests existants restent tels quels) :

```swift
    /// Des préférences à part, vidées : les tests ne touchent pas à celles
    /// de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.reglages"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    /// L'écrou du panneau ouvre l'onglet Voix ; ⌘, rouvre le dernier.
    func testOpeningOnATabSelectsItAndPlainOpeningKeepsTheLast() {
        let nav = SettingsNavigation(defaults: freshDefaults())
        let opener = SettingsOpener(activate: {}, navigation: nav)
        opener.install {}
        nav.select(.advanced)
        opener.open()
        XCTAssertEqual(nav.tab, .advanced, "⌘, : le dernier onglet")
        opener.open(tab: .voice)
        XCTAssertEqual(nav.tab, .voice)
    }

    func testTheLastTabSurvivesARelaunch() {
        let defaults = freshDefaults()
        SettingsNavigation(defaults: defaults).select(.models)
        XCTAssertEqual(SettingsNavigation(defaults: defaults).tab, .models)
        defaults.set("inconnu", forKey: SettingsNavigation.key)
        XCTAssertEqual(SettingsNavigation(defaults: defaults).tab, .voice, "valeur illisible : l'onglet Voix")
    }
```

`macos/Tests/Samples.swift` :

```swift
@testable import Sillon

extension VoiceModel {
    /// Un paquet de catalogue pour les tests.
    static func sample(id: String, name: String = "Modèle", bytes: UInt64 = 300_000_000,
                       installed: Bool = true, active: Bool = false) -> VoiceModel {
        VoiceModel(id: id, name: name, bytes: bytes, installed: installed, active: active,
                   recommended: false, summary: "Rapide et léger.", family: "supertonic",
                   precision: "f16", languagesLabel: "31 langues, dont le français",
                   languages: ["fr", "en"], pace: "Lecture fluide.")
    }
}
```

`macos/Tests/VoiceTabTests.swift` :

```swift
import XCTest
@testable import Sillon

final class VoiceTabTests: XCTestCase {
    func testWithoutAnActiveModelTheSystemVoiceIsListedFirst() {
        let none = VoiceTab.modelOptions([.sample(id: "a", name: "A")])
        XCTAssertEqual(none.map(\.value), ["", "a"])
        XCTAssertEqual(none.first?.title, "Voix du système")
        let active = VoiceTab.modelOptions([.sample(id: "a", name: "A", active: true)])
        XCTAssertEqual(active.map(\.value), ["a"])
    }

    func testLanguagesAreThoseOfTheModelPlusTheOneInPlace() {
        let options = VoiceTab.languageOptions(["fr", "en"], current: "ja")
        XCTAssertEqual(options.map(\.value), ["fr", "en", "ja"])
        XCTAssertEqual(options[0].title, "Français")
        XCTAssertEqual(options[1].title, "Anglais")
        XCTAssertTrue(options[2].title.hasSuffix("(hors liste)"))
        XCTAssertEqual(VoiceTab.languageOptions(["fr"], current: "fr").count, 1)
    }

    func testTheVoiceStateLine() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading), "Voix en chargement…")
        XCTAssertEqual(VoiceTab.voiceLine(.fallback(reason: "modèle absent")),
                       "Voix du système : modèle absent")
        XCTAssertNil(VoiceTab.voiceLine(.ready))
    }

    func testThePreviewLineSaysWhyFirst() {
        let idle = PreviewView(pending: false, playing: false, error: nil)
        XCTAssertNil(VoiceTab.previewLine(idle, refusal: nil))
        XCTAssertEqual(VoiceTab.previewLine(idle, refusal: "sortie audio indisponible"),
                       "sortie audio indisponible")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: true, playing: false, error: nil), refusal: nil),
                       "Synthèse de l'aperçu…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: true, error: nil), refusal: nil),
                       "Écoute…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: false, error: "échec"), refusal: nil),
                       "échec")
    }

    func testAVoiceOutsideTheListIsShownAsIs() {
        let voices = Capabilities.supertonic.voices
        XCTAssertNil(VoiceTab.unlistedVoice("", voices))
        XCTAssertNil(VoiceTab.unlistedVoice("M1", voices))
        XCTAssertEqual(VoiceTab.unlistedVoice("Aria", voices), "Aria")
    }

    func testAClonedVoiceIsAskedForOnlyWhenNeeded() {
        XCTAssertFalse(VoiceTab.needsReference(Capabilities.supertonic))
        let cloning = ModelCapabilities(family: "chatterbox", voices: [], languages: ["en"],
                                        honoured: [], requiresReference: false)
        XCTAssertTrue(VoiceTab.needsReference(cloning), "aucune voix intégrée")
        let system = ModelCapabilities(family: "", voices: [], languages: [], honoured: [],
                                       requiresReference: false)
        XCTAssertFalse(VoiceTab.needsReference(system), "voix du système")
    }

    func testNumbersAreWrittenTheFrenchWay() {
        XCTAssertEqual(SettingFormat.decimal(1.25), "1,25")
        XCTAssertEqual(SettingFormat.decimal(1), "1")
        XCTAssertEqual(SettingFormat.percent(0.6), "60 %")
        XCTAssertEqual(LanguageName.display("de"), "Allemand")
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: FAIL — compilation : `cannot find 'SettingsNavigation'`,
`cannot find 'VoiceTab'`, `extra argument 'navigation' in call`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/SettingsTab.swift` :

```swift
import SwiftUI

/// Les six onglets de la fenêtre des réglages.
enum SettingsTab: String, CaseIterable {
    case voice = "voix"
    case reading = "lecture"
    case models = "modeles"
    case shortcuts = "raccourcis"
    case advanced = "avance"
    case general = "general"

    var label: String {
        switch self {
        case .voice: "Voix"
        case .reading: "Lecture"
        case .models: "Modèles"
        case .shortcuts: "Raccourcis"
        case .advanced: "Avancé"
        case .general: "Général"
        }
    }

    var symbol: String {
        switch self {
        case .voice: "waveform"
        case .reading: "text.alignleft"
        case .models: "square.and.arrow.down"
        case .shortcuts: "keyboard"
        case .advanced: "slider.horizontal.3"
        case .general: "gearshape"
        }
    }
}

/// L'onglet affiché, retenu d'une ouverture et d'un lancement à l'autre :
/// `⌘,` rouvre le dernier, l'écrou du panneau choisit Voix.
@MainActor
@Observable
final class SettingsNavigation {
    static let shared = SettingsNavigation()
    static let key = "reglages.onglet"

    @ObservationIgnored private let defaults: UserDefaults
    private(set) var tab: SettingsTab

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        tab = defaults.string(forKey: Self.key).flatMap(SettingsTab.init(rawValue:)) ?? .voice
    }

    func select(_ tab: SettingsTab) {
        self.tab = tab
        defaults.set(tab.rawValue, forKey: Self.key)
    }

    var binding: Binding<SettingsTab> {
        Binding(get: { self.tab }, set: { self.select($0) })
    }
}

extension View {
    /// L'étiquette et l'étiquette de sélection d'un onglet.
    func settingsTab(_ tab: SettingsTab) -> some View {
        tabItem { Label(tab.label, systemImage: tab.symbol) }.tag(tab)
    }
}
```

`macos/Sources/Settings/SettingsOpener.swift` : remplacer la classe
`SettingsOpener` par :

```swift
@MainActor
final class SettingsOpener {
    static let shared = SettingsOpener()

    private let activate: () -> Void
    private let navigation: SettingsNavigation
    private var action: (() -> Void)?
    private var pending = false

    init(activate: @escaping () -> Void = { NSApp.activate() },
         navigation: SettingsNavigation? = nil) {
        self.activate = activate
        self.navigation = navigation ?? .shared
    }

    /// L'action `openSettings` d'une vue ; la première seule est gardée.
    func install(_ action: @escaping () -> Void) {
        guard self.action == nil else { return }
        self.action = action
        if pending {
            pending = false
            action()
        }
    }

    /// Une app de barre des menus n'est pas au premier plan : on l'y met
    /// d'abord, sinon la fenêtre s'ouvrirait derrière l'application active.
    /// Sans onglet, la fenêtre rouvre le dernier utilisé.
    func open(tab: SettingsTab? = nil) {
        if let tab { navigation.select(tab) }
        activate()
        if let action { action() } else { pending = true }
    }
}
```

`macos/Sources/Settings/SettingsView.swift` : supprimer la déclaration
`struct ReglagesView: View { … }` (lignes 4 à 19, avec son commentaire), et
retirer `private` devant `struct ReadingSettings`, `struct ShortcutSettings`,
`struct VoiceSettings` et `struct GeneralSettings`.

`macos/Sources/Settings/ReglagesView.swift` :

```swift
import SwiftUI

/// La fenêtre des réglages : six onglets et un magasin de réglages commun.
/// Le nom `SettingsView` est pris par l'enregistrement du moteur (UniFFI).
struct ReglagesView: View {
    let model: ReaderModel
    @State private var store: SettingsStore

    init(model: ReaderModel) {
        self.model = model
        _store = State(initialValue: SettingsStore(backend: model))
    }

    var body: some View {
        TabView(selection: SettingsNavigation.shared.binding) {
            VoiceTab(model: model, store: store).settingsTab(.voice)
            ReadingSettings(model: model).settingsTab(.reading)
            VoiceSettings(model: model).settingsTab(.models)
            ShortcutSettings().settingsTab(.shortcuts)
            GeneralSettings().settingsTab(.general)
        }
        .navigationTitle("Réglages")
        .frame(width: 600, height: 540)
        .font(.system(size: 13))
        .task { await store.load() }
        // Panneau masqué, l'état de la voix (chargement, repli, aperçu) n'est
        // plus relu : la fenêtre le relit à 4 Hz tant qu'elle est ouverte. Un
        // instantané se lit sous verrou, sans passer par le fil du lecteur.
        .task {
            while !Task.isCancelled {
                model.refresh()
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }
}
```

`macos/Sources/Settings/SettingControls.swift` :

```swift
import Foundation
import SwiftUI

/// Un réglage que le modèle actif ignore, montré sur demande : il le dit.
struct IgnorableNote<Content: View>: View {
    let ignored: Bool
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            content
            if ignored {
                Text("sans effet sur ce modèle")
            }
        }
    }
}

/// Nombres à la française, à l'écran comme pour VoiceOver.
enum SettingFormat {
    /// Deux décimales au plus, virgule : « 1,25 », « 1 ».
    static func decimal(_ v: Float) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = 2
        return f.string(from: NSNumber(value: v)) ?? "\(v)"
    }

    static func percent(_ v: Float) -> String { "\(Int((v * 100).rounded())) %" }
}

/// Le nom français d'une langue : « fr » → « Français ».
enum LanguageName {
    private static let french = Locale(identifier: "fr_FR")

    static func display(_ code: String) -> String {
        guard let name = french.localizedString(forLanguageCode: code), name != code else { return code }
        return name.prefix(1).uppercased() + name.dropFirst()
    }
}
```

`macos/Sources/Settings/VoiceTab.swift` :

```swift
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Onglet Voix : modèle actif, voix et « Écouter », langue, vitesse, volume,
/// voix clonée quand le modèle l'exige.
struct VoiceTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var installed: [VoiceModel] = []
    @State private var modelError: String?

    var body: some View {
        Form {
            Section("Modèle") {
                LabeledContent("Modèle actif") {
                    PopUpPicker(label: "Modèle actif", identifier: "modele-actif",
                                options: Self.modelOptions(installed), selection: activeModel)
                        .fixedSize()
                }
                if let line = Self.voiceLine(model.snapshot.voice) {
                    Text(line).accessibilityIdentifier("etat-voix")
                }
                if case .fallback = model.snapshot.voice {
                    Button("Voir les modèles") { SettingsNavigation.shared.select(.models) }
                }
                if let modelError { Text(modelError).foregroundStyle(.red) }
            }
            Section("Voix") {
                voicePicker
                HStack {
                    Button("Écouter") { Task { await store.listen() } }
                        .accessibilityIdentifier("ecouter")
                    Button("Arrêter") { Task { await store.stopListening() } }
                        .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                    if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                        Text(line).accessibilityIdentifier("etat-ecoute")
                    }
                }
                if store.isShown(.language) {
                    IgnorableNote(ignored: store.isIgnored(.language)) {
                        LabeledContent("Langue") {
                            PopUpPicker(label: "Langue", identifier: "langue",
                                        options: Self.languageOptions(store.capabilities.languages,
                                                                      current: store.values.language),
                                        selection: store.binding(\.language))
                                .fixedSize()
                        }
                    }
                }
                if store.isShown(.speed) {
                    IgnorableNote(ignored: store.isIgnored(.speed)) {
                        Slider(value: store.binding(\.speed), in: 0.5...3, step: 0.05) {
                            Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                        } onEditingChanged: { store.dragChanged($0) }
                        .accessibilityLabel("Vitesse de lecture")
                        .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
                    }
                }
                Slider(value: store.binding(\.volume), in: 0...2, step: 0.05) {
                    Text("Volume : \(SettingFormat.percent(store.values.volume))")
                }
                .accessibilityLabel("Volume")
                .accessibilityValue(SettingFormat.percent(store.values.volume))
            }
            if [SettingKey.pitch, .energy, .emotion, .instruction].contains(where: { store.isShown($0) }) {
                Section("Expression") { expression }
            }
            Toggle("Afficher aussi les réglages ignorés par ce modèle", isOn: $store.showIgnored)
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-voix")
        .task { installed = await model.loadVoices().filter(\.installed) }
    }

    @ViewBuilder private var voicePicker: some View {
        let voices = store.capabilities.voices
        if !voices.isEmpty {
            List(voices, id: \.id, selection: voiceSelection) { voice in
                Text(voice.label).tag(voice.id)
            }
            .frame(height: 150)
            .accessibilityLabel("Voix du modèle")
            .accessibilityIdentifier("liste-voix")
            // Espace fait entendre la voix choisie, comme « Écouter ».
            .onKeyPress(.space) {
                Task { await store.listen() }
                return .handled
            }
            if let unlisted = Self.unlistedVoice(store.values.voice, voices) {
                Text("Voix en place : \(unlisted) (hors liste)")
            }
        }
        if Self.needsReference(store.capabilities) {
            LabeledContent("Voix clonée") {
                HStack {
                    Text(store.values.voice.isEmpty
                         ? "Aucun fichier"
                         : URL(fileURLWithPath: store.values.voice).lastPathComponent)
                    Button("Choisir un fichier WAV…") {
                        if let path = Self.chooseWav() { store.set(\.voice, path) }
                    }
                }
            }
        }
    }

    @ViewBuilder private var expression: some View {
        if store.isShown(.pitch) {
            IgnorableNote(ignored: store.isIgnored(.pitch)) {
                Slider(value: store.binding(\.pitch), in: -1...1, step: 0.05) {
                    Text("Hauteur : \(SettingFormat.decimal(store.values.pitch))")
                }
                .accessibilityLabel("Hauteur")
                .accessibilityValue(SettingFormat.decimal(store.values.pitch))
            }
        }
        if store.isShown(.energy) {
            IgnorableNote(ignored: store.isIgnored(.energy)) {
                Slider(value: store.binding(\.energy), in: 0...2, step: 0.05) {
                    Text("Énergie : \(SettingFormat.decimal(store.values.energy))")
                }
                .accessibilityLabel("Énergie")
                .accessibilityValue(SettingFormat.decimal(store.values.energy))
            }
        }
        if store.isShown(.emotion) {
            IgnorableNote(ignored: store.isIgnored(.emotion)) {
                LabeledContent("Intonation") {
                    PopUpPicker(label: "Intonation", identifier: "intonation",
                                options: Choices.including(Self.emotions, current: store.values.emotion) { $0 },
                                selection: store.binding(\.emotion))
                        .fixedSize()
                }
            }
        }
        if store.isShown(.instruction) {
            IgnorableNote(ignored: store.isIgnored(.instruction)) {
                TextField("Consigne de style", text: store.binding(\.instruction))
                    .accessibilityLabel("Consigne de style")
            }
        }
    }

    private var voiceSelection: Binding<String?> {
        Binding(get: { store.values.voice.isEmpty ? store.capabilities.voices.first?.id : store.values.voice },
                set: { id in
                    guard let id, id != store.values.voice else { return }
                    store.set(\.voice, id)
                })
    }

    private var activeModel: Binding<String> {
        Binding(get: { installed.first(where: \.active)?.id ?? "" },
                set: { id in
                    guard !id.isEmpty else { return }
                    Task {
                        do {
                            try await model.useVoiceAsync(id)
                            modelError = nil
                        } catch {
                            modelError = SettingsStore.message(of: error)
                        }
                        installed = await model.loadVoices().filter(\.installed)
                        // Le modèle change : voix, langues et réglages honorés aussi.
                        await store.load()
                    }
                })
    }

    static let emotions: [(title: String, value: String)] = [
        ("Aucune", ""), ("Neutre", "neutre"), ("Calme", "calme"), ("Chaleureux", "chaleureux"),
        ("Sérieux", "sérieux"), ("Enjoué", "enjoué"), ("Posé", "posé"),
    ]

    /// Les modèles installés ; sans modèle actif, la voix du système d'abord.
    static func modelOptions(_ installed: [VoiceModel]) -> [(title: String, value: String)] {
        let models = installed.map { (title: $0.name, value: $0.id) }
        return installed.contains(where: \.active) ? models : [(title: "Voix du système", value: "")] + models
    }

    static func languageOptions(_ codes: [String], current: String) -> [(title: String, value: String)] {
        Choices.including(codes.map { (title: LanguageName.display($0), value: $0) }, current: current) {
            "\(LanguageName.display($0)) (hors liste)"
        }
    }

    static func voiceLine(_ state: VoiceState) -> String? {
        switch state {
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système : \(reason)"
        default: nil
        }
    }

    static func previewLine(_ preview: PreviewView, refusal: String?) -> String? {
        if let refusal { return refusal }
        if let error = preview.error { return error }
        if preview.pending { return "Synthèse de l'aperçu…" }
        if preview.playing { return "Écoute…" }
        return nil
    }

    static func unlistedVoice(_ voice: String, _ voices: [VoiceChoice]) -> String? {
        voice.isEmpty || voices.contains(where: { $0.id == voice }) ? nil : voice
    }

    /// Un modèle qui clone, ou qui n'a aucune voix intégrée.
    static func needsReference(_ c: ModelCapabilities) -> Bool {
        c.requiresReference || (c.voices.isEmpty && !c.family.isEmpty)
    }

    @MainActor
    static func chooseWav() -> String? {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.wav]
        panel.allowsMultipleSelection = false
        panel.prompt = "Choisir"
        return panel.runModal() == .OK ? panel.url?.path : nil
    }
}
```

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : vérifier à la main**

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build build && open build/Build/Products/Debug/Sillon.app`, puis `⌘,` depuis le panneau.
Expected: la fenêtre s'ouvre sur l'onglet Voix ; choisir une autre voix puis
« Écouter » fait entendre la voix choisie ; la langue ne propose que celles
du modèle ; `⌘W` puis `⌘,` rouvre le dernier onglet choisi.

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/Settings/SettingsTab.swift macos/Sources/Settings/SettingsOpener.swift macos/Sources/Settings/ReglagesView.swift macos/Sources/Settings/SettingsView.swift macos/Sources/Settings/SettingControls.swift macos/Sources/Settings/VoiceTab.swift macos/Tests/SettingsOpenerTests.swift macos/Tests/Samples.swift macos/Tests/VoiceTabTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): la fenêtre à onglets retient le dernier, et l'onglet Voix

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 8 : l'onglet Lecture

Police, taille, interlignage (0 à 3), règle de lecture, blocs de code
(annoncer / sauter / lire, valeur écrite à la main gardée), lecture dès
l'ouverture, pas de l'avance rapide (1 à 50). La vitesse a rejoint l'onglet
Voix. L'ancien onglet et `ReadingValues` disparaissent.

**Fichiers :**
- Créer : `macos/Sources/Settings/ReadingTab.swift`
- Modifier : `macos/Sources/Settings/ReglagesView.swift`
- Modifier : `macos/Sources/Settings/SettingsView.swift` (supprimer `ReadingSettings` et `ReadingValues`)
- Modifier : `macos/Tests/CommitterTests.swift` (supprimer `ReadingValuesPatchTests`)
- Créer : `macos/Tests/ReadingTabTests.swift`

**Interfaces :**
- Consomme : `SettingsStore.{values, binding(_:), error}`, `Choices.including`, `PopUpPicker`, `DisplayDefaults.{fontKey, sizeKey, defaultSize}`, `ReadingFont`.
- Produit : `struct ReadingTab: View` (`store`), `static func codeOptions(current: String) -> [(title: String, value: String)]`.

- [ ] **Étape 1 : écrire le test qui échoue**

`macos/Tests/ReadingTabTests.swift` :

```swift
import XCTest
@testable import Sillon

final class ReadingTabTests: XCTestCase {
    func testTheThreeCodePoliciesAreNamedInFrench() {
        let options = ReadingTab.codeOptions(current: "announce")
        XCTAssertEqual(options.map(\.value), ["announce", "skip", "read"])
        XCTAssertEqual(options.map(\.title), ["Annoncer", "Sauter", "Lire"])
    }

    /// `code_blocks = "lire"`, accepté par le moteur, s'affiche tel quel.
    func testAHandWrittenPolicyStaysInTheMenu() {
        let options = ReadingTab.codeOptions(current: "lire")
        XCTAssertEqual(options.map(\.value), ["announce", "skip", "read", "lire"])
        XCTAssertEqual(options.last?.title, "lire (écrit à la main)")
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/ReadingTabTests`
Expected: FAIL — `cannot find 'ReadingTab' in scope`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/ReadingTab.swift` :

```swift
import SwiftUI

/// Onglet Lecture : l'affichage du texte et la façon de le parcourir.
struct ReadingTab: View {
    @Bindable var store: SettingsStore
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize

    var body: some View {
        Form {
            Section("Texte") {
                LabeledContent("Police de lecture") {
                    PopUpPicker(label: "Police de lecture", identifier: "police-lecture",
                                options: ReadingFont.allCases.map { ($0.label, $0.rawValue) },
                                selection: $fontName)
                        .fixedSize()
                }
                Slider(value: $size, in: 14...40, step: 1) { Text("Taille : \(Int(size)) pt") }
                    .accessibilityLabel("Taille du texte")
                    .accessibilityValue("\(Int(size)) points")
                Stepper("Interlignage : \(store.values.lineSpacing)",
                        value: store.binding(\.lineSpacing), in: 0...3)
                    .accessibilityLabel("Interlignage")
                    .accessibilityValue("\(store.values.lineSpacing)")
                Toggle("Règle de lecture", isOn: store.binding(\.readingRuler))
            }
            Section("Lecture") {
                LabeledContent("Blocs de code") {
                    PopUpPicker(label: "Blocs de code", identifier: "blocs-code",
                                options: Self.codeOptions(current: store.values.codeBlocks),
                                selection: store.binding(\.codeBlocks))
                        .fixedSize()
                }
                Toggle("Lire dès l'ouverture", isOn: store.binding(\.autoplay))
                Stepper("Pas de l'avance rapide : \(store.values.seekStep) phrases",
                        value: store.binding(\.seekStep), in: 1...50)
                    .accessibilityLabel("Pas de l'avance rapide")
                    .accessibilityValue("\(store.values.seekStep) phrases")
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-lecture")
    }

    static func codeOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Annoncer", "announce"), ("Sauter", "skip"), ("Lire", "read")],
                          current: current) { "\($0) (écrit à la main)" }
    }
}
```

`macos/Sources/Settings/ReglagesView.swift` : la ligne
`ReadingSettings(model: model).settingsTab(.reading)` devient
`ReadingTab(store: store).settingsTab(.reading)`.

`macos/Sources/Settings/SettingsView.swift` : supprimer `struct ReadingSettings`
et `struct ReadingValues` en entier.

`macos/Tests/CommitterTests.swift` : supprimer la classe
`ReadingValuesPatchTests` et son commentaire.

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Settings/ReadingTab.swift macos/Sources/Settings/ReglagesView.swift macos/Sources/Settings/SettingsView.swift macos/Tests/CommitterTests.swift macos/Tests/ReadingTabTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): l'onglet Lecture — blocs de code, lecture à l'ouverture, pas d'avance rapide

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 9 : les onglets Général et Raccourcis

Général : ouverture à la connexion, durée de chargement de la voix (une
durée écrite à la main, 7 minutes, s'affiche telle quelle), et les données —
vider le cache des documents, gérer les signets, ouvrir le dossier de
configuration. Raccourcis passe dans son propre fichier, inchangé.

**Fichiers :**
- Créer : `macos/Sources/Settings/GeneralTab.swift`
- Créer : `macos/Sources/Settings/ShortcutsTab.swift`
- Modifier : `macos/Sources/Settings/KeepVoiceChoice.swift`
- Modifier : `macos/Sources/Settings/ReglagesView.swift`
- Modifier : `macos/Sources/Settings/SettingsView.swift` (supprimer `ShortcutSettings`, `GeneralSettings`, et le choix « Garder la voix chargée » de `VoiceSettings`)
- Modifier : `macos/Tests/KeepVoiceChoiceTests.swift`
- Créer : `macos/Tests/GeneralTabTests.swift`

**Interfaces :**
- Consomme : `SettingsStore.{values, set(_:_:), error}`, `ReaderModel.{clearDocumentCache, loadAllBookmarks, removeBookmarkAsync(document:sentence:), configDirectory}` (Tâche 5), `StoredBookmarkView`, `LoginItem.{isEnabled, set(_:)}`.
- Produit :
  - `KeepVoiceChoice.init(minutes:)` garde une durée hors liste (`.minutes(7)`), `static func options(stored: Int64) -> [KeepVoiceChoice]` ; `needsSave` disparaît.
  - `struct BookmarkGroup: Equatable { let document: String; let label: String; let items: [StoredBookmarkView] }`, `enum BookmarkGroups { static func group(_: [StoredBookmarkView]) -> [BookmarkGroup] }`
  - `enum CacheReport { static func text(freed: UInt64) -> String }`
  - `struct GeneralTab: View` (`model`, `store`), `struct ShortcutsTab: View`

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/KeepVoiceChoiceTests.swift`, en entier :

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

    /// Une durée écrite à la main s'affiche telle quelle, à sa place.
    func testAnUnlistedValueIsKeptAsIs() {
        XCTAssertEqual(KeepVoiceChoice(minutes: 7), .minutes(7))
        XCTAssertEqual(KeepVoiceChoice(minutes: 7).label, "7 minutes")
        XCTAssertEqual(KeepVoiceChoice.options(stored: 7),
                       [.end, .minutes(2), .minutes(5), .minutes(7), .minutes(10), .minutes(30), .always])
        XCTAssertEqual(KeepVoiceChoice.options(stored: 10), KeepVoiceChoice.allCases)
        XCTAssertEqual(KeepVoiceChoice.options(stored: -1), KeepVoiceChoice.allCases)
    }
}
```

`macos/Tests/GeneralTabTests.swift` :

```swift
import XCTest
@testable import Sillon

final class GeneralTabTests: XCTestCase {
    private func bookmark(_ document: String, _ label: String, _ sentence: UInt32) -> StoredBookmarkView {
        StoredBookmarkView(document: document, label: label, sentence: sentence,
                           sectionTitle: "Titre", snippet: "Phrase \(sentence).", when: "2026-10-06 10:00")
    }

    func testBookmarksAreGroupedByDocumentInReadingOrder() {
        let groups = BookmarkGroups.group([
            bookmark("/a/cours.md", "cours.md", 9),
            bookmark("sélection:01", "sélection", 2),
            bookmark("/a/cours.md", "cours.md", 3),
        ])
        XCTAssertEqual(groups.map(\.label), ["cours.md", "sélection"])
        XCTAssertEqual(groups[0].items.map(\.sentence), [3, 9])
    }

    /// Deux fichiers de même nom, deux dossiers : deux groupes.
    func testTwoDocumentsWithTheSameNameStayApart() {
        let groups = BookmarkGroups.group([
            bookmark("/a/notes.md", "notes.md", 1),
            bookmark("/b/notes.md", "notes.md", 1),
        ])
        XCTAssertEqual(groups.count, 2)
    }

    func testTheCacheReportSaysWhatWasFreed() {
        XCTAssertEqual(CacheReport.text(freed: 0), "Le cache était déjà vide")
        XCTAssertTrue(CacheReport.text(freed: 12_000_000).hasSuffix("libérés"))
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/KeepVoiceChoiceTests -only-testing:SillonTests/GeneralTabTests`
Expected: FAIL — `type 'KeepVoiceChoice' has no member 'options'`,
`cannot find 'BookmarkGroups'`, `cannot find 'CacheReport'`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/KeepVoiceChoice.swift`, en entier :

```swift
/// Combien de temps la voix reste chargée après la dernière lecture.
enum KeepVoiceChoice: Hashable {
    case end
    case minutes(Int64)
    case always

    static let steps: [Int64] = [2, 5, 10, 30]
    static var allCases: [KeepVoiceChoice] { [.end] + steps.map { .minutes($0) } + [.always] }

    /// Une durée hors liste (écrite à la main) reste telle quelle.
    init(minutes: Int64) {
        switch minutes {
        case ..<0: self = .always
        case 0: self = .end
        default: self = .minutes(minutes)
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

    /// Les choix du menu, plus la durée en place si elle n'en fait pas
    /// partie, rangée à sa place.
    static func options(stored: Int64) -> [KeepVoiceChoice] {
        let current = KeepVoiceChoice(minutes: stored)
        guard !allCases.contains(current) else { return allCases }
        return [.end] + (steps + [stored]).sorted().map { .minutes($0) } + [.always]
    }
}
```

`macos/Sources/Settings/ShortcutsTab.swift` :

```swift
import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les deux raccourcis globaux.
struct ShortcutsTab: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-raccourcis")
    }
}
```

`macos/Sources/Settings/GeneralTab.swift` :

```swift
import AppKit
import SwiftUI

/// Onglet Général : démarrage, voix gardée, données.
struct GeneralTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var login = false
    @State private var bookmarks: [StoredBookmarkView] = []
    @State private var cacheReport: String?
    @State private var dataError: String?

    var body: some View {
        Form {
            Section("Démarrage") {
                Toggle("Ouvrir sillon à la connexion", isOn: $login)
                    .onAppear { login = LoginItem.isEnabled }
                    .onChange(of: login) { _, on in
                        // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                        if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                    }
                    .accessibilityLabel("Ouvrir sillon à la connexion")
                LabeledContent("Garder la voix chargée") {
                    PopUpPicker(label: "Garder la voix chargée", identifier: "garder-voix",
                                options: KeepVoiceChoice.options(stored: store.values.keepVoiceMinutes)
                                    .map { ($0.label, $0) },
                                selection: keepVoice)
                        .fixedSize()
                }
            }
            Section("Données") {
                HStack {
                    Button("Vider le cache des documents") { clearCache() }
                    if let cacheReport { Text(cacheReport) }
                }
                Button("Ouvrir le dossier de configuration") { openConfigFolder() }
                    .disabled(model.configDirectory == nil)
                if let dataError { Text(dataError).foregroundStyle(.red) }
            }
            Section("Signets") {
                if bookmarks.isEmpty { Text("Aucun signet") }
                ForEach(BookmarkGroups.group(bookmarks), id: \.document) { group in
                    Text(group.label).fontWeight(.semibold)
                    ForEach(group.items, id: \.self) { b in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(b.snippet)
                                Text("\(b.sectionTitle) · \(b.when)")
                            }
                            Spacer()
                            Button("Supprimer") { remove(b) }
                                .accessibilityLabel("Supprimer le signet « \(b.snippet) »")
                        }
                        .accessibilityElement(children: .contain)
                    }
                }
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-general")
        .task { bookmarks = await model.loadAllBookmarks() }
    }

    private var keepVoice: Binding<KeepVoiceChoice> {
        Binding(get: { KeepVoiceChoice(minutes: store.values.keepVoiceMinutes) },
                set: { store.set(\.keepVoiceMinutes, $0.minutes) })
    }

    private func clearCache() {
        Task {
            do {
                cacheReport = CacheReport.text(freed: try await model.clearDocumentCache())
                dataError = nil
            } catch {
                dataError = SettingsStore.message(of: error)
            }
        }
    }

    private func remove(_ b: StoredBookmarkView) {
        Task {
            do {
                try await model.removeBookmarkAsync(document: b.document, sentence: b.sentence)
                dataError = nil
            } catch {
                dataError = SettingsStore.message(of: error)
            }
            bookmarks = await model.loadAllBookmarks()
        }
    }

    private func openConfigFolder() {
        guard let dir = model.configDirectory else { return }
        let url = URL(fileURLWithPath: dir, isDirectory: true)
        // Pas encore de config.toml : le dossier s'ouvre quand même.
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        NSWorkspace.shared.open(url)
    }
}

/// Les signets d'un document.
struct BookmarkGroup: Equatable {
    let document: String
    let label: String
    let items: [StoredBookmarkView]
}

enum BookmarkGroups {
    /// Par document, dans l'ordre de leur premier signet ; dans chacun,
    /// l'ordre de lecture. Deux fichiers de même nom restent séparés.
    static func group(_ items: [StoredBookmarkView]) -> [BookmarkGroup] {
        var order: [String] = []
        var byDocument: [String: [StoredBookmarkView]] = [:]
        for b in items {
            if byDocument[b.document] == nil { order.append(b.document) }
            byDocument[b.document, default: []].append(b)
        }
        return order.map { document in
            let list = (byDocument[document] ?? []).sorted { $0.sentence < $1.sentence }
            return BookmarkGroup(document: document, label: list.first?.label ?? document, items: list)
        }
    }
}

enum CacheReport {
    static func text(freed bytes: UInt64) -> String {
        guard bytes > 0 else { return "Le cache était déjà vide" }
        return "\(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)) libérés"
    }
}
```

`macos/Sources/Settings/ReglagesView.swift` : `ShortcutSettings().settingsTab(.shortcuts)`
devient `ShortcutsTab().settingsTab(.shortcuts)`, et
`GeneralSettings().settingsTab(.general)` devient
`GeneralTab(model: model, store: store).settingsTab(.general)`.

`macos/Sources/Settings/SettingsView.swift` :
- supprimer `struct ShortcutSettings` et `struct GeneralSettings` ;
- dans `VoiceSettings`, supprimer les `@State` `keep` et `storedKeep`, le
  `LabeledContent("Garder la voix chargée") { … }`, le bloc
  `.onChange(of: keep) { … }`, et dans `.task` les trois lignes qui lisent et rangent
  `keepVoiceMinutes` (ne garder que `voices = await model.loadVoices()`).

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Settings/GeneralTab.swift macos/Sources/Settings/ShortcutsTab.swift macos/Sources/Settings/KeepVoiceChoice.swift macos/Sources/Settings/ReglagesView.swift macos/Sources/Settings/SettingsView.swift macos/Tests/KeepVoiceChoiceTests.swift macos/Tests/GeneralTabTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): l'onglet Général — cache, signets, dossier de configuration

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 10 : l'onglet Modèles

Le catalogue décrit chaque modèle (nom, description, langues, taille,
précision, rythme mesuré) ; on installe, on annule un téléchargement, on
supprime après confirmation dans une alerte — jamais le modèle actif — et
l'espace disque libre est affiché. Un échec de suppression se dit, et le
catalogue est relu. L'ancien fichier `SettingsView.swift` disparaît.

**Fichiers :**
- Créer : `macos/Sources/Settings/ModelsStore.swift`
- Créer : `macos/Sources/Settings/ModelsTab.swift`
- Modifier : `macos/Sources/Settings/ReglagesView.swift`
- Supprimer : `macos/Sources/Settings/SettingsView.swift`
- Créer : `macos/Tests/FakeModelsBackend.swift`
- Créer : `macos/Tests/ModelsStoreTests.swift`

**Interfaces :**
- Consomme : `ReaderModel.{loadVoices, installVoiceAsync, cancelDownloadAsync, removeModelAsync, useVoiceAsync, freeSpace, snapshot, isDownloading(_:)}` (Tâche 5), `VoiceModel` enrichi, `SettingsStore.message(of:)`, `VoiceModel.sample` (tests, Tâche 7).
- Produit :
  - `@MainActor protocol ModelsBackend: AnyObject { loadVoices() async -> [VoiceModel]; installVoiceAsync(_:) async throws; cancelDownloadAsync() async throws; removeModelAsync(_:) async throws; useVoiceAsync(_:) async throws; freeSpace() async -> UInt64? }` ; `ReaderModel` s'y conforme.
  - `enum DeletionRequest: Equatable { case confirm(title: String, message: String); case refuse(String) }`
  - `@MainActor @Observable final class ModelsStore` : `init(backend:)`, `catalogue: [VoiceModel]`, `freeSpace: UInt64?`, `error: String?`, `pendingDeletion: VoiceModel?`, `cancelling: Bool`, `reload() async`, `static func deletionRequest(for:) -> DeletionRequest`, `askDeletion(_:)`, `delete(_:) async`, `install(_:) async`, `use(_:) async`, `cancelDownload() async`, `downloadSettled() async`
  - `struct ModelsTab: View` (`init(model:)`)

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/FakeModelsBackend.swift` :

```swift
@testable import Sillon

/// Un catalogue pour les tests : il note ce qu'on lui demande.
@MainActor
final class FakeModelsBackend: ModelsBackend {
    var catalogue: [VoiceModel] = []
    var free: UInt64? = 10_000_000_000
    private(set) var loads = 0
    private(set) var removed: [String] = []
    var failRemoval: String?

    func loadVoices() async -> [VoiceModel] { loads += 1; return catalogue }
    func installVoiceAsync(_ id: String) async throws {}
    func cancelDownloadAsync() async throws {}
    func removeModelAsync(_ id: String) async throws {
        if let failRemoval { throw SillonError.Failed(message: failRemoval) }
        removed.append(id)
        catalogue.removeAll { $0.id == id }
    }
    func useVoiceAsync(_ id: String) async throws {}
    func freeSpace() async -> UInt64? { free }
}
```

`macos/Tests/ModelsStoreTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class ModelsStoreTests: XCTestCase {
    func testTheActiveModelIsRefusedWithoutAnAlert() {
        let store = ModelsStore(backend: FakeModelsBackend())
        let active = VoiceModel.sample(id: "a", name: "Supertonic 3 · F16", active: true)
        guard case .refuse(let reason) = ModelsStore.deletionRequest(for: active) else {
            return XCTFail("le modèle actif ne se supprime pas")
        }
        XCTAssertTrue(reason.contains("modèle actif"))
        store.askDeletion(active)
        XCTAssertNil(store.pendingDeletion, "aucune alerte")
        XCTAssertEqual(store.error, reason)
    }

    func testAnInstalledModelAsksForConfirmation() {
        let store = ModelsStore(backend: FakeModelsBackend())
        let model = VoiceModel.sample(id: "b", name: "MagpieTTS", bytes: 1_562_142_912)
        guard case .confirm(let title, let message) = ModelsStore.deletionRequest(for: model) else {
            return XCTFail("confirmation attendue")
        }
        XCTAssertEqual(title, "Supprimer MagpieTTS ?")
        XCTAssertTrue(message.contains(ByteCountFormatter.string(fromByteCount: 1_562_142_912, countStyle: .file)))
        store.askDeletion(model)
        XCTAssertEqual(store.pendingDeletion?.id, "b")
    }

    func testAConfirmedDeletionRemovesAndRereadsTheCatalogue() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "b")]
        let store = ModelsStore(backend: backend)
        await store.reload()
        store.askDeletion(backend.catalogue[0])
        await store.delete(backend.catalogue[0])
        XCTAssertEqual(backend.removed, ["b"])
        XCTAssertTrue(store.catalogue.isEmpty)
        XCTAssertNil(store.pendingDeletion)
        XCTAssertEqual(backend.loads, 2)
    }

    func testAFailedDeletionSaysWhyAndRereadsTheCatalogue() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "b")]
        backend.failRemoval = "suppression de magpie.gguf : permission refusée"
        let store = ModelsStore(backend: backend)
        await store.reload()
        await store.delete(backend.catalogue[0])
        XCTAssertEqual(store.error, "suppression de magpie.gguf : permission refusée")
        XCTAssertEqual(backend.loads, 2, "catalogue relu")
        XCTAssertEqual(store.catalogue.count, 1)
    }

    func testTheFreeSpaceIsRead() async {
        let backend = FakeModelsBackend()
        let store = ModelsStore(backend: backend)
        await store.reload()
        XCTAssertEqual(store.freeSpace, 10_000_000_000)
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/ModelsStoreTests`
Expected: FAIL — `cannot find type 'ModelsBackend'`, `cannot find 'ModelsStore'`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/ModelsStore.swift` :

```swift
import Foundation

/// Ce que l'onglet Modèles demande au moteur.
@MainActor
protocol ModelsBackend: AnyObject {
    func loadVoices() async -> [VoiceModel]
    func installVoiceAsync(_ id: String) async throws
    func cancelDownloadAsync() async throws
    func removeModelAsync(_ id: String) async throws
    func useVoiceAsync(_ id: String) async throws
    func freeSpace() async -> UInt64?
}

extension ReaderModel: ModelsBackend {}

/// Ce que l'onglet répond à une demande de suppression.
enum DeletionRequest: Equatable {
    /// Une alerte, avant d'effacer des centaines de mégaoctets.
    case confirm(title: String, message: String)
    /// Le modèle actif : on en choisit un autre d'abord.
    case refuse(String)
}

/// Le catalogue et ce qu'on y fait. Chaque action relit le catalogue, qu'elle
/// réussisse ou non : l'écran dit l'état réel du disque.
@MainActor
@Observable
final class ModelsStore {
    @ObservationIgnored private let backend: ModelsBackend
    private(set) var catalogue: [VoiceModel] = []
    private(set) var freeSpace: UInt64?
    private(set) var error: String?
    /// Le modèle dont la suppression attend confirmation.
    var pendingDeletion: VoiceModel?
    /// Une annulation est demandée ; le transfert la voit à son prochain bloc.
    private(set) var cancelling = false

    init(backend: ModelsBackend) { self.backend = backend }

    func reload() async {
        catalogue = await backend.loadVoices()
        freeSpace = await backend.freeSpace()
    }

    static func deletionRequest(for model: VoiceModel) -> DeletionRequest {
        if model.active {
            return .refuse("\(model.name) est le modèle actif : choisissez-en un autre avant de le supprimer.")
        }
        let size = ByteCountFormatter.string(fromByteCount: Int64(model.bytes), countStyle: .file)
        return .confirm(title: "Supprimer \(model.name) ?",
                        message: "\(size) seront libérés. Il faudra le télécharger à nouveau pour l'utiliser.")
    }

    func askDeletion(_ model: VoiceModel) {
        switch Self.deletionRequest(for: model) {
        case .refuse(let reason): error = reason
        case .confirm: pendingDeletion = model
        }
    }

    /// Supprime après confirmation. Le modèle est passé par l'alerte : sa
    /// fermeture remet `pendingDeletion` à nil avant que la tâche ne parte.
    func delete(_ model: VoiceModel) async {
        pendingDeletion = nil
        await run { try await self.backend.removeModelAsync(model.id) }
    }

    func install(_ id: String) async { await run { try await self.backend.installVoiceAsync(id) } }

    func use(_ id: String) async { await run { try await self.backend.useVoiceAsync(id) } }

    func cancelDownload() async {
        cancelling = true
        await run { try await self.backend.cancelDownloadAsync() }
    }

    /// Un téléchargement terminé, échoué ou annulé : on relit le catalogue.
    func downloadSettled() async {
        cancelling = false
        await reload()
    }

    private func run(_ action: () async throws -> Void) async {
        do {
            try await action()
            error = nil
        } catch {
            self.error = SettingsStore.message(of: error)
        }
        await reload()
    }
}
```

`macos/Sources/Settings/ModelsTab.swift` :

```swift
import SwiftUI

/// Onglet Modèles : le catalogue, installer, annuler, supprimer.
struct ModelsTab: View {
    let model: ReaderModel
    @State private var store: ModelsStore

    init(model: ReaderModel) {
        self.model = model
        _store = State(initialValue: ModelsStore(backend: model))
    }

    private var download: DownloadView? { model.snapshot.download }

    var body: some View {
        Form {
            Section("Catalogue") {
                if store.catalogue.isEmpty {
                    Text("Aucun modèle proposé : cette version est compilée sans audio.cpp.")
                }
                ForEach(store.catalogue, id: \.id) { m in row(m) }
            }
            if let free = store.freeSpace {
                Text("Espace libre : \(ByteCountFormatter.string(fromByteCount: Int64(free), countStyle: .file))")
                    .accessibilityIdentifier("espace-libre")
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-modeles")
        .task { await store.reload() }
        // Un téléchargement qui finit, échoue ou s'annule change le catalogue.
        .onChange(of: ReaderModel.isDownloading(download)) { _, running in
            if !running { Task { await store.downloadSettled() } }
        }
        .alert(deletionTitle, isPresented: deletionShown, presenting: store.pendingDeletion) { m in
            Button("Supprimer", role: .destructive) { Task { await store.delete(m) } }
            Button("Annuler", role: .cancel) { store.pendingDeletion = nil }
        } message: { m in
            if case .confirm(_, let message) = ModelsStore.deletionRequest(for: m) { Text(message) }
        }
    }

    @ViewBuilder private func row(_ m: VoiceModel) -> some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(m.name).fontWeight(.semibold)
                Text(m.summary)
                Text("\(m.languagesLabel) · \(ByteCountFormatter.string(fromByteCount: Int64(m.bytes), countStyle: .file)) · \(m.precision.uppercased())")
                Text(m.pace)
                if let d = download, d.id == m.id, let failed = d.failed {
                    Text("Téléchargement interrompu : \(failed)").foregroundStyle(.red)
                }
            }
            Spacer()
            actions(m)
        }
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private func actions(_ m: VoiceModel) -> some View {
        if m.active {
            Text("Actif")
        } else if let d = download, d.id == m.id, d.failed == nil {
            VStack(alignment: .trailing) {
                ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                    .frame(width: 120)
                    .accessibilityLabel("Téléchargement de \(m.name)")
                Button(store.cancelling ? "Annulation…" : "Annuler") {
                    Task { await store.cancelDownload() }
                }
                .disabled(store.cancelling)
                .accessibilityLabel("Annuler le téléchargement de \(m.name)")
            }
        } else if m.installed {
            VStack(alignment: .trailing) {
                Button("Utiliser") { Task { await store.use(m.id) } }
                    .accessibilityLabel("Utiliser \(m.name)")
                Button("Supprimer…") { store.askDeletion(m) }
                    .accessibilityLabel("Supprimer \(m.name)")
            }
        } else {
            // Après un échec, le même bouton relance l'installation.
            Button(download?.id == m.id ? "Réessayer" : "Installer") {
                Task { await store.install(m.id) }
            }
            .accessibilityLabel("Installer \(m.name)")
            .disabled(ReaderModel.isDownloading(download))
        }
    }

    private var deletionTitle: String {
        guard let m = store.pendingDeletion,
              case .confirm(let title, _) = ModelsStore.deletionRequest(for: m) else { return "" }
        return title
    }

    private var deletionShown: Binding<Bool> {
        Binding(get: { store.pendingDeletion != nil },
                set: { if !$0 { store.pendingDeletion = nil } })
    }
}
```

`macos/Sources/Settings/ReglagesView.swift` : `VoiceSettings(model: model).settingsTab(.models)`
devient `ModelsTab(model: model).settingsTab(.models)`.

Supprimer l'ancien fichier : `git rm macos/Sources/Settings/SettingsView.swift`.

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Settings/ModelsStore.swift macos/Sources/Settings/ModelsTab.swift macos/Sources/Settings/ReglagesView.swift macos/Tests/FakeModelsBackend.swift macos/Tests/ModelsStoreTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): l'onglet Modèles — catalogue décrit, annulation, suppression confirmée

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

(`git rm` a déjà indexé la suppression de `SettingsView.swift`.)

### Tâche 11 : l'onglet Avancé

Moteur (automatique / audio.cpp / voix du système), calcul (Metal / CPU),
fils, sessions, phrases préparées d'avance — chacun recharge la voix —,
échantillonnage (température, top-k, guidage, pas d'inférence, graine ; top-p
et pénalité de répétition sur demande), et les options libres du modèle
(`[extra]`) en liste clé = valeur.

**Fichiers :**
- Modifier : `macos/Sources/Settings/SettingControls.swift` (ajout de `OptionalSlider`, `OptionalStepper`, `SeedField`)
- Créer : `macos/Sources/Settings/AdvancedTab.swift`
- Modifier : `macos/Sources/Settings/ReglagesView.swift`
- Créer : `macos/Tests/AdvancedTabTests.swift`

**Interfaces :**
- Consomme : `SettingsStore.{values, binding(_:), set(_:_:), isShown, isIgnored, showIgnored, error}`, `Choices.including`, `IgnorableNote`, `SettingFormat`, `ExtraOption`.
- Produit :
  - `struct OptionalSlider: View { title: String; ignored: Bool; @Binding value: Float?; range: ClosedRange<Float>; step: Float; fallback: Float }`
  - `struct OptionalStepper: View { title: String; ignored: Bool; @Binding value: UInt32?; range: ClosedRange<UInt32>; fallback: UInt32 }`
  - `struct SeedField: View { ignored: Bool; @Binding seed: UInt64? }`
  - `enum ExtraOptions { static func adding(key:value:to:) -> [ExtraOption]?; static func removing(_:from:) -> [ExtraOption] }`
  - `struct AdvancedTab: View` (`store`), `static func engineOptions(current:)`, `static func computeOptions(current:)`, `static func sessionsText(_: UInt32) -> String`

- [ ] **Étape 1 : écrire le test qui échoue**

`macos/Tests/AdvancedTabTests.swift` :

```swift
import XCTest
@testable import Sillon

final class AdvancedTabTests: XCTestCase {
    func testEnginesAndComputeAreNamedAndKeepAHandWrittenValue() {
        XCTAssertEqual(AdvancedTab.engineOptions(current: "auto").map(\.title),
                       ["Automatique", "audio.cpp", "Voix du système"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "metal").map(\.value), ["metal", "cpu"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "cuda").map(\.value), ["metal", "cpu", "cuda"])
    }

    func testZeroSessionsMeansTheModelDecides() {
        XCTAssertEqual(AdvancedTab.sessionsText(0), "Sessions : automatique")
        XCTAssertEqual(AdvancedTab.sessionsText(3), "Sessions : 3")
    }

    func testAnOptionIsAddedReplacedOrRefused() {
        let one = ExtraOptions.adding(key: " cfg_scale ", value: " 3 ", to: [])
        XCTAssertEqual(one, [ExtraOption(key: "cfg_scale", value: "3")])
        let replaced = ExtraOptions.adding(key: "cfg_scale", value: "4", to: one ?? [])
        XCTAssertEqual(replaced, [ExtraOption(key: "cfg_scale", value: "4")])
        XCTAssertNil(ExtraOptions.adding(key: "  ", value: "1", to: []), "clé vide")
        XCTAssertNil(ExtraOptions.adding(key: "a=b", value: "1", to: []), "« = » dans la clé")
        XCTAssertEqual(ExtraOptions.removing("cfg_scale", from: replaced ?? []), [])
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/AdvancedTabTests`
Expected: FAIL — `cannot find 'AdvancedTab'`, `cannot find 'ExtraOptions'`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Settings/SettingControls.swift`, ajouter à la fin :

```swift
/// Un réglage que le modèle peut fixer seul : « valeur du modèle », ou une
/// valeur posée au curseur.
struct OptionalSlider: View {
    let title: String
    let ignored: Bool
    @Binding var value: Float?
    let range: ClosedRange<Float>
    let step: Float
    /// La valeur proposée quand on reprend la main.
    let fallback: Float

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("\(title) : valeur du modèle",
                   isOn: Binding(get: { value == nil }, set: { value = $0 ? nil : fallback }))
            if let current = value {
                Slider(value: Binding(get: { current }, set: { value = $0 }), in: range, step: step) {
                    Text("\(title) : \(SettingFormat.decimal(current))")
                }
                .accessibilityLabel(title)
                .accessibilityValue(SettingFormat.decimal(current))
            }
        }
    }
}

/// La même chose pour un entier.
struct OptionalStepper: View {
    let title: String
    let ignored: Bool
    @Binding var value: UInt32?
    let range: ClosedRange<UInt32>
    let fallback: UInt32

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("\(title) : valeur du modèle",
                   isOn: Binding(get: { value == nil }, set: { value = $0 ? nil : fallback }))
            if let current = value {
                Stepper("\(title) : \(current)",
                        value: Binding(get: { current }, set: { value = $0 }), in: range)
                    .accessibilityLabel(title)
                    .accessibilityValue("\(current)")
            }
        }
    }
}

/// La graine : un tirage au hasard, ou une valeur fixe pour une synthèse
/// reproductible.
struct SeedField: View {
    let ignored: Bool
    @Binding var seed: UInt64?

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("Graine : tirage au hasard",
                   isOn: Binding(get: { seed == nil }, set: { seed = $0 ? nil : 0 }))
            if seed != nil {
                TextField("Graine", value: Binding(get: { seed ?? 0 }, set: { seed = $0 }),
                          format: IntegerFormatStyle<UInt64>().grouping(.never))
                    .accessibilityLabel("Graine")
            }
        }
    }
}
```

`macos/Sources/Settings/AdvancedTab.swift` :

```swift
import SwiftUI

/// Onglet Avancé : le moteur, l'échantillonnage, les options libres.
struct AdvancedTab: View {
    @Bindable var store: SettingsStore
    @State private var newKey = ""
    @State private var newValue = ""
    @State private var extraError: String?

    var body: some View {
        Form {
            Section("Moteur") {
                LabeledContent("Moteur") {
                    PopUpPicker(label: "Moteur", identifier: "moteur",
                                options: Self.engineOptions(current: store.values.engine),
                                selection: store.binding(\.engine))
                        .fixedSize()
                }
                LabeledContent("Calcul") {
                    PopUpPicker(label: "Calcul", identifier: "calcul",
                                options: Self.computeOptions(current: store.values.compute),
                                selection: store.binding(\.compute))
                        .fixedSize()
                }
                Stepper("Fils de calcul : \(store.values.threads)", value: store.binding(\.threads), in: 1...64)
                    .accessibilityLabel("Fils de calcul")
                    .accessibilityValue("\(store.values.threads)")
                Stepper(Self.sessionsText(store.values.sessions), value: store.binding(\.sessions), in: 0...16)
                    .accessibilityLabel("Sessions")
                    .accessibilityValue(Self.sessionsText(store.values.sessions))
                Stepper("Phrases préparées d'avance : \(store.values.prefetch)",
                        value: store.binding(\.prefetch), in: 2...64)
                    .accessibilityLabel("Phrases préparées d'avance")
                    .accessibilityValue("\(store.values.prefetch)")
                Text("Chaque changement ici recharge la voix ; la lecture reprend à la même phrase.")
            }
            Section("Échantillonnage") {
                if store.isShown(.temperature) {
                    OptionalSlider(title: "Température", ignored: store.isIgnored(.temperature),
                                   value: store.binding(\.temperature), range: 0...2, step: 0.05, fallback: 0.7)
                }
                if store.isShown(.topP) {
                    OptionalSlider(title: "Top-p", ignored: store.isIgnored(.topP),
                                   value: store.binding(\.topP), range: 0...1, step: 0.05, fallback: 0.9)
                }
                if store.isShown(.topK) {
                    OptionalStepper(title: "Top-k", ignored: store.isIgnored(.topK),
                                    value: store.binding(\.topK), range: 1...200, fallback: 50)
                }
                if store.isShown(.guidance) {
                    OptionalSlider(title: "Guidage", ignored: store.isIgnored(.guidance),
                                   value: store.binding(\.guidanceScale), range: 0...20, step: 0.5, fallback: 3)
                }
                if store.isShown(.repetition) {
                    OptionalSlider(title: "Pénalité de répétition", ignored: store.isIgnored(.repetition),
                                   value: store.binding(\.repetitionPenalty), range: 0.5...2, step: 0.05,
                                   fallback: 1.1)
                }
                if store.isShown(.steps) {
                    OptionalStepper(title: "Pas d'inférence", ignored: store.isIgnored(.steps),
                                    value: store.binding(\.inferenceSteps), range: 1...200, fallback: 16)
                }
                if store.isShown(.seed) {
                    SeedField(ignored: store.isIgnored(.seed), seed: store.binding(\.seed))
                }
                Toggle("Afficher aussi les réglages ignorés par ce modèle", isOn: $store.showIgnored)
            }
            Section("Options du modèle") {
                ForEach(store.values.extra, id: \.key) { option in
                    HStack {
                        Text("\(option.key) = \(option.value)")
                        Spacer()
                        Button("Retirer") {
                            store.set(\.extra, ExtraOptions.removing(option.key, from: store.values.extra))
                        }
                        .accessibilityLabel("Retirer l'option \(option.key)")
                    }
                }
                HStack {
                    TextField("Clé", text: $newKey).accessibilityLabel("Clé de l'option")
                    TextField("Valeur", text: $newValue).accessibilityLabel("Valeur de l'option")
                    Button("Ajouter") { add() }
                        .disabled(newKey.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                if let extraError { Text(extraError).foregroundStyle(.red) }
                Text("Transmises telles quelles au modèle (section [extra] de config.toml).")
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-avance")
    }

    private func add() {
        guard let next = ExtraOptions.adding(key: newKey, value: newValue, to: store.values.extra) else {
            extraError = "Il faut une clé, sans « = »."
            return
        }
        store.set(\.extra, next)
        newKey = ""
        newValue = ""
        extraError = nil
    }

    static func engineOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Automatique", "auto"), ("audio.cpp", "audio_cpp"), ("Voix du système", "system")],
                          current: current) { "\($0) (écrit à la main)" }
    }

    static func computeOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Metal", "metal"), ("CPU", "cpu")], current: current) { "\($0) (écrit à la main)" }
    }

    static func sessionsText(_ n: UInt32) -> String {
        n == 0 ? "Sessions : automatique" : "Sessions : \(n)"
    }
}

/// Les options libres du modèle, en liste clé = valeur.
enum ExtraOptions {
    /// Ajoute ou remplace une option ; nil pour une clé vide ou qui contient « = ».
    static func adding(key: String, value: String, to options: [ExtraOption]) -> [ExtraOption]? {
        let key = key.trimmingCharacters(in: .whitespaces)
        guard !key.isEmpty, !key.contains("=") else { return nil }
        let value = value.trimmingCharacters(in: .whitespaces)
        return options.filter { $0.key != key } + [ExtraOption(key: key, value: value)]
    }

    static func removing(_ key: String, from options: [ExtraOption]) -> [ExtraOption] {
        options.filter { $0.key != key }
    }
}
```

`macos/Sources/Settings/ReglagesView.swift` : entre la ligne des raccourcis et
celle de Général, ajouter `AdvancedTab(store: store).settingsTab(.advanced)`.

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Settings/SettingControls.swift macos/Sources/Settings/AdvancedTab.swift macos/Sources/Settings/ReglagesView.swift macos/Tests/AdvancedTabTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): l'onglet Avancé — moteur, échantillonnage, options libres

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 12 : l'écrou ⚙ et le panneau qui suit les réglages

Un écrou en bout de barre de lecture ouvre l'onglet Voix (libellé VoiceOver
« Réglages de la voix ») ; au clavier, la touche `v` fait de même — dans le
panneau, Espace lit et Tab ne parcourt pas des boutons sans « navigation au
clavier » activée. La ligne de message dit « Voix en chargement… » pendant un
rechargement. Le panneau relit son affichage à chaque réglage écrit, et le
pas de l'avance rapide est enfin celui des réglages (il valait 5 en dur).

**Fichiers :**
- Modifier : `macos/Sources/Panel/PlayerBar.swift`
- Modifier : `macos/Sources/Input/KeyMap.swift`
- Modifier : `macos/Sources/Panel/PanelController.swift`
- Modifier : `macos/Sources/Panel/PanelContent.swift` (`HelpOverlay`, `PanelStatus`, relecture des réglages)
- Modifier : `macos/Tests/KeyMapTests.swift`
- Modifier : `macos/Tests/PanelControllerTests.swift` (`PanelStatusTests`)

**Interfaces :**
- Consomme : `SettingsOpener.shared.open(tab:)` (Tâche 7), `ReadingDisplay` et `ReaderModel.settingsRevision` (Tâche 5).
- Produit : `ReaderAction.openVoiceSettings` ; `KeyMap.action(for:seekStep:)` rend `.openVoiceSettings` pour `v` ; `PanelController.reloadReading()` ; `PanelStatus.loadingLine = "Voix en chargement…"`.

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/KeyMapTests.swift`, dans la classe de tests :

```swift
    func testVOpensTheVoiceSettings() {
        XCTAssertEqual(KeyMap.action(for: KeyPress(characters: "v")), .openVoiceSettings)
        XCTAssertNil(KeyMap.action(for: KeyPress(characters: "v", option: true)))
    }

    func testTheSeekStepComesFromTheSettings() {
        let key = KeyPress(characters: "", keyCode: KeyCode.right, shift: true)
        XCTAssertEqual(KeyMap.action(for: key, seekStep: 12), .command(.sentences(delta: 12)))
    }
```

`macos/Tests/PanelControllerTests.swift`, dans `PanelStatusTests` :

```swift
    func testAReloadingVoiceSaysSo() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .loading)),
                       "Voix en chargement…")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Signet ajouté", voice: .loading)),
                       "Signet ajouté", "le lecteur parle d'abord")
    }
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/KeyMapTests -only-testing:SillonTests/PanelStatusTests`
Expected: FAIL — `type 'ReaderAction' has no member 'openVoiceSettings'` ; une
fois ajouté, `testAReloadingVoiceSaysSo` échoue (`nil` au lieu de
« Voix en chargement… »).

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Input/KeyMap.swift` :
- dans `enum ReaderAction`, la ligne
  `case toggleSidebar, toggleBookmarks, showHelp, openSettings` devient
  `case toggleSidebar, toggleBookmarks, showHelp, openSettings, openVoiceSettings` ;
- dans le `switch key.characters`, avant `default: return nil` :
  `case "v": return .openVoiceSettings`

`macos/Sources/Panel/PanelController.swift` :
- dans `perform`, après `case .openSettings: SettingsOpener.shared.open()` :

```swift
        case .openVoiceSettings:
            SettingsOpener.shared.open(tab: .voice)
```

- dans `installKeyMonitor`, la ligne
  `guard let action = KeyMap.action(for: KeyPress(event)) else { return event }`
  devient :

```swift
            guard let action = KeyMap.action(for: KeyPress(event),
                                             seekStep: Int32(self.reading.seekStep)) else { return event }
```

- dans `show()`, la ligne `Task { reading = ReadingDisplay(await model.loadSettings()) }`
  devient `reloadReading()`, et ajouter après `show()` :

```swift
    /// Relit ce que le panneau affiche des réglages, hors du fil principal :
    /// un changement dans la fenêtre des réglages s'y voit aussitôt.
    func reloadReading() {
        Task { reading = ReadingDisplay(await model.loadSettings()) }
    }
```

`macos/Sources/Panel/PlayerBar.swift`, dans le `HStack`, après le bloc
`if let skipped = … { … }` :

```swift
            // L'écrou : les réglages de la voix, pour en essayer une autre.
            Button {
                SettingsOpener.shared.open(tab: .voice)
            } label: {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.plain)
            .help("Réglages de la voix (v)")
            .accessibilityLabel("Réglages de la voix")
            .accessibilityIdentifier("reglages-voix")
```

`macos/Sources/Panel/PanelContent.swift` :
- dans `HelpOverlay.rows`, la ligne
  `("⏎", "Agrandir"), ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),`
  devient
  `("⏎", "Agrandir"), ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"), ("v", "Réglages de la voix"),` ;
- remplacer `enum PanelStatus` par :

```swift
enum PanelStatus {
    static let fallbackHint = "Voix du système — installer une voix : ⌘,"
    static let loadingLine = "Voix en chargement…"

    static func line(message: String?, snapshot: Snapshot) -> String? {
        if let message { return message }
        if !snapshot.status.isEmpty { return snapshot.status }
        if case .loading = snapshot.voice { return loadingLine }
        if case .fallback = snapshot.voice { return fallbackHint }
        return nil
    }
}
```

- dans `PanelContent.body`, après `.onChange(of: controller.model.snapshot) { … }` :

```swift
        // Un réglage écrit (interlignage, règle, pas d'avance) s'applique
        // au panneau ouvert, sans attendre sa prochaine ouverture.
        .onChange(of: controller.model.settingsRevision) { _, _ in controller.reloadReading() }
```

- [ ] **Étape 4 : vérifier que tout passe**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : vérifier à la main que rien ne tourne en boucle**

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build build && open build/Build/Products/Debug/Sillon.app`, afficher le panneau, cliquer l'écrou, laisser la fenêtre des réglages ouverte 20 s, puis `sample Sillon 10 -file /tmp/sillon-reglages.txt` et `top -l 2 -pid $(pgrep -x Sillon) -stats cpu`.
Expected: l'onglet Voix s'ouvre ; `v` dans le panneau fait de même ; en
pause, l'usage processeur reste sous 2 % (la relecture à 4 Hz de la fenêtre
ne relance rien) — un chiffre plus haut est à expliquer avant de continuer.

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/Panel/PlayerBar.swift macos/Sources/Input/KeyMap.swift macos/Sources/Panel/PanelController.swift macos/Sources/Panel/PanelContent.swift macos/Tests/KeyMapTests.swift macos/Tests/PanelControllerTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): l'écrou du panneau ouvre l'onglet Voix, le panneau suit les réglages

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Tâche 13 : tests d'interface et audit d'accessibilité

L'écrou ouvre l'onglet Voix, `⌘,` rouvre le dernier onglet, chaque onglet se
parcourt au clavier et passe l'audit d'accessibilité. Les tests sont écrits
et compilés ici ; **leur exécution attend le feu vert de l'utilisateur**
(étape 4). Le lecteur des tests est muet et hors disque : aucun réglage,
modèle, cache ou signet de l'utilisateur n'est touché.

**Fichiers :**
- Modifier : `macos/UITests/SettingsUITests.swift` (réécrit en entier)

**Interfaces :**
- Consomme : identifiants d'accessibilité `onglet-voix`, `onglet-lecture`, `onglet-modeles`, `onglet-raccourcis`, `onglet-avance`, `onglet-general` (Tâches 7 à 11), `reglages-voix` (Tâche 12), `panneau-lecture`, fenêtre `Réglages`, `performAccessibilityAuditOutsideTouchBar()` (`AuditSupport.swift`).
- Produit : aucune interface.

- [ ] **Étape 1 : écrire les tests**

`macos/UITests/SettingsUITests.swift` :

```swift
import XCTest

/// La fenêtre des réglages au clavier et sous l'audit d'accessibilité.
/// Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/SettingsUITests`. Prérequis : « Navigation
/// au clavier » activée (Réglages Système › Clavier), sans quoi Tab ne
/// s'arrête que sur les champs de texte.
final class SettingsUITests: XCTestCase {
    private let tabs = [
        ("Voix", "onglet-voix"), ("Lecture", "onglet-lecture"), ("Modèles", "onglet-modeles"),
        ("Raccourcis", "onglet-raccourcis"), ("Avancé", "onglet-avance"), ("Général", "onglet-general"),
    ]

    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"] + arguments
        app.launch()
        return app
    }

    private func focused(in window: XCUIElement) -> XCUIElement {
        window.descendants(matching: .any)
            .matching(NSPredicate(format: "hasKeyboardFocus == true")).firstMatch
    }

    /// Chaque onglet s'ouvre, se parcourt au clavier et passe l'audit.
    func testEveryTabIsKeyboardReachableAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        for (title, marker) in tabs {
            window.toolbars.buttons[title].click()
            XCTAssertTrue(window.descendants(matching: .any)[marker].waitForExistence(timeout: 3), title)
            var seen = Set<String>()
            for _ in 0..<6 {
                window.typeKey("\t", modifierFlags: [])
                let element = focused(in: window)
                XCTAssertTrue(element.exists, "\(title) : aucun contrôle n'a le focus")
                seen.insert(element.debugDescription)
            }
            XCTAssertGreaterThan(seen.count, 1, "\(title) : Tab ne parcourt pas l'onglet")
            try app.performAccessibilityAuditOutsideTouchBar()
        }
    }

    /// L'écrou du panneau ouvre les réglages sur l'onglet Voix.
    func testTheGearOpensTheVoiceTab() throws {
        let app = launch(["--show-panel"])
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        let gear = panel.buttons["reglages-voix"]
        XCTAssertTrue(gear.exists)
        XCTAssertEqual(gear.label, "Réglages de la voix")
        gear.click()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-voix"].waitForExistence(timeout: 3))
    }

    /// `v` dans le panneau fait comme l'écrou.
    func testVFromThePanelOpensTheVoiceTab() throws {
        let app = launch(["--show-panel"])
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey("v", modifierFlags: [])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-voix"].waitForExistence(timeout: 3))
    }

    /// ⌘, depuis le panneau rouvre le dernier onglet utilisé.
    func testCommandCommaReopensTheLastTab() throws {
        let app = launch(["--show-panel", "--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.toolbars.buttons["Avancé"].click()
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
        window.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(window.waitForNonExistence(timeout: 3))
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
    }
}
```

- [ ] **Étape 2 : vérifier que les tests compilent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build-for-testing`
Expected: `** TEST BUILD SUCCEEDED **`.

- [ ] **Étape 3 : commit**

```bash
git add macos/UITests/SettingsUITests.swift
git commit -m "$(cat <<'EOF'
test(macos): les six onglets au clavier et sous l'audit, l'écrou, ⌘, rouvre le dernier

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Étape 4 : exécuter les tests d'interface — seulement avec le feu vert de l'utilisateur**

Demander d'abord : « Les tests d'interface prennent la main sur le clavier et
la souris pendant deux à trois minutes. Je les lance ? » Sans accord
explicite, s'arrêter ici et le signaler dans le compte rendu.

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test-without-building -only-testing:SillonUITests/SettingsUITests -only-testing:SillonUITests/AccessibilityAuditUITests`
Expected: `** TEST SUCCEEDED **`. Un échec d'audit se corrige dans l'onglet
fautif (libellé, valeur, contraste), dans un commit à part
`fix(macos): …`, puis on relance cette étape.

---

## Couverture de la spec

| Exigence de la spec | Tâche |
|---|---|
| Onglet Voix : modèle actif, voix + Écouter, langue du modèle, vitesse, volume, voix clonée | 2, 3, 7 |
| Onglet Lecture : police, taille, interlignage, règle, blocs de code, lecture à l'ouverture, pas d'avance rapide | 1, 8, 12 (pas réellement appliqué) |
| Onglet Modèles : catalogue décrit, installer, annuler, supprimer confirmé, espace libre | 2, 4, 10 |
| Onglet Raccourcis | 9 |
| Onglet Avancé : moteur, calcul, fils, sessions, phrases d'avance, échantillonnage, `[extra]` | 1, 11 |
| Onglet Général : connexion, voix gardée, cache, signets, dossier de configuration | 4, 9 |
| Écrou ⚙ 13 pt, clavier, « Réglages de la voix », onglet Voix ; `⌘,` → dernier onglet | 7, 12, 13 |
| Réglages ignorés cachés, case « Afficher aussi… » (Voix et Avancé), « sans effet sur ce modèle », filtre qui suit le modèle | 2, 6, 7, 11 |
| Tout s'applique tout de suite, écriture après une pause (`Committer`) | 6 |
| Effet par nature : resynthèse, remise en forme, rechargement, volume, affichage | 1 |
| Rechargement : « Voix en chargement… » panneau et onglet, reprise à la même phrase | 1, 7, 12 |
| Bornes du moteur, interlignage 0–3 et pas 1–50 partout ; langues du modèle | 1, 2, 5, 8 |
| Écouter : sans texte, `Espace` sur la liste, un essai coupe le précédent, refus expliqués | 3, 6, 7 |
| Supprimer : alerte, jamais le modèle actif | 4, 10 |
| Valeurs non listées affichées, jamais réécrites | 1, 6, 7, 8, 9, 11 |
| Accessibilité : clavier, libellés et valeurs VoiceOver, 13 pt, audit | 7–13 |
| Erreurs : bornes, écriture impossible, rechargement échoué (repli + lien Modèles), aperçu impossible, suppression échouée | 1, 6, 7, 3, 10 |
| Rust : `SettingsView` complet, `SettingsPatch`, `ModelCapabilities`, catalogue enrichi, actions, FFI | 1–5 |
| Swift : six fichiers d'onglets, modèle de vue (réglages, capacités, ignorés), `SettingsOpener` par onglet, appels hors fil principal | 5–12 |
| Tests Rust, Swift unitaires, tests d'interface (avec feu vert) | 1–13 |
| Hors périmètre respecté : chemins non modifiables (seul « ouvrir le dossier »), pas de positions, pas d'import/export | 9 |

Décisions prises là où la spec se tait :

- **Remise en forme à la même position** : la position suit la phrase de
  *texte* (rang parmi les phrases hors code), pas son numéro (Tâche 1).
- **Effet mesuré sur la configuration**, pas sur les champs présents :
  renvoyer une valeur déjà en place ne relance rien (Tâche 1).
- **Échantillonnage borné** dans `normalised` (bornes de l'écran du terminal),
  un `NaN` rend la main au modèle (Tâche 1).
- **Le terminal aussi** propose les langues du modèle (Tâche 2).
- **Lecteur de test hors disque** : l'app lancée avec `--ui-testing` n'écrit
  pas `config.toml` et refuse tout effacement (Tâches 1, 4).
- **Changer de modèle recharge en place** (`use_voice`), comme les autres
  réglages du moteur (Tâche 1).
- **Gérer les signets** = les lister tous, par document, et en retirer un
  (Tâches 4, 9).
- **L'écrou au clavier** : touche `v` dans le panneau, en plus de `⌘,`
  (Tâche 12).
- **Voix clonée** : proposée quand le modèle l'exige *ou* n'a aucune voix
  intégrée (l'exigence n'est connue qu'une fois la voix chargée) (Tâche 7).
- **Fenêtre ouverte** : l'état de la voix y est relu à 4 Hz, sous verrou,
  sans passer par le fil du lecteur (Tâche 7) ; vérifié à la main (Tâche 12).
- **Repli après un modèle introuvable sur disque** : le moteur se replie sans
  passer par l'état « repli » (avis perdu par la fabrique du FFI) ; le cas ne
  survient pas depuis l'app, qui ne propose que des modèles installés et
  refuse de supprimer l'actif. Non traité ici.
