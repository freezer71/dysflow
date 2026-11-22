# Moteurs par système — fondations (lots 0, W, 1, 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Poser ce dont dépendent tous les ports de modèles : une seule copie de MLX dans l'app, `dysflow-cli` sous Windows, une seule copie d'ONNX Runtime pour `ort` et `sherpa-onnx`, et un catalogue qui choisit le moteur par système.

**Architecture:** Les lots 0 et 1 sont des **portes** : chacun finit par une décision écrite dans la spec (section « Décisions »), qui fixe la forme des plans suivants. Le lot W rend le cœur compilable, testé et empaqueté sous Windows. Le lot 2 remplace le booléen `mlx` du catalogue par une liste d'accélérateurs, sans changer ce que voit un Mac déjà installé.

**Tech Stack:** Rust 2021 (MSRV 1.88 cœur, 1.91 `ffi/`), `mlx-rs` =0.34.0, `ort` =2.0.0-rc.13 (ONNX Runtime ≥ 1.31 dynamique), `sherpa-onnx` 2.0.2, UniFFI 0.32, GitHub Actions (`windows-2025`).

**Spec:** `docs/superpowers/specs/2026-11-21-moteurs-par-plateforme-design.md`

Les lots 3 à 8 (ports de modèles) auront leur propre plan, écrit avec les décisions des lots 0 et 1.

## Global Constraints

- Ne maintenir que du Rust ; bibliothèques C++ en dessous acceptées (MLX, ONNX Runtime, sherpa-onnx).
- `ort` reste épinglé `=2.0.0-rc.13`, ONNX Runtime **≥ 1.31** chargé dynamiquement (`MINIMUM: (1, 31)` dans `src/tts/kokoro/onnx.rs`).
- L'app macOS n'embarque **pas** ONNX Runtime.
- Jamais deux copies d'ONNX Runtime de versions différentes dans un binaire sans vérification sur les trois systèmes.
- Un Mac déjà installé ne voit aucun changement : mêmes identifiants de paquet (`kokoro_82m_mlx`, `parakeet_tdt_v3`…), mêmes dossiers, rien à retélécharger.
- Le repli ne télécharge jamais rien.
- Messages utilisateur bilingues par `crate::tr!` / `Tr::new(en, fr)`, comme le reste du code.
- La mise à jour automatique peut rester absente sous Windows dans ce projet.
- Les mesures se font machine au calme (charge < 2, pas de swap), et le relevé l'indique.
- Commits : messages en français, suivis de `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Un `onnxruntime.dll` plus ancien chargé à la place du nôtre sous Windows.** Windows 11 livre son propre `onnxruntime.dll` dans `System32`, et l'ordre de recherche des DLL peut le préférer. Attendu : on charge toujours celle d'à côté du binaire par chemin absolu, sinon `check_version` refuse avec un message clair. Testé en Task 3.
2. **Un nom d'utilisateur Windows avec espace ou accent** (`C:\Users\Élodie Martin\…`) pour la DLL, `dysflow-espeak.exe` et `espeak-ng-data`. Attendu : la voix se charge. Testé en Task 4.
3. **Un moteur forcé indisponible** (`DYSFLOW_ACCELERATOR=mlx` sous Linux). Attendu : ignoré, avec un avertissement, et le meilleur moteur disponible est pris. Testé en Task 7.
4. **Un Mac déjà installé.** Attendu : `available_with(true)` rend exactement la même liste d'identifiants qu'avant le lot 2. Testé en Task 7.
5. **Deux ONNX Runtime de versions différentes dans un même processus.** Attendu : détecté et refusé, pas un plantage silencieux ni des résultats faux. Testé en Task 6.

---

### Task 1: Lot 0 — `mlx-rs` lié dans l'app à côté de mlx-swift

**Files:**
- Modify: `Cargo.toml` (feature `mlx`, dépendance cible macOS aarch64)
- Create: `src/compute/mod.rs`, `src/compute/mlx_smoke.rs`
- Modify: `src/lib.rs` (`pub mod compute;`)
- Modify: `ffi/Cargo.toml` (active `dysflow/mlx`), `ffi/src/lib.rs` (export `mlx_rust_smoke`)
- Create: `macos/Tests/MLXCoexistenceTests.swift`

**Interfaces:**
- Produces: `dysflow::compute::mlx_smoke::smoke() -> anyhow::Result<f32>` (cfg `feature = "mlx"`), et l'export UniFFI `mlx_rust_smoke() -> Result<f32, FfiError>` (avec le type d'erreur déjà utilisé dans `ffi/src/lib.rs`).

- [ ] **Step 1: Ajouter la feature.** Dans `Cargo.toml`, sous `[target.'cfg(all(target_os = "macos", target_arch = "aarch64"))'.dependencies]` : `mlx-rs = { version = "=0.34.0", optional = true }` ; dans `[features]` : `mlx = ["dep:mlx-rs"]`, avec un commentaire qui dit que MLX est compilé par CMake au premier build (~4 min, réseau).
- [ ] **Step 2: Écrire le test qui échoue** dans `src/compute/mlx_smoke.rs` :

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn mlx_sums_on_the_gpu() {
        assert_eq!(super::smoke().expect("MLX"), 6.0);
    }
}
```

