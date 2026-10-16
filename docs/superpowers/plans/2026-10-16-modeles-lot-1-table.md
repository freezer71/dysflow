# Modèles, lot 1 : table, choix, voix unique, suppression — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sur la page des réglages, une table Lecture (Kokoro, voix de macOS) et une table Dictée (dictée de macOS) : télécharger, annuler, utiliser, **supprimer** (même le modèle utilisé), et une seule voix choisie parmi toutes celles du modèle.

**Architecture:** Le cœur Rust garde le catalogue et les fichiers (`src/models.rs`, `src/reader/`) ; on étend l'API existante (`voices`, `install_voice`, `use_voice`, `remove_model`, `cancel_download`) au lieu d'en créer une. La voix de macOS devient une ligne « system » choisie explicitement (`engine = "system"`). L'app Swift dessine la table (logique pure dans `ModelTable`, testée ; vue dans `ModelsSection`).

**Tech Stack:** Rust (cœur, `uniffi` pour le pont), Swift 6 / SwiftUI macOS, xcodegen.

**Spec:** `docs/superpowers/specs/2026-10-16-modeles-stt-tts-design.md` (lot 1).

## Global Constraints

- App macOS seulement ; le terminal ne change pas de comportement visible (il garde son choix de voix par langue, qui continue de marcher).
- Une seule voix pour la lecture, quelle que soit la langue du texte ; par défaut `p-fenrir60`.
- Supprimer : possible sur tout modèle installé, y compris celui utilisé ; confirmation toujours ; le modèle utilisé supprimé → voix de macOS.
- Lignes « système » (voix de macOS, dictée de macOS) : toujours installées, jamais supprimables.
- Un seul téléchargement à la fois.
- Tests macOS : **jamais `xcodebuild test` en local** (l'app s'ouvre et bloque le Mac) — compiler seulement (`build-for-testing`), les tests Swift tournent sur la CI.
- Textes de l'interface en français, typographie française (espace avant « : », guillemets « »).
- Commits : message en français, `feat(macos):` / `feat(moteur):` / `feat(ffi):`, fin `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**Écart à la spec, assumé :** le choix du modèle de dictée reste dans `UserDefaults` (`dictationModel`, comme la langue de dictée aujourd'hui), pas dans `config.toml` : la dictée vit entièrement côté Swift. Au lot 1 il n'a qu'une valeur, `system`.

**Écart à la spec, assumé :** la table est une `Grid` SwiftUI à en-tête, pas une `Table` : une `Table` défile seule et se coince dans la page des réglages, qui défile déjà.

## Commandes

- **RUST** : `cargo test --workspace --locked` puis `cargo clippy --workspace --all-targets --locked -- -D warnings`
- **FFI** : `./scripts/build-ffi.sh` (régénère `macos/Generated/sillon_ffi.swift` et `macos/Libraries/libsillon_ffi.a`) — après tout changement Rust visible du pont, avant `xcodebuild`.
- **UIBUILD** : `(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build-for-testing)`

## Review Focus

1. **Kokoro supprimé puis relance de l'app** : l'installation automatique (`ensure_default_voice`) ne doit **pas** le retélécharger quand la voix de macOS a été choisie. → test Tâche 2.
2. **Voix de macOS choisie alors que `family = "kokoro"` reste en config** : `say` ne doit pas recevoir `p-fenrir60` comme voix. → test Tâche 2.
3. **Supprimer le modèle en train de lire** : la lecture passe à la voix de macOS, sans plantage, puis les fichiers partent. → test Tâche 4.
4. **Voix anglaise de Kokoro sur un texte français** : elle est gardée (plus filtrée par langue), et la prononciation reste française. → test Tâche 1.
5. **Supprimer pendant un téléchargement du même modèle** : refusé avec un message clair (« annulez d'abord »). → test Tâche 4 (existant gardé).

---

### Task 1 : Catalogue — type de modèle, toutes les voix de Kokoro, voix libre de sa langue

**Files:**
- Modify: `src/models.rs` (struct `ModelPackage`, `CATALOG`, `KOKORO_VOICES`, `voice_for_language`, tests)

**Interfaces:**
- Produces: `pub enum Kind { Speech, Dictation }` ; champ `ModelPackage.kind: Kind` ; `KOKORO_VOICES` : 30 voix (p-fenrir60 en tête) ; `voice_for_language(family, language, configured)` accepte toute voix de la famille, quelle que soit sa langue.

- [ ] **Step 1 : tests qui échouent** — dans `mod tests` de `src/models.rs` :

```rust
#[test]
fn every_kokoro_voice_of_the_package_is_offered() {
    let offered: Vec<&str> = voices_for_family("kokoro").iter().map(|v| v.id).collect();
    assert_eq!(offered[0], "p-fenrir60", "la recommandée en tête");
    for companion in KOKORO_SHARED.iter().filter(|c| c.dest.starts_with("voices/")) {
        let id = companion.dest.trim_start_matches("voices/").trim_end_matches(".safetensors");
        assert!(offered.contains(&id), "{id} manque au choix");
    }
    assert_eq!(offered.len(), 30);
}

#[test]
fn a_voice_reads_any_language_of_its_model() {
    // Une seule voix pour tout : af_bella lit aussi le français.
    assert_eq!(voice_for_language("kokoro", "fr", Some("af_bella")), "af_bella");
    assert_eq!(voice_for_language("kokoro", "en", Some("p-fenrir60")), "p-fenrir60");
    // Une voix inconnue retombe sur la recommandée de la langue.
    assert_eq!(voice_for_language("kokoro", "fr", Some("inconnue")), "p-fenrir60");
}

