# Moteurs par système — les modèles (lots 3 à 8) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tout le calcul de la voix et de la dictée vit dans le cœur Rust : MLX sur Mac, ONNX Runtime sous Windows et Linux, et `dysflow-cli` sait dicter un fichier sur les trois systèmes.

**Architecture:** Sur Mac, un fil unique possède tous les modèles MLX de Rust et prend le même verrou que le MLX Swift restant. La dictée est une machine à états en Rust (découpe aux silences, relecture bornée au segment) au-dessus de deux contrats, `Recognizer` et `StreamingRecognizer`. Chaque modèle passe de Swift à Rust seul : fixture figée, parité, bascule, suppression du Swift.

**Tech Stack:** Rust 2021 (MSRV 1.88 cœur, 1.91 `ffi/`), `mlx-rs` =0.34.0 (MLX 0.32.2), `ort` =2.0.0-rc.13 (ONNX Runtime 1.31.0), `sherpa-onnx` =2.0.2, UniFFI 0.32.

**Spec:** `docs/superpowers/specs/2026-11-21-moteurs-par-plateforme-design.md`

**Procédure commune des tâches « Port » :** `docs/superpowers/plans/2026-11-22-moteurs-modeles-procedure-de-port.md`

**Plan précédent (fait) :** `docs/superpowers/plans/2026-11-21-moteurs-fondations.md`

## Jalons

| Jalon | Tâches | Ce qui marche à la fin | Point d'arrêt |
|---|---|---|---|
| Lot 3 | 1–6 | Kokoro en Rust dans l'app Mac et au terminal | **Porte : tâche 4**, test de charge des deux MLX |
| Lot 4 | 7–13 | Dictée d'un fichier sous Windows, Linux et Mac | **Porte : tâche 10**, qualité de la découpe ; tâche 13, PC de l'utilisateur |
| Lot 5 | 14–15 | L'app Mac dicte avec Parakeet en Rust | Essai au micro par l'utilisateur |
| Lot 6 | 16–23 | Tous les modèles de dictée Mac en Rust | — |
| Lot 7 | 24–26 | Plus de calcul Swift, plus de mlx-swift | Écoute par l'utilisateur |
| Lot 8 | 27–28 | Whisper small sous Windows/Linux si la mesure le garde | PC de l'utilisateur |

Chaque lot laisse une app qui marche et peut être fusionné seul. Fusion, push
de `main` et publication : seulement avec l'accord de l'utilisateur.

## Décisions de ce plan

La spec les laissait au constructeur, ou le code les impose.

- **D1. Un fil MLX.** `mlx_rs::Array` n'est pas `Send`, et `KokoroBackend` exige `Send + Sync` : les modèles vivent sur un fil, les appelants tiennent des poignées.
- **D2. Un verrou pour les deux MLX.** `MLXGate.shared` (Swift) prend désormais un verrou exporté par Rust.
- **D3. Pas de panique MLX.** Le terminal est en `panic = "abort"` : `mlx.metallib` est vérifié avant le premier appel.
- **D4. Découpe.** Silence 0,416 s (13 trames), parole minimale 0,25 s, marge 0,2 s, segment de 12 s au plus, coupé à la trame la plus calme de ses 3 dernières secondes.
- **D5. Silence sur Mac.** D'abord un détecteur d'énergie en Rust pur, sans dépendance ; Silero porté en `mlx-rs` seulement s'il échoue à la porte de la tâche 10.
- **D6. Le binaire du terminal exige ses bibliothèques** de dictée. C'est sûr : l'installateur et `--update` remplacent tout le dossier du paquet.
- **D7. Un Kokoro int8 déjà installé au terminal d'un Mac reste la voix** : pas de téléchargement des 327 Mo du paquet MLX sans demande.
- **D8. Phonon passe en tête du lot 6** (la spec le met en dernier) : il partage le graphe de Parakeet, dont le Swift ne peut partir qu'après lui.

## Global Constraints

