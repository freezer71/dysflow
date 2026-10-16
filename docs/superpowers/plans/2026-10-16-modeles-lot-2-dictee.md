# Modèles, lot 2 : moteurs de dictée MLX et table Dictée — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dicter avec Parakeet v3, Whisper (tiny, small, large-v3-turbo), Canary 1b-v2 ou Voxtral Realtime, choisis, téléchargés et supprimés dans une table « Dictée » des réglages, à côté de la dictée de macOS.

**Architecture:** Le catalogue Rust (`src/models.rs`) gagne six paquets de type `Dictation`, chacun dans son propre dossier ; le cœur ne fait que télécharger, mesurer et supprimer. Le calcul est côté Swift : les fichiers des quatre modèles sont **copiés** depuis mlx-audio-swift (commit `244458d`, MIT) dans le framework `KokoroMLX` (le seul lié à MLX 0.32.3). Un aiguilleur `RoutingDictationEngine` donne au contrôleur de dictée le moteur du modèle choisi ; Parakeet, Whisper et Canary font une **relecture glissante** (toutes les ~1,5 s) ; Voxtral transcrit au fil de l'eau.

**Tech Stack:** Rust (cœur, uniffi), Swift 6 / SwiftUI macOS 26, MLX Swift 0.32.3, AVFoundation, xcodegen.

**Spec:** `docs/superpowers/specs/2026-10-16-modeles-stt-tts-design.md` (section « Dictée : moteurs nouveaux », révisée le 16 oct. 2026). **Relevé technique** (fichiers à copier, signatures, ids Whisper, code du décodeur tiktoken) : `docs/superpowers/plans/2026-10-16-modeles-lot-2-releve.md` — cité « relevé §n ».

## Global Constraints

- App macOS seulement ; le terminal ne voit aucun paquet de dictée (`available_with(false)` les exclut).
- MLX : `mlx-swift` **0.32.3 exact**, lié **une seule fois** (framework `KokoroMLX`). Aucune nouvelle dépendance SPM : pas de mlx-audio-swift, pas de mlx-swift-lm, pas de swift-transformers, pas de HuggingFace.
- Fichiers copiés : depuis `https://github.com/Blaizzy/mlx-audio-swift` au commit `244458d7b61e54ff135073ddc9ed76782bb20758`, licence MIT déjà jointe ; chaque changement noté dans `macos/Vendor/KokoroMLX/ORIGINE.md`.
- **Une seule inférence MLX à la fois dans l'app** : Kokoro et la dictée passent par le même verrou.
- Un dossier par paquet de dictée (les chargeurs lisent tous les `*.safetensors` du dossier).
- Variantes : Parakeet TDT 0.6B v3 (mlx-community, bf16), Whisper tiny/small/large-v3-turbo **4 bits**, Canary 1b-v2 **q8** (Mediform), Voxtral Mini 4B Realtime 2602 **4 bits**.
- Relecture glissante : intervalle **1,5 s** ; jamais deux relectures en même temps ; texte définitif = dernière lecture à `finish`.
- Le choix du modèle de dictée : `UserDefaults` clé `dictationModel`, défaut `"system"`. Modèle absent ou qui ne se charge pas → dictée de macOS, et la section Dictée le dit.
- Supprimer le modèle de dictée choisi → la dictée repasse à macOS (confirmation, comme la lecture).
- Tests macOS : **jamais `xcodebuild test` en local** — compiler seulement ; tests Swift sur la CI. Les tests qui demandent un vrai modèle (gigaoctets) s'ignorent (`XCTSkip`) sans la variable `SILLON_STT_MODELS`.
- Textes en français, typographie française ; commits en français, fin `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Commandes

- **RUST** : `cargo test --workspace --locked --no-fail-fast` puis `cargo clippy --workspace --all-targets --locked -- -D warnings`. (`tests/clipboard_input.rs` lit le vrai presse-papiers : il échoue si celui-ci ne contient pas de texte — sans rapport.)
- **FFI** : `./scripts/build-ffi.sh`
- **UIBUILD** : `(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build-for-testing)` → `** TEST BUILD SUCCEEDED **`
- **CLONE** (tâches de copie) : `git clone https://github.com/Blaizzy/mlx-audio-swift "$CLAUDE_JOB_DIR/tmp/mlx-audio" 2>/dev/null; git -C "$CLAUDE_JOB_DIR/tmp/mlx-audio" checkout -q 244458d7b61e54ff135073ddc9ed76782bb20758` — source en lecture seule, jamais compilée ni exécutée. Chemins « R/Sources/… » du relevé = ce clone.

## Review Focus

1. **Un paquet de dictée pris pour une voix** : `first_installed_with`, l'activation d'un téléchargement fini (`poll_download` → `use_voice`) et `use_voice` ne doivent jamais faire lire le texte par un modèle de dictée. → tests Tâche 1.
2. **Kokoro qui lit pendant une relecture** : les deux passent par le même verrou MLX ; aucun plantage Metal. → Tâche 6 (verrou partagé, test).
3. **Le modèle choisi supprimé ou corrompu** pendant que l'app tourne : la dictée suivante passe à macOS avec un message, sans planter. → test Tâche 7.
4. **Dictée longue** (plusieurs minutes) : la relecture ralentit d'elle-même (jamais deux à la fois) ; Canary ne coupe pas le texte (`maxTokens` relevé). → Tâches 3 et 7.
5. **Place disque** : un téléchargement plus gros que la place libre est refusé d'avance avec la place manquante. → test Tâche 1.