#[test]
fn kokoro_reads_aloud() {
    assert!(CATALOG.iter().all(|p| p.kind == Kind::Speech));
}
```

- [ ] **Step 2 : lancer** `cargo test -p sillon models::tests` — attendu : échec de compilation (`Kind` inconnu, `KOKORO_SHARED` privé au module mais visible des tests : OK).

- [ ] **Step 3 : implémenter.**

Ajouter après `enum Runtime` :

```rust
/// Ce que fait un modèle : lire à voix haute, ou écrire ce qu'on dicte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Speech,
    Dictation,
}
```

Dans `ModelPackage`, après `runtime` : `pub kind: Kind,` ; dans les deux entrées Kokoro de `CATALOG` : `kind: Kind::Speech,`.

Remplacer `KOKORO_VOICES` (et son commentaire) par :

```rust
/// Voix de Kokoro : toutes celles du paquet, plus `p-fenrir60` livrée avec
/// Sillon (16 oct. 2026 : une seule voix pour tout, choisie librement).
///
/// `language` dit l'accent de la voix, pas ce qu'elle a le droit de lire :
/// la prononciation suit la langue du texte. La première d'une langue est
/// sa voix par défaut.
const KOKORO_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr" },
    VoiceOption { id: "ff_siwis", label: "ff_siwis · française, femme", language: "fr" },
    VoiceOption { id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en" },
    VoiceOption { id: "af_heart", label: "af_heart · américaine, femme", language: "en" },
    VoiceOption { id: "af_alloy", label: "af_alloy · américaine, femme", language: "en" },
    VoiceOption { id: "af_aoede", label: "af_aoede · américaine, femme", language: "en" },
    VoiceOption { id: "af_jessica", label: "af_jessica · américaine, femme", language: "en" },
    VoiceOption { id: "af_kore", label: "af_kore · américaine, femme", language: "en" },
    VoiceOption { id: "af_nicole", label: "af_nicole · américaine, femme", language: "en" },
    VoiceOption { id: "af_nova", label: "af_nova · américaine, femme", language: "en" },
    VoiceOption { id: "af_river", label: "af_river · américaine, femme", language: "en" },
    VoiceOption { id: "af_sarah", label: "af_sarah · américaine, femme", language: "en" },
    VoiceOption { id: "af_sky", label: "af_sky · américaine, femme", language: "en" },
    VoiceOption { id: "am_fenrir", label: "am_fenrir · américaine, homme", language: "en" },
    VoiceOption { id: "am_adam", label: "am_adam · américaine, homme", language: "en" },
    VoiceOption { id: "am_echo", label: "am_echo · américaine, homme", language: "en" },
    VoiceOption { id: "am_eric", label: "am_eric · américaine, homme", language: "en" },
    VoiceOption { id: "am_liam", label: "am_liam · américaine, homme", language: "en" },
    VoiceOption { id: "am_michael", label: "am_michael · américaine, homme", language: "en" },
    VoiceOption { id: "am_onyx", label: "am_onyx · américaine, homme", language: "en" },
    VoiceOption { id: "am_puck", label: "am_puck · américaine, homme", language: "en" },
    VoiceOption { id: "am_santa", label: "am_santa · américaine, homme", language: "en" },
    VoiceOption { id: "bf_alice", label: "bf_alice · britannique, femme", language: "en" },
    VoiceOption { id: "bf_emma", label: "bf_emma · britannique, femme", language: "en" },
    VoiceOption { id: "bf_isabella", label: "bf_isabella · britannique, femme", language: "en" },
    VoiceOption { id: "bf_lily", label: "bf_lily · britannique, femme", language: "en" },
    VoiceOption { id: "bm_daniel", label: "bm_daniel · britannique, homme", language: "en" },
    VoiceOption { id: "bm_fable", label: "bm_fable · britannique, homme", language: "en" },
    VoiceOption { id: "bm_george", label: "bm_george · britannique, homme", language: "en" },
    VoiceOption { id: "bm_lewis", label: "bm_lewis · britannique, homme", language: "en" },
];
```

Dans `voice_for_language`, retirer le filtre de langue :

```rust
/// La voix qui parle `language` : celle de la configuration si la famille la
/// connaît (quelle que soit son accent : une voix pour tout), sinon la voix
/// par défaut de la langue — sans réécrire la configuration.
pub fn voice_for_language(family: &str, language: &str, configured: Option<&str>) -> &'static str {
    let code = language_code(language);
    configured
        .map(str::trim)
        .and_then(|c| voices_for_family(family).iter().find(|v| v.id == c))
        .map_or_else(|| default_voice_for_language(family, code), |v| v.id)
}
```

Mettre à jour `KOKORO_VOICES_LABEL` : `"30 voix : p-fenrir60 (française, recommandée), af_bella (américaine, recommandée)…"` ; et le commentaire de `KOKORO_SHARED` (« les 29 voix du paquet sont toutes proposées »).

- [ ] **Step 4 : lancer** **RUST**. Des tests existants qui supposaient 4 voix ou le filtre de langue (`src/app/config.rs` : cycle des voix anglaises, `src/reader/language.rs`) échoueront : les corriger pour le nouveau comportement (le cycle du terminal parcourt désormais les 30 voix ; adapter les valeurs attendues, pas le code du terminal), sauf s'ils révèlent un vrai défaut. Attendu : tout vert.
- [ ] **Step 5 : commit** `feat(moteur): toutes les voix de Kokoro au choix, chacune libre de lire toute langue ; type Lecture/Dictée au catalogue`

---

### Task 2 : Voix unique, voix de macOS choisie, pas de réinstallation forcée

**Files:**
- Modify: `src/tts/params.rs` (`tts_params_in`)
- Modify: `src/reader/settings.rs` (`SettingsPatch.family_voice`, `apply`)
- Modify: `src/reader/migration.rs` (`ensure_default_voice`, `migrate_voice` → appel de `unify_voices`)
- Modify: `src/models.rs` (`impl Config { fn unify_voices }`)
- Modify: `ffi/src/lib.rs` (record `SettingsPatch` du pont : `family_voice: Option<String>` et sa conversion)

**Interfaces:**
- Consumes: `voice_for_language` (Tâche 1).
- Produces: `SettingsPatch.family_voice: Option<String>` — écrit la même voix pour `fr` et `en` dans `[voice.per_language]` ; `Config::unify_voices(&mut self) -> bool` ; règle : `engine = "system"` ⇒ `TtsParams.voice = voice.voice`.

- [ ] **Step 1 : tests qui échouent.**

Dans `src/tts/params.rs` (ajouter `#[cfg(test)] mod tests` s'il n'existe pas) :

```rust
#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn the_system_voice_never_receives_a_kokoro_voice() {
        let mut c = Config::default();
        c.engine.family = "kokoro".into();
        c.engine.engine = "system".into();
        c.voice.voice = "Thomas".into();
        c.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        assert_eq!(c.tts_params_in("fr").voice, "Thomas");
    }
}
```

Dans `src/models.rs`, tests :

```rust
#[test]
fn voices_are_unified_on_the_french_one() {
    let mut c = crate::config::Config::default();
    c.engine.family = "kokoro".into();
    c.voice.per_language.insert("fr".into(), "p-fenrir60".into());
    c.voice.per_language.insert("en".into(), "af_bella".into());
    assert!(c.unify_voices());
    assert_eq!(c.voice.per_language["en"], "p-fenrir60");
    assert!(!c.unify_voices(), "déjà unifiées : rien à écrire");
}
```