- Ne maintenir que du Rust ; bibliothèques C++ en dessous acceptées (MLX, ONNX Runtime, sherpa-onnx).
- `ort` reste épinglé `=2.0.0-rc.13`, ONNX Runtime **≥ 1.31** chargé dynamiquement ; `mlx-rs` reste `=0.34.0`, `sherpa-onnx` `=2.0.2`.
- L'app macOS n'embarque **pas** ONNX Runtime.
- Jamais deux copies d'ONNX Runtime de versions différentes dans un binaire.
- Un Mac déjà installé ne retélécharge rien : mêmes identifiants, mêmes dossiers, mêmes réglages.
- Le repli ne télécharge jamais rien.
- Ne pas casser l'app Mac pendant la transition : chaque modèle passe seul, quand sa parité passe, et son Swift part dans le même lot.
- Parité voix : mêmes durées, écart moyen **< 0,01**. Parité dictée : même texte que le Swift sur un audio fixé.
- Chaîne de dictée : taux d'erreur de mots **au plus 1 point** au-dessus de la relecture complète, sur au moins trois audios de 40 s ou plus.
- Dictée : texte définitif **≤ 1 s** après la fin d'un segment sur le M5 en processeur seul, délai **indépendant** de la durée de la dictée.
- Kokoro : ≤ 10 % occupé à 1×, 2× sans coupure.
- MSRV 1.88 (cœur), 1.91 (`ffi/`).
- Messages utilisateur bilingues par `crate::tr!` / `Tr::new(en, fr)`.
- Téléchargements de build et de CI épinglés par SHA-256 ou par commit complet.
- Une anomalie se mesure avant d'être classée ; un test ne se modifie jamais pour faire disparaître un symptôme non expliqué.
- Les mesures se font machine au calme (charge < 2, pas de swap), et le relevé l'indique.
- Une étape qui demande la CI demande de pousser : accord de l'utilisateur d'abord, par branche.
- Hors sujet : interface, micro et capture hors macOS, carte graphique sous Windows/Linux, nouveaux modèles.
- Commits : messages en français, suivis de `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Parole continue sans pause, ou bruit de fond constant.** Attendu : coupe à 12 s au plus, aucun mot perdu ni doublé, mémoire bornée. Tâches 7 et 10.
2. **Touche de dictée relâchée pendant le chargement, ou nouvelle dictée pendant que l'ancienne finit.** Attendu : tout l'audio est lu, le texte de l'une ne tombe jamais dans l'autre. Tâche 8.
3. **Lecture à voix haute et dictée en même temps, une copie de MLX chacune.** Attendu : ni blocage, ni calcul simultané, ni résultat faux. Tâches 2 et 4.
4. **Réglages venus d'un autre état** : int8 installé sur un Mac, identifiant MLX lu sous Linux. Attendu : le modèle installé sert, rien ne se télécharge. Tâche 5.
5. **Installation abîmée** : `mlx.metallib`, bibliothèque de dictée ou poids absents ou tronqués. Attendu : erreur lisible et repli annoncé quand le programme peut démarrer ; jamais d'arrêt brutal sur un appel MLX. Tâches 2, 3, 9 et 12.

---

## Lot 3 — Kokoro en Rust sur Mac

### Task 1: Un build MLX qu'on peut livrer

**Files:**
- Create: `scripts/mlx-sources.sh`, `scripts/mlx-sources.cmake`
- Modify: `scripts/build-ffi.sh`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `NOTICE`, script des licences de `macos/project.yml`

**Interfaces:**
- Produces: `scripts/mlx-sources.sh`, à sourcer ; il exporte `CMAKE_TOOLCHAIN_FILE_aarch64_apple_darwin` et `MLX_RS_METAL_PATH="$PWD/target/mlx-metallib"`. Après un build, `target/mlx-metallib/mlx.metallib` existe.

- [ ] **Step 1: Épingler les cinq sources de `mlx-sys` 0.7.0.**

| Source | Épingle |
|---|---|
| MLX | commit `1f8e74e3f12f31365464a6867c6579f0e9b29d85` (v0.32.2) |
| fmt | commit `407c905e45ad75fc29bf0f9bb7c5c2fd3475976f` (12.1.0) |
| gguflib | commit `8fa6eb65236618e28fd7710a0fba565f7faa1848` |
| nlohmann/json | archive `v3.11.3/json.tar.xz`, SHA-256 `d6c65aca6b1ed68e7a182f4757257b107ae403032760ed6ef121c9d55e81757d` |
| metal-cpp | archive `metal-cpp_26.zip` (developer.apple.com), SHA-256 `4df3c078b9aadcb516212e9cb03004cbc5ce9a3e9c068fa3144d021db585a3a4` |

  Le script les range dans `target/mlx-sources/` ; le fichier CMake pose `FETCHCONTENT_SOURCE_DIR_{MLX,FMT,GGUFLIB,JSON,METAL_CPP}`. Il ne pose pas `CMAKE_SYSTEM_NAME` : ce n'est pas une compilation croisée.
- [ ] **Step 2: Prouver qu'aucune source ne vient d'ailleurs.** Build à froid avec `FETCHCONTENT_FULLY_DISCONNECTED=ON` → succès. Une empreinte modifiée à la main → le script refuse. `tesseract-rs`, qui lit la même variable de toolchain, se construit toujours.
- [ ] **Step 3: Licences.** Joindre celles de MLX, mlx-c, metal-cpp, nlohmann/json, fmt et gguflib depuis ces sources, préfixées `mlx-rs-`. `build-macos.sh` refuse l'app s'il en manque une.
- [ ] **Step 4: MSRV sur Mac.** Ajouter au job macOS `cargo +1.88 check -p dysflow --features mlx --locked` et `cargo +1.91 check -p dysflow-ffi --features mlx --locked`. Les deux passent sur le M5 au 2026-11-22. Un échec est remonté avec la version requise : ne pas relever la MSRV seul.
- [ ] **Step 5: Déplacer le cache.** `cache-directories` passe de `~/.mlx` à `target/mlx-metallib` : `mlx-sys` ne réécrit pas ses noyaux quand son build est restauré. Puis vérifier : un second run ne recompile pas MLX (temps de `build-ffi` joint).
- [ ] **Step 6: Commit.** `build(mlx): sources épinglées et licences de la copie Rust`

### Task 2: Le fil MLX, le verrou commun, le budget

**Files:**
- Create: `src/compute/mlx/{mod.rs,worker.rs,budget.rs,metallib.rs}`, `src/compute/gpu.rs`
- Delete: `src/compute/mlx_smoke.rs` (`smoke` passe dans `compute::mlx`)
- Modify: `src/compute/mod.rs`, `ffi/src/lib.rs`, le fichier de `MLXGate` sous `macos/Sources/`
- Test: modules `tests` de ces fichiers, `macos/Tests/MLXGateTests.swift`

**Interfaces:**
- Produces:

```rust
// src/compute/gpu.rs — un seul verrou, pour les deux copies de MLX.
pub fn lock();    // os_unfair_lock sur macOS : la priorité se transmet
pub fn unlock();  // toujours sur le fil qui a pris le verrou
pub fn run<T>(body: impl FnOnce() -> T) -> T;

// src/compute/mlx/worker.rs
/// Un modèle possédé par le fil MLX. `Send + Sync` sans `unsafe`.
pub struct Held<T: 'static> { /* numéro de case */ }
impl<T: 'static> Held<T> {
    pub fn load(build: impl FnOnce() -> Result<T> + Send + 'static) -> Result<Self>;
    pub fn with<R: Send + 'static>(&self, body: impl FnOnce(&mut T) -> Result<R> + Send + 'static) -> Result<R>;
}
// Drop : dépose la libération sur le fil MLX sans l'attendre (un Drop peut
// avoir lieu dans un travail), puis le cache y est vidé.

// src/compute/mlx/budget.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need { pub memory: usize, pub cache: usize }
pub fn set(key: &str, need: Need);
pub fn remove(key: &str);
pub fn total() -> Need;