- [ ] **Step 3: Vérifier l'échec.** `cargo test -p dysflow --features mlx compute::mlx_smoke` → erreur de compilation, `smoke` absent.
- [ ] **Step 4: Implémenter `smoke()`.** Somme de `[1.0, 2.0, 3.0]` sur le périphérique GPU par défaut, évaluée, puis lue en `f32`.
- [ ] **Step 5: Vérifier le succès.** Même commande → PASS.
- [ ] **Step 6: Exporter par UniFFI.** Ajouter `mlx_rust_smoke` dans `ffi/src/lib.rs` et activer `features = ["clipboard", "mlx"]` pour `dysflow` dans `ffi/Cargo.toml`. Lancer `./scripts/build-ffi.sh`.
- [ ] **Step 7: Écrire le test Swift** `MLXCoexistenceTests` :
  - `testRustAndSwiftMLXInOneProcess` : une opération MLX Swift (`MLXArray([1, 2, 3]).sum().item(Float.self) == 6`), puis `try mlxRustSmoke() == 6`, puis de nouveau l'opération Swift.
  - Relancer aussi `KokoroParityTests` dans le même passage.
- [ ] **Step 8: Construire et tester l'app.**

```bash
cd macos && xcodegen generate && xcodebuild -project Dysflow.xcodeproj -scheme Dysflow \
  -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived \
  test -only-testing:DysflowTests/MLXCoexistenceTests -only-testing:DysflowTests/KokoroParityTests
```

  Attendu, l'option (a) qui tient : l'édition des liens passe sans `duplicate symbol`, et les deux tests passent.
- [ ] **Step 9: Si l'édition des liens échoue** (`duplicate symbol _mlx_…`), essayer l'option (a) de la spec :
  - lire le `build.rs` de `mlx-sys` 0.34 pour trouver comment le faire construire sur une copie de MLX fournie ;
  - le pointer sur les sources `Cmlx` de mlx-swift (`$CHECKOUTS/mlx-swift/Source/Cmlx`, voir `macos/project.yml`) ;
  - aligner les versions en épinglant mlx-swift ou `mlx-rs` sur la même version de MLX ;
  - refaire les steps 6 à 8.
- [ ] **Step 10: Écrire la décision.** Ajouter une section « Décisions » à la fin de la spec, avec une entrée « Lot 0 » :
  - le résultat (a) ou (b) ;
  - les symboles en conflit s'il y en a eu ;
  - la conséquence pour l'ordre des lots : modèle par modèle, ou d'un bloc.