---

### Task 1 : Catalogue — six paquets de dictée, chacun son dossier, jamais une voix

**Files:**
- Modify: `src/models.rs` (`Runtime`, `CATALOG`, `available_with`, `first_installed_with`, tests)
- Modify: `src/reader/settings.rs` (`use_voice` refuse la dictée ; `install_voice` vérifie la place libre)
- Modify: `src/reader/settings.rs` (`poll_download` : n'active qu'un paquet de lecture)
- Modify: `ffi/src/lib.rs` (`model_directory(id) -> Option<String>`)

**Interfaces:**
- Produces: `Runtime::{ParakeetMlx, WhisperMlx, CanaryMlx, VoxtralRealtimeMlx}` ; ids `parakeet_tdt_v3`, `whisper_tiny_4bit`, `whisper_small_4bit`, `whisper_large_v3_turbo_4bit`, `canary_1b_v2_q8`, `voxtral_realtime_4bit` ; `family` = l'id (un dossier chacun) ; FFI `model_directory(id: String) -> Option<String>` (dossier `models/<family>` si le paquet est installé, sinon `None`).

- [ ] **Step 1 : tests qui échouent** — dans `src/models.rs` :

```rust
#[test]
fn dictation_packages_each_have_their_own_folder() {
    let dictation: Vec<&ModelPackage> = CATALOG.iter().filter(|p| p.kind == Kind::Dictation).collect();
    assert_eq!(dictation.len(), 6);
    for p in &dictation {
        assert_eq!(p.family, p.id, "{} : un dossier par paquet", p.id);
    }
}

#[test]
fn dictation_is_only_offered_to_the_app() {
    assert!(available_with(false).iter().all(|p| p.kind == Kind::Speech));
    assert_eq!(available_with(true).iter().filter(|p| p.kind == Kind::Dictation).count(), 6);
}

#[test]
fn a_dictation_model_is_never_the_first_voice() {
    // Même installé, un modèle de dictée ne lit pas.
    assert!(first_installed_with(true).map_or(true, |p| p.kind == Kind::Speech));
}

#[test]
fn whisper_reads_its_vocabulary_from_tiktoken() {
    let p = CATALOG.iter().find(|p| p.id == "whisper_tiny_4bit").unwrap();
    assert!(p.companions.iter().any(|c| c.dest == "multilingual.tiktoken"));
    assert!(p.companions.iter().any(|c| c.dest == "config.json"));
}
```

Dans `src/reader/settings.rs` (constructeur de test du fichier) :

```rust
#[test]
fn a_dictation_model_cannot_become_the_voice() {
    let mut r = reader_on("");
    let err = r.use_voice("parakeet_tdt_v3", Instant::now()).unwrap_err().to_string();
    assert!(err.contains("dictée"), "{err}");
}

#[test]
fn a_download_larger_than_the_free_space_is_refused() {
    // `free_space` injectable : voir Step 3.
    let mut r = reader_on("");
    r.free_space_for_tests = Some(1_000);
    let err = r.install_voice("parakeet_tdt_v3").unwrap_err().to_string();
    assert!(err.contains("Il manque"), "{err}");
}
```