// src/compute/mlx/metallib.rs
pub fn locate_from(exe_dir: &Path) -> Result<PathBuf>;  // <exe_dir>/mlx.metallib
```

  UniFFI : `gpu_lock()`, `gpu_unlock()`, hors de la feature `mlx`.

- [ ] **Step 1: Écrire les tests.**

```rust
#[test]
fn two_threads_never_compute_at_once() {
    // 8 fils, 50 passages chacun, moitié `gpu::run`, moitié `Held::with`
    assert_eq!(max_inside.load(SeqCst), 1);
}
#[test]
fn a_held_value_need_not_be_send() {
    let held = Held::load(|| Ok(std::rc::Rc::new(6))).unwrap();
    assert_eq!(held.with(|v| Ok(**v)).unwrap(), 6);
}
#[test]
fn a_nested_call_is_an_error_not_a_deadlock() {
    let held = Arc::new(Held::load(|| Ok(1)).unwrap());
    let inner = held.clone();
    assert!(held.with(move |_| inner.with(|v| Ok(*v))).is_err());
}
#[test]
fn a_failing_job_leaves_the_worker_usable() { /* with → Err, puis with → Ok(1) */ }
#[test]
fn the_limits_are_the_sum_of_the_needs() {
    set("a", Need { memory: 1 << 30, cache: 64 << 20 });
    set("b", Need { memory: 2 << 30, cache: 256 << 20 });
    assert_eq!(total(), Need { memory: 3 << 30, cache: 320 << 20 });
}
#[test]
fn a_missing_metallib_is_an_error_that_names_the_path() {
    assert!(locate_from(dir.path()).unwrap_err().to_string().contains("mlx.metallib"));
}
```

  Swift : `testTheSharedGateExcludesRust` — pendant `MLXGate.shared.run`, un calcul Rust lancé d'un autre fil ne commence pas.
- [ ] **Step 2: Vérifier l'échec.**
- [ ] **Step 3: Implémenter.** Le fil démarre au premier usage, en QoS `userInitiated`, et prend `gpu::run` autour de chaque travail. Avant son premier appel MLX : `locate_from`, puis `mlx_rs::metal::set_metallib_path` ; sous `cfg(test)`, `target/mlx-metallib` est accepté en plus, et `DYSFLOW_MLX_METALLIB` (un fichier) prime, pour les exemples et les tests de `ffi/`. Le build de test de l'app copie `mlx.metallib` à côté de son image dès cette tâche. `MLXGate.shared` appelle `gpuLock` et `defer { gpuUnlock() }` ; `MLXGate()` garde son verrou propre pour les tests. Aucun code Swift n'appelle un calcul Rust en tenant `MLXGate`.
- [ ] **Step 4: Vérifier.** `cargo test -p dysflow --features mlx compute`, puis `DysflowTests`.
- [ ] **Step 5: Commit.** `feat(compute): un fil MLX et un seul verrou pour les deux copies`

### Task 3: Port — Kokoro

**Files:**
- Create: `src/tts/kokoro/mlx/{mod.rs,config.rs,albert.rs,modules.rs,decoder.rs}`, `examples/voice_bench.rs` (premier appel, médiane à chaud, facteur temps réel, RSS, pic MLX ; JSON)
- Test: `src/tts/kokoro/mlx/` (`parity`), fixture existante `macos/Tests/Fixtures/kokoro-parite.json`
- Référence : `.superpowers/spikes/spike-kokoro-mlxrs/src/kokoro.rs` (hors git), `macos/Vendor/KokoroMLX/{Albert,Kokoro*}.swift`, `Blocks/`

**Interfaces:**
- Consumes: `Held`, `budget` (Task 2), `KokoroBackend`.
- Produces: `dysflow::tts::kokoro::mlx::MlxBackend`, `MlxBackend::new() -> Self`, `impl KokoroBackend` ; `pub fn warm_up_ids() -> Vec<i64>`.

- [ ] **Step 1: Lire la procédure de port, puis écrire les tests** (P1 existe déjà : la fixture est celle du Swift) :

```rust
#[test]
#[ignore = "poids Kokoro"]
fn parity() {
    // graine 7, ids et style de la fixture
    assert_eq!(out.durations, reference.durations);
    assert_eq!(out.samples.len(), expected.len());
    assert!(mean_error(&out.samples, &expected) < 0.01);
}
#[test]
#[ignore = "poids Kokoro"]
fn a_cancelled_synthesis_is_cancelled_not_failed() {
    assert!(backend.infer(&ids, &style, &Cancel::when(|| true)).unwrap_err().is::<tts::Cancelled>());
}
#[test]
fn truncated_weights_are_an_error() { /* safetensors de 16 octets → Err qui nomme le fichier */ }
#[test]
fn warm_up_ids_match_the_swift_ones() {
    let ids = warm_up_ids();
    assert_eq!((ids.len(), ids[0], ids[ids.len() - 1]), (38, 0, 0));
}
```

- [ ] **Step 2: Vérifier l'échec.**
- [ ] **Step 3: Porter depuis le spike**, en remplaçant chaque `unwrap` par une erreur. Points d'arrêt de `cancel` : avant le passage, après les durées, après `encode`, à chaque bloc de `decode`, à chaque étage du générateur. Budget `"kokoro"` : `memory: 1 << 30, cache: 64 << 20`. Phrase de préparation : `la vwa se prepar, avek yn fraz kurt.` (mêmes identifiants qu'en Swift) ; elle ne refait rien si un passage complet a déjà eu lieu dans le processus.
- [ ] **Step 4: Parité et mesures (P5).** Ajouter les phrases de 34 et 162 jetons au relevé. Le pic de ~1,44 Go des phrases longues est noté, pas corrigé ici.
- [ ] **Step 5: Commit.** `feat(kokoro): calcul MLX en Rust, à parité du Swift`

### Task 4: PORTE — deux MLX sous charge, en Release

Rien de MLX en Rust n'entre dans l'app livrée avant que cette porte passe.

**Files:**
- Create: `macos/Tests/MLXLoadTests.swift`, `scripts/mlx-charge.sh`
- Modify: `ffi/src/lib.rs` (export de test `mlx_rust_kokoro_check`, feature `mlx`), spec (« Décisions », Lot 3)

- [ ] **Step 1: Le test.** Dans un même processus, 200 tours : Kokoro Rust (fixture, graine 7) sur un fil, Parakeet v3 Swift (audio fixé) sur un autre, plafonds de l'app posés, `mlx.metallib` à côté de l'image et `~/.mlx` absent (`HOME` temporaire). Assertions : durées identiques et écart < 0,01 à chaque tour ; texte identique à chaque tour ; jamais deux calculs à la fois ; aucun blocage (30 s par tour au plus).
- [ ] **Step 2: Le lancer en Release**, puis 10 minutes de lecture et de dictée alternées.
- [ ] **Step 3: Relever** pour chaque copie le pic et la mémoire active, la mémoire résidente du processus au début et à la fin (croissance ≤ 5 % entre le 20e et le 200e tour), et le swap.
- [ ] **Step 4: Décider et écrire « Lot 3 » dans la spec.** Échec → **arrêt du plan** : rapporter à l'utilisateur les options (a) et (b) de la spec, avec les mesures.
- [ ] **Step 5: Commit.** `test(mlx): charge des deux copies en Release (porte du lot 3)`

### Task 5: Le catalogue sait ce que Rust calcule, le repli est branché

**Files:**
- Modify: `src/models.rs`, `src/compute/mod.rs`, `src/tts/mod.rs`, `src/reader/{migration,settings}.rs`, `src/launch.rs`

**Interfaces:**
- Consumes: `available`, `forced`, `fallbacks`, `MlxBackend`.
- Produces:

```rust
impl Runtime {
    /// Ce binaire sait le calculer : en Rust s'il est porté, sinon par l'app.
    pub fn loadable(self, app_mlx: bool) -> bool;
}
/// `available` + `DYSFLOW_ACCELERATOR`, lu une fois par processus.
pub fn ordered(app_mlx: bool) -> (Vec<Accelerator>, Option<String>);
/// Le paquet à charger pour un choix enregistré : lui, sinon un installé de sa famille.
pub fn resolve(configured: &str, accelerators: &[Accelerator], installed: impl Fn(&ModelPackage) -> bool) -> Option<&'static ModelPackage>;
```

- [ ] **Step 1: Écrire les tests** (accélérateurs et installation passés en paramètres) :

```rust
fn the_app_still_sees_the_same_fifteen_packages();          // BEFORE_LOT_2_MAC_IDS
fn the_mac_terminal_shows_only_ported_mlx_runtimes();       // Kokoro oui ; Qwen3-TTS, Magpie, Whisper non
fn an_installed_int8_on_a_mac_is_kept_and_nothing_downloads(); // default_voice_to_install == None
fn an_mlx_choice_read_on_linux_resolves_to_its_family();    // kokoro_82m_mlx → kokoro_82m_int8 installé
fn an_mlx_choice_with_nothing_installed_downloads_nothing();// resolve == None
fn a_failed_mlx_load_falls_back_to_installed_int8();        // note non vide, moteur int8
fn an_unported_best_engine_gives_way_to_a_loadable_one(); // famille à deux paquets, Mlx non porté → OrtCpu
fn the_failed_package_is_not_retried();                     // un seul essai par paquet
fn a_forced_accelerator_is_used();
fn an_unavailable_forced_accelerator_warns_once();
```

- [ ] **Step 2: Vérifier l'échec.**
- [ ] **Step 3: Implémenter.** `compute::available` : `Mlx` si `app_mlx || cfg!(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))`. `loadable` liste chaque variante, sans joker ; seul `KokoroMlx` est vrai sans `app_mlx` à ce stade. Le filtre `loadable` s'applique **avant** le choix du meilleur moteur par famille, dans `available_for` comme dans `fallbacks` et `resolve` : filtrer après laisserait sans paquet une famille dont seul le moteur MLX n'est pas porté. L'avis du moteur forcé et celui du repli passent par le `Option<String>` de `build_engine_with`. Le repli essaie les installés dans l'ordre, sans celui qui vient d'échouer, puis la voix du système.
- [ ] **Step 4: Vérifier** sans feature, avec `mlx`, avec `kokoro-onnx`, avec les deux.
- [ ] **Step 5: Commit.** `feat(compute): catalogue des moteurs portés, repli et moteur forcé branchés`

### Task 6: L'app et le terminal du Mac utilisent Kokoro Rust

**Files:**
- Modify: `ffi/{Cargo.toml,src/lib.rs}`, `scripts/{build-ffi,build-macos,package,install}.sh`, `macos/project.yml`, `macos/Sources/Voice/KokoroWarmUp.swift`, `macos/Sources/App/Measure.swift`, spec
- Delete: `macos/Sources/Voice/KokoroMLX.swift`, le Kokoro de `macos/Vendor/KokoroMLX/`, leurs tests une fois portés

**Interfaces:**
- Produces: `AppCompute.kokoro` construit par le cœur ; UniFFI perd `KokoroCompute`, `with_mlx(text)` reste, et gagne `prepare_kokoro(model_dir) -> Result<(), DysflowError>` et `kokoro_stats() -> KokoroStats { calls: u64, forward_seconds: f64 }`.

- [ ] **Step 1: Tests.** Porter `KokoroMLXComputeTests` (parts, rechargement, abandon) en Rust ; `KokoroWarmUpTests` passe sur `prepare_kokoro` ; un test de construction : `Contents/MacOS/mlx.metallib` existe et `nm` trouve la copie Rust.
- [ ] **Step 2: Basculer.** `mlx` devient une feature par défaut de `ffi/` ; `build-macos.sh` remplace son refus par le contrôle inverse, garde le refus d'`ort`, joint `mlx.metallib` et le signe. `package.sh` sur Mac : `ocr,kokoro-onnx,mlx`, `mlx.metallib` à côté du binaire.
- [ ] **Step 3: Vérifier l'app Release signée**, `HOME` temporaire puis réglages réels : lecture, changement de voix, aperçu pendant la lecture, abandon, `codesign --verify --deep --strict`, notarisation à blanc si le certificat est là.
- [ ] **Step 4: Supprimer le Swift de Kokoro (P8).**
- [ ] **Step 5: Mesurer et écrire dans la spec** la taille de l'app avant et après, `nm … | grep -c ' _mlx_'`, occupé à 1× (≤ 10 %), 2× sans coupure.
- [ ] **Step 6: Commit.** `feat(app): Kokoro calculé par le cœur Rust`

---

## Lot 4 — La dictée dans le cœur

### Task 7: Les contrats et la découpe

**Files:**
- Create: `src/stt/{recognizer.rs,detector.rs,segmenter.rs}`
- Modify: `src/stt/mod.rs`

**Interfaces:**
- Produces:

```rust
// recognizer.rs — audio 16 kHz mono f32
pub trait Recognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String>;
    fn release(&self);
}
pub trait StreamingRecognizer: Send + Sync {
    fn load(&self, model_dir: &Path) -> Result<()>;
    fn warm_up(&self) -> Result<()>;
    fn open_stream(&self, language: &str) -> Result<Box<dyn RecognizerStream>>;
    fn release(&self);
}
pub trait RecognizerStream: Send {
    fn feed(&mut self, samples: &[f32]) -> Result<String>;  // texte nouveau
    fn finish(&mut self) -> Result<String>;                 // texte entier
}