Dans `src/reader/settings.rs`, tests :

```rust
#[test]
fn one_voice_for_every_language() {
    let mut c = Config::default();
    SettingsPatch { family_voice: Some("am_fenrir".into()), ..Default::default() }.apply(&mut c);
    assert_eq!(c.voice.per_language["fr"], "am_fenrir");
    assert_eq!(c.voice.per_language["en"], "am_fenrir");
}
```

Dans `src/reader/migration.rs`, tests (reprendre le constructeur `reader_with`/`quiet` des tests du fichier) :

```rust
#[test]
fn the_chosen_system_voice_is_not_replaced_by_an_automatic_install() {
    let mut config = quiet();
    config.engine.engine = "system".into();
    let mut r = reader_with(config);
    r.ensure_default_voice(Instant::now()).unwrap();
    assert!(r.download().is_none(), "rien ne se télécharge : l'utilisateur a choisi macOS");
}
```

- [ ] **Step 2 : lancer** **RUST** — attendu : échecs (`family_voice`, `unify_voices` inconnus ; voix « p-fenrir60 » au lieu de « Thomas » ; téléchargement lancé).

- [ ] **Step 3 : implémenter.**

`src/tts/params.rs`, début de `tts_params_in` :

```rust
let family = &self.engine.family;
let system = self.engine_preference() == EnginePreference::System;
let voice = if !system && crate::models::voices_follow_language(family) {
```

(le reste inchangé ; la branche `else` donne déjà `voice.voice`).

`src/models.rs`, dans `impl crate::config::Config` :

```rust
/// Une voix pour tout (16 oct. 2026) : la voix française de Kokoro devient
/// celle de chaque langue. Vrai si quelque chose a changé.
pub fn unify_voices(&mut self) -> bool {
    if !voices_follow_language(&self.engine.family) {
        return false;
    }
    let voice = self
        .voice
        .per_language
        .get("fr")
        .cloned()
        .unwrap_or_else(|| default_voice_for_language(&self.engine.family, "fr").to_string());
    let mut changed = false;
    for language in ["fr", "en"] {
        if self.voice.per_language.get(language) != Some(&voice) {
            self.voice.per_language.insert(language.to_string(), voice.clone());
            changed = true;
        }
    }
    changed
}
```

`src/reader/settings.rs` : champ `pub family_voice: Option<String>,` (doc : « Une voix pour toutes les langues du modèle (Kokoro). ») ; dans `apply`, après `voice_for_language` :

```rust
if let Some(voice) = &self.family_voice {
    for language in ["fr", "en"] {
        c.voice.per_language.insert(language.to_string(), voice.clone());
    }
}
```

`src/reader/migration.rs`, `ensure_default_voice`, juste après le test `persist` :

```rust
// La voix de macOS choisie : on ne la remplace pas par Kokoro.
if self.current_config().engine_preference() == crate::tts::EnginePreference::System {
    return Ok(());
}
```

`migrate_voice`, en tête : unifier et enregistrer (jamais sur config illisible) :

```rust
if !self.config_unreadable {
    self.sync_config();
    if self.config.unify_voices() {
        if let Some(app) = self.session.as_mut() {
            app.config = self.config.clone();
        }
        let _ = self.save_config();
    }
}
```

`ffi/src/lib.rs` : ajouter `pub family_voice: Option<String>,` au record `SettingsPatch` du pont, et `family_voice: p.family_voice,` dans sa conversion vers `sillon::reader::settings::SettingsPatch`.

- [ ] **Step 4 : lancer** **RUST** puis **FFI** — attendu : vert, bibliothèque régénérée.
- [ ] **Step 5 : commit** `feat(moteur): une voix pour toutes les langues (family_voice, unification au lancement) ; la voix de macOS choisie reçoit sa voix et n'est plus remplacée par l'installation automatique`

---

### Task 3 : Les voix de macOS au choix

**Files:**
- Create: `src/tts/system_voices.rs`
- Modify: `src/tts/mod.rs` (`pub mod system_voices;`)
- Modify: `src/reader/settings.rs` (`capabilities`)

**Interfaces:**
- Produces: `pub fn parse_say_voices(listing: &str) -> Vec<SystemVoice>` ; `pub fn system_voices() -> &'static [SystemVoice]` (lu une fois) ; `pub struct SystemVoice { pub name: String, pub locale: String }` ; `capabilities()` rend `family = "system"` et ces voix (français et anglais seulement) quand `engine = "system"`.

- [ ] **Step 1 : test qui échoue** — dans `src/tts/system_voices.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "\
Amélie              fr_CA    # Bonjour, je m’appelle Amélie.
Eddy (French (France)) fr_FR    # Bonjour, je m’appelle Eddy.
Samantha            en_US    # Hello! My name is Samantha.
Kyoko               ja_JP    # こんにちは、私の名前はKyokoです。
";

    #[test]
    fn names_may_hold_spaces_and_parentheses() {
        let voices = parse_say_voices(LISTING);
        assert_eq!(voices[1], SystemVoice { name: "Eddy (French (France))".into(), locale: "fr_FR".into() });
    }

    #[test]
    fn only_french_and_english_voices_are_kept() {
        let names: Vec<String> = parse_say_voices(LISTING).into_iter().map(|v| v.name).collect();
        assert_eq!(names, ["Amélie", "Eddy (French (France))", "Samantha"]);
    }
}
```

- [ ] **Step 2 : lancer** `cargo test -p sillon system_voices` — attendu : échec (module vide).

- [ ] **Step 3 : implémenter.**

```rust
//! Les voix que `say` sait prendre : celles installées sur ce Mac.
//!
//! L'app ne peut pas en télécharger d'autres (aucune API) : l'utilisateur
//! les ajoute dans Réglages Système > Accessibilité > Contenu énoncé.

use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemVoice {
    pub name: String,
    /// `fr_FR`, `en_US`…
    pub locale: String,
}

impl SystemVoice {
    /// `fr` ou `en`.
    pub fn language(&self) -> &str {
        self.locale.split('_').next().unwrap_or("")
    }
}

/// Lit la sortie de `say -v '?'` : « Nom (peut avoir des espaces) locale # phrase ».
/// Garde le français et l'anglais, dans l'ordre de `say`.
pub fn parse_say_voices(listing: &str) -> Vec<SystemVoice> {
    listing
        .lines()
        .filter_map(|line| {
            let head = line.split('#').next()?.trim_end();
            let (name, locale) = head.rsplit_once(char::is_whitespace)?;
            Some(SystemVoice { name: name.trim().to_string(), locale: locale.to_string() })
        })
        .filter(|v| matches!(v.language(), "fr" | "en"))
        .collect()
}

/// Les voix de ce Mac, lues une fois. Vide hors macOS ou si `say` manque.
pub fn system_voices() -> &'static [SystemVoice] {
    static VOICES: OnceLock<Vec<SystemVoice>> = OnceLock::new();
    VOICES.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        Command::new("say")
            .args(["-v", "?"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| parse_say_voices(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    })
}
```