- [ ] **Step 11: Mettre en cache le build de MLX dans la CI.** Dans `.github/workflows/ci.yml`, job `macos-app` : le `Swatinem/rust-cache` existant couvre `target/`. Vérifier sur une 2e exécution que MLX ne se recompile pas, en comparant la durée de l'étape `build-ffi.sh` entre les deux.
- [ ] **Step 12: Commit.**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/compute ffi macos/Tests/MLXCoexistenceTests.swift docs/superpowers/specs/2026-11-21-moteurs-par-plateforme-design.md .github/workflows/ci.yml
git commit -m "feat(compute): MLX en Rust lié dans l'app à côté de mlx-swift (lot 0)"
```

  `THIRD-PARTY-RUST.txt` doit être régénéré (`python3 -I scripts/third-party-rust.py`) et ajouté au même commit.

---

### Task 2: Lot W — le cœur se compile et se teste sous Windows

**Files:**
- Modify: `.github/workflows/ci.yml` (job `rust` : `windows-2025` dans la matrice)
- Modify: `src/tts/kokoro/phonemes.rs` (nom de programme, test d'exécutabilité)
- Modify: `src/update.rs` (Windows : erreur claire)
- Modify: tout autre fichier que la compilation Windows signale (le constructeur suit les erreurs)

**Interfaces:**
- Produces: `phonemes::PROGRAM: &str`, qui vaut `"dysflow-espeak.exe"` sous Windows et `"dysflow-espeak"` ailleurs ; `phonemes::is_executable(&Path) -> bool`, aussi défini sous Windows (fichier ordinaire dont l'extension est `exe`).

- [ ] **Step 1: Ajouter `windows-2025` à la matrice** du job `rust`.
  - Garder `cargo test --workspace --locked` et les deux `clippy`.
  - Les étapes Linux restent conditionnées par `runner.os == 'Linux'`.
  - Pousser la branche et relever la liste des erreurs de compilation Windows : c'est la liste de travail de cette tâche.
- [ ] **Step 2: Écrire les tests** dans `src/tts/kokoro/phonemes.rs` :

```rust
#[test]
fn the_program_name_follows_the_platform() {
    assert_eq!(PROGRAM, format!("dysflow-espeak{}", std::env::consts::EXE_SUFFIX));
}

#[cfg(windows)]
#[test]
fn an_exe_file_is_executable_and_a_text_file_is_not() {
    let dir = crate::paths::test_scratch("exe");
    std::fs::write(dir.join("a.exe"), b"MZ").unwrap();
    std::fs::write(dir.join("a.txt"), b"x").unwrap();
    assert!(is_executable(&dir.join("a.exe")));
    assert!(!is_executable(&dir.join("a.txt")));
}
```

- [ ] **Step 3: Vérifier l'échec**, en local pour le premier test : `cargo test -p dysflow phonemes::tests::the_program_name` → FAIL, `PROGRAM` absent.
- [ ] **Step 4: Implémenter.**
  - Remplacer chaque `"dysflow-espeak"` littéral de `phonemes.rs` par `PROGRAM`.
  - `is_executable` : `cfg(unix)` comme aujourd'hui, `cfg(windows)` par l'extension `exe`.
  - `update::run` sous Windows : `bail!` avec `tr!("automatic update is not available on Windows yet", "la mise à jour automatique n'existe pas encore sous Windows")`.
  - Corriger le reste des erreurs relevées au Step 1, en suivant ce motif :
    - `cfg(unix)` pour le code Unix ;
    - un équivalent Windows quand le comportement compte pour `dysflow-cli` ;
    - un repli neutre sinon, comme `src/models.rs:1005` pour l'espace disque.
  - `src/paths.rs` : `test_base` et `sweep_dead_test_roots` sont en `cfg(test)` et appellent `libc`. Sous Windows, ne garder que l'effacement à la sortie par `atexit` du CRT (`libc` n'est pas une dépendance Windows), ou sauter le balayage. Au choix du constructeur.
- [ ] **Step 5: Vérifier.** La CI est verte sur `windows-2025` (tests et clippy), et toujours verte sur Linux et macOS.
- [ ] **Step 6: Commit.** `git commit -m "feat(windows): le cœur se compile et se teste sous Windows (lot W)"`

---

### Task 3: Lot W — ONNX Runtime sous Windows, chargé par chemin absolu

**Files:**
- Modify: `src/tts/kokoro/onnx.rs` (`LIBRARY`, `library_path`)
- Test: `src/tts/kokoro/onnx.rs` (module `tests`)

**Interfaces:**
- Consumes: `phonemes::exe_dir()`.
- Produces: `onnx::LIBRARY`, qui vaut `"onnxruntime.dll"` sous Windows. `library_path()` ne rend jamais un nom nu sous Windows (Review Focus 1).

- [ ] **Step 1: Écrire les tests.**

```rust
#[cfg(windows)]
#[test]
fn on_windows_the_library_is_never_a_bare_name() {
    // Un nom nu laisserait Windows prendre System32\onnxruntime.dll.
    std::env::remove_var("DYSFLOW_ORT_LIB");
    assert!(library_path().is_absolute(), "{}", library_path().display());
}

