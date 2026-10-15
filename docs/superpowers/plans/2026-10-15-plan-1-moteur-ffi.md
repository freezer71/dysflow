# Simplification de l'expérience, plan 1 : moteur et FFI — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** donner au moteur Rust et au pont UniFFI ce dont l'app (plan 2) et le terminal (plan 3) ont besoin : police et taille dans `config.toml`, installation automatique de la voix, enregistrement automatique d'un réglage du terminal. Rien n'est retiré ici.

**Architecture:** la police et la taille rejoignent `[reading]` de `Config`, passent par `reader::settings::SettingsView` / `SettingsPatch` puis par les records UniFFI (les nouveaux champs du patch ont `#[uniffi(default = None)]`, pour que le Swift existant compile sans changement). L'installation automatique est une décision pure dans `models` (`default_voice_to_install`), utilisée par le terminal (`models::ensure_default_voice`) et par le lecteur de l'app (`Reader::ensure_default_voice`, qui passe par l'offre Kokoro et `install_voice` existants). Le terminal reçoit `App::config_adjust_and_save` et `App::config_commit_edit_and_save`, qui n'écrivent dans `config.toml` que le réglage touché, relu sur le disque.

**Tech Stack:** Rust 2021 (crates `sillon`, MSRV 1.88, et `sillon-ffi`), UniFFI 0.32.2, `toml` 1, Swift 5 / XcodeGen / xcodebuild pour la vérification de l'app.

**Spec:** `docs/superpowers/specs/2026-10-15-simplification-ux-design.md` (§ 1.6, 1.7, 2, 3, 4 et « Ordre de livraison », lot 1). La lire avant toute tâche.

## Global Constraints

- **Plan strictement additif** : aucun type, champ, fonction, variante ou touche n'est retiré ni renommé. Signets (`src/bookmark.rs`, `Command::ToggleBookmark`, `Snapshot.bookmarked`…), `ReaderCommand::Words` / `Blocks`, Notes, onglets : **hors de ce plan** (plans 2 et 3). Chaque tâche laisse l'app et le terminal compilables.
- Noms figés (les plans 2 et 3 les utilisent tels quels) : `ReadingConfig.font: String`, `ReadingConfig.size: f32`, `SettingsView.reading_font: String`, `SettingsView.reading_size: f32`, `SettingsPatch.reading_font: Option<String>`, `SettingsPatch.reading_size: Option<f32>`, `SillonReader::ensure_default_voice(&self) -> Result<(), SillonError>`, `models::ensure_default_voice`, `App::config_adjust_and_save`, `App::config_commit_edit_and_save`.
- Valeurs de police : **`"literata"`, `"atkinson"`, `"openDyslexic"`** — ce sont les `rawValue` de `ReadingFont` (`macos/Sources/Style/ReadingFont.swift` : `case literata, atkinson, openDyslexic`), stockées aujourd'hui sous la clé UserDefaults `readingFont`. Le contrat initial disait `"opendyslexic"` : **l'orthographe retenue est `"openDyslexic"`**, alignée sur l'app, pour que la reprise des valeurs existantes (plan 2) recopie la chaîne telle quelle. La normalisation accepte toute casse (`"opendyslexic"` devient `"openDyslexic"`).
- Taille : défaut **22.0**, bornée **14–40** (le curseur de `ReadingTab.swift` : `in: 14...40`, `DisplayDefaults.defaultSize = 22`). Clé UserDefaults actuelle : `readingSize`.
- Police et taille ne changent rien à la voix : effet `SettingsEffect::Display`, jamais de rechargement ni de resynthèse.
- Tests : `cargo test --workspace --locked` (le dépôt est un espace de travail `.` + `ffi`). Sous `cfg(test)`, `paths::config_dir`, `data_dir` et `cache_dir` sont des dossiers temporaires du processus : **aucun test ne lit ni n'écrit la configuration ou les modèles réels**. Aucun test ne touche le réseau : les téléchargements passent par `Download::fake` / `fake_ended` ou par `Reader::start_download` remplacé.
- Lint, comme la CI (`.github/workflows/ci.yml`) : `cargo clippy --workspace --all-targets --locked -- -D warnings` et `cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`.
- Pas d'ONNX Runtime dans l'app : `sillon-ffi` n'active jamais `kokoro-onnx` (`scripts/build-ffi.sh` le refuse).
- Liaison Swift : `./scripts/build-ffi.sh` (produit `macos/Generated/sillon_ffi.swift`, `macos/Libraries/libsillon_ffi.a`, tous deux hors du dépôt). L'étape de pré-construction de `macos/project.yml` refuse de construire l'app si une source Rust est plus récente que `libsillon_ffi.a` : relancer le script après tout changement Rust, avant `xcodebuild`.
- Construction de l'app : `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build`. Tests unitaires : même commande avec `test -only-testing:SillonTests`. Tests d'interface : **compilés seulement** (`build-for-testing`), exécutés uniquement avec le feu vert de l'utilisateur.
- Messages de commit en français, style du dépôt (`feat(moteur): …`, `feat(ffi): …`, `feat(terminal): …`, `test(ffi): …`), terminés par la ligne `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Ne jamais journaliser un texte lu.

## Review Focus

1. **Un `config.toml` illisible** (abîmé, écrit à la main) : l'enregistrement automatique du terminal ne doit jamais l'écraser ; le réglage s'applique et l'écran dit qu'il n'est pas enregistré. Test : Tâche 5, `an_unreadable_config_is_never_overwritten_by_the_autosave`.
2. **Une option de la ligne de commande** (`--speed 2`, `--voice …`) : changer un autre réglage à l'écran ne doit pas l'écrire dans `config.toml`. Test : Tâche 5, `the_autosave_writes_only_the_setting_changed_on_screen`.
3. **`ensure_default_voice` appelé à chaque lancement, à chaque « Réessayer », deux fois de suite** : un seul téléchargement à la fois, jamais relancé tant qu'il court, relancé après un échec. Tests : Tâche 4, `without_kokoro_the_recommended_voice_installs_by_itself_once` et `a_failed_installation_is_retried_by_the_next_call` ; Tâche 3, `a_running_download_is_never_doubled`.
4. **Une police ou une taille écrites à la main ou par une ancienne version** (`"OpenDyslexic"`, `"comic"`, `size = nan`, `size = 99`) : ramenées à une valeur que l'app sait afficher. Test : Tâche 1, `an_unknown_font_or_a_size_out_of_bounds_is_brought_back`.
5. **Changer la police ou la taille pendant la lecture** : la voix ne se recharge pas, la lecture continue, le document n'est pas republié. Test : Tâche 2, `a_font_or_size_change_never_reloads_nor_interrupts_the_voice`.

---

## Réponses aux questions du contrat

- **Enregistrement des réglages côté app** : déjà fait. `SillonReader::update_settings` (`ffi/src/lib.rs`) appelle `Reader::update_settings` (`src/reader/settings.rs`), qui applique le patch puis écrit `config.toml` (`save_config`, jamais hors disque ni par-dessus un fichier illisible). `apply_patch` applique sans écrire. Rien à ajouter côté app : la police et la taille passent par ce chemin dès la Tâche 2.
- **Enregistrement côté terminal** : rien n'existe (l'écran applique, `w` écrit toute la configuration, options de la ligne de commande comprises). La Tâche 5 ajoute `App::config_adjust_and_save` et `App::config_commit_edit_and_save`.
- **Barre de progression cliquable** : **aucune commande ajoutée**. `Snapshot.progress` vaut `(phrase + avancement dans la phrase) / nombre de phrases` (`src/reader/view.rs`, ligne « `progress: ((sentence as f32 + app.sentence_progress()) / total)` »), et le nombre de phrases est déjà exposé (`DocView.sentence_count`, lu en Swift comme `document.sentenceCount`, par exemple dans `macos/Sources/Panel/SectionMarkers.swift`). L'app convertit une fraction `f` (0…1) ainsi :
  ```swift
  let count = Int(document.sentenceCount)
  guard count > 0 else { return }
  let target = min(Int(f * Double(count)), count - 1)
  model.send(.seekTo(sentence: UInt32(max(target, 0))))
  ```
  La Tâche 6 épingle ce contrat par un test Rust.
- **Bindings Swift** : `./scripts/build-ffi.sh` (aucune cible Makefile ; l'étape `preBuildScripts` de `macos/project.yml` ne génère rien, elle vérifie seulement la fraîcheur de `libsillon_ffi.a`). Construction : `xcodegen generate` puis `xcodebuild` (voir Global Constraints ; `scripts/build-macos.sh` fait la construction Release complète).

## Structure des fichiers

| Fichier | Rôle dans ce plan |
|---|---|
| `src/config.rs` | `ReadingConfig.font`, `ReadingConfig.size`, constantes de bornes, normalisation ; `Config::read_from` (Tâche 5). |
| `src/reader/settings.rs` | `SettingsView` / `SettingsPatch` du lecteur portent `reading_font`, `reading_size`. |
| `ffi/src/lib.rs` | Records UniFFI, `SillonReader::ensure_default_voice`, tests du pont. |
| `src/models.rs` | `default_voice_to_install` (décision pure), `ensure_default_voice` (terminal). |
| `src/reader/migration.rs` | `Reader::ensure_default_voice` (offre Kokoro acceptée, ou installation du recommandé). |
| `src/app/config.rs` | `Field::copy_value` : recopier un seul réglage d'une configuration à l'autre. |
| `src/app/mod.rs` | Champ `config_target` de `App` ; tests de l'enregistrement automatique. |
| `src/app/settings.rs` | `set_config_target`, `save_field`, `config_adjust_and_save`, `config_commit_edit_and_save`. |
| `macos/Sources/Settings/SettingsDefaults.swift` | **Seulement si** la liaison générée n'a pas de valeur par défaut pour les nouveaux champs (Tâche 6, étape conditionnelle). |

---

### Task 1: Police et taille dans `config.toml`

**Files:**
- Modify: `src/config.rs` (`ReadingConfig`, son `Default`, `Config::normalised`, module `tests`)

**Interfaces:**
- Consumes: rien.
- Produces:
  - `pub struct ReadingConfig { …, pub font: String, pub size: f32 }` (clés TOML `[reading] font`, `[reading] size`)
  - `pub const READING_FONTS: &[&str] = &["literata", "atkinson", "openDyslexic"];`
  - `pub const READING_SIZE_MIN: f32 = 14.0;` `pub const READING_SIZE_MAX: f32 = 40.0;` `pub const READING_SIZE_DEFAULT: f32 = 22.0;`
  - `Config::normalised` ramène `font` à l'une des `READING_FONTS` (toute casse, espaces retirés ; inconnue : `"literata"`) et `size` dans 14–40 (non finie : 22).

- [ ] **Step 1: Write the failing tests**

Ajouter à la fin du module `tests` de `src/config.rs` :

```rust
    #[test]
    fn the_reading_font_and_size_default_to_literata_at_22_points() {
        let config = Config::default();
        assert_eq!(config.reading.font, "literata");
        assert_eq!(config.reading.size, 22.0);
        assert_eq!(READING_FONTS, ["literata", "atkinson", "openDyslexic"], "les rawValue de ReadingFont (Swift)");
    }

    #[test]
    fn a_config_written_before_the_font_keeps_the_defaults() {
        let parsed: Config = toml::from_str("[reading]\nline_spacing = 2\n").unwrap();
        assert_eq!(parsed.reading.line_spacing, 2);
        assert_eq!(parsed.reading.font, "literata");
        assert_eq!(parsed.reading.size, 22.0);
    }

    #[test]
    fn font_and_size_round_trip_through_toml() {
        let mut config = Config::default();
        config.reading.font = "openDyslexic".into();
        config.reading.size = 30.0;
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("font = \"openDyslexic\""), "{text}");
        assert!(text.contains("size = 30.0"), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    /// Une police écrite à la main (casse libre, nom inconnu) ou une taille
    /// hors du curseur de l'app (14 à 40) reviennent à ce que l'app affiche.
    #[test]
    fn an_unknown_font_or_a_size_out_of_bounds_is_brought_back() {
        let read = |font: &str, size: &str| -> Config {
            toml::from_str::<Config>(&format!("[reading]\nfont = \"{font}\"\nsize = {size}\n"))
                .unwrap()
                .normalised()
        };
        assert_eq!(read("OPENDYSLEXIC", "22.0").reading.font, "openDyslexic");
        assert_eq!(read(" atkinson ", "22.0").reading.font, "atkinson");
        assert_eq!(read("comic", "22.0").reading.font, "literata", "inconnue : le défaut");
        assert_eq!(read("", "22.0").reading.font, "literata");
        assert_eq!(read("literata", "3.0").reading.size, 14.0);
        assert_eq!(read("literata", "99.0").reading.size, 40.0);
        assert_eq!(read("literata", "nan").reading.size, 22.0, "NaN redevient le défaut");
        assert_eq!(read("atkinson", "18.0").reading.size, 18.0, "une valeur sage ne bouge pas");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib config::tests`
Expected: FAIL à la compilation : « no field `font` on type `ReadingConfig` », « cannot find value `READING_FONTS` ».

- [ ] **Step 3: Write minimal implementation**

Dans `src/config.rs`, juste avant `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] … pub struct ReadingConfig`, ajouter :

```rust
/// Les polices de lecture de l'app, par l'identifiant que `ReadingFont`
/// (Swift) leur donne : la valeur de `[reading] font`.
pub const READING_FONTS: &[&str] = &["literata", "atkinson", "openDyslexic"];
/// Bornes et défaut de `[reading] size`, en points : celles du curseur de l'app.
pub const READING_SIZE_MIN: f32 = 14.0;
pub const READING_SIZE_MAX: f32 = 40.0;
pub const READING_SIZE_DEFAULT: f32 = 22.0;
```

Dans `pub struct ReadingConfig`, après `pub reading_ruler: bool,` :

```rust
    /// Police du texte dans l'app : l'une des `READING_FONTS`. Le terminal
    /// ne l'utilise pas.
    pub font: String,
    /// Taille du texte dans l'app, en points (14 à 40). Le terminal ne
    /// l'utilise pas.
    pub size: f32,
```

Dans `impl Default for ReadingConfig`, après `reading_ruler: true,` :

```rust
            font: READING_FONTS[0].to_string(),
            size: READING_SIZE_DEFAULT,
```

Dans `Config::normalised`, juste avant `self` (la dernière ligne) :

```rust
        // Une police inconnue de l'app (écrite à la main, ancienne version)
        // reprend la première ; la casse n'importe pas.
        let font = self.reading.font.trim().to_string();
        self.reading.font = READING_FONTS
            .iter()
            .find(|known| known.eq_ignore_ascii_case(&font))
            .map_or_else(|| defaults.reading.font.clone(), |known| known.to_string());
        self.reading.size = finite(self.reading.size, defaults.reading.size)
            .clamp(READING_SIZE_MIN, READING_SIZE_MAX);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib config::tests`
Expected: PASS, dont les tests existants (`a_config_read_from_disk_is_brought_back_within_bounds` vérifie encore `Config::default().normalised() == Config::default()`).

Run: `cargo test --workspace --locked`
Expected: PASS (aucun autre fichier ne construit `ReadingConfig` champ à champ).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "$(cat <<'EOF'
feat(moteur): police et taille de lecture dans config.toml ([reading] font, size), bornées comme le curseur de l'app

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Police et taille dans les réglages du lecteur et le pont

**Files:**
- Modify: `src/reader/settings.rs` (`SettingsView`, `SettingsView::of`, `SettingsPatch`, `SettingsPatch::write_into`, module `tests`)
- Modify: `ffi/src/lib.rs` (`SettingsView`, `SettingsPatch`, `settings_of`, `patch_from`, module `tests`)

**Interfaces:**
- Consumes: `ReadingConfig.font: String`, `ReadingConfig.size: f32` (Tâche 1).
- Produces:
  - `sillon::reader::settings::SettingsView { …, pub reading_font: String, pub reading_size: f32 }`
  - `sillon::reader::settings::SettingsPatch { …, pub reading_font: Option<String>, pub reading_size: Option<f32> }`
  - FFI `SettingsView { …, pub reading_font: String, pub reading_size: f32 }` (Swift : `readingFont: String`, `readingSize: Float`)
  - FFI `SettingsPatch { …, #[uniffi(default = None)] pub reading_font: Option<String>, #[uniffi(default = None)] pub reading_size: Option<f32> }` (Swift : `readingFont: String? = nil`, `readingSize: Float? = nil`)
  - Écriture : `SillonReader::update_settings(patch)` (existant) applique **et** écrit `config.toml` ; `apply_patch` applique sans écrire.

- [ ] **Step 1: Write the failing tests (lecteur)**

Dans `src/reader/settings.rs`, test `every_setting_round_trips_through_a_patch` : ajouter au `patch`, après `voice_for_language: …,` :

```rust
            reading_font: Some("atkinson".into()),
            reading_size: Some(30.0),
```

et à `expected`, après `voices_by_language: …,` :

```rust
            reading_font: "atkinson".into(),
            reading_size: 30.0,
```

Dans `each_setting_triggers_the_effect_of_its_kind`, après la ligne `assert_eq!(effect_of(|c| c.app.keep_voice_minutes = 2), Display);` :

```rust
        assert_eq!(effect_of(|c| c.reading.font = "atkinson".into()), Display, "la police ne touche pas la voix");
        assert_eq!(effect_of(|c| c.reading.size = 30.0), Display);
```

Dans `values_out_of_bounds_come_back_as_applied`, ajouter au patch `reading_font: Some("comic".into()), reading_size: Some(99.0),` et après les assertions existantes :

```rust
        assert_eq!(applied.reading_font, "literata");
        assert_eq!(applied.reading_size, 40.0);
```

Ajouter à la fin du module `tests` :

```rust
    #[test]
    fn a_font_or_size_change_never_reloads_nor_interrupts_the_voice() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.toggle_play());
        let revision = r.revision();
        r.apply_patch(
            &SettingsPatch { reading_font: Some("atkinson".into()), reading_size: Some(30.0), ..Default::default() },
            t0,
        );
        assert_eq!(loads.load(Ordering::SeqCst), 1, "aucune voix rechargée");
        assert_eq!(r.revision(), revision, "le document n'est pas republié");
        let app = r.session().unwrap();
        assert!(app.player.is_playing(), "la lecture continue");
        assert_eq!(app.config.reading.font, "atkinson");
        assert_eq!(app.config.reading.size, 30.0);
    }

    #[test]
    fn font_and_size_are_written_to_config_toml() {
        let dir = std::env::temp_dir().join(format!("sillon-police-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut r = reader();
        r.set_config_path(Some(path.clone()));
        let applied = r
            .update_settings(
                &SettingsPatch { reading_font: Some("openDyslexic".into()), reading_size: Some(18.0), ..Default::default() },
                Instant::now(),
            )
            .unwrap();
        let written: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(applied.reading_font, "openDyslexic");
        assert_eq!(applied.reading_size, 18.0);
        assert_eq!(written.reading.font, "openDyslexic");
        assert_eq!(written.reading.size, 18.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib reader::settings`
Expected: FAIL à la compilation : « struct `SettingsPatch` has no field named `reading_font` ».

- [ ] **Step 3: Implement (lecteur)**

Dans `src/reader/settings.rs`, `pub struct SettingsView`, après `pub voices_by_language: BTreeMap<String, String>,` :

```rust
    // [reading], pour l'app seule
    /// La police du texte : l'une des `config::READING_FONTS`.
    pub reading_font: String,
    /// La taille du texte, en points (14 à 40).
    pub reading_size: f32,
```

Dans `SettingsView::of`, après `voices_by_language: c.voice.per_language.clone(),` :

```rust
            reading_font: c.reading.font.clone(),
            reading_size: c.reading.size,
```

Dans `pub struct SettingsPatch`, après `pub voice_for_language: Option<(String, String)>,` :

```rust
    pub reading_font: Option<String>,
    pub reading_size: Option<f32>,
```

Dans `SettingsPatch::write_into`, après `set(&mut c.app.output_device, &self.output_device);` :

```rust
        set(&mut c.reading.font, &self.reading_font);
        set(&mut c.reading.size, &self.reading_size);
```

(`apply_patch` normalise déjà le résultat ; `SettingsEffect::between_in` classe déjà tout changement de `reading` en `Display`.)

- [ ] **Step 4: Run tests to verify they pass (lecteur)**

Run: `cargo test -p sillon --lib reader::settings`
Expected: PASS.

Run: `cargo build -p sillon-ffi`
Expected: FAIL : « missing fields `reading_font` and `reading_size` in initializer of `sillon::reader::settings::SettingsPatch` » (dans `patch_from`). C'est la partie FFI de cette tâche.

- [ ] **Step 5: Write the failing FFI test**

Dans `ffi/src/lib.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn the_reading_font_and_size_cross_the_bridge() {
        let defaults = default_settings();
        assert_eq!(defaults.reading_font, "literata");
        assert_eq!(defaults.reading_size, 22.0);
        let reader = SillonReader::silent();
        let applied = reader.apply_patch(SettingsPatch {
            reading_font: Some("openDyslexic".into()),
            reading_size: Some(26.0),
            ..Default::default()
        });
        assert_eq!(applied.reading_font, "openDyslexic");
        assert_eq!(applied.reading_size, 26.0);
        assert_eq!(reader.settings().reading_font, "openDyslexic");
        // Le lecteur muet n'écrit rien, mais la réponse est la même.
        let updated = reader
            .update_settings(SettingsPatch { reading_size: Some(99.0), ..Default::default() })
            .unwrap();
        assert_eq!(updated.reading_size, 40.0, "ramenée dans les bornes");
    }
```

- [ ] **Step 6: Implement (pont)**

Dans `ffi/src/lib.rs`, `pub struct SettingsView`, après `pub voices_by_language: Vec<LanguageVoice>,` :

```rust
    /// La police du texte : `literata`, `atkinson` ou `openDyslexic` (les
    /// `rawValue` de `ReadingFont`).
    pub reading_font: String,
    /// La taille du texte, en points (14 à 40).
    pub reading_size: f32,
```

Dans `pub struct SettingsPatch`, après `pub voice_for_language: Option<LanguageVoice>,` :

```rust
    /// La police du texte. Défaut `nil` côté Swift : les appels existants
    /// de `SettingsPatch(…)` restent valides.
    #[uniffi(default = None)]
    pub reading_font: Option<String>,
    /// La taille du texte, en points.
    #[uniffi(default = None)]
    pub reading_size: Option<f32>,
```

Dans `fn settings_of`, après le champ `voices_by_language: …collect(),` :

```rust
        reading_font: s.reading_font,
        reading_size: s.reading_size,
```

Dans `fn patch_from`, après `voice_for_language: p.voice_for_language.map(|v| (v.language, v.voice)),` :

```rust
        reading_font: p.reading_font,
        reading_size: p.reading_size,
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS, dont `the_reading_font_and_size_cross_the_bridge`.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: aucun avertissement.

- [ ] **Step 8: Commit**

```bash
git add src/reader/settings.rs ffi/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(ffi): police et taille dans les réglages du lecteur (reading_font, reading_size), écrites par update_settings, sans toucher à la voix

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Installation automatique, la décision et le terminal

**Files:**
- Modify: `src/models.rs` (après `first_installed_with`, et module `tests`)

**Interfaces:**
- Consumes: `models::available_with`, `models::is_installed`, `models::Download::start` (existants).
- Produces:
  - `pub fn default_voice_to_install(available: &[&'static ModelPackage], installed: impl Fn(&ModelPackage) -> bool) -> Option<&'static ModelPackage>` : le Kokoro recommandé de `available` si aucun Kokoro de `available` n'est installé ; sinon `None`.
  - `pub fn ensure_default_voice(mlx: bool, running: Option<&Download>) -> anyhow::Result<Option<Download>>` : `Ok(None)` si `running` n'est pas fini, ou si rien n'est à installer ; sinon le téléchargement démarré. Pour le terminal (plan 3) : `models::ensure_default_voice(false, app.screen.download.as_ref())`, puis ranger le `Some(download)` dans `app.screen.download` ; `App::poll_download` (existant) sélectionne le modèle à la fin.

- [ ] **Step 1: Write the failing tests**

Ajouter au module `tests` de `src/models.rs` :

```rust
    #[test]
    fn the_recommended_kokoro_is_chosen_when_none_is_installed() {
        let mlx = package("kokoro_82m_mlx");
        let int8 = package("kokoro_82m_int8");
        assert_eq!(default_voice_to_install(&[mlx], |_| false), Some(mlx));
        assert_eq!(default_voice_to_install(&[int8], |_| false), Some(int8));
    }

    #[test]
    fn an_installed_kokoro_needs_no_installation() {
        let mlx = package("kokoro_82m_mlx");
        assert_eq!(default_voice_to_install(&[mlx], |_| true), None);
    }

    #[test]
    fn a_build_without_kokoro_installs_nothing() {
        assert_eq!(default_voice_to_install(&[], |_| false), None);
    }

    /// Appelée à chaque lancement et à chaque « Réessayer » : un
    /// téléchargement qui court n'est jamais doublé (rien ne part sur le
    /// réseau ici).
    #[test]
    fn a_running_download_is_never_doubled() {
        let running = Download::fake(&CATALOG[0], false);
        assert!(ensure_default_voice(true, Some(&running)).unwrap().is_none());
    }

    /// Un terminal construit sans `kokoro-onnx` n'a aucun Kokoro à installer.
    #[cfg(not(feature = "kokoro-onnx"))]
    #[test]
    fn the_terminal_without_onnx_has_nothing_to_install() {
        assert!(ensure_default_voice(false, None).unwrap().is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib models::tests`
Expected: FAIL à la compilation : « cannot find function `default_voice_to_install` ».

- [ ] **Step 3: Write minimal implementation**

Dans `src/models.rs`, après `pub fn first_installed_with(…) { … }` :

```rust
/// Le paquet que l'installation automatique télécharge : le Kokoro
/// recommandé de ce catalogue, quand aucun Kokoro du catalogue n'est
/// installé. `None` : déjà là, ou pas de Kokoro dans ce binaire.
///
/// Seul compte ce que ce binaire sait charger : un Kokoro int8 posé par le
/// terminal ne dispense pas l'app (MLX) d'installer le sien.
pub fn default_voice_to_install(
    available: &[&'static ModelPackage],
    installed: impl Fn(&ModelPackage) -> bool,
) -> Option<&'static ModelPackage> {
    let kokoro: Vec<&'static ModelPackage> = available.iter().copied().filter(|p| p.family == "kokoro").collect();
    if kokoro.iter().any(|p| installed(p)) {
        return None;
    }
    kokoro.iter().copied().find(|p| p.recommended).or_else(|| kokoro.first().copied())
}

/// L'installation automatique de la voix, sans rien demander : démarre le
/// téléchargement du Kokoro recommandé s'il n'y en a aucun d'installé.
/// Idempotente : `running` qui court encore, ou une voix déjà là, ne
/// déclenchent rien. Un téléchargement fini (échoué compris) se relance.
pub fn ensure_default_voice(mlx: bool, running: Option<&Download>) -> Result<Option<Download>> {
    if running.is_some_and(|d| !d.is_finished()) {
        return Ok(None);
    }
    match default_voice_to_install(&available_with(mlx), is_installed) {
        Some(package) => Download::start(package).map(Some),
        None => Ok(None),
    }
}
```

(`Result` est celui d'`anyhow`, déjà importé en tête de `src/models.rs` : `Download::start` le rend.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib models::tests`
Expected: PASS.

Run: `cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: aucun avertissement (le test `cfg(not(feature = "kokoro-onnx"))` disparaît sans laisser d'import inutilisé).

- [ ] **Step 5: Commit**

```bash
git add src/models.rs
git commit -m "$(cat <<'EOF'
feat(moteur): installation automatique de la voix — le Kokoro recommandé se télécharge seul s'il n'y en a aucun (models::ensure_default_voice)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Installation automatique dans le lecteur et le pont

**Files:**
- Modify: `src/reader/migration.rs` (nouvelle méthode dans `impl Reader`, module `tests`)
- Modify: `ffi/src/lib.rs` (`impl SillonReader`, module `tests`)

**Interfaces:**
- Consumes: `models::default_voice_to_install` (Tâche 3) ; `Reader::accept_offer`, `Reader::install_voice`, `Reader::poll_download`, champs `offer`, `download`, `persist`, `mlx` (existants).
- Produces:
  - `impl Reader { pub fn ensure_default_voice(&mut self, now: Instant) -> anyhow::Result<()> }`
  - FFI `impl SillonReader { pub fn ensure_default_voice(&self) -> Result<(), SillonError> }` (Swift : `func ensureDefaultVoice() throws`). Ordre d'appel prévu par l'app (plan 2) : `migrateVoice()` au lancement, puis `ensureDefaultVoice()` ; « Réessayer » rappelle `ensureDefaultVoice()`. La progression se lit dans `Snapshot.download` (existant), un échec dans `Snapshot.download.failed`.
  - Comportement, dans l'ordre : lecteur hors disque (`silent()`) → rien, `Ok` ; un téléchargement qui court → rien ; une offre Kokoro en attente → acceptée (`accept_offer` : téléchargement, puis passage à Kokoro à la fin) ; aucun Kokoro disponible installé → `install_voice` du recommandé (il devient la voix active à la fin s'il n'y en a aucune, par `poll_download`) ; sinon rien.

- [ ] **Step 1: Write the failing tests (lecteur)**

Dans `src/reader/migration.rs`, module `tests`, ajouter l'import `use std::sync::atomic::{AtomicUsize, Ordering};` avec les autres, puis à la fin du module :

```rust
    /// Téléchargements démarrés par `counting_offline_reader` (sous
    /// `models_lock` : un test à la fois).
    static STARTS: AtomicUsize = AtomicUsize::new(0);

    fn counting_offline_reader(config: Config) -> Reader {
        let mut r = offline_reader(config);
        r.start_download = |package| {
            STARTS.fetch_add(1, Ordering::SeqCst);
            Ok(Download::fake(package, false))
        };
        r
    }

    #[test]
    fn without_kokoro_the_recommended_voice_installs_by_itself_once() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let _ = models::remove(mlx);
        let mut r = counting_offline_reader(Config::default());
        let before = STARTS.load(Ordering::SeqCst);
        r.ensure_default_voice(Instant::now()).unwrap();
        assert_eq!(r.download().map(|d| d.id), Some(mlx.id.to_string()));
        r.ensure_default_voice(Instant::now()).unwrap();
        assert_eq!(STARTS.load(Ordering::SeqCst), before + 1, "un seul téléchargement");
        assert_eq!(r.offer(), None);

        // Le téléchargement aboutit : Kokoro devient la voix active.
        install_hollow(mlx);
        r.download = Some(Download::fake(mlx, true));
        r.tick(Instant::now());
        assert_eq!(r.config().engine.family, "kokoro");
        r.ensure_default_voice(Instant::now()).unwrap();
        assert!(r.download().is_none(), "installé : plus rien à faire");
        models::remove(mlx).unwrap();
    }

    #[test]
    fn a_pending_offer_is_accepted_without_asking() {
        let _guard = models_lock();
        let _ = models::remove(by_id("kokoro_82m_mlx"));
        let mut r = offered();
        r.ensure_default_voice(Instant::now()).unwrap();
        assert_eq!(r.offer(), None, "plus de question");
        assert_eq!(r.download().map(|d| d.id), Some("kokoro_82m_mlx".to_string()));
        assert_eq!(
            r.switch_when_downloaded.as_ref().map(|(id, _)| id.as_str()),
            Some("kokoro_82m_mlx"),
            "le passage à Kokoro attend la fin"
        );
    }

    #[test]
    fn an_installed_kokoro_downloads_nothing() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let mut r = offline_reader(Config::default());
        r.ensure_default_voice(Instant::now()).unwrap();
        assert!(r.download().is_none());
        assert_eq!(r.offer(), None);
        models::remove(mlx).unwrap();
    }

    /// « Réessayer » après un échec (réseau coupé) : l'échec reste affiché
    /// jusqu'à l'appel suivant, qui relance.
    #[test]
    fn a_failed_installation_is_retried_by_the_next_call() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let _ = models::remove(mlx);
        let mut r = offline_reader(Config::default());
        r.download = Some(Download::fake_ended(mlx, Progress::Failed("réseau".into())));
        r.tick(Instant::now());
        assert!(r.download().and_then(|d| d.failed).is_some(), "l'échec reste affiché");
        r.ensure_default_voice(Instant::now()).unwrap();
        let download = r.download().expect("relancé");
        assert_eq!(download.failed, None);
    }

    #[test]
    fn a_reader_off_disk_installs_nothing() {
        let mut r = offline_reader(Config::default());
        r.set_persistent(false);
        r.ensure_default_voice(Instant::now()).unwrap();
        assert!(r.download().is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib reader::migration`
Expected: FAIL à la compilation : « no method named `ensure_default_voice` found for struct `Reader` ».

- [ ] **Step 3: Implement (lecteur)**

Dans `src/reader/migration.rs`, dans `impl Reader`, après `pub fn dismiss_offer(&mut self) { … }` :

```rust
    /// L'installation automatique de la voix (plus d'onglet Modèles) : une
    /// offre Kokoro en attente est acceptée ; sans offre, le Kokoro
    /// recommandé se télécharge si aucun n'est installé, et devient la voix
    /// active à la fin s'il n'y en a aucune. Idempotente : un téléchargement
    /// qui court ne se double pas ; un échec se relance. Un lecteur hors
    /// disque ne télécharge rien.
    pub fn ensure_default_voice(&mut self, now: Instant) -> Result<()> {
        if !self.persist {
            return Ok(());
        }
        // Une issue pas encore relevée (fini, échoué) passe d'abord.
        self.poll_download(now);
        if self.download.as_ref().is_some_and(|d| !d.is_finished()) {
            return Ok(());
        }
        if self.offer.is_some() {
            return self.accept_offer(now);
        }
        let available = models::available_with(self.mlx);
        match models::default_voice_to_install(&available, models::is_installed) {
            Some(package) => self.install_voice(package.id),
            None => Ok(()),
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass (lecteur)**

Run: `cargo test -p sillon --lib reader::migration`
Expected: PASS.

- [ ] **Step 5: Write the failing FFI test**

Dans `ffi/src/lib.rs`, module `tests` :

```rust
    /// L'app de test (lecteur muet) appelle l'installation comme l'app
    /// réelle : rien ne se télécharge, rien n'échoue.
    #[test]
    fn the_silent_reader_never_installs_a_voice() {
        let reader = SillonReader::silent();
        reader.ensure_default_voice().expect("sans effet, sans erreur");
        reader.ensure_default_voice().expect("idempotente");
        assert!(reader.snapshot().download.is_none());
    }
```

Run: `cargo test -p sillon-ffi the_silent_reader_never_installs_a_voice`
Expected: FAIL à la compilation : « no method named `ensure_default_voice` found for struct `Arc<SillonReader>` ».

- [ ] **Step 6: Implement (pont)**

Dans `ffi/src/lib.rs`, `#[uniffi::export] impl SillonReader`, après `pub fn dismiss_voice_offer(&self) { … }` :

```rust
    /// L'installation automatique de la voix, sans rien demander : accepte
    /// l'offre Kokoro en attente, ou télécharge le Kokoro recommandé si
    /// aucun n'est installé ; sinon rien. Idempotente ; sans effet pendant
    /// un téléchargement. La progression : `Snapshot.download`. « Réessayer »
    /// rappelle cette fonction.
    pub fn ensure_default_voice(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.ensure_default_voice(Instant::now()).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: aucun avertissement.

- [ ] **Step 8: Commit**

```bash
git add src/reader/migration.rs ffi/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(ffi): ensure_default_voice — l'offre Kokoro acceptée sans question, ou le modèle recommandé installé seul ; idempotente, relance après un échec

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Enregistrement automatique d'un réglage du terminal

**Files:**
- Modify: `src/config.rs` (`impl Config` : `read_from` ; module `tests`)
- Modify: `src/app/config.rs` (`impl Field` : `copy_value` ; module `tests`)
- Modify: `src/app/mod.rs` (champ `config_target` de `App`, son initialisation dans `from_parts`, module `tests`)
- Modify: `src/app/settings.rs` (`impl App` : `set_config_target`, `save_field`, `config_adjust_and_save`, `config_commit_edit_and_save`)

**Interfaces:**
- Consumes: `Config::save_to` (existant, `pub(crate)`), `Config::path` (existant), `App::config_adjust`, `App::config_commit_edit`, `ConfigScreen::current_field` (existants).
- Produces:
  - `impl Config { pub fn read_from(path: &std::path::Path) -> anyhow::Result<Config> }` : le fichier tel quel (sans `normalised`), les défauts s'il n'existe pas, une erreur s'il est illisible ou invalide.
  - `impl Field { pub fn copy_value(&self, from: &Config, to: &mut Config) }` : recopie ce seul réglage. Pour `Field::Voice` sur Kokoro : la seule ligne `[voice.per_language]` de la langue par défaut. Pour `Field::Family` : la famille, puis `align_voice_with_family`.
  - `impl App { pub fn set_config_target(&mut self, path: Option<std::path::PathBuf>) }` (défaut : `Config::path()` ; `None` : rien ne s'écrit).
  - `impl App { pub fn save_field(&mut self, field: Field) -> anyhow::Result<std::path::PathBuf> }` : relit `config.toml`, y recopie `field` depuis `self.config`, l'écrit ; refuse un fichier illisible.
  - `impl App { pub fn config_adjust_and_save(&mut self, delta: i32) }` : comme `config_adjust`, puis `save_field` du champ touché ; un échec s'affiche (« Réglage appliqué, non enregistré : … »), le réglage reste appliqué.
  - `impl App { pub fn config_commit_edit_and_save(&mut self) }` : comme `config_commit_edit`, puis `save_field`.
  - Le plan 3 branche les flèches et Entrée de l'écran de réglages sur ces deux fonctions et retire `w`. S'il ajoute des variantes à `Field` (voix FR, voix EN…), il doit compléter `copy_value` (le `match` est exhaustif : le compilateur le rappellera).

- [ ] **Step 1: Write the failing tests**

Dans `src/config.rs`, module `tests` :

```rust
    #[test]
    fn read_from_gives_the_file_as_written_or_the_defaults_or_an_error() {
        let dir = std::env::temp_dir().join(format!("sillon-read-from-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        assert_eq!(Config::read_from(&path).unwrap(), Config::default(), "absent : les défauts");
        std::fs::write(&path, "[reading]\nline_width = 5\n").unwrap();
        assert_eq!(Config::read_from(&path).unwrap().reading.line_width, 5, "tel quel, sans bornes");
        std::fs::write(&path, "[voice\nspeed = 1.4").unwrap();
        let err = Config::read_from(&path).unwrap_err().to_string();
        std::fs::remove_dir_all(&dir).ok();
        assert!(err.contains("invalide"), "{err}");
    }
```

Dans `src/app/config.rs`, module `tests` :

```rust
    #[test]
    fn copying_a_field_copies_nothing_else() {
        let mut from = Config::default();
        from.voice.speed = 2.5; // une option --speed, par exemple
        from.reading.line_spacing = 3;
        let mut to = Config::default();
        Field::LineSpacing.copy_value(&from, &mut to);
        assert_eq!(to.reading.line_spacing, 3);
        assert_eq!(to.voice.speed, Config::default().voice.speed, "la vitesse ne suit pas");
    }

    #[test]
    fn copying_the_kokoro_voice_copies_only_its_language_line() {
        let mut from = Config::default();
        from.engine.family = "kokoro".into();
        from.voice.language = "en".into();
        from.voice.voice = "voix-de-la-ligne-de-commande".into();
        Field::Voice.adjust(&mut from, 1);
        let chosen = from.voice.per_language["en"].clone();
        let mut to = Config::default();
        to.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        Field::Voice.copy_value(&from, &mut to);
        assert_eq!(to.voice.per_language["en"], chosen);
        assert_eq!(to.voice.per_language["fr"], "p-fenrir60", "l'autre langue reste");
        assert_eq!(to.voice.voice, "", "voice.voice n'est pas écrit");
    }
```

Dans `src/app/mod.rs`, module `tests` (à côté de `choosing_a_model_asks_for_the_configuration_to_be_written`) :

```rust
    /// Un fichier `config.toml` propre au test, dans un dossier neuf.
    fn autosave_target(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sillon-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        (dir, path)
    }

    /// L'écran des réglages ouvert sur « Lecture › Interlignage ».
    fn on_line_spacing(app: &mut App) {
        app.screen.goto_section(2);
        app.screen.row = 1;
        assert_eq!(app.screen.current_field(&app.config), Some(Field::LineSpacing));
    }

    #[test]
    fn a_setting_changed_on_screen_is_written_at_once() {
        let (dir, path) = autosave_target("immediat");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        on_line_spacing(&mut app);
        let before = app.config.reading.line_spacing;
        app.config_adjust_and_save(1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(app.config.reading.line_spacing, before + 1, "appliqué");
        assert_eq!(written.reading.line_spacing, before + 1, "écrit sans w");
        assert!(!app.config_needs_saving());
    }

    #[test]
    fn the_autosave_writes_only_the_setting_changed_on_screen() {
        let (dir, path) = autosave_target("seul");
        let mut on_disk = Config::default();
        on_disk.voice.speed = 1.2;
        on_disk.save_to(&path).unwrap();
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.config.voice.speed = 2.5; // venue de --speed 2.5
        on_line_spacing(&mut app);
        app.config_adjust_and_save(1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.voice.speed, 1.2, "l'option de la ligne de commande n'est pas écrite");
        assert!(written.reading.autoplay, "l'autoplay coupé du test non plus");
        assert_eq!(written.reading.line_spacing, app.config.reading.line_spacing);
    }

    #[test]
    fn an_unreadable_config_is_never_overwritten_by_the_autosave() {
        let (dir, path) = autosave_target("illisible");
        let witness = b"[voice\nvoice = \"F2\" # \xc3\xa9crit \xc3\xa0 la main".to_vec();
        std::fs::write(&path, &witness).unwrap();
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        on_line_spacing(&mut app);
        let before = app.config.reading.line_spacing;
        app.config_adjust_and_save(1);
        let after = std::fs::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(after, witness, "le fichier de l'utilisateur est intact");
        assert_eq!(app.config.reading.line_spacing, before + 1, "le réglage s'applique quand même");
        assert!(app.status().contains("non enregistré"), "{}", app.status());
    }

    #[test]
    fn a_text_field_is_written_when_its_edit_is_committed() {
        let (dir, path) = autosave_target("texte");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.screen.goto_section(1);
        app.screen.show_all = true;
        app.screen.row = 4;
        assert_eq!(app.screen.current_field(&app.config), Some(Field::Voice));
        app.screen.editing = Some("~/voix.wav".into());
        app.config_commit_edit_and_save();
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.voice.voice, "~/voix.wav");
        assert!(app.screen.editing.is_none());
    }

    #[test]
    fn without_a_target_nothing_is_written_and_the_screen_says_so() {
        let mut app = app_reading(DOC);
        app.set_config_target(None);
        on_line_spacing(&mut app);
        app.config_adjust_and_save(1);
        assert!(app.status().contains("non enregistré"), "{}", app.status());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib`
Expected: FAIL à la compilation : « no function or associated item named `read_from` found for struct `Config` », « no method named `copy_value` », « no method named `set_config_target` ».

- [ ] **Step 3: Implement `Config::read_from`**

Dans `src/config.rs`, `impl Config`, après `pub(crate) fn save_to(…) { … }` :

```rust
    /// La configuration écrite à ce chemin, telle quelle (sans
    /// `normalised`) ; les défauts s'il n'existe pas. Illisible ou invalide :
    /// une erreur — on n'écrit jamais par-dessus un fichier qu'on n'a pas su
    /// lire.
    pub fn read_from(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("configuration illisible ({})", path.display()))?;
        toml::from_str(&text).with_context(|| format!("configuration invalide ({})", path.display()))
    }
```

- [ ] **Step 4: Implement `Field::copy_value`**

Dans `src/app/config.rs`, `impl Field`, après `pub fn set_text(…) { … }` :

```rust
    /// Recopie la valeur de ce réglage de `from` dans `to`, et rien d'autre :
    /// l'enregistrement automatique n'écrit que ce que l'écran vient de
    /// changer, jamais une option de la ligne de commande (`--speed`…).
    pub fn copy_value(&self, from: &Config, to: &mut Config) {
        match self {
            Field::Speed => to.voice.speed = from.voice.speed,
            Field::Pitch => to.voice.pitch = from.voice.pitch,
            Field::Volume => to.voice.volume = from.voice.volume,
            Field::Language => to.voice.language = from.voice.language.clone(),
            Field::Voice => {
                if models::voices_follow_language(&from.engine.family) {
                    // Kokoro : le champ écrit la ligne de la langue par défaut.
                    let language = from.default_language();
                    let code = crate::text::language::Lang::parse(&language)
                        .map_or_else(|| language.clone(), |l| l.code().to_string());
                    if let Some(voice) = from.voice.per_language.get(&code) {
                        to.voice.per_language.insert(code, voice.clone());
                    }
                } else {
                    to.voice.voice = from.voice.voice.clone();
                }
            }
            Field::CodeBlocks => to.reading.code_blocks = from.reading.code_blocks.clone(),
            Field::Autoplay => to.reading.autoplay = from.reading.autoplay,
            Field::SeekStep => to.reading.seek_step = from.reading.seek_step,
            Field::LineWidth => to.reading.line_width = from.reading.line_width,
            Field::LineSpacing => to.reading.line_spacing = from.reading.line_spacing,
            Field::ReadingRuler => to.reading.reading_ruler = from.reading.reading_ruler,
            Field::EnginePref => to.engine.engine = from.engine.engine.clone(),
            Field::Threads => to.engine.threads = from.engine.threads,
            Field::Sessions => to.engine.sessions = from.engine.sessions,
            Field::Prefetch => to.engine.prefetch = from.engine.prefetch,
            Field::ModelPath => to.engine.model_path = from.engine.model_path.clone(),
            Field::Family => {
                to.engine.family = from.engine.family.clone();
                to.align_voice_with_family();
            }
        }
    }
```

- [ ] **Step 5: Add `config_target` to `App`**

Dans `src/app/mod.rs`, `pub struct App`, après `config_dirty: bool,` :

```rust
    /// Où l'enregistrement automatique écrit (`config.toml` ; un fichier
    /// temporaire sous test). `None` : rien ne s'écrit.
    config_target: Option<std::path::PathBuf>,
```

Dans `App::from_parts`, dans le littéral `Self { … }`, après `config_dirty: false,` :

```rust
            config_target: Config::path(),
```

- [ ] **Step 6: Implement the App functions**

Dans `src/app/settings.rs`, `impl App`, après `pub fn flush_config(&mut self) { … }` :

```rust
    /// Où l'enregistrement automatique écrit ; `None` : nulle part.
    pub fn set_config_target(&mut self, path: Option<std::path::PathBuf>) {
        self.config_target = path;
    }

    /// Écrit ce seul réglage dans `config.toml` : le fichier est relu, le
    /// champ y est recopié depuis la configuration en cours, le reste du
    /// fichier ne bouge pas (les options de la ligne de commande n'y entrent
    /// pas). Un fichier illisible n'est jamais réécrit.
    pub fn save_field(&mut self, field: Field) -> anyhow::Result<std::path::PathBuf> {
        let path = self
            .config_target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("répertoire de configuration introuvable"))?;
        let mut on_disk = Config::read_from(&path)?;
        field.copy_value(&self.config, &mut on_disk);
        on_disk.save_to(&path)?;
        Ok(path)
    }

    /// Ajuste le champ sélectionné, répercute son effet, et l'enregistre
    /// aussitôt. Un enregistrement impossible laisse le réglage appliqué et
    /// le dit.
    pub fn config_adjust_and_save(&mut self, delta: i32) {
        let Some(field) = self.screen.current_field(&self.config) else { return };
        self.config_adjust(delta);
        self.report_save(field);
    }

    /// Valide la saisie d'un champ textuel, puis l'enregistre aussitôt.
    pub fn config_commit_edit_and_save(&mut self) {
        if self.screen.editing.is_none() {
            return;
        }
        let Some(field) = self.screen.current_field(&self.config) else { return };
        self.config_commit_edit();
        self.report_save(field);
    }

    fn report_save(&mut self, field: Field) {
        if let Err(err) = self.save_field(field) {
            self.notify(&format!("Réglage appliqué, non enregistré : {err:#}"));
        }
    }
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS, dont les cinq tests d'enregistrement automatique, `copying_*` et `read_from_*`.

Run: `cargo test --workspace --locked`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings && cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: aucun avertissement.

- [ ] **Step 8: Commit**

```bash
git add src/config.rs src/app/config.rs src/app/mod.rs src/app/settings.rs
git commit -m "$(cat <<'EOF'
feat(terminal): enregistrement automatique d'un réglage (config_adjust_and_save, config_commit_edit_and_save) — seul le champ touché est écrit, jamais une option de la ligne de commande ni par-dessus un fichier illisible

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Contrat de la barre de progression, liaison Swift régénérée, app construite

**Files:**
- Modify: `ffi/src/lib.rs` (module `tests`)
- Modify, **seulement si l'étape 5 l'exige** : `macos/Sources/Settings/SettingsDefaults.swift`

**Interfaces:**
- Consumes: tout ce que les Tâches 2 et 4 exposent au pont ; `ReaderCommand::SeekTo`, `DocView.sentence_count`, `Snapshot.progress` (existants).
- Produces: la liaison `macos/Generated/sillon_ffi.swift` à jour (hors dépôt) avec `SettingsView.readingFont: String`, `SettingsView.readingSize: Float`, `SettingsPatch(… readingFont: String? = nil, readingSize: Float? = nil)`, `SillonReader.ensureDefaultVoice() throws` ; aucune nouvelle variante de `ReaderCommand`.

- [ ] **Step 1: Write the contract test**

Dans `ffi/src/lib.rs`, module `tests` :

```rust
    /// La barre de progression cliquable de l'app convertit une fraction en
    /// phrase par `DocView.sentence_count` et `ReaderCommand::SeekTo`, sans
    /// commande dédiée : `Snapshot.progress` doit rester « phrase courante
    /// (plus l'avancement dans la phrase) / nombre de phrases ».
    #[test]
    fn progress_is_the_sentence_over_the_sentence_count() {
        let reader = SillonReader::silent();
        reader.open_text("Un. Deux. Trois. Quatre.".into(), "test".into());
        let deadline = Instant::now() + Duration::from_secs(5);
        while reader.snapshot().revision != 1 {
            assert!(Instant::now() < deadline, "texte jamais publié");
            std::thread::sleep(Duration::from_millis(5));
        }
        let count = reader.document().sentence_count;
        assert_eq!(count, 4);
        reader.command(ReaderCommand::SeekTo { sentence: 2 });
        while reader.snapshot().sentence != 2 {
            assert!(Instant::now() < deadline, "saut jamais appliqué");
            std::thread::sleep(Duration::from_millis(5));
        }
        let progress = reader.snapshot().progress;
        assert!((0.5..0.75).contains(&progress), "{progress}");
        // La conversion de l'app : partie entière de fraction × nombre,
        // bornée à la dernière phrase.
        let at = |fraction: f32| ((fraction * count as f32) as u32).min(count - 1);
        assert_eq!(at(0.6), 2);
        assert_eq!(at(1.0), 3, "le bout de la barre : la dernière phrase");
        assert_eq!(at(0.0), 0);
    }
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p sillon-ffi progress_is_the_sentence_over_the_sentence_count`
Expected: PASS dès l'écriture (test de caractérisation : il épingle un comportement existant dont dépend le plan 2). S'il échoue, **ne pas modifier le test** : la formule de `src/reader/view.rs` a changé, et il faut alors ajouter `ReaderCommand::SeekToFraction { fraction: f32 }` (le dire à l'utilisateur avant).

- [ ] **Step 3: Full Rust verification**

Run: `cargo test --workspace --locked`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings && cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: aucun avertissement.

- [ ] **Step 4: Regenerate the Swift binding**

Run: `./scripts/build-ffi.sh`
Expected: se termine par « Liaison prête dans macos/Generated et macos/Libraries ».

- [ ] **Step 5: Check the generated names and defaults**

Run: `grep -n "readingFont\|readingSize\|func ensureDefaultVoice" macos/Generated/sillon_ffi.swift`
Expected : entre autres `public var readingFont: String`, `public var readingSize: Float`, un initialiseur de `SettingsPatch` contenant `readingFont: String? = nil, readingSize: Float? = nil`, et `func ensureDefaultVoice() throws`.

**Seulement si** l'initialiseur de `SettingsPatch` n'a pas `= nil` pour ces deux paramètres : dans `macos/Sources/Settings/SettingsDefaults.swift`, remplacer

```swift
            keepVoiceMinutes: nil, outputDevice: nil, voiceForLanguage: nil)
```

par

```swift
            keepVoiceMinutes: nil, outputDevice: nil, voiceForLanguage: nil,
            readingFont: nil, readingSize: nil)
```

- [ ] **Step 6: Build the app and run its unit tests**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build`
Expected: `** BUILD SUCCEEDED **`, sans nouvel avertissement de concurrence.

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **` (rien n'a changé côté Swift).

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build-for-testing`
Expected: `** TEST BUILD SUCCEEDED **` (tests d'interface compilés, pas exécutés).

- [ ] **Step 7: Commit**

```bash
git add ffi/src/lib.rs
# seulement si l'étape 5 l'a modifié :
git add macos/Sources/Settings/SettingsDefaults.swift
git commit -m "$(cat <<'EOF'
test(ffi): la progression vaut phrase / nombre de phrases — la barre cliquable de l'app saute par SeekTo, sans commande dédiée

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

## Self-review

**Couverture de la spec (part du lot 1 « Moteur et FFI »)**
- § 3 « `SettingsPatch` gagne `reading_font` et `reading_size` » : Tâches 1 et 2. § 1.6 « Police et taille passent des UserDefaults à `config.toml` » : le stockage est prêt (Tâches 1-2) ; la reprise des valeurs UserDefaults (`readingFont`, `readingSize`) et le retrait de `@AppStorage` sont au plan 2.
- § 1.7 et § 3 « Installation automatique : réutilise `install_model` et l'offre Kokoro existantes » : Tâches 3 (décision, terminal) et 4 (lecteur, pont : `accept_offer` et `install_voice`). L'affichage de la progression et le bouton « Réessayer » sont au plan 2 (`Snapshot.download`, rappel de `ensureDefaultVoice`) et au plan 3.
- § 2 « Enregistrement automatique à chaque changement ; `w` disparaît » : la fonction est prête (Tâche 5) ; le branchement des touches et le retrait de `w` sont au plan 3. § 4 « tests ajoutés pour l'enregistrement automatique des réglages » : Tâche 5.
- § 3 « ajout de `seek_to_fraction` si la barre cliquable l'exige » : elle ne l'exige pas (section « Réponses aux questions du contrat ») ; contrat épinglé, Tâche 6.
- § 3 signets, `Words` / `Blocks`, § 4 « lancement sans argument » : **hors de ce plan** (retraits et terminal, plans 2 et 3).

**Placeholders** : aucun « TBD » ; l'étape conditionnelle de la Tâche 6 donne le code exact.

**Cohérence des types** : `reading_font: String` / `reading_size: f32` (vue), `Option<String>` / `Option<f32>` (patch), identiques dans `reader::settings` et le pont ; `ensure_default_voice` : `Reader` → `anyhow::Result<()>` avec `now: Instant`, `SillonReader` → `Result<(), SillonError>`, `models` → `anyhow::Result<Option<Download>>` ; `default_voice_to_install` utilisé par `models::ensure_default_voice` et `Reader::ensure_default_voice` avec la même signature.

**Review Focus** : chaque ligne a son test dans la tâche propriétaire (1 → Tâche 5, 2 → Tâche 5, 3 → Tâches 3 et 4, 4 → Tâche 1, 5 → Tâche 2).

## Bloc des interfaces produites

```rust
// src/config.rs
pub const READING_FONTS: &[&str] = &["literata", "atkinson", "openDyslexic"];
pub const READING_SIZE_MIN: f32 = 14.0;
pub const READING_SIZE_MAX: f32 = 40.0;
pub const READING_SIZE_DEFAULT: f32 = 22.0;
pub struct ReadingConfig { /* … */ pub font: String, pub size: f32 }     // [reading] font, size
impl Config { pub fn read_from(path: &std::path::Path) -> anyhow::Result<Config> }

// src/reader/settings.rs
pub struct SettingsView  { /* … */ pub reading_font: String, pub reading_size: f32 }
pub struct SettingsPatch { /* … */ pub reading_font: Option<String>, pub reading_size: Option<f32> }
impl Reader { pub fn update_settings(&mut self, patch: &SettingsPatch, now: Instant) -> anyhow::Result<SettingsView> } // existant : applique ET écrit

// src/models.rs
pub fn default_voice_to_install(available: &[&'static ModelPackage], installed: impl Fn(&ModelPackage) -> bool) -> Option<&'static ModelPackage>;
pub fn ensure_default_voice(mlx: bool, running: Option<&Download>) -> anyhow::Result<Option<Download>>;

// src/reader/migration.rs
impl Reader { pub fn ensure_default_voice(&mut self, now: Instant) -> anyhow::Result<()> }

// src/app/config.rs
impl Field { pub fn copy_value(&self, from: &Config, to: &mut Config) }

// src/app/settings.rs
impl App {
    pub fn set_config_target(&mut self, path: Option<std::path::PathBuf>);
    pub fn save_field(&mut self, field: Field) -> anyhow::Result<std::path::PathBuf>;
    pub fn config_adjust_and_save(&mut self, delta: i32);
    pub fn config_commit_edit_and_save(&mut self);
}

// ffi/src/lib.rs (UniFFI)
pub struct SettingsView  { /* … */ pub reading_font: String, pub reading_size: f32 }
pub struct SettingsPatch { /* … */ #[uniffi(default = None)] pub reading_font: Option<String>,
                                   #[uniffi(default = None)] pub reading_size: Option<f32> }
impl SillonReader { pub fn ensure_default_voice(&self) -> Result<(), SillonError> }
// Pas de ReaderCommand::SeekToFraction : SeekTo { sentence } + DocView.sentence_count.
```

```swift
// Swift généré (macos/Generated/sillon_ffi.swift)
SettingsView.readingFont: String        // "literata" | "atkinson" | "openDyslexic" (= ReadingFont.rawValue)
SettingsView.readingSize: Float         // 14…40, défaut 22
SettingsPatch(…, readingFont: String? = nil, readingSize: Float? = nil)
SillonReader.ensureDefaultVoice() throws
```