Dans `src/reader/settings.rs`, `capabilities()` :

```rust
pub fn capabilities(&self) -> ModelCapabilities {
    let config = self.current_config();
    if config.engine_preference() == crate::tts::EnginePreference::System {
        return ModelCapabilities {
            family: "system".to_string(),
            voices: crate::tts::system_voices::system_voices()
                .iter()
                .map(|v| VoiceChoice { id: v.name.clone(), label: format!("{} · {}", v.name, v.locale), language: v.language().to_string() })
                .collect(),
            languages: vec!["fr".into(), "en".into()],
            honoured: vec!["voice".into()],
        };
    }
    let family = config.engine.family.clone();
    // … suite existante inchangée (en utilisant `family`)
}
```

Test dans `src/reader/settings.rs` :

```rust
#[test]
fn the_system_voice_offers_the_voices_of_the_mac() {
    let mut config = Config::default();
    config.engine.engine = "system".into();
    let r = reader_with(config);
    assert_eq!(r.capabilities().family, "system");
}
```

(utiliser le constructeur de test déjà présent dans ce fichier ; s'il s'appelle autrement, le reprendre tel quel.)

- [ ] **Step 4 : lancer** **RUST** — attendu : vert.
- [ ] **Step 5 : commit** `feat(moteur): les voix de macOS (say -v ?) proposées quand la voix de macOS est choisie`

---

### Task 4 : Lignes de la table — ligne système, place sur le disque, suppression du modèle utilisé

**Files:**
- Modify: `src/reader/settings.rs` (`VoiceModel`, `voice_model`, `voices`, `use_voice`)
- Modify: `src/reader/data.rs` (`removal_refusal`, `remove_model`, tests)
- Modify: `ffi/src/lib.rs` (record `VoiceModel`, `voices()`, `remove_model`, nouvelle `models_disk_usage`)

**Interfaces:**
- Consumes: `Kind` (Tâche 1), règle `engine = "system"` (Tâche 2).
- Produces:
  - `VoiceModel` gagne `kind: String` (`"speech"`/`"dictation"`), `disk_bytes: u64`, `system: bool`.
  - `voices()` rend d'abord la ligne `id = "system"` (« Voix de macOS », installée, `active` si `engine = "system"`), puis le catalogue ; un paquet n'est `active` que si `engine != "system"`.
  - `use_voice("system")` : `engine = "system"`, rechargement, enregistrement ; `use_voice(<paquet>)` remet `engine = "auto"`.
  - `remove_model(id)` : si actif, passe d'abord à `system` puis supprime ; refuse `system` ; refuse pendant son propre téléchargement.
  - FFI `models_disk_usage() -> u64` : octets occupés par les modèles du catalogue de l'app.

- [ ] **Step 1 : tests qui échouent** — dans `src/reader/data.rs`, remplacer `the_active_model_cannot_be_removed` par :

```rust
#[test]
fn removing_the_active_model_switches_to_the_system_voice_first() {
    let package = &DISPOSABLE;
    let path = install_fake(package);
    let mut config = quiet();
    config.engine.model_path = path.to_string_lossy().into_owned();
    let mut r = reader_with(config);
    assert_eq!(r.removal_refusal(package), None, "le modèle utilisé se supprime aussi");
    r.remove_package_at(package, Instant::now()).unwrap();
    assert_eq!(r.config().engine.engine, "system");
    assert!(!path.exists());
}

#[test]
fn the_system_row_cannot_be_removed() {
    let mut r = reader_with(quiet());
    assert!(r.remove_model("system").unwrap_err().to_string().contains("macOS"));
}
```

Dans `src/reader/settings.rs` :

```rust
#[test]
fn the_system_voice_comes_first_and_is_active_when_chosen() {
    let mut config = Config::default();
    config.engine.engine = "system".into();
    let r = reader_with(config);
    let rows = r.voices();
    assert_eq!(rows[0].id, "system");
    assert!(rows[0].installed && rows[0].active && rows[0].system);
    assert!(rows.iter().skip(1).all(|row| !row.active));
}
```

- [ ] **Step 2 : lancer** **RUST** — attendu : échec de compilation (`remove_package_at`, `system` inconnus).

- [ ] **Step 3 : implémenter.**

`VoiceModel` : ajouter

```rust
/// `speech` (lecture) ou `dictation`.
pub kind: String,
/// Octets réellement sur le disque (0 si rien).
pub disk_bytes: u64,
/// Ligne du système (voix de macOS) : jamais téléchargée ni supprimée.
pub system: bool,
```

`voice_model(p, active_path, system_chosen: bool)` : `active: !system_chosen && …` (inchangé sinon), `kind: match p.kind { Kind::Speech => "speech", Kind::Dictation => "dictation" }.into()`, `disk_bytes: models::installed_bytes(p)`, `system: false`.

Ligne système :

```rust
fn system_voice_row(active: bool) -> VoiceModel {
    VoiceModel {
        id: "system".into(),
        name: "Voix de macOS".into(),
        bytes: 0,
        installed: true,
        active,
        recommended: false,
        summary: "Les voix installées sur ce Mac (Réglages Système › Accessibilité › Contenu énoncé).".into(),
        family: "system".into(),
        precision: String::new(),
        languages_label: "Selon les voix installées".into(),
        languages: vec!["fr".into(), "en".into()],
        pace: "Immédiate".into(),
        voices_label: String::new(),
        licenses: "Apple".into(),
        kind: "speech".into(),
        disk_bytes: 0,
        system: true,
    }
}

pub fn voices(&self) -> Vec<VoiceModel> {
    let config = self.current_config();
    let system = config.engine_preference() == crate::tts::EnginePreference::System;
    let active = config.engine.model_path.clone();
    std::iter::once(system_voice_row(system))
        .chain(models::available_with(self.mlx).into_iter().map(|p| voice_model(p, &active, system)))
        .collect()
}
```