#[test]
fn the_library_name_follows_the_platform() {
    let expected = if cfg!(windows) { "onnxruntime.dll" } else if cfg!(target_os = "macos") { "libonnxruntime.dylib" } else { "libonnxruntime.so" };
    assert_eq!(LIBRARY, expected);
}
```

- [ ] **Step 2: Vérifier l'échec en CI Windows.** `library_path()` rend `onnxruntime.dll` nu.
- [ ] **Step 3: Implémenter.**
  - `LIBRARY` en trois `cfg`.
  - Sous Windows, quand rien n'est trouvé à côté du binaire, `library_path()` rend `exe_dir().join(LIBRARY)`, même si le fichier est absent. Le message « bibliothèque absente », déjà testé, nomme alors ce chemin.
- [ ] **Step 4: Vérifier.** CI verte sur les trois systèmes.
- [ ] **Step 5: Commit.** `git commit -m "fix(windows): ONNX Runtime chargé à côté du binaire, jamais par la recherche de DLL"`

---

### Task 4: Lot W — `dysflow-espeak.exe` et le paquet Windows de `dysflow-cli`

**Files:**
- Modify: `scripts/build-espeak.sh` (branche Windows : CMake et MinGW-w64 d'MSYS2 (UCRT64), lancé sous le bash de Git for Windows ; espeak-ng 1.52.0 ne se compile pas avec MSVC)
- Modify: `scripts/dysflow-espeak.c` (seulement si un appel POSIX bloque la compilation MinGW-w64)
- Modify: `scripts/package.sh` (cas `windows-x86_64` : ORT par le bloc NuGet de la Task 1b, sortie `.zip`)
- Modify: `.github/workflows/ci.yml` (étape « Paquet et vérification (Windows) »)

**Interfaces:**
- Consumes: `PROGRAM`, `LIBRARY` (Tasks 2 et 3).
- Produces: un artefact CI `dysflow-Windows-X64` qui contient :
  - `dysflow.exe`, `dysflow-espeak.exe`, `espeak-ng-data/` (français et anglais), `onnxruntime.dll` ;
  - les licences jointes comme sous Linux : `COPYING`, `SOURCE.txt`, LICENSE et ThirdPartyNotices d'ORT.

- [ ] **Step 1: Construire espeak-ng 1.52.0 sous Windows.**
  - Garder la même archive et la même empreinte SHA-256 que `build-espeak.sh`.
  - Construire par CMake sur le runner `windows-2025`.
  - Si le CMake d'espeak-ng 1.52.0 ne construit pas sous MSVC, essayer MinGW (paquet MSYS2 du runner).
  - Noter dans le script le chemin retenu et pourquoi.
- [ ] **Step 2: Ajouter le cas Windows à `package.sh`.**
  - Réutiliser le bloc ONNX Runtime de la Task 1b : le paquet NuGet `microsoft.ml.onnxruntime.1.31.0.nupkg`, empreinte déjà épinglée, cas `win-x64` déjà prêt. Le dépôt GitHub de Microsoft a disparu.
  - Copier `runtimes/win-x64/native/onnxruntime.dll` à côté de `dysflow.exe`.
  - Construire avec `--features ocr,kokoro-onnx`. Si Tesseract ne se construit pas sous Windows, retirer `ocr` du paquet Windows et le noter dans le script.
- [ ] **Step 3: Ajouter l'étape CI de vérification**, dans un dossier dont le chemin contient une espace et un accent (Review Focus 2) :

```bash
dest="$RUNNER_TEMP/Élodie Martin/dysflow"
mkdir -p "$dest" && unzip -q dist/dysflow-windows-x86_64.zip -d "$dest"
"$dest/dysflow.exe" --version
"$dest/dysflow.exe" --check-voice   # voir Step 4
```

- [ ] **Step 4: Faire de `--check-voice` le contrôle d'installation.**
  - Si `install.sh` vérifie déjà que Kokoro se charge par une option existante, réutiliser cette option et la nommer ici.
  - Sinon, ajouter `--check-voice` : il charge Kokoro int8 avec le modèle installé, synthétise « Bonjour. », sort 0, et sort non nul avec le message d'erreur.
  - La CI télécharge le modèle Kokoro int8 dans un cache `actions/cache` (clé : `KOKORO_ONNX_REVISION`).
- [ ] **Step 5: Vérifier.** L'étape passe, et l'artefact `dysflow-Windows-X64` est joint.
- [ ] **Step 6: Faire vérifier sur le PC Windows de l'utilisateur.**
  - Lui donner l'artefact et les commandes : `dysflow.exe --check-voice`, puis `dysflow.exe fichier.txt`, et `dysflow.exe -o test.wav fichier.txt`.
  - Joindre son retour à la PR.
  - Ne pas cocher tant qu'il n'a pas répondu.
- [ ] **Step 7: Commit.** `git commit -m "feat(windows): paquet de dysflow-cli avec espeak-ng et ONNX Runtime (lot W)"`

---

### Task 5: Lot 1 — `sherpa-onnx` sur la même copie d'ONNX Runtime

**Files:**
- Modify: `Cargo.toml` (feature `stt-sherpa`)
- Create: `src/stt/mod.rs` (vide sauf `pub mod sherpa;`), `src/stt/sherpa.rs`
- Modify: `src/lib.rs` (`pub mod stt;`)
- Create: `examples/ort_single_copy.rs`

**Interfaces:**
- Produces:
  - `dysflow::stt::sherpa::onnxruntime_version() -> anyhow::Result<String>` : la version d'ORT que sherpa-onnx utilise réellement ;
  - `dysflow::stt::sherpa::vad_smoke(model: &Path, samples: &[f32]) -> anyhow::Result<usize>` : le nombre de segments de parole trouvés par Silero VAD.

- [ ] **Step 1: Ajouter la dépendance.**
  - Feature `stt-sherpa = ["dep:sherpa-onnx", "kokoro-onnx"]`.
  - `sherpa-onnx = { version = "=2.0.2", optional = true, default-features = false, features = ["shared"] }`, ou le nom réel de l'option de liaison dynamique, à vérifier dans le `Cargo.toml` de `sherpa-onnx-sys` 2.0.2.
  - Lire le `build.rs` de `sherpa-onnx-sys` pour savoir comment lui faire utiliser **notre** ORT 1.31 au lieu de son 1.28.2 : variable d'environnement de bibliothèque ORT, ou construction depuis les sources avec `onnxruntime` fourni.
- [ ] **Step 2: Écrire l'exemple-porte `examples/ort_single_copy.rs`**, lancé par la CI sur les trois systèmes. Il doit, dans un seul processus :
  1. charger Kokoro int8 par `OrtBackend` et synthétiser « Bonjour. » ;
  2. lancer `vad_smoke` sur 3 s d'audio fixe (1 s de silence, 1 s de la sortie Kokoro, 1 s de silence) et exiger exactement 1 segment ;
  3. exiger `onnx::version_of(&onnx::library_path())` == `onnxruntime_version()` == `"1.31.0"` ;
  4. lister les bibliothèques ONNX Runtime chargées dans le processus (Linux : `/proc/self/maps` ; macOS : `_dyld_get_image_name` ; Windows : `EnumProcessModules`) et exiger **une seule** entrée.

  Il sort 0 si tout passe, et non nul avec la raison sinon.
- [ ] **Step 3: Vérifier l'échec d'abord.** Avec le sherpa-onnx statique par défaut, le point 3 doit échouer (`1.28.2 != 1.31.0`). Ça prouve que la porte détecte le problème (Review Focus 5).
- [ ] **Step 4: Faire passer la porte** par la liaison dynamique trouvée au Step 1, sur macOS, Linux x86_64, Linux ARM64 et Windows en CI. Ajouter à `ci.yml` l'étape `cargo run --release --example ort_single_copy --features stt-sherpa`, avec le modèle Silero VAD et Kokoro int8 en cache.
- [ ] **Step 5: Si la porte ne passe pas sur un système**, appliquer le repli de la spec :
  - noter le système et l'erreur ;
  - ne pas forcer ;
  - le plan des lots suivants prévoira la dictée dans un processus séparé sur ce système.
- [ ] **Step 6: Écrire la décision.** Ajouter l'entrée « Lot 1 » à la section « Décisions » de la spec :
  - le résultat par système ;
  - comment sherpa-onnx est lié ;
  - la taille ajoutée au paquet.
- [ ] **Step 7: Faire vérifier sur les PC Windows et Linux de l'utilisateur.** Lui donner l'exemple compilé (artefact CI) et sa commande, et joindre le résultat.
- [ ] **Step 8: Commit.** `git commit -m "feat(stt): sherpa-onnx sur la même copie d'ONNX Runtime que Kokoro (lot 1)"`

---

### Task 6: Lot 1 — refus de deux ONNX Runtime différents

**Files:**
- Modify: `src/stt/sherpa.rs`
- Test: `src/stt/sherpa.rs` (module `tests`)

**Interfaces:**
- Consumes: `onnx::version_of`, `onnx::library_path`, `onnxruntime_version()` (Task 5).
- Produces: `dysflow::stt::sherpa::check_same_runtime(kokoro: &str, sherpa: &str) -> anyhow::Result<()>`, appelé avant le premier chargement d'un modèle sherpa.

- [ ] **Step 1: Écrire les tests.**

```rust
#[test]
fn the_same_runtime_passes() {
    assert!(check_same_runtime("1.31.0", "1.31.0").is_ok());
}