// detector.rs
pub const FRAME: usize = 512;
pub trait SpeechDetector: Send {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool>;
    fn reset(&mut self);
}
pub struct EnergyDetector;  // plancher de bruit suivi, sans modèle

// segmenter.rs
pub const SILENCE_FRAMES: usize = 13;
pub const MIN_SPEECH: usize = 4_000;
pub const MARGIN: usize = 3_200;
pub const MAX_SEGMENT: usize = 192_000;
pub const CUT_SEARCH: usize = 48_000;
pub struct Segment { pub start: u64, pub samples: Vec<f32>, pub forced: bool }
pub struct Segmenter;
impl Segmenter {
    pub fn new(detector: Box<dyn SpeechDetector>) -> Self;
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Segment>>;
    pub fn open(&self) -> Option<&[f32]>;                  // segment en cours
    pub fn finish(&mut self) -> Result<Option<Segment>>;
    pub fn buffered(&self) -> usize;
}
```

- [ ] **Step 1: Écrire les tests**, avec un détecteur scripté :

```rust
fn silence_alone_gives_no_segment();
fn two_sentences_give_two_segments_with_their_margins();   // 0,2 s de part et d'autre
fn a_pause_shorter_than_thirteen_frames_does_not_cut();
fn a_click_shorter_than_a_quarter_second_is_not_speech();
fn finish_returns_speech_still_open();
fn one_sample_at_a_time_equals_all_at_once();
fn endless_speech_is_cut_by_twelve_seconds() {
    // 120 s de parole
    assert!(segments.iter().all(|s| s.forced && s.samples.len() <= MAX_SEGMENT));
    assert_eq!(rebuilt_from(&segments), speech); // ni trou ni doublon
}
fn a_forced_cut_lands_on_the_quietest_frame_of_the_last_three_seconds();
fn the_buffer_stays_bounded_over_an_hour() {
    assert!(max_buffered <= MAX_SEGMENT + 2 * MARGIN + FRAME);
}
fn constant_noise_is_not_endless_speech();   // EnergyDetector, bruit blanc à −45 dBFS
fn speech_over_noise_is_detected();          // EnergyDetector, 10 dB au-dessus du bruit
```

- [ ] **Step 2: Vérifier l'échec.**
- [ ] **Step 3: Implémenter.** Les marges ne se chevauchent que dans le silence ; une coupe forcée n'a pas de marge.
- [ ] **Step 4: Vérifier** sur les trois systèmes, sans feature.
- [ ] **Step 5: Commit.** `feat(stt): contrats de dictée et découpe aux silences`

### Task 8: `Dictation`

**Files:**
- Create: `src/stt/{rolling.rs,dictation.rs,holder.rs}`
- Référence : `macos/Sources/Dictation/{MLXDictationEngine,StreamingDictationEngine,RollingTranscription,SpeechModelRunner}.swift`

**Interfaces:**
- Produces:

```rust
pub enum Event {
    Loading,
    Loaded(Duration),
    Partial { committed: String, pending: String },
    Committed { text: String, delay: Duration },
    Memory(u64),
    Ended { text: String },
    Failed(String),
}
pub enum Engine {
    Segmented(Arc<dyn Recognizer>, Box<dyn SpeechDetector>),
    Streaming(Arc<dyn StreamingRecognizer>),
}
pub struct Dictation;
impl Dictation {
    pub fn start(engine: Engine, model_dir: &Path, language: &str, sink: impl Fn(Event) + Send + 'static) -> Self;
    pub fn push(&self, samples: &[f32]);   // ne bloque jamais
    pub fn finish(&self) -> Result<String>;
    pub fn cancel(&self);
}
// holder.rs : garde le modèle chargé entre deux dictées.
pub fn idle_delay(keep_voice_minutes: i64) -> Option<Duration>;
```

- [ ] **Step 1: Écrire les tests**, horloge et modèle simulés. Porter tous les cas de `RollingTranscriptionTests` et `SpeechModelRunnerTests`, plus :

```rust
fn the_partial_rereads_only_the_open_segment();
fn a_committed_segment_is_never_read_again();
fn the_final_delay_does_not_grow_with_duration();        // 10 min simulées : dernier tiers ≤ premier + 0,1 s
fn pending_finals_go_before_provisional_reads();          // machine lente : aucune relecture provisoire en attente
fn finish_during_loading_reads_everything_said();
fn less_than_a_tenth_of_a_second_ends_empty();
fn cancel_emits_nothing_more();
fn a_new_dictation_never_receives_the_text_of_the_previous_one();
fn a_segment_failure_is_reported_and_keeps_what_was_committed();
fn streaming_reads_each_sample_once();
fn the_model_is_not_released_during_a_long_pause();
```

- [ ] **Step 2: Vérifier l'échec.**
- [ ] **Step 3: Implémenter.** Un fil par dictée. Première relecture immédiate, puis `max(0,4 s, 3 × la dernière lecture)`, au moins 1 600 échantillons ; flux par pas de 80 ms. Segments joints par une espace. Libération : négatif, jamais ; 0, après 5 s ; sinon ce nombre de minutes.
- [ ] **Step 4: Vérifier.**
- [ ] **Step 5: Commit.** `feat(stt): Dictation, provisoire borné au segment`

### Task 9: Parakeet v3 et Silero par sherpa-onnx

**Files:**
- Modify: `src/stt/sherpa.rs` → `src/stt/sherpa/{mod.rs,parakeet.rs,vad.rs}`, `src/models.rs`

**Interfaces:**
- Produces: `ParakeetSherpa::new(threads: u32)` (`impl Recognizer`), `SileroDetector::new(model: &Path) -> Result<Self>` (`impl SpeechDetector`, par `detected()`), `Runtime::ParakeetSherpa` (`OrtCpu`, loadable avec `stt-sherpa`).
- Paquet `parakeet_tdt_v3_int8`, famille `parakeet_tdt_v3`, recommandé, CC-BY-4.0, dépôt `csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`, révision `ed5d745623472f97cfc2e6c2916ee248798fc5a8` :

| Fichier | Octets | SHA-256 |
|---|---|---|
| `encoder.int8.onnx` (principal) | 652282294 | `1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479` |
| `decoder.int8.onnx` | 11845274 | `0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5` |
| `joiner.int8.onnx` | 6355277 | `cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98` |
| `tokens.txt` | 93939 | `d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d` |

- [ ] **Step 1: Écrire les tests.**

```rust
fn linux_and_windows_see_parakeet_int8_only();
fn the_mac_terminal_sees_parakeet_int8_until_the_mlx_port();
fn an_mlx_choice_resolves_to_installed_parakeet_int8();
fn the_app_never_sees_a_sherpa_package();
fn removing_one_parakeet_keeps_the_other_in_the_same_folder();
fn a_missing_model_file_is_named();
fn two_different_runtimes_are_refused_before_loading();
#[ignore] fn parakeet_reads_the_french_sample();   // test_wavs/fr.wav du dépôt, texte du spike
```

- [ ] **Step 2: Vérifier l'échec** par `scripts/ort-gate.sh` (les tests `stt-sherpa` ne trouvent pas la bibliothèque autrement).
- [ ] **Step 3: Implémenter.** `nemo_transducer`, fils `min(engine.threads, 4)`, fournisseur `cpu`. `check_same_runtime` une fois par processus, résultat gardé ; la porte le réutilise pour son point 3. `vad_smoke` passe par `SileroDetector`.
- [ ] **Step 4: Vérifier** la porte sur les quatre systèmes, chemins « Élodie Martin » et « Łukasz Gaweł » compris.
- [ ] **Step 5: Commit.** `feat(stt): Parakeet v3 int8 et Silero par sherpa-onnx`

### Task 10: PORTE — banc, qualité de la découpe, détecteur du Mac

**Files:**
- Create: `examples/dictation_bench.rs`, `scripts/dictee-corpus.sh`, `src/stt/wer.rs`
- Modify: spec (« Décisions », Lot 4)

**Interfaces:**
- Produces: `wer(reference: &str, heard: &str) -> f64` ; le banc prend `--model-dir --runtime --detector --wav --ref --realtime --full` et rend un JSON (premier appel, facteur temps réel, pic MLX, WER, délai p50/p95 après coupe et après fin de parole, pire retard du provisoire, RSS, charge, swap).

- [ ] **Step 1: Tests de `wer`** : casse et ponctuation ignorées ; `("le chat dort", "le chien dort")` → 1/3 ; référence vide et texte vide → 0.
- [ ] **Step 2: Le corpus**, hors git, épinglé par empreinte : quatre audios de 40 s ou plus avec leur texte — parole continue en français, parole continue en anglais, français avec pauses, français mêlé de bruit à 10 dB. Sources sous licence redistribuable (LibriVox domaine public, FLEURS CC-BY-4.0), notées dans le script. Plus un passage de 10 minutes en boucle.
- [ ] **Step 3: Mesurer** sur le M5 en processeur seul, Parakeet int8, 4 fils, en temps réel : relecture complète, découpe Silero, découpe énergie.
- [ ] **Step 4: Décider.**
  - Découpe Silero : WER ≤ relecture + 1 point sur chaque audio, p95 ≤ 1 s après coupe, dernier tiers des 10 minutes ≤ premier + 0,1 s. Échec → ajuster les constantes de D4 une fois, remesurer ; second échec → **arrêt du plan**, mesures à l'utilisateur.
  - Énergie : mêmes seuils. Réussite → détecteur du Mac. Échec → porter Silero en `mlx-rs` dans la tâche 14, parité des décisions trame à trame ≥ 99 %.
- [ ] **Step 5: Écrire la décision et committer.** `test(stt): banc de dictée et décision sur la découpe (porte du lot 4)`

### Task 11: `dysflow-cli` dicte un fichier

**Files:**
- Create: `src/stt/file.rs`
- Modify: `src/launch.rs`, `README.md`

**Interfaces:**
- Produces: `--transcribe <WAV>`, `--dictation-model <ID>`, `--dictation-language <fr|en>`, `--install-model <ID>` ; `stt::engine_for(package: &ModelPackage, threads: u32) -> Result<Engine>`.

- [ ] **Step 1: Tests de bout en bout**, modèle simulé : le texte seul sur stdout, la progression sur stderr ; code ≠ 0 si le fichier manque, n'est pas un WAV 16 kHz mono (le message donne la commande `ffmpeg`), ou si le modèle manque (le message donne `--install-model <id>`) ; `--transcribe` ne télécharge jamais ; chemin accentué accepté.
- [ ] **Step 2: Vérifier l'échec, implémenter, vérifier.** Le fichier passe par `Dictation`, pas par une lecture d'un bloc.
- [ ] **Step 3: Commit.** `feat(cli): dictée d'un fichier`

### Task 12: Les paquets livrent la dictée

**Files:**
- Modify: `scripts/{package,install,ort-gate,sherpa-onnx,paquet-commun}.sh`, `install.sh`, `scripts/windows-imports.py`, `.github/workflows/{ci,release}.yml`, `src/tts/kokoro/onnx.rs`, `NOTICE`, `docs/superpowers/specs/2026-10-11-voix-kokoro-design.md`

- [ ] **Step 1: Paquet.** `ocr,kokoro-onnx,stt-sherpa` (plus `mlx` sur Mac), avec la bibliothèque C, `silero_vad.onnx` (empreinte de la porte), la licence Apache-2.0 de sherpa-onnx, celle de Silero (MIT) et, sous Windows, l'avis du runtime MinGW-w64 de `dysflow-espeak.exe`.
- [ ] **Step 2: Mise à jour.** Test CI : installer la dernière version publiée, mettre à jour vers ce paquet → `--version` et `--check-kokoro` passent.
- [ ] **Step 3: Bout en bout en CI**, depuis le paquet, sous Linux x86_64 et Windows : synthèse du texte au WAV par `dysflow -o`, puis `--transcribe` du français de référence (modèles en cache, empreintes vérifiées).
- [ ] **Step 4: Points remis par les fondations.**
  - Imports Windows : liste blanche stricte, sous-dossiers compris, `dysflow-espeak.exe` absent → échec.
  - `exe_dir()` absent sous Windows → erreur, jamais de nom nu ; documenter l'exception de `DYSFLOW_ORT_LIB`.
  - MSRV et Clippy `stt-sherpa` : mêmes `--config`, donc plus de téléchargement d'ORT 1.28.2.
  - Porte : binaire de test en profil `dev`, échec si deux binaires correspondent ; relever le gain.
  - Écriture vers espeak dans un fil avec délai maximal ; mesurer le coût des lots de 2 Kio.
  - La spec Kokoro cite NuGet, plus l'adresse GitHub disparue d'ORT.
- [ ] **Step 5: Installation abîmée.** Sans `silero_vad.onnx` ou sans poids : erreur lisible. Sans bibliothèque C : le README et `LISEZMOI.txt` disent de réinstaller le paquet entier.
- [ ] **Step 6: Commit.** `feat(paquet): dictée livrée sous Linux, Windows et Mac`

### Task 13: Relevés sur les PC de l'utilisateur

- [ ] **Step 1:** Donner à l'utilisateur, pour Windows et Linux, une archive de CI et **une seule commande** qui lance la porte ORT, une synthèse et le banc de dictée, et écrit `releve.json`.
- [ ] **Step 2:** Joindre les relevés à la spec ; clore « Reste à vérifier » du lot 1.
- [ ] **Step 3:** Un seuil manqué est rapporté avec ses chiffres. La tâche reste ouverte sans bloquer les lots 5 à 7, qui ne touchent que le Mac.

---

## Lot 5 — Parakeet sur Mac

### Task 14: Port — Parakeet v3 et v2

**Files:**
- Create: `src/stt/mlx/{mod.rs,dsp.rs}`, `src/stt/mlx/nemo/`, `src/stt/mlx/parakeet/`
- Référence : `macos/Vendor/KokoroMLX/STT/{DSP.swift,Parakeet/,Nemo/}`

**Interfaces:**
- Produces: `ParakeetMlx::new()` (`impl Recognizer`) ; `stt::mlx::dsp` et `stt::mlx::nemo`, réutilisés par les tâches 16 à 20 ; `stt::mlx::need(dir: &Path) -> Need` (poids + 2 Gio, cache 256 Mio, clé `"dictee"`).

- [ ] **Step 1: Procédure P1–P4.** Entrées : français court, anglais court, 12 s, silence, 0,1 s ; v3 et v2 ; quantification 8 bits lue dans `config.json`.
- [ ] **Step 2: Si la tâche 10 l'a décidé**, porter Silero (`src/stt/mlx/silero.rs`, parité ≥ 99 %).
- [ ] **Step 3: P5, puis le banc** avec le détecteur du Mac : mêmes seuils qu'à la tâche 10.
- [ ] **Step 4: Commit.** `feat(stt): Parakeet en MLX Rust`

### Task 15: L'app dicte par `Dictation` du cœur

**Files:**
- Create: `ffi/src/dictation.rs`, `macos/Sources/Dictation/CoreDictationEngine.swift`
- Modify: `DictationEngine.swift`, `RoutingDictationEngine.swift`, `MicTrialController.swift`, `App/Measure.swift`

**Interfaces:**
- Produces (UniFFI): `DictationSession::start(id, language, listener)`, `push(samples)`, `finish()`, `cancel()` ; `DictationListener::on_update(DictationUpdate)` (`with_foreign`), jamais appelé sous le verrou GPU ; `transcribe_once(id, samples, language)` pour l'essai du micro.
- Correspondance : `Partial` → `.text(committed + " " + pending, isFinal: false)` ; `Committed.delay` → `.latency` ; `Memory` → `.memory` ; `Ended` → `.text(text, isFinal: true)` ; chargement et échec → les cas actuels.

- [ ] **Step 1: Tests Swift**, session simulée : même suite d'événements qu'aujourd'hui pour une dictée, une annulation, une fin pendant le chargement et un échec (repli vers la dictée de macOS inchangé) ; le spectre du micro continue pendant le chargement.
- [ ] **Step 2: Implémenter.** `parakeet_tdt_v3` et `v2` passent par le cœur, les autres modèles gardent leur moteur Swift. Une libération d'un côté libère aussi sa part de budget.
- [ ] **Step 3: Vérifier en Release** : dictée de 2 minutes, dictée pendant une lecture, puis un modèle Swift juste après Parakeet Rust.
- [ ] **Step 4: Retirer le chargement Swift de Parakeet.** Le graphe reste pour Phonon jusqu'à la tâche 16.
- [ ] **Step 5: Essai au micro par l'utilisateur**, français et anglais.
- [ ] **Step 6: Commit.** `feat(app): Parakeet dicte par le cœur Rust`

---

## Lot 6 — Les autres dictées du Mac

Chaque tâche suit P1 à P8 de la procédure, bascule ses identifiants dans
`stt::engine_for`, `Runtime::loadable` et `Dictation.makeModelEngine`, et ne
change aucun autre modèle.

### Task 16: Port — Phonon-2

**Files:** Create `src/stt/mlx/phonon/` ; référence `STT/Parakeet/PhononLoader.swift`, `PhononContainerTests.swift`.

- [ ] P1–P8, `phonon_2` → `PhononMlx`. L'archive `.tar.zst` est lue telle quelle, par des crates Rust sous licence MIT ou Apache-2.0.
- [ ] Cas propres : `FiveValueLinear` exact (deux plans 2 bits, groupes de 128) ; archive tronquée, entrée `../`, taille annoncée démesurée → erreurs.
- [ ] P8 retire tout `STT/Parakeet/`. Commit `feat(stt): Phonon-2 en MLX Rust`.

### Task 17: Port — Whisper (tiny, small, large-v3-turbo)

**Files:** Create `src/stt/mlx/whisper/` ; référence `STT/Whisper/`, `TiktokenDecoderTests.swift`, `WhisperSmokeTests.swift`.

- [ ] P1–P8, les trois identifiants `whisper_*_4bit` → `WhisperMlx`.
- [ ] Cas propres : langue imposée (`fr`, `en`), jamais devinée ; décodage glouton ; `mini.tiktoken` repris. Le cache des fenêtres de 30 s n'est pas porté (segments ≤ 12 s) ; un fichier plus long passe par fenêtres de 30 s.
- [ ] Commit `feat(stt): Whisper en MLX Rust`.

### Task 18: Canary — expliquer les 21 échecs avant de porter

**Files:** `macos/Tests/CanaryDecoderCacheTests.swift` (lecture), rapport, spec.

- [ ] Reproduire sur le M5 ; relever écart maximal, jeton, type numérique ; rejouer sur processeur et sur GPU, et comparer à la version de MLX et de macOS de la CI, où ils passent.
- [ ] Conclure, preuve à l'appui : défaut du cache, du test, ou de MLX sur cette puce. Mesurer l'effet sur le texte de trois audios (cache contre suite entière).
- [ ] Écrire la cause dans la spec. Ne modifier ni seuil ni test sans cause prouvée. Commit `docs(canary): cause des échecs du cache sur M5`.

### Task 19: Port — Canary 1b-v2

**Files:** Create `src/stt/mlx/canary/` ; référence `STT/Canary/`, `CanarySmokeTests.swift`, `CanaryDecoderCacheTests.swift`.

- [ ] P1–P8, `canary_1b_v2_q8` → `CanaryMlx`.
- [ ] Cas propres : langue source et cible ; 1 024 jetons au plus ; `<|…|>` retirés ; tests du cache portés, conformes à la cause de la tâche 18 ; positions au-delà de la table répétées.
- [ ] Commit `feat(stt): Canary en MLX Rust`.

### Task 20: Port — Nemotron 3.5 Streaming

**Files:** Create `src/stt/mlx/nemotron/` ; référence `STT/NemotronASR/`, `STT/Nemo/`, `NemotronSmokeTests.swift`.

- [ ] P1–P8, `nemotron_35_asr_streaming_8bit` → `NemotronMlx`, `impl StreamingRecognizer`.
- [ ] Cas propres : tranches de 560 ms ; invite de langue ; suites d'espaces réduites à une, le texte ne fait que s'allonger ; même texte quel que soit le découpage des `feed`.
- [ ] P8 retire `STT/Nemo/`. Commit `feat(stt): Nemotron en flux, MLX Rust`.

### Task 21: Port — Voxtral Realtime

**Files:** Create `src/stt/mlx/voxtral/` ; référence `STT/VoxtralRealtime/`, `VoxtralSmokeTests.swift`.

- [ ] P1–P8, `voxtral_realtime_4bit` → `VoxtralRealtimeMlx`, `impl StreamingRecognizer`.
- [ ] Cas propres : retard 480 ms, pas de 1 280 échantillons ; langue ignorée ; cache MLX vidé en fin de session ; mémoire stable sur 10 minutes de flux.
- [ ] Commit `feat(stt): Voxtral Realtime en MLX Rust`.

### Task 22: Port — Qwen3-ASR (0.6B, 1.7B)

**Files:** Create `src/stt/mlx/qwen3_asr/`, `src/compute/mlx/tokenizer/bpe.rs` ; référence `STT/Qwen3ASR/`, `TTS/Qwen3TTS/Qwen2BPETokenizer.swift`, `Qwen2BPETokenizerTests.swift`.

- [ ] P1–P8, les deux identifiants `qwen3_asr_*` → `Qwen3AsrMlx`.
- [ ] Cas propres : l'invite finit par `language French<asr_text>` ; fixtures BPE reprises. Le tokenizer est commun avec la tâche 24.
- [ ] Commit `feat(stt): Qwen3-ASR en MLX Rust`.

### Task 23: Port — Cohere Transcribe, fin de la dictée Swift

**Files:** Create `src/stt/mlx/cohere/`, `src/compute/mlx/tokenizer/sentencepiece.rs` ; référence `STT/CohereTranscribe/`, `STT/SentencePieceTokenizer.swift`, `CohereSmokeTests.swift`.

- [ ] P1–P8, `cohere_transcribe_8bit` → `CohereMlx`.
- [ ] Cas propres : langue imposée ; tranches de 30 s au plus pour un audio long ; plafond de jetons levé, 1 024 positions par tranche ; une seule ligne de texte.
- [ ] Supprimer `MLXDictationEngine`, `StreamingDictationEngine`, `RollingTranscription`, `SpeechModelRunner` et `STT/`. Restent en Swift : micro, spectre, insertion, dictée de macOS.
- [ ] Les 12 paquets de dictée sont `loadable(false)` sur Mac. Commit `feat(stt): Cohere en MLX Rust, fin de la dictée Swift`.

---

## Lot 7 — Les voix à texte, fin de mlx-swift

### Task 24: Port — Qwen3-TTS

**Files:** Create `src/tts/mlx/{mod.rs,qwen3/}` ; référence `TTS/Qwen3TTS/`, `TTS/Qwen3TTSRunner.swift`, `TTS/StableSeed.swift`, `TextSpeechMLX.swift` et leurs tests.

**Interfaces:**
- Produces: `tts::mlx::MlxText::new()` (`impl TextCompute`) : un modèle par dossier, parts comptées.

- [ ] P1–P7, `qwen3_tts_06b_base_8bit` → `Qwen3TtsMlx`, 24 000 Hz ; même graine stable, mêmes voix.
- [ ] Budget `lecture:<dossier>` : poids + 1 Gio, cache 256 Mio. `cancel` toutes les ~12 trames.
- [ ] Parité : mêmes codes, écart moyen < 0,01. Pendant la tâche 24, `AppCompute.text` aiguille par `runtime` : Qwen en Rust, Magpie en Swift.
- [ ] P8, écoute par l'utilisateur. Commit `feat(voix): Qwen3-TTS en MLX Rust`.

### Task 25: Port — MagpieTTS

**Files:** Create `src/tts/mlx/magpie/` ; déplacer `cmudict_ipa_en.txt` ; référence `TTS/Magpie/`, `TTS/MagpieRunner.swift` et leurs tests.

- [ ] P1–P8, `magpie_tts_357m_8bit` → `MagpieMlx`, 22 050 Hz ; même G2P anglais, même dictionnaire, même licence jointe.
- [ ] Budget : 4 × poids + 512 Mio, cache 128 Mio. `cancel` toutes les ~20 trames.
- [ ] Écoute par l'utilisateur. Commit `feat(voix): MagpieTTS en MLX Rust`.

### Task 26: Une seule copie de MLX

**Files:**
- Delete: `macos/Vendor/KokoroMLX/`, `TextSpeechMLX.swift`, `MLXBudget.swift`, `MLXGate` et leurs tests
- Modify: `ffi/src/lib.rs`, `src/compute/{mod,gpu}.rs`, `src/models.rs`, `macos/project.yml`, `scripts/build-macos.sh`, `NOTICE`, spec

- [ ] **Step 1: Retirer** `TextSpeechCompute`, `gpu_lock`/`gpu_unlock`, mlx-swift et le framework, avec sa ligne `codesign`. `gpu::run` reste, interne au cœur.
- [ ] **Step 2: Retirer `app_mlx`** : `available()` et `loadable()` ne dépendent plus que du binaire. Le test des 15 identifiants passe inchangé.
- [ ] **Step 3: Prouver.** `grep -rnE "import MLX|KokoroMLX" macos/Sources macos/Tests macos/project.yml` → vide ; l'app construite ne contient qu'une copie de MLX ; toutes les parités passent en Release.
- [ ] **Step 4: Relever** taille de l'app, mémoire lecture + dictée, et l'écrire dans la spec.
- [ ] **Step 5: Commit.** `refactor(app): plus de calcul Swift, une seule copie de MLX`

---

## Lot 8 — Whisper sous Windows et Linux

### Task 27: PORTE — Whisper small int8, gardé si la mesure le garde

**Files:** Create `src/stt/sherpa/whisper.rs` ; Modify `src/models.rs`, spec.

**Interfaces:**
- Produces: `WhisperSherpa::new(threads: u32)`, `Runtime::WhisperSherpa`, paquet `whisper_small_int8`, famille `whisper_small_4bit`, dépôt `csukuangfj/sherpa-onnx-whisper-small`, révision `8f3c18b358db4d1f2fc1eae49d75cd20989e4309` :

| Fichier | Octets | SHA-256 |
|---|---|---|
| `small-encoder.int8.onnx` (principal) | 112442483 | `4cbe7b22fa9026b843b60a68640c747de05bafb1a11b57edc0e66c232d9f33a9` |
| `small-decoder.int8.onnx` | 262226114 | `acad50b5c782696e91b55914cc5ab4f756f1532f76e22aa6fc615f39fb69a8ee` |
| `small-tokens.txt` | 816730 | `b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126` |

- [ ] **Step 1:** Implémenter derrière l'exemple de banc seulement : langue imposée, `tail_paddings: -1`.
- [ ] **Step 2:** Mesurer sur le M5 en processeur seul. Gardé si p95 ≤ 1 s après coupe, WER ≤ Whisper small MLX + 1 point, RSS ≤ 1,5 Go. Le spike donnait 0,5 à 1,2 s par segment : l'abandon est une issue normale.
- [ ] **Step 3:** Gardé → catalogue et tests de la tâche 9 transposés. Abandonné → retirer le code. Dans les deux cas, écrire « Lot 8 » dans la spec.
- [ ] **Step 4: Commit.** `feat(stt): …` ou `docs(spec): …`, selon l'issue.

### Task 28: Bilan sur les trois systèmes

- [ ] **Step 1:** Rejouer toutes les parités et tous les bancs sur le M5, en Release.
- [ ] **Step 2:** Faire lancer à l'utilisateur la commande de la tâche 13 sur ses deux PC ; joindre les relevés.
- [ ] **Step 3:** Écrire dans la spec ce qui marche par système, les seuils tenus et manqués, et ce qui passe aux projets 2 (interface) et 3 (intégration au système).
- [ ] **Step 4: Commit.** `docs(spec): bilan du projet des moteurs`

---

## Ordre d'exécution

1 → 2 → 3 → **4** → 5 → 6 → 7 → 8 → 9 → **10** → 11 → 12 → 13 → 14 → 15 →
16 … 26 → 27 → 28. La tâche 13 attend l'utilisateur sans bloquer 14 à 26.

## Connu, hors de ce plan

- Le téléchargeur de modèles ne vérifie que la taille, pas l'empreinte (révisions épinglées, HTTPS) : les SHA-256 des tableaux servent à la CI et aux bancs. À décider avec l'utilisateur.
- Pic mémoire de Kokoro sur les phrases longues (~1,44 Go).
- Pas de voix du système de repli sous Windows, ni de mise à jour automatique (projet 3).
- `cargo fmt --check` échoue sur tout le dépôt depuis avant la branche des fondations.
- Publication d'un paquet Windows dans `release.yml` : à décider après les relevés de la tâche 13.