`use_voice` : en tête,

```rust
if id == "system" {
    self.sync_config();
    if self.config.engine_preference() == crate::tts::EnginePreference::System {
        return Ok(());
    }
    self.config.engine.engine = "system".to_string();
    if let Some(app) = self.session.as_mut() {
        app.config = self.config.clone();
    }
    self.reload_voice(now);
    return self.save_config();
}
```

et, avant le test « déjà la voix active », la condition devient `… && self.config.engine.engine != "system"` ; dans `switch_voice_config`, ajouter `self.config.engine.engine = "auto".to_string();`.

`src/reader/data.rs` :

```rust
pub fn removal_refusal(&self, package: &ModelPackage) -> Option<String> {
    if self.download.as_ref().is_some_and(|d| d.package.id == package.id && !d.is_finished()) {
        return Some(format!("{} se télécharge : annulez d'abord le téléchargement", package.display_name));
    }
    None
}

pub fn remove_model(&mut self, id: &str) -> Result<()> {
    if id == "system" {
        bail!("Les voix de macOS se gèrent dans Réglages Système");
    }
    let package = self.package(id)?;
    self.remove_package_at(package, std::time::Instant::now())
}

/// Le modèle utilisé passe d'abord la main à la voix de macOS (la lecture
/// continue avec elle), puis ses fichiers partent.
pub(crate) fn remove_package_at(&mut self, package: &ModelPackage, now: std::time::Instant) -> Result<()> {
    if let Some(reason) = self.removal_refusal(package) {
        bail!(reason);
    }
    if !self.persist {
        bail!(TRIAL);
    }
    let config = self.current_config();
    if config.engine_preference() != crate::tts::EnginePreference::System
        && models::is_active_package(&config.engine.model_path, package)
    {
        self.use_voice("system", now)?;
    }
    models::remove(package)
}
```

Supprimer l'ancien `remove_package` et faire pointer ses appels sur `remove_package_at(…, Instant::now())`.

`ffi/src/lib.rs` : ajouter `kind`, `disk_bytes`, `system` au record `VoiceModel` et à sa conversion dans `voices()` ; nouvelle méthode exportée :

```rust
/// Octets occupés par les modèles que l'app sait charger.
pub fn models_disk_usage(&self) -> u64 {
    sillon::models::available_with(true).into_iter().map(sillon::models::installed_bytes).sum()
}
```

- [ ] **Step 4 : lancer** **RUST** puis **FFI** — attendu : vert.
- [ ] **Step 5 : commit** `feat(moteur): la voix de macOS est une ligne qu'on choisit ; supprimer le modèle utilisé passe d'abord à elle ; place occupée par les modèles`

---

### Task 5 : Swift — logique de la table (pure, testée) et pont

**Files:**
- Create: `macos/Sources/Settings/ModelTable.swift`
- Modify: `macos/Sources/Model/ReaderModel+Engine.swift` (méthodes de modèles)
- Modify: `macos/Sources/Settings/SettingsStore.swift` (`setFamilyVoice`)
- Create: `macos/Tests/ModelTableTests.swift`
- Modify: `macos/Tests/FakeSettingsBackend.swift` (`family_voice`)

**Interfaces:**
- Consumes: FFI `voices()`, `installVoice(id:)`, `useVoice(id:)`, `removeModel(id:)`, `cancelDownload()`, `modelsDiskUsage()`, `freeSpace()`, `VoiceModel.{kind,diskBytes,system}`, `SettingsPatch.familyVoice`, `Snapshot.download`.
- Produces:
  - `enum ModelRowState: Equatable { case system, notInstalled, downloading(Double), failed(String), installed }`
  - `enum ModelTable` : `static func state(_ row: VoiceModel, download: DownloadView?) -> ModelRowState`, `static func size(_ row: VoiceModel) -> String`, `static func confirmation(_ row: VoiceModel, fallback: String) -> (title: String, message: String)`, `static func canDownload(_ row: VoiceModel, download: DownloadView?) -> Bool`, `static func footer(used: UInt64, free: UInt64?) -> String`
  - `ReaderModel` : `loadModels() async -> [VoiceModel]`, `installModel(_:) async throws`, `useModel(_:) async throws`, `removeModel(_:) async throws`, `cancelDownload() async throws`, `modelsDiskUsage() async -> UInt64`, `freeSpace() async -> UInt64?`
  - `SettingsStore.setFamilyVoice(_ voice: String)`

- [ ] **Step 1 : tests qui échouent** — `macos/Tests/ModelTableTests.swift` :

```swift
import XCTest
@testable import Sillon

final class ModelTableTests: XCTestCase {
    func row(_ id: String = "kokoro_82m_mlx", installed: Bool = false, active: Bool = false,
             bytes: UInt64 = 342_000_000, disk: UInt64 = 0, system: Bool = false) -> VoiceModel {
        VoiceModel(id: id, name: system ? "Voix de macOS" : "Kokoro 82M · MLX", bytes: bytes,
                   installed: installed, active: active, recommended: false, summary: "",
                   family: system ? "system" : "kokoro", precision: "bf16", languagesLabel: "Français, anglais",
                   languages: ["fr", "en"], pace: "", voicesLabel: "", licenses: "",
                   kind: "speech", diskBytes: disk, system: system)
    }

    func testStates() {
        XCTAssertEqual(ModelTable.state(row(system: true), download: nil), .system)
        XCTAssertEqual(ModelTable.state(row(), download: nil), .notInstalled)
        XCTAssertEqual(ModelTable.state(row(installed: true), download: nil), .installed)
        let half = DownloadView(id: "kokoro_82m_mlx", received: 50, total: 100, failed: nil)
        XCTAssertEqual(ModelTable.state(row(), download: half), .downloading(0.5))
        let failed = DownloadView(id: "kokoro_82m_mlx", received: 0, total: 100, failed: "réseau")
        XCTAssertEqual(ModelTable.state(row(), download: failed), .failed("réseau"))
        // Le téléchargement d'un autre modèle ne change pas cette ligne.
        let other = DownloadView(id: "autre", received: 1, total: 2, failed: nil)
        XCTAssertEqual(ModelTable.state(row(), download: other), .notInstalled)
    }

    func testOneDownloadAtATime() {
        let running = DownloadView(id: "autre", received: 1, total: 2, failed: nil)
        XCTAssertFalse(ModelTable.canDownload(row(), download: running))
        XCTAssertTrue(ModelTable.canDownload(row(), download: nil))
    }

    func testSizeIsWhatIsOnDiskOnceInstalled() {
        XCTAssertEqual(ModelTable.size(row(system: true)), "—")
        XCTAssertFalse(ModelTable.size(row(installed: true, disk: 1_000_000)).isEmpty)
    }

    func testConfirmationSaysWhatIsFreedAndWhatTakesOver() {
        let inUse = ModelTable.confirmation(row(installed: true, active: true, disk: 342_000_000),
                                            fallback: "La lecture passera à la voix de macOS.")
        XCTAssertEqual(inUse.title, "Supprimer Kokoro 82M · MLX ?")
        XCTAssertTrue(inUse.message.contains("seront libérés"))
        XCTAssertTrue(inUse.message.contains("voix de macOS"))
        let idle = ModelTable.confirmation(row(installed: true, disk: 342_000_000), fallback: "x")
        XCTAssertFalse(idle.message.contains("x."))
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec de compilation (`ModelTable` inconnu).

- [ ] **Step 3 : implémenter** `macos/Sources/Settings/ModelTable.swift` :

```swift
import Foundation