#[test]
fn two_runtimes_are_refused_with_both_versions_named() {
    let err = check_same_runtime("1.31.0", "1.28.2").unwrap_err().to_string();
    assert!(err.contains("1.31.0") && err.contains("1.28.2"), "{err}");
}
```

- [ ] **Step 2: Vérifier l'échec.** `cargo test -p dysflow --features stt-sherpa stt::sherpa` → FAIL, fonction absente.
- [ ] **Step 3: Implémenter `check_same_runtime`.** Message : `tr!(en = "two ONNX Runtime copies: {kokoro} (voice) and {sherpa} (dictation)", fr = "deux copies d'ONNX Runtime : {kokoro} (voix) et {sherpa} (dictée)")`.
- [ ] **Step 4: Vérifier le succès.**
- [ ] **Step 5: Commit.** `git commit -m "feat(stt): deux ONNX Runtime de versions différentes sont refusés"`

---

### Task 7: Lot 2 — le catalogue choisit le moteur par accélérateur

**Files:**
- Modify: `src/compute/mod.rs` (créé en Task 1)
- Modify: `src/models.rs:703-732` (`available_with`, `available`), plus `Runtime`
- Test: `src/compute/mod.rs`, `src/models.rs` (module `tests`)

**Interfaces:**
- Consumes: `Runtime`, `CATALOG`, `ModelPackage::family` (`src/models.rs`).
- Produces :

```rust
// src/compute/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator { Mlx, OrtCpu }
/// Du meilleur au moins bon. `app_mlx` : l'app fournit le calcul MLX (Swift),
/// tant que les ports du lot 3 et suivants ne sont pas faits.
pub fn available(app_mlx: bool) -> Vec<Accelerator>;
/// `DYSFLOW_ACCELERATOR` = "mlx" | "ort-cpu" : met cet accélérateur en tête
/// s'il est dans `found` ; sinon `found` inchangé et `Some(avertissement)`.
pub fn forced(found: Vec<Accelerator>, value: Option<&str>) -> (Vec<Accelerator>, Option<String>);