(adapter `reader_on`/le champ de test aux noms réels du fichier ; si `Reader` a déjà une fonction injectée pour démarrer un téléchargement — `start_download` —, ajouter à côté une fonction injectée `free_space: Box<dyn Fn() -> Option<u64> + Send>` plutôt qu'un champ `_for_tests`.)

- [ ] **Step 2 : RUST** — attendu : échecs (compilation).

- [ ] **Step 3 : implémenter.**

`Runtime` : ajouter les quatre variantes (doc : « Dictée, dans l'app seulement : calcul MLX côté Swift »). `available_with` : ces runtimes ⇒ `mlx`.

Paquets (tailles et révisions : relevé de la recherche HF du 16 oct. 2026 ; licences : `licenses`) — `languages: &["fr", "en"]` pour tous (les modèles en savent plus ; la dictée de Sillon propose fr/en), `languages_label` : « 25 langues européennes » (Parakeet, Canary), « 99 langues » (Whisper), « 13 langues » (Voxtral) ; `voices_label: ""` ; `recommended: false` sauf Parakeet ; `pace` : « Relecture toutes les 1,5 s » (Parakeet, Whisper, Canary), « Au fil de la parole » (Voxtral) :

| id | repo / revision | `file` (bytes) | companions (`dest` = chemin dans le dépôt, bytes) | licence |
|---|---|---|---|---|
| `parakeet_tdt_v3` | `mlx-community/parakeet-tdt-0.6b-v3` / `ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15` | `model.safetensors` (2 508 288 736) | `config.json` (244 093) | CC-BY-4.0 |
| `whisper_tiny_4bit` | `mlx-community/whisper-tiny-4bit` / `064afea920b0373f8e19d51dc698bfbb91f501ff` | `model.safetensors` (22 029 431) | `config.json` (335), `multilingual.tiktoken` (816 730) | Apache-2.0 |
| `whisper_small_4bit` | `mlx-community/whisper-small-4bit` / `1ac90abbb1b2f53863bfd2a483a3c31d792c3e5b` | `model.safetensors` (139 111 057) | `config.json` (339), `multilingual.tiktoken` (816 730) | Apache-2.0 |
| `whisper_large_v3_turbo_4bit` | `mlx-community/whisper-large-v3-turbo-4bit` / `0f058d38170d183f9fdee07908f5b515d91793a8` | `model.safetensors` (463 462 982) | `config.json` (341), `multilingual.tiktoken` (816 730) | Apache-2.0 |
| `canary_1b_v2_q8` | `Mediform/canary-1b-v2-mlx-q8` / `0b6b32ee10f30c89e3ead7249bb636445e3019ee` | `model.safetensors` (1 136 436 574) | `config.json` (673 039 ; le vocabulaire y est, clé `tokenizer.model_base64`) | CC-BY-4.0 |
| `voxtral_realtime_4bit` | `mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit` / `fdebf7b2af834a1db4b8a3c99ab7480b333adf9e` | `model.safetensors` (3 133 798 126) | `config.json` (1 513), `tekken.json` (14 910 348) | Apache-2.0 |

Les `Companion` de ces paquets ont `repo`/`revision` égaux à ceux du paquet. `display_name` : « Parakeet v3 », « Whisper tiny », « Whisper small », « Whisper large-v3-turbo », « Canary 1b-v2 », « Voxtral Realtime » ; `precision` : « bf16 », « 4 bits », « q8 », « 4 bits » ; `description` courte en français.

`first_installed_with` : filtrer `p.kind == Kind::Speech`. Mettre à jour le test `kokoro_is_the_only_catalogued_model` en « Kokoro est le seul modèle de lecture » (filtre `Kind::Speech`).

`use_voice(id)` : après `self.package(id)?`, `if package.kind != Kind::Speech { bail!("{} sert à la dictée, pas à la lecture", package.display_name) }`.

`poll_download` : le passage « aucune voix active → ce téléchargement devient la voix » ne vaut que si `package.kind == Kind::Speech`.

`install_voice` : avant `start_download`, si `free_space()` est connu et `< package.total_bytes() - models::installed_bytes(package)`, refuser : `bail!("Il manque {} sur le disque pour {}", models::human_size(manque), package.display_name)`.

FFI :

```rust
/// Le dossier d'un paquet installé (dictée : l'app y charge le modèle).
pub fn model_directory(&self, id: String) -> Option<String> {
    let package = sillon::models::available_with(true).into_iter().find(|p| p.id == id)?;
    sillon::models::is_installed(package)
        .then(|| sillon::models::install_path(package))
        .flatten()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
}
```

- [ ] **Step 4 : RUST** puis **FFI** — attendu : vert.
- [ ] **Step 5 : commit** `feat(moteur): six modèles de dictée au catalogue (Parakeet v3, Whisper tiny/small/large-v3-turbo, Canary 1b-v2, Voxtral Realtime), un dossier chacun, jamais pris pour une voix ; place libre vérifiée avant un téléchargement`

---

### Task 2 : Copie du socle et de Parakeet dans `KokoroMLX`

**Files:**
- Create: `macos/Vendor/KokoroMLX/STT/Generation.swift`, `STT/STTOutput.swift`, `STT/DSP.swift`, `STT/BaseConfiguration.swift`
- Create: `macos/Vendor/KokoroMLX/STT/Nemo/*.swift` (5 fichiers), `STT/Parakeet/*.swift` (7 fichiers)
- Modify: `macos/Vendor/KokoroMLX/ORIGINE.md`
- Test: `macos/Tests/ParakeetSmokeTests.swift`

**Interfaces:**
- Produces (publics dans le module `KokoroMLX`) : `STTGenerateParameters`, `STTGenerationModel`, `STTGeneration`, `STTOutput`, `ParakeetModel.fromDirectory(_:computeDType:) throws -> ParakeetModel`, `generate(audio:generationParameters:) -> STTOutput` (relevé §2).

- [ ] **Step 1 :** **CLONE**. Copier les fichiers listés au relevé §1.a (socle sans `SentencePieceTokenizer.swift`, Nemo, Parakeet) sous `macos/Vendor/KokoroMLX/STT/…` (dossiers à plat par modèle).
- [ ] **Step 2 :** appliquer le relevé §1.c pour ces fichiers : retirer `import MLXLMCommon`, `import HuggingFace`, `import MLXAudioCore` ; créer `STT/BaseConfiguration.swift` avec la copie locale proposée au relevé (§1.c, ~70 lignes, seul symbole de MLXLMCommon utilisé) ; supprimer `fromPretrained` de Parakeet ; rendre publics les types/méthodes nécessaires s'ils sont `internal` dans des extensions (`public extension` comme au relevé §2).
- [ ] **Step 3 :** test de fumée qui s'ignore sans modèle :

```swift
import XCTest
import KokoroMLX

final class ParakeetSmokeTests: XCTestCase {
    /// `SILLON_STT_MODELS` : dossier contenant `parakeet_tdt_v3/` (copie de
    /// ~/Library/Application Support/sillon/models). Sans lui : ignoré.
    func testTranscribesOneSecondOfSilenceWithoutCrashing() throws {
        guard let root = ProcessInfo.processInfo.environment["SILLON_STT_MODELS"] else {
            throw XCTSkip("SILLON_STT_MODELS absent")
        }
        let model = try ParakeetModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent("parakeet_tdt_v3"))
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: model.defaultGenerationParameters)
        XCTAssertNotNil(output.text)
    }
}
```

(la cible de tests importe `MLX` via le framework : reprendre ce que font `KokoroParityTests.swift`.)

- [ ] **Step 4 :** **UIBUILD** — attendu : compile. Mettre à jour `ORIGINE.md` (fichiers ajoutés, changements, commit source).
- [ ] **Step 5 : commit** `feat(macos): Parakeet v3 copié de mlx-audio-swift dans le framework MLX (socle STT, Nemo, Parakeet), sans HuggingFace ni MLXLMCommon`

---

### Task 3 : Copie de Canary

**Files:**
- Create: `macos/Vendor/KokoroMLX/STT/SentencePieceTokenizer.swift`, `STT/Canary/CanaryConfig.swift`, `STT/Canary/CanaryModel.swift`
- Modify: `ORIGINE.md`
- Test: `macos/Tests/CanarySmokeTests.swift`

**Interfaces:**
- Produces: `CanaryModel.fromModelDirectory(_:) throws -> CanaryModel` (rendu **synchrone** : l'`async` d'origine n'attend rien, relevé §2) ; `generate(audio:generationParameters:)`.

- [ ] **Step 1 :** copier (relevé §1.a), retirer les imports et `fromPretrained`, `BaseConfiguration` local (Tâche 2).
- [ ] **Step 2 :** relevé §3 : Mediform se charge tel quel (vocabulaire dans `config.json`). Noter dans `ORIGINE.md` la limite de 200 jetons par défaut : les appels de Sillon passeront `maxTokens: 1024` (Tâche 7).
- [ ] **Step 3 :** test de fumée sur le modèle de Tâche 2 (`canary_1b_v2_q8/`, `XCTSkip` sans `SILLON_STT_MODELS`, `language: "fr"`).
- [ ] **Step 4 :** **UIBUILD**.
- [ ] **Step 5 : commit** `feat(macos): Canary 1b-v2 copié de mlx-audio-swift (vocabulaire lu dans config.json)`

---

### Task 4 : Copie de Whisper, avec un décodeur tiktoken à nous

**Files:**
- Create: `macos/Vendor/KokoroMLX/STT/Whisper/{WhisperAudio,WhisperConfig,WhisperLayers,WhisperModel}.swift`
- Create: `macos/Vendor/KokoroMLX/STT/Whisper/WhisperTokenizer.swift` (réécrit), `STT/Whisper/TiktokenDecoder.swift`
- Modify: `ORIGINE.md`
- Test: `macos/Tests/TiktokenDecoderTests.swift`, `macos/Tests/Fixtures/mini.tiktoken`

**Interfaces:**
- Produces: `WhisperModel.fromDirectory(_:) throws -> WhisperModel` (synchrone, exige `multilingual.tiktoken` et `config.json` dans le dossier, **aucun téléchargement**) ; `TiktokenDecoder(contentsOf: URL) throws`, `decode(_ ids: [Int], skipSpecial: Bool) -> String` ; `WhisperSpecialTokens(vocabularySize: Int)`.

- [ ] **Step 1 : test qui échoue** — fixture `macos/Tests/Fixtures/mini.tiktoken` (format tiktoken « base64 rang ») :

```
SGVsbG8= 0
IHdvcmxk 1
IMOp 2
```

```swift
import XCTest
@testable import KokoroMLX

final class TiktokenDecoderTests: XCTestCase {
    func fixture() -> URL {
        Bundle(for: Self.self).url(forResource: "mini", withExtension: "tiktoken")!
    }

    func testDecodesBytesInOrder() throws {
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([0, 1], skipSpecial: true), "Hello world")
    }

    func testJoinsUTF8SplitAcrossTokens() throws {
        // « é » = C3 A9 : un jeton peut n'en porter qu'une partie.
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([0, 2], skipSpecial: true), "Hello é")
    }

    func testSpecialIdsAreSkipped() throws {
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([50258, 0, 50257], skipSpecial: true), "Hello")
    }

    func testTurboShiftsTheTaskTokens() {
        let base = WhisperSpecialTokens(vocabularySize: 51865)
        let turbo = WhisperSpecialTokens(vocabularySize: 51866)
        XCTAssertEqual(base.transcribe, 50359)
        XCTAssertEqual(turbo.transcribe, 50360)
        XCTAssertEqual(turbo.noTimestamps, 50364)
        XCTAssertEqual(base.french, 50265)
        XCTAssertEqual(turbo.french, 50265)
    }
}
```

(ajouter `Tests/Fixtures/mini.tiktoken` aux ressources de la cible de tests dans `project.yml` si les fixtures n'y sont pas déjà.)

- [ ] **Step 2 :** **UIBUILD** — attendu : échec (types inconnus).
- [ ] **Step 3 :** copier Whisper (relevé §1.a), retirer HF/Transformers ; écrire `TiktokenDecoder.swift` et `WhisperTokenizer.swift` d'après le **code complet du relevé §4** (dont le cas de la ligne `= 50256`, les ids §4 « Ids spéciaux » et `suppress_tokens`) ; `fromDirectory` synchrone, `fatalError` du tokenizer absent remplacé par une erreur `STTError` ; à `WhisperAudio.swift:23`, `MLXAudioCore.melFilters(` → `KokoroMLX.melFilters(` (relevé §1.c, piège de masquage).
- [ ] **Step 4 :** **UIBUILD** — attendu : compile. Test de fumée Whisper tiny comme Tâche 2 (`whisper_tiny_4bit/`, `language: "fr"`).
- [ ] **Step 5 : commit** `feat(macos): Whisper copié de mlx-audio-swift, vocabulaire lu dans multilingual.tiktoken par un décodeur à nous (sans swift-transformers) ; ids spéciaux de turbo décalés`

---

### Task 5 : Copie de Voxtral Realtime, sans le VAD

**Files:**
- Create: `macos/Vendor/KokoroMLX/STT/VoxtralRealtime/*.swift` (8 fichiers)
- Modify: `ORIGINE.md`
- Test: `macos/Tests/VoxtralSmokeTests.swift`

**Interfaces:**
- Produces: `VoxtralRealtimeModel.fromDirectory(_:) throws`, `makeStreamSession(temperature:maxTokens:transcriptionDelayMs:) -> VoxtralRealtimeStreamSession`, `step(_ samples: [Float]) -> Delta`, `finish() -> Delta`, `Delta.text` (relevé §2).

- [ ] **Step 1 :** copier (relevé §1.a), retirer l'import et la surcharge VAD (relevé §5 : `VoxtralRealtime.swift:5` et `:126-201`), `fromPretrained`, `BaseConfiguration` local.
- [ ] **Step 2 :** test de fumée (`voxtral_realtime_4bit/`, `XCTSkip` sans modèle) : une session, `step` de 0,5 s de silence par blocs de 80 ms, `finish()`, aucun plantage.
- [ ] **Step 3 :** **UIBUILD**.
- [ ] **Step 4 : commit** `feat(macos): Voxtral Realtime copié de mlx-audio-swift, sans détection de voix`

---

### Task 6 : Verrou MLX partagé, micro en 16 kHz, et coureur des modèles de dictée

**Files:**
- Create: `macos/Sources/Voice/MLXGate.swift`
- Modify: `macos/Sources/Voice/KokoroMLX.swift` (le verrou `mlx` devient `MLXGate.shared` par défaut)
- Create: `macos/Sources/Dictation/MicrophoneCapture.swift`
- Create: `macos/Sources/Dictation/SpeechModelRunner.swift`
- Test: `macos/Tests/MLXGateTests.swift`, `macos/Tests/SpeechModelRunnerTests.swift`

**Interfaces:**
- Produces:
  - `final class MLXGate: @unchecked Sendable { static let shared; func run<T>(_ body: () throws -> T) rethrows -> T }` (un `OSAllocatedUnfairLock`).
  - `final class MicrophoneCapture` : `init(spectrum: @escaping @Sendable ([[Float]]) -> Void)`, `func start() throws`, `func stop()`, `func samples() -> [Float]` (tout l'audio depuis `start`, 16 kHz mono Float32, copie), `func drain() -> [Float]` (ce qui est arrivé depuis le dernier `drain`).
  - `protocol LoadedSpeechModel: AnyObject { func transcribe(_ samples: [Float], language: String) throws -> String }` et `final class SpeechModelRunner: @unchecked Sendable` : `init(load: @escaping (URL) throws -> LoadedSpeechModel, gate: MLXGate = .shared)`, `func prepare(directory: URL) async throws` (charge hors du fil principal, garde le modèle), `func transcribe(_ samples: [Float], language: String) async throws -> String` (sous `gate`, hors du fil principal), `func release()`, et `releaseAfter: Duration` (libère après inactivité ; défaut 10 min, comme `keep_voice_minutes`).

- [ ] **Step 1 : tests qui échouent.**

```swift
final class MLXGateTests: XCTestCase {
    func testOneBodyAtATime() async {
        let gate = MLXGate()
        let inside = OSAllocatedUnfairLock(initialState: 0)
        let maxInside = OSAllocatedUnfairLock(initialState: 0)
        await withTaskGroup(of: Void.self) { group in
            for _ in 0..<8 {
                group.addTask {
                    gate.run {
                        let now = inside.withLock { $0 += 1; return $0 }
                        maxInside.withLock { $0 = max($0, now) }
                        usleep(2_000)
                        inside.withLock { $0 -= 1 }
                    }
                }
            }
        }
        XCTAssertEqual(maxInside.withLock { $0 }, 1)
    }
}

final class SpeechModelRunnerTests: XCTestCase {
    final class Echo: LoadedSpeechModel {
        func transcribe(_ samples: [Float], language: String) throws -> String { "\(samples.count) \(language)" }
    }

    func testLoadsOnceThenTranscribes() async throws {
        var loads = 0
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() })
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        let text = try await runner.transcribe([0, 0, 0], language: "fr")
        XCTAssertEqual(text, "3 fr")
        XCTAssertEqual(loads, 1)
    }

    func testAnotherDirectoryReloads() async throws {
        var loads = 0
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() })
        try await runner.prepare(directory: URL(fileURLWithPath: "/a"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/b"))
        XCTAssertEqual(loads, 2)
    }

    func testTranscribingBeforePreparingFails() async {
        let runner = SpeechModelRunner(load: { _ in Echo() })
        do { _ = try await runner.transcribe([], language: "fr"); XCTFail() } catch {}
    }
}
```

- [ ] **Step 2 :** **UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter.**
  - `MLXGate` : `OSAllocatedUnfairLock`, `run` = `withLockUnchecked` ; `KokoroMLXCompute` reçoit `gate: MLXGate = .shared` et remplace `mlx.withLockUnchecked` par `gate.run` (aucun autre changement ; `KokoroMLXComputeTests` doivent compiler tels quels — s'ils créent le calcul sans argument, c'est le défaut).
  - `MicrophoneCapture` : `AVAudioEngine` + `AVAudioSinkNode` comme `SystemDictationEngine` (relire son `AudioTap` : même conversion `AVAudioConverter` vers `AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)`, même `SpectrumAnalyzer` pour le spectre) ; les échantillons s'accumulent dans un tampon sous `OSAllocatedUnfairLock`. Erreurs : `DictationError.noMicrophone`, `.failed`. Si `AudioTap` peut être généralisé sans toucher au comportement de `SystemDictationEngine`, le partager ; sinon dupliquer la petite partie conversion et le noter.
  - `SpeechModelRunner` : état sous verrou (répertoire chargé, modèle) ; `prepare` et `transcribe` sur `Task.detached(priority: .userInitiated)` ; `transcribe` passe par `gate.run` ; libération après `releaseAfter` sans appel (une `Task` annulée/relancée à chaque usage) ; `release()` vide le modèle et appelle `KokoroRunner.clearCache()` (ou `MLX.GPU.clearCache()`) sous le verrou.
- [ ] **Step 4 :** **UIBUILD**.
- [ ] **Step 5 : commit** `feat(macos): un verrou MLX pour Kokoro et la dictée, le micro en 16 kHz mono, et un coureur qui charge, garde et libère un modèle de dictée`

---

### Task 7 : Moteurs de dictée MLX, relecture glissante et aiguilleur

**Files:**
- Create: `macos/Sources/Dictation/RollingTranscription.swift` (logique pure)
- Create: `macos/Sources/Dictation/MLXDictationEngine.swift` (Parakeet, Whisper, Canary)
- Create: `macos/Sources/Dictation/VoxtralDictationEngine.swift`
- Create: `macos/Sources/Dictation/RoutingDictationEngine.swift`
- Modify: `macos/Sources/Dictation/DictationEngine.swift` (`Dictation.makeEngine()` rend l'aiguilleur ; `DictationModel` : clé `dictationModel`)
- Test: `macos/Tests/RollingTranscriptionTests.swift`, `macos/Tests/RoutingDictationEngineTests.swift`

**Interfaces:**
- Consumes: `SpeechModelRunner`, `MicrophoneCapture`, `MLXGate` (Tâche 6) ; modèles copiés (Tâches 2-5) ; FFI `modelDirectory(id:)` (Tâche 1).
- Produces:
  - `enum DictationModel { static let defaultsKey = "dictationModel"; static func stored(_ defaults: UserDefaults = .standard) -> String }` (défaut `"system"`).
  - `struct RollingTranscription { let interval: Duration; mutating func shouldRead(now: ContinuousClock.Instant, busy: Bool) -> Bool }` : vrai si pas de lecture en cours et `interval` écoulé depuis le début de la précédente.
  - `final class RoutingDictationEngine: DictationEngine` : `init(system: DictationEngine?, chosen: @escaping () -> String, directory: @escaping (String) -> URL?, make: @escaping (String, URL) -> DictationEngine?)`, avec `fallbackNotice: String?` (« Parakeet v3 indisponible : dictée de macOS ») lu par la section Dictée.

- [ ] **Step 1 : tests qui échouent.**

```swift
final class RollingTranscriptionTests: XCTestCase {
    func testWaitsForTheIntervalAndNeverOverlaps() {
        let clock = ContinuousClock()
        let t0 = clock.now
        var r = RollingTranscription(interval: .milliseconds(1500))
        XCTAssertTrue(r.shouldRead(now: t0, busy: false), "première lecture tout de suite")
        XCTAssertFalse(r.shouldRead(now: t0.advanced(by: .milliseconds(500)), busy: false))
        XCTAssertFalse(r.shouldRead(now: t0.advanced(by: .milliseconds(2000)), busy: true), "jamais deux à la fois")
        XCTAssertTrue(r.shouldRead(now: t0.advanced(by: .milliseconds(2000)), busy: false))
    }
}

@MainActor
final class RoutingDictationEngineTests: XCTestCase {
    final class Fake: DictationEngine {
        var started = 0
        func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws { started += 1 }
        func finish() async {}
        func cancel() async {}
    }

    func testSystemByDefault() async throws {
        let system = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "system" }, directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(system.started, 1)
        XCTAssertNil(router.fallbackNotice)
    }

    func testAMissingModelFallsBackToTheSystemAndSaysSo() async throws {
        let system = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" }, directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(system.started, 1)
        XCTAssertNotNil(router.fallbackNotice)
    }

    func testTheChosenModelIsUsed() async throws {
        let system = Fake(), parakeet = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                            directory: { _ in URL(fileURLWithPath: "/m") }, make: { _, _ in parakeet })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(parakeet.started, 1)
        XCTAssertEqual(system.started, 0)
    }
}
```

- [ ] **Step 2 :** **UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter.**
  - `MLXDictationEngine` (`@MainActor`, `DictationEngine`) : `init(runner: SpeechModelRunner, directory: URL)`. `start` : `MicrophoneCapture.start()`, `events(.listening)` tout de suite ; puis en tâche : `runner.prepare(directory:)` (le modèle se charge pendant qu'on parle) ; une boucle toutes les 100 ms consulte `RollingTranscription.shouldRead` et lance `runner.transcribe(capture.samples(), language: locale.language.languageCode?.identifier ?? "fr")` → `events(.text(texte, isFinal: false))`. `finish` : `capture.stop()`, attend la relecture en cours, une dernière lecture → `events(.text(texte, isFinal: true))`. `cancel` : arrête tout, n'émet rien. Erreur de chargement : `throw DictationError.failed("le modèle ne se charge pas : …")`.
  - Les `LoadedSpeechModel` : `ParakeetModel` (`generate` avec `defaultGenerationParameters`), `WhisperModel` (`language` passé), `CanaryModel` (`language` obligatoire, `maxTokens: 1024`) — un petit adaptateur chacun dans `MLXDictationEngine.swift`, construisant `STTGenerateParameters` (relevé §2) et `MLXArray(samples)`.
  - `VoxtralDictationEngine` : même micro ; la session Voxtral reçoit `capture.drain()` toutes les 80 ms via le coureur (sous `MLXGate`) ; chaque `Delta.text` s'ajoute au provisoire (`events(.text(cumul, isFinal: false))`) ; `finish` → `session.finish()` → texte définitif. Délai 480 ms.
  - `RoutingDictationEngine` : à `start`, `chosen()` ; `"system"` → `system` ; sinon `directory(id)` puis `make(id, dir)` ; si l'un manque, `fallbackNotice` est posé et `system` démarre ; si le moteur choisi lève à `start` une erreur autre que micro/annulation, repli sur `system` avec `fallbackNotice`. `finish`/`cancel` vont au moteur qui a démarré.
  - `Dictation.makeEngine()` rend un `RoutingDictationEngine` dont `directory` appelle le pont (`ReaderModel` : `func modelDirectory(_ id: String) -> URL?` — l'ajouter, appel synchrone court au moteur Rust, hors fil principal si l'API existante l'impose) et `make` choisit le moteur selon l'id (`parakeet_tdt_v3`, `whisper_*`, `canary_1b_v2_q8` → `MLXDictationEngine` avec l'adaptateur ; `voxtral_realtime_4bit` → `VoxtralDictationEngine`). Un `SpeechModelRunner` unique, partagé, gardé par l'aiguilleur. Ajuster `AppDelegate.installDictation` si la signature change.
- [ ] **Step 4 :** **UIBUILD**.
- [ ] **Step 5 : commit** `feat(macos): dicter avec Parakeet, Whisper, Canary (relecture toutes les 1,5 s) ou Voxtral (au fil de la parole) ; un aiguilleur passe au modèle choisi, ou à la dictée de macOS s'il manque`

---

### Task 8 : La table Dictée, et les restes du lot 1

**Files:**
- Modify: `macos/Sources/Settings/SettingsSections.swift` (`DictationSection` : table des modèles de dictée + ligne « Dictée de macOS » ; message `fallbackNotice`)
- Modify: `macos/Sources/Settings/ModelsSection.swift` (`ModelsTable` : sélection et confirmation paramétrables pour la dictée ; annonce de la progression)
- Modify: `macos/Sources/Settings/ModelTable.swift` (si besoin : `progressAnnouncement`)
- Modify: `macos/Sources/Settings/ReglagesView.swift` (passer les lignes de dictée)
- Modify: `macos/Sources/Settings/SettingsSections.swift` (`VoiceSection.voiceOptions` : voix Premium et Améliorées de macOS en tête)
- Test: `macos/Tests/ModelTableTests.swift`, `macos/Tests/VoiceSectionTests.swift`, `macos/UITests/ModelsUITests.swift`

**Interfaces:**
- Consumes: `VoiceModel.kind == "dictation"` (Tâche 1), `DictationModel` (Tâche 7), `RoutingDictationEngine.fallbackNotice`.
- Produces: identifiants `modele-system-dictation`, `utiliser-<id>`, `supprimer-<id>` pour la dictée ; `ModelTable.progressAnnouncement(previous: Double?, current: Double) -> String?` (« Téléchargement de X : 25 % » aux paliers 25/50/75/100).

- [ ] **Step 1 : tests qui échouent.**

```swift
func testProgressIsAnnouncedEveryQuarter() {
    XCTAssertNil(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.10, current: 0.20))
    XCTAssertEqual(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.20, current: 0.26),
                   "Téléchargement de Parakeet v3 : 25 %")
    XCTAssertEqual(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.74, current: 1.0),
                   "Téléchargement de Parakeet v3 : 100 %")
}

func testDictationRowsAreUsedFromTheirOwnChoice() {
    // La ligne active de dictée vient de `dictationModel`, pas du moteur Rust.
    let rows = [VoiceModel.sample(id: "parakeet_tdt_v3", kind: "dictation", installed: true)]
    let shown = DictationSection.rows(rows, chosen: "parakeet_tdt_v3")
    XCTAssertEqual(shown.first?.id, "system-dictation")
    XCTAssertTrue(shown[1].active)
    XCTAssertFalse(shown[0].active)
}

func testPremiumMacVoicesComeFirst() {
    let ordered = VoiceSection.ordered(["Thomas", "Amélie (Premium)", "Audrey (Améliorée)"],
                                       quality: ["Amélie (Premium)": 3, "Audrey (Améliorée)": 2])
    XCTAssertEqual(ordered, ["Amélie (Premium)", "Audrey (Améliorée)", "Thomas"])
}
```

(`VoiceModel.sample(id:kind:installed:)` : étendre l'aide de `Tests/Samples.swift`.)

- [ ] **Step 2 :** **UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter.**
  - `ModelsTable` reçoit `isUsed: (VoiceModel) -> Bool` et `use: (VoiceModel) async throws -> Void` (lecture : `row.active` / `model.useModel` ; dictée : `id == DictationModel.stored()` / écrire `UserDefaults` puis recharger), et `fallback` (dictée : « La dictée passera à celle de macOS. »). Supprimer le modèle de dictée choisi : après la suppression, `dictationModel` repasse à `"system"`.
  - `DictationSection` : `static func rows(_ all: [VoiceModel], chosen: String) -> [VoiceModel]` (ligne système « Dictée de macOS » en tête, `id "system-dictation"`, puis les lignes `kind == "dictation"`, `active` recalculé d'après `chosen`) ; la table en tête du groupe ; sous elle, `fallbackNotice` s'il y en a (texte entier, `ErrorStyle`). Le sélecteur de langue reste (Parakeet et Voxtral reconnaissent seuls ; Whisper et Canary la reçoivent).
  - Annonce : dans `ModelsTable`, `onChange(of: model.snapshot.download)` → `ModelTable.progressAnnouncement` → `Announcement.post`.
  - Voix Premium : `VoiceSection.ordered(_:quality:)` pure ; la qualité vient de `AVSpeechSynthesisVoice.speechVoices()` (`.premium` 3, `.enhanced` 2, sinon 1), appariée par nom ; appliquée dans `voiceOptions` quand `family == "system"` (après « Voix par défaut de macOS »).
  - `ModelsUITests` : la ligne `modele-system-dictation` existe et n'a pas de « Supprimer ».
- [ ] **Step 4 :** **UIBUILD**.
- [ ] **Step 5 : commit** `feat(macos): la table Dictée — dictée de macOS, Parakeet, Whisper, Canary, Voxtral : télécharger, utiliser, supprimer ; progression annoncée tous les 25 % ; voix Premium de macOS en tête`

---

### Task 9 : Vérification

- [ ] **Step 1 :** **RUST**, **FFI**, **UIBUILD** — verts (hors `clipboard_input` si le presse-papiers est vide).
- [ ] **Step 2 :** construire l'app (`./scripts/build-macos.sh`) ; l'utilisateur, à la main : télécharger Whisper tiny (23 Mo), le choisir, dicter une phrase en français (texte provisoire qui se met à jour, puis saisi) ; Parakeet v3 (2,5 Go) idem ; dicter pendant que Kokoro vient de lire (pas de plantage) ; supprimer le modèle choisi → la dictée suivante passe à macOS avec le message ; Voxtral si la place le permet.
- [ ] **Step 3 :** relever pour le lot 4 le temps de chargement et d'une relecture de 10 s de parole par modèle (journal `os.Logger` catégorie `dictee`).
- [ ] **Step 4 :** pousser pour la CI seulement avec l'accord de l'utilisateur.

---

## Bilan d'exécution (17 oct. 2026)

Fait : tâches 1 à 8, relues une à une (corrections : copie du tampon sur le fil audio, tri des voix Premium), puis relecture de toute la branche et une vague de corrections (course de l'aiguilleur qui pouvait laisser le micro ouvert ; licence mlx-swift-lm livrée ; modèle de dictée libéré dès qu'on le supprime ou qu'on en change ; délai de libération = `keep_voice_minutes`, 5 s au minimum).

Vérifié : `cargo test` 817 passés, 0 échec ; clippy propre ; app et tests compilés. **Aucun modèle n'a encore tourné** : les tests de fumée s'ignorent sans `SILLON_STT_MODELS` ; l'essai à la main de l'utilisateur (Tâche 9, étape 2) reste à faire.

Décisions prises en cours de route :
- Les tests de fumée des modèles s'ignorent sans modèle ; la preuve réelle est l'essai de l'utilisateur.
- Canary : balises `<|…|>` retirées par l'adaptateur ; langue toujours en code ISO à 2 lettres (Whisper, Canary).
- Les plafonds mémoire MLX de Kokoro (1 Go / 64 Mo) sont remplacés par ceux de la dictée pendant qu'elle tourne, et remis à sa libération.
- `dropping_the_pool_never_hangs` (préexistant, hors lot 2) : mesuré seul 4,5–6,7 s, sous charge ×4 jusqu'à 18 s, 12/12 réussis : lenteur sous charge, pas un blocage ; non modifié.

À mesurer à l'essai : chargement de chaque modèle ; coût d'une relecture sur une longue dictée (Canary sans cache KV) ; durée de `finish` (au-delà de ~3 s, ajouter un délai maximal) ; noms des voix Premium sur un Mac qui en a.

Reporté : parole perdue lors d'un repli sur macOS (rien ne le dit dans la fenêtre) ; ligne choisie mais absente sans indice ; annonce 100 % parfois manquée ; message « CancellationError » qui peut clignoter si l'on termine avant l'ouverture du micro.