/// L'état d'une ligne de la table des modèles.
enum ModelRowState: Equatable {
    case system
    case notInstalled
    case downloading(Double)
    case failed(String)
    case installed
}

/// Ce que la table des modèles montre, sans vue : testable seul.
enum ModelTable {
    static func state(_ row: VoiceModel, download: DownloadView?) -> ModelRowState {
        if row.system { return .system }
        if let d = download, d.id == row.id {
            if let failed = d.failed { return .failed(failed) }
            if d.received < d.total { return .downloading(d.total == 0 ? 0 : Double(d.received) / Double(d.total)) }
        }
        return row.installed ? .installed : .notInstalled
    }

    /// Un seul téléchargement à la fois.
    static func canDownload(_ row: VoiceModel, download: DownloadView?) -> Bool {
        !row.system && !row.installed && !ReaderModel.isDownloading(download)
    }

    static func bytes(_ count: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(count), countStyle: .file)
    }

    /// Sur le disque si installé, sinon ce qu'il faut télécharger.
    static func size(_ row: VoiceModel) -> String {
        if row.system { return "—" }
        return bytes(row.installed ? row.diskBytes : row.bytes)
    }

    static func confirmation(_ row: VoiceModel, fallback: String) -> (title: String, message: String) {
        var message = "\(bytes(row.diskBytes)) seront libérés. Il faudra le retélécharger pour l'utiliser."
        if row.active { message += " " + fallback }
        return ("Supprimer \(row.name) ?", message)
    }

    static func footer(used: UInt64, free: UInt64?) -> String {
        let freeText = free.map { " · \(bytes($0)) libres" } ?? ""
        return "\(bytes(used)) utilisés par les modèles\(freeText)"
    }
}
```

`ReaderModel+Engine.swift`, section « Voix » :

```swift
/// Les lignes de la table : la voix de macOS, puis le catalogue.
func loadModels() async -> [VoiceModel] { await call { $0.voices() } }

func installModel(_ id: String) async throws {
    try await tryCall { try $0.installVoice(id: id) }
    wake()
}

func useModel(_ id: String) async throws {
    try await tryCall { try $0.useVoice(id: id) }
    wake()
    onVoiceMayHaveChanged?()
}

func removeModel(_ id: String) async throws {
    try await tryCall { try $0.removeModel(id: id) }
    wake()
    onVoiceMayHaveChanged?()
}

func cancelDownload() async throws {
    try await tryCall { try $0.cancelDownload() }
    wake()
}

func modelsDiskUsage() async -> UInt64 { await call { $0.modelsDiskUsage() } }
func freeSpace() async -> UInt64? { await call { $0.freeSpace() } }
```

(si `cancelDownload` ou `freeSpace` existent déjà sur `ReaderModel`, garder l'existant.)

`SettingsStore.swift`, à côté de `setVoice` :

```swift
/// Une voix pour toutes les langues (Kokoro) ; pour la voix de macOS, `voice`.
func setFamilyVoice(_ voice: String) {
    if capabilities.family == "system" {
        set(\.voice, voice, reloadsVoice: true)
    } else {
        set(\.voicesByLanguage, ["fr", "en"].map { LanguageVoice(language: $0, voice: voice) }, reloadsVoice: true)
    }
}
```

(si `set(\.voicesByLanguage, …)` passe déjà par un patch `voice_for_language` par langue — voir `SettingsDiff.swift` — y préférer `familyVoice` : ajouter dans `SettingsDiff` le cas « toutes les langues ont la même nouvelle voix » → `patch.familyVoice = voice`. Test correspondant dans `SettingsDiffTests`.)

`FakeSettingsBackend.updateSettings` : `if let v = patch.familyVoice { stored.voicesByLanguage = ["fr", "en"].map { LanguageVoice(language: $0, voice: v) } }`.

- [ ] **Step 4 : UIBUILD** — attendu : compile. Les tests Swift tournent sur la CI.
- [ ] **Step 5 : commit** `feat(macos): logique de la table des modèles (états, taille, confirmation, un téléchargement à la fois) et pont des modèles`

---

### Task 6 : Swift — la table sur la page des réglages

**Files:**
- Create: `macos/Sources/Settings/ModelsSection.swift`
- Modify: `macos/Sources/Settings/SettingsSections.swift` (`VoiceSection` : la voix unique remplace les deux sélecteurs par langue ; `DictationSection` : table d'une ligne)
- Modify: `macos/Sources/Settings/ReglagesView.swift` (charger et relire les lignes)
- Modify: `macos/Sources/Model/UITestStaging.swift` (voix Kokoro du test)
- Modify: `macos/Tests/VoiceSectionTests.swift`, `macos/Tests/FakeSettingsBackend.swift` (`Capabilities.kokoro` : 30 voix inutile — garder 4 suffit, mais la voix unique)
- Create: `macos/UITests/ModelsUITests.swift`

**Interfaces:**
- Consumes: `ModelTable`, `ModelRowState`, méthodes `ReaderModel` (Tâche 5), `SettingsStore.setFamilyVoice`.
- Produces: vue `ModelsTable(kind: String, rows: [VoiceModel], model: ReaderModel, fallback: String, onChange: () async -> Void)` ; identifiants d'accessibilité `modele-<id>`, `utiliser-<id>`, `telecharger-<id>`, `annuler-<id>`, `supprimer-<id>`, `voix-unique`, `ecouter-voix`, `place-modeles`.

- [ ] **Step 1 : test qui échoue** — dans `VoiceSectionTests.swift` :

```swift
func testOneVoiceIsShownForEveryLanguage() {
    var values = SettingsView.defaults
    values.voicesByLanguage = [LanguageVoice(language: "fr", voice: "am_fenrir"),
                               LanguageVoice(language: "en", voice: "am_fenrir")]
    XCTAssertEqual(VoiceSection.voice(in: values, Capabilities.kokoro), "am_fenrir")
}