// src/models.rs
impl Runtime { pub fn accelerator(self) -> Accelerator; }
/// Par famille, seulement les paquets du meilleur accélérateur disponible.
pub fn available_for(accelerators: &[Accelerator]) -> Vec<&'static ModelPackage>;
/// Les paquets installés de la famille, dans l'ordre des accélérateurs : le repli.
pub fn fallbacks(family: &str, accelerators: &[Accelerator], is_installed: impl Fn(&ModelPackage) -> bool) -> Vec<&'static ModelPackage>;
```

  `available_with(mlx)` et `available()` restent, réécrites par-dessus `available_for(&compute::available(mlx))`. Leurs appelants (`ffi/src/lib.rs:784`, `:952`, `src/models.rs:839`, `:867`) ne changent pas.

- [ ] **Step 1: Écrire les tests** dans `src/compute/mod.rs` :

```rust
#[test]
fn the_app_on_mac_prefers_mlx() {
    assert_eq!(available(true).first(), Some(&Accelerator::Mlx));
}

#[test]
fn without_mlx_only_ort_cpu_remains_when_onnx_is_built() {
    let found = available(false);
    assert!(!found.contains(&Accelerator::Mlx));
    assert_eq!(found.contains(&Accelerator::OrtCpu), cfg!(feature = "kokoro-onnx"));
}