func testAVoiceGoneFromTheModelFallsBackToItsFirst() {
    var values = SettingsView.defaults
    values.voicesByLanguage = [LanguageVoice(language: "fr", voice: "inconnue")]
    XCTAssertEqual(VoiceSection.voice(in: values, Capabilities.kokoro), "p-fenrir60")
}
```

(et retirer les tests de `voice(for:in:_:)` par langue, remplacés par ceux-ci.)

- [ ] **Step 2 : UIBUILD** — attendu : échec (`voice(in:_:)` inconnu).

- [ ] **Step 3 : implémenter.**

`ModelsSection.swift` :

```swift
import SwiftUI

/// Une table de modèles : choix, nom, taille, état et action. Une `Grid` à
/// en-tête : la page des réglages défile déjà, une `Table` s'y coincerait.
struct ModelsTable: View {
    let rows: [VoiceModel]
    let model: ReaderModel
    /// Ce qui prend la main si le modèle utilisé est supprimé.
    let fallback: String
    let onChange: () async -> Void
    @State private var confirming: VoiceModel?
    @State private var error = ErrorNote()

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 8) {
                GridRow {
                    Text("").gridColumnAlignment(.center)
                    Text("Modèle"); Text("Taille"); Text("État")
                }
                .font(Typo.detail).foregroundStyle(Palette.muted)
                Divider()
                ForEach(rows, id: \.id) { row in
                    line(row)
                }
            }
            ErrorText(error.text, identifier: "erreur-modeles")
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .confirmationDialog(confirming.map { ModelTable.confirmation($0, fallback: fallback).title } ?? "",
                            isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
                            presenting: confirming) { row in
            Button("Supprimer", role: .destructive) { act { try await model.removeModel(row.id) } }
            Button("Annuler", role: .cancel) {}
        } message: { row in
            Text(ModelTable.confirmation(row, fallback: fallback).message)
        }
    }

    @ViewBuilder private func line(_ row: VoiceModel) -> some View {
        let state = ModelTable.state(row, download: model.snapshot.download)
        GridRow {
            Button { act { try await model.useModel(row.id) } } label: {
                Image(systemName: row.active ? "largecircle.fill.circle" : "circle")
            }
            .buttonStyle(.plain)
            .disabled(!(row.installed || row.system) || row.active)
            .accessibilityLabel(row.active ? "\(row.name), utilisé" : "Utiliser \(row.name)")
            .accessibilityIdentifier("utiliser-\(row.id)")
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name)
                Text([row.languagesLabel, row.precision].filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(Typo.detail).foregroundStyle(Palette.muted)
            }
            Text(ModelTable.size(row)).monospacedDigit()
            action(row, state)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("modele-\(row.id)")
    }

    @ViewBuilder private func action(_ row: VoiceModel, _ state: ModelRowState) -> some View {
        switch state {
        case .system:
            Text(row.active ? "Utilisé" : "Sur ce Mac").foregroundStyle(Palette.muted)
        case .notInstalled:
            Button("Télécharger") { act { try await model.installModel(row.id) } }
                .buttonStyle(.notion)
                .disabled(!ModelTable.canDownload(row, download: model.snapshot.download))
                .help(ModelTable.canDownload(row, download: model.snapshot.download) ? "" : "Un téléchargement est en cours")
                .accessibilityIdentifier("telecharger-\(row.id)")
        case .downloading(let fraction):
            HStack {
                ProgressView(value: fraction).frame(width: 90)
                    .accessibilityLabel("Téléchargement de \(row.name)")
                    .accessibilityValue("\(Int(fraction * 100)) %")
                Button("Annuler") { act { try await model.cancelDownload() } }
                    .buttonStyle(.notionPlain)
                    .accessibilityIdentifier("annuler-\(row.id)")
            }
        case .failed(let message):
            HStack {
                Text(message).foregroundStyle(.red).lineLimit(1).help(message)
                Button("Réessayer") { act { try await model.installModel(row.id) } }
                    .buttonStyle(.notion)
                    .accessibilityIdentifier("telecharger-\(row.id)")
            }
        case .installed:
            Button("Supprimer…") { confirming = row }
                .buttonStyle(.notionPlain)
                .accessibilityLabel("Supprimer \(row.name)")
                .accessibilityIdentifier("supprimer-\(row.id)")
        }
    }

    private func act(_ body: @escaping () async throws -> Void) {
        Task {
            do { try await body(); error.clear() } catch { self.error.fail(SettingsStore.message(of: error)) }
            await onChange()
        }
    }
}
```

`VoiceSection` (dans `SettingsSections.swift`) : ajouter `let rows: [VoiceModel]`, `let used: UInt64`, `let free: UInt64?`, `let onChange: () async -> Void` ; en tête du groupe :

```swift
ModelsTable(rows: rows.filter { $0.kind == "speech" }, model: model,
            fallback: "La lecture passera à la voix de macOS.", onChange: onChange)
if !store.capabilities.voices.isEmpty && !Self.suggestsModels(model.snapshot.voice) {
    SettingRow(title: "Voix", detail: "Lit tout le texte, quelle que soit sa langue") {
        HStack(spacing: 8) {
            PopUpPicker(label: "Voix", identifier: "voix-unique",
                        options: store.capabilities.voices.map { ($0.label, $0.id) },
                        selection: Binding(get: { Self.voice(in: store.values, store.capabilities) },
                                           set: { store.setFamilyVoice($0) }))
                .frame(width: 260)
            Button { Task { await store.listen(language: Self.language(of: Self.voice(in: store.values, store.capabilities), store.capabilities)) } } label: {
                HStack(spacing: 5) { Image(systemName: "play.fill").font(.system(size: 10)); Text("Écouter") }
            }
            .buttonStyle(.notion).fixedSize()
            .accessibilityLabel("Écouter la voix")
            .accessibilityIdentifier("ecouter-voix")
        }
    }
    if store.capabilities.family == "system" {
        Link("Ajouter des voix dans Réglages Système…",
             destination: URL(string: "x-apple.systempreferences:com.apple.preference.universalaccess")!)
            .padding(.horizontal, 16)
    }
}
Text(ModelTable.footer(used: used, free: free))
    .font(Typo.detail).foregroundStyle(Palette.muted)
    .padding(.horizontal, 16)
    .accessibilityIdentifier("place-modeles")
```

et retirer `languageVoicePicker`, `followsLanguage`, `showsKokoroVoices`, `voices(_:language:)`, `voice(for:in:_:)`, `voiceOptions`. Nouvelles fonctions statiques :

```swift
/// La voix en place : la française (une voix pour tout ; la voix de macOS
/// garde la sienne dans `voice`) ; absente du modèle : sa première.
static func voice(in values: SettingsView, _ c: ModelCapabilities) -> String {
    let chosen = c.family == "system" ? values.voice
        : values.voicesByLanguage.first(where: { $0.language == "fr" })?.voice
    return c.voices.first(where: { $0.id == chosen })?.id ?? c.voices.first?.id ?? ""
}

/// L'aperçu se dit dans la langue de la voix.
static func language(of voice: String, _ c: ModelCapabilities) -> String {
    c.voices.first(where: { $0.id == voice })?.language ?? "fr"
}
```

`DictationSection` : ajouter en tête du groupe une table d'une ligne :

```swift
ModelsTable(rows: [Self.systemRow], model: model, fallback: "", onChange: {})
```

avec

```swift
/// Au lot 1, la seule dictée : celle de macOS (lot 2 : les modèles MLX).
static let systemRow = VoiceModel(id: "system-dictation", name: "Dictée de macOS", bytes: 0, installed: true,
    active: true, recommended: false, summary: "", family: "system", precision: "",
    languagesLabel: "Sur votre Mac : rien n'en sort", languages: ["fr", "en"], pace: "",
    voicesLabel: "", licenses: "", kind: "dictation", diskBytes: 0, system: true)
```

(`DictationSection` reçoit `model: ReaderModel`.)

`ReglagesView` : état `@State private var rows: [VoiceModel] = []`, `used: UInt64 = 0`, `free: UInt64?` ; fonction

```swift
private func reloadModels() async {
    rows = await model.loadModels()
    used = await model.modelsDiskUsage()
    free = await model.freeSpace()
    await store.reloadCapabilities()
}
```

appelée dans `.task` après `store.load()`, dans `onChange` des sections, et dans le `.onChange(of: ReaderModel.isDownloading(…))` existant (à la fin d'un téléchargement). Passer `rows/used/free/onChange: reloadModels` à `VoiceSection` et `model` à `DictationSection`.

`UITestStaging` : rien à changer pour les capacités ; ajouter un argument `--ui-testing-modeles` qui fait rendre par `voices()` du lecteur muet la ligne système + Kokoro installé (si le lecteur muet passe par le vrai `Reader`, la ligne système vient déjà du cœur : ne rien ajouter).

`macos/UITests/ModelsUITests.swift` :

```swift
import XCTest

final class ModelsUITests: XCTestCase {
    func testTheSystemVoiceRowIsThereAndCannotBeDeleted() {
        let app = XCUIApplication()
        app.launchArguments += ["--ui-testing", "--ui-testing-reglages"]
        app.launch()
        let row = app.descendants(matching: .any)["modele-system"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["supprimer-system"].exists)
        XCTAssertTrue(app.staticTexts["place-modeles"].exists)
    }
}
```

(reprendre les arguments de lancement exacts des UITests existants qui ouvrent les réglages, p. ex. dans `ReaderModesUITests.swift`.)

- [ ] **Step 4 : UIBUILD** — attendu : compile (tests unitaires et UI compris). Ne pas lancer les tests en local.
- [ ] **Step 5 : commit** `feat(macos): la table des modèles dans les réglages — Lecture (voix de macOS, Kokoro) et Dictée (macOS) ; utiliser, télécharger, annuler, supprimer avec confirmation ; une voix pour tout avec Écouter ; place occupée et libre`

---

### Task 7 : Vérification de bout en bout

**Files:** aucun nouveau (corrections seulement si un contrôle échoue).

- [ ] **Step 1 :** **RUST** complet — attendu : vert, clippy sans avertissement.
- [ ] **Step 2 :** **FFI** puis **UIBUILD** — attendu : compile.
- [ ] **Step 3 :** pousser la branche et lire la CI (tests Swift unitaires et UI) — ou, si l'utilisateur ne veut pas pousser, lui demander de lancer les tests sur la CI ; ne jamais lancer `xcodebuild test` en local.
- [ ] **Step 4 :** construire l'app (`./scripts/build-macos.sh`) et faire vérifier par l'utilisateur, à la main : choisir la voix de macOS, choisir une voix, Écouter ; revenir à Kokoro ; choisir `am_fenrir`, ouvrir un texte français (accent attendu) ; supprimer Kokoro en cours d'usage (confirmation, passage à macOS), relancer l'app : **Kokoro ne se retélécharge pas** ; le retélécharger depuis la table, l'annuler, le reprendre.
- [ ] **Step 5 :** mettre à jour la mémoire `voix-retenues` (une voix pour tout, 30 voix Kokoro) et commit final si des corrections ont été faites.

---

## Bilan d'exécution (16 oct. 2026)

Fait : tâches 1 à 6, relues une à une, puis relecture de toute la branche et une vague de corrections (offre Kokoro qui reprenait la main après un choix de macOS ; téléchargement annulé non supprimable ; « Utilisée » ; message d'échec entier ; tailles à la française ; pas de préchauffage Kokoro sous macOS ; voix du Mac relues).

Décisions prises en cours de route :
- Pas de table Dictée sur cette branche : la dictée n'est pas encore commitée dans `main`.
- Lignes en `HStack` à colonnes fixes (une ligne = un élément VoiceOver), pas `Grid`.
- L'unification des voix se fait une fois au lancement (marqueur `[app] single_voice`) et à chaque passage à Kokoro ; un choix fait ensuite au terminal est respecté.
- Choisir la voix de macOS dans l'app la choisit aussi pour le terminal (`engine` partagé).

Reporté au lot 2 : `[dictation] model` / `dictationModel` et la table Dictée ; annonce de la progression tous les 25 % ; contrôle de la place libre avant téléchargement ; voix du Mac Premium et Améliorées en tête.

À vérifier à l'œil : la colonne d'action (deux boutons dans 190 pt) ; l'élargir à ~210 pt si ça déborde.