#[test]
fn an_unavailable_forced_accelerator_is_ignored_with_a_warning() {
    let (order, warning) = forced(vec![Accelerator::OrtCpu], Some("mlx"));
    assert_eq!(order, vec![Accelerator::OrtCpu]);
    assert!(warning.is_some_and(|w| w.contains("mlx")));
}

#[test]
fn an_unknown_forced_value_is_ignored_with_a_warning() {
    let (order, warning) = forced(vec![Accelerator::Mlx, Accelerator::OrtCpu], Some("cuda"));
    assert_eq!(order, vec![Accelerator::Mlx, Accelerator::OrtCpu]);
    assert!(warning.is_some());
}
```

  Et dans `src/models.rs` :

```rust
#[test]
fn an_installed_mac_sees_exactly_the_same_catalog() {
    let ids: Vec<_> = available_with(true).iter().map(|p| p.id).collect();
    assert_eq!(ids, BEFORE_LOT_2_MAC_IDS); // liste relevée avant le changement (Step 2)
}

#[test]
fn each_family_shows_only_its_best_accelerator() {
    let ids: Vec<_> = available_for(&[Accelerator::Mlx, Accelerator::OrtCpu]).iter().map(|p| p.id).collect();
    assert!(ids.contains(&"kokoro_82m_mlx") && !ids.contains(&"kokoro_82m_int8"));
    let ids: Vec<_> = available_for(&[Accelerator::OrtCpu]).iter().map(|p| p.id).collect();
    assert_eq!(ids, vec!["kokoro_82m_int8"]);
}

#[test]
fn fallback_lists_only_installed_packages_of_the_family_in_order() {
    let order = fallbacks("kokoro", &[Accelerator::Mlx, Accelerator::OrtCpu], |p| p.id == "kokoro_82m_int8");
    assert_eq!(order.iter().map(|p| p.id).collect::<Vec<_>>(), vec!["kokoro_82m_int8"]);
}
```

- [ ] **Step 2: Relever `BEFORE_LOT_2_MAC_IDS`.** Avant de toucher au code, écrire dans le test la constante qui liste dans l'ordre les identifiants de `available_with(true)` sur `main`. Il y en a 15 : `kokoro_82m_mlx` … `magpie_tts_357m_8bit`, sans `kokoro_82m_int8`.
- [ ] **Step 3: Vérifier l'échec.** `cargo test -p dysflow compute models::tests::each_family models::tests::fallback models::tests::an_installed_mac` → erreurs de compilation (`available_for`, `fallbacks`, `Accelerator` absents).
- [ ] **Step 4: Implémenter.**
  - `Runtime::accelerator` : `KokoroOnnx` → `OrtCpu`, tous les autres → `Mlx`.
  - `compute::available(app_mlx)` : `Mlx` si `app_mlx` ou `cfg!(feature = "mlx")` ; `OrtCpu` si `cfg!(feature = "kokoro-onnx")`.
  - `available_for` garde l'ordre de `CATALOG`.
  - Le moteur forcé est lu une fois, au démarrage, par l'appelant de `available`. Son avertissement passe par le canal d'avis existant (`build_engine_with` rend déjà un `Option<String>`).
  - `fallbacks` est fourni ici. Il sera branché dans `build_engine_with` au lot 3, qui en a besoin pour le repli MLX → int8.
- [ ] **Step 5: Vérifier.** `cargo test --workspace --locked` et `cargo test -p dysflow --features kokoro-onnx` → PASS, dont les tests existants du catalogue (`src/models.rs:1807-1872`). Puis les tests de l'app : `xcodebuild … test -only-testing:DysflowTests`.
- [ ] **Step 6: Commit.** `git commit -m "feat(compute): catalogue par accélérateur, repli sur les paquets installés (lot 2)"`

---

## Après ce plan

Avec les décisions « Lot 0 » et « Lot 1 » écrites dans la spec, écrire le plan des lots 3 à 8 : Kokoro en `mlx-rs`, `src/stt/` et `Dictation`, Parakeet, puis les autres modèles. Le code des deux spikes est gardé, hors git, dans `.superpowers/spikes/` : `spike-kokoro-mlxrs/src/kokoro.rs` (557 lignes) sert de point de départ au lot 3, et `spike-parakeet-sherpa/` aux lots 4 et 5.
