# Modèles, lot 3 : moteurs de lecture Qwen3-TTS et MagpieTTS — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lire avec Qwen3-TTS 0.6B Base (une voix française fixe, timbre SIWIS), Qwen3-TTS 1.7B CustomVoice 8 bits (annoncé lourd) ou MagpieTTS 357M 8 bits (français d'abord, ses 5 voix anglaises), choisis, téléchargés et supprimés dans la table Lecture des réglages, à côté de Kokoro et de la voix de macOS.

**Architecture:** Le catalogue Rust (`src/models.rs`) gagne trois paquets de lecture (`Runtime::Qwen3TtsMlx`, `Runtime::MagpieMlx`), un dossier chacun. Le cœur gagne un moteur à une session, `TextComputeEngine` (`src/tts/text_compute.rs`), qui envoie le **texte** (forme parlée) à un calcul étranger `TextSpeechCompute` (FFI), coupe les silences de tête et de queue, et laisse `Synthesis.words = None` (suivi des mots estimé par lettres, déjà en place). Côté Swift, les fichiers utiles de Qwen3-TTS (mlx-audio-swift `244458d`, MIT) et de MagpieTTS (speech-swift `28aef99`, Apache-2.0) sont **copiés** dans le framework `KokoroMLX`, avec deux portes `Qwen3TTSRunner` et `MagpieRunner` ; l'app les sert par `TextSpeechMLXCompute`, sous `MLXGate`, avec un registre `MLXBudget` qui somme les besoins mémoire des modèles chargés.

**Tech Stack:** Rust (cœur, uniffi), Swift 6 / SwiftUI macOS 26 (framework en Swift 5), MLX Swift 0.32.3, xcodegen, Python (mlx-audio 0.5.8, transformers 5.19.0) pour deux références calculées hors ligne.

**Spec:** `docs/superpowers/specs/2026-10-16-modeles-stt-tts-design.md` (section « Lecture : moteurs nouveaux », révisée le 17 oct. 2026 pour le lot 3). **Relevé technique** (fichiers à copier, numéros de ligne, octets, défauts du port, risques) : `docs/superpowers/plans/2026-10-17-modeles-lot-3-releve.md` — cité « relevé §n ». Le plan du lot 2 (`2026-10-16-modeles-lot-2-dictee.md`) sert de modèle de méthode.

## Global Constraints

- App macOS seulement ; le terminal est inchangé : `available_with(false)` ne montre aucun des trois paquets.
- MLX : `mlx-swift` **0.32.3 exact**, lié **une seule fois** (framework `KokoroMLX`). **Aucune nouvelle dépendance SPM** (ni mlx-audio-swift, ni speech-swift, ni mlx-swift-lm, ni swift-transformers, ni HuggingFace), rien d'ajouté aux `dependencies` de `KokoroMLX` dans `project.yml`.
- Fichiers copiés : Qwen3-TTS depuis `https://github.com/Blaizzy/mlx-audio-swift` au commit `244458d7b61e54ff135073ddc9ed76782bb20758` (MIT, licence déjà jointe) ; MagpieTTS depuis `https://github.com/soniqo/speech-swift` au commit `28aef99510e438bcc5014df2582820d9fdba80e0` (Apache-2.0) ; `KVCache` depuis mlx-swift-lm `3.32.3` (MIT, licence déjà jointe). Chaque changement noté dans `macos/Vendor/KokoroMLX/ORIGINE.md`, plus un patch `diff -ruN` par modèle dans `patches/`.
- Licences livrées : chaque nouveau texte de licence est joint par `joindre` dans `macos/project.yml`, vérifié par la liste `licences=(…)` de `scripts/build-macos.sh`, et cité dans `NOTICE`.
- **Une seule inférence MLX à la fois dans l'app** : chargement, synthèse et libération passent par `MLXGate.shared`.
- Plafonds MLX : **la somme des besoins des modèles chargés** (`MLXBudget`) ; plus aucune remise « en dur » des plafonds de Kokoro.
- Qwen3 0.6B Base : **voix française fixe** = x-vector précalculé hors ligne sur l'extrait SIWIS `expr_chap_s01_0011` (CC-BY-4.0, Idiap), livré dans l'app ; pas de clonage par l'utilisateur. Libellé « Qwen3 0.6B · voix française ».
- Qwen3 1.7B CustomVoice 8 bits : `pace` qui commence par « Lourd : lecture hachée au-delà de … » ; le chiffre affiché vient d'une **mesure sur le port Swift** (Tâche 9), jamais du banc Python.
- Défauts du port Qwen corrigés : le dialecte d'Eric/Dylan ne remplace la langue que pour `chinese`/`auto` ; le chargeur n'écrit, ne modifie ni n'efface rien dans le dossier du modèle.
- Suivi des mots : estimation existante (`words = None`) + coupe du silence de tête et de queue (−50 dBFS, 30 ms gardés). Pas d'attention de Magpie dans ce lot.
- Tests macOS : **jamais `xcodebuild test` en local** — compiler seulement (`build-for-testing`) ; tests Swift sur la CI. Les tests qui demandent un vrai modèle s'ignorent (`XCTSkip`) sans la variable `SILLON_TTS_MODELS`.
- L'encodeur BPE de Qwen a un test d'identité de jetons contre des ids de référence produits hors ligne par `transformers` (fixture).
- Textes en français, typographie française ; commits en français, fin `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Commandes

- **RUST** : `cargo test --workspace --locked --no-fail-fast` puis `cargo clippy --workspace --all-targets --locked -- -D warnings`. (`tests/clipboard_input.rs` lit le vrai presse-papiers : il échoue si celui-ci ne contient pas de texte — sans rapport.)
- **FFI** : `./scripts/build-ffi.sh`
- **UIBUILD** : `(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build-for-testing)` → `** TEST BUILD SUCCEEDED **`
- **PY** : `~/Library/Caches/sillon-bench/eval/venv/bin/python -I` (mlx-audio 0.5.8, mlx 0.32.3, transformers 5.19.0 ; `HF_HOME=~/Library/Caches/sillon-bench/eval/hf`).
- **CLONE** (tâches de copie ; sources en lecture seule, jamais compilées ni exécutées) :
  - `git clone https://github.com/Blaizzy/mlx-audio-swift "$CLAUDE_JOB_DIR/tmp/mas" 2>/dev/null; git -C "$CLAUDE_JOB_DIR/tmp/mas" checkout -q 244458d7b61e54ff135073ddc9ed76782bb20758`
  - `git clone https://github.com/soniqo/speech-swift "$CLAUDE_JOB_DIR/tmp/ss" 2>/dev/null; git -C "$CLAUDE_JOB_DIR/tmp/ss" checkout -q 28aef99510e438bcc5014df2582820d9fdba80e0`
  - `git clone https://github.com/ml-explore/mlx-swift-lm "$CLAUDE_JOB_DIR/tmp/mlm" 2>/dev/null; git -C "$CLAUDE_JOB_DIR/tmp/mlm" checkout -q 3.32.3`
  - Abréviations du relevé : `Q = $CLAUDE_JOB_DIR/tmp/mas/Sources/MLXAudioTTS/Models/Qwen3TTS/`, `M = $CLAUDE_JOB_DIR/tmp/ss/Sources/MagpieTTS/`.

## Review Focus

1. **Une ancienne configuration `magpie_tts` (audio.cpp, `.gguf`) ou une famille inconnue** ne doit jamais être prise pour le nouveau Magpie : elle reste « non pris en charge », voix de macOS, avec la note. → test Tâche 2 (`an_old_magpie_tts_config_is_not_the_new_magpie`).
2. **Deux modèles à texte tenus en même temps** (la lecture sur Qwen 1.7B, l'aperçu de la bibliothèque sur Magpie) : chacun parle avec **ses** poids, et libérer l'un ne vide pas l'autre. → tests Tâche 2 (`each_engine_speaks_with_its_own_model`) et Tâche 8 (`testTwoDirectoriesKeepTheirOwnModel`).
3. **La voix d'avant ne va pas au nouveau modèle** (`p-fenrir60`, une voix de macOS) : la voix par défaut du modèle, sans erreur ; et Eric/Dylan lisant du français reçoivent l'id de langue `french`, pas celui d'un dialecte. → tests Tâche 1 (`a_voice_of_another_model_gives_the_default`), Tâche 2 (`the_voice_of_another_model_reaches_the_compute_as_the_default`), Tâche 5 (`testADialectSpeakerKeepsTheRequestedLanguage`).
4. **Phrase longue et saut du curseur** : une phrase de plus de 23 s n'est pas tronquée par Magpie (`maxSteps` proportionnel) ; un saut en pleine synthèse l'arrête en route et rend un abandon, jamais un échec qui compterait vers le repli. → tests Tâche 2 (`a_cancelled_synthesis_never_counts_as_a_failure`), Tâche 6 (`testMaxStepsGrowsWithTheText`), Tâche 8 (`testStoppedOnTheWayIsCancelled`).
5. **Une dictée qui se termine pendant que Qwen 1.7B est chargé** ne remet pas les plafonds MLX à 1 Go (ceux de Kokoro) : ils redeviennent la somme de ce qui reste chargé. → test Tâche 7 (`testReleasingDictationKeepsTheReadingModelsNeed`).

---

### Task 1 : Catalogue — Qwen3 0.6B (voix française), Qwen3 1.7B CustomVoice, MagpieTTS 357M, et leurs voix

**Files:**
- Modify: `src/models.rs` (`Runtime`, constante `CATALOG`, `available_with`, `voices_for_family`, `sessions_for_family`, `honoured_options`, `voices_follow_language`, tests)
- Modify: `src/reader/library.rs` (`model_label`, accent et genre des nouvelles voix, tests)
- Modify: `src/reader/migration.rs` (`plan` : une famille de lecture du catalogue n'est pas migrée vers Kokoro, test)

**Interfaces:**
- Produces:
  - `Runtime::Qwen3TtsMlx`, `Runtime::MagpieMlx` (app seulement).
  - ids = familles : `qwen3_tts_06b_base_8bit`, `qwen3_tts_17b_customvoice_8bit`, `magpie_tts_357m_8bit`.
  - Voix : Qwen 0.6B `["siwis"]` ; Qwen 1.7B `["vivian", "aiden", "serena", "uncle_fu", "dylan", "eric", "ryan", "ono_anna", "sohee"]` ; Magpie `["sofia", "aria", "jason", "leo", "john_van_stan"]`.
  - `pub fn voice_traits(family: &str, id: &str) -> (&'static str, &'static str)` dans `library.rs` (accent, genre).
  - Le fichier principal de Magpie est `config.json` (32 octets, à la racine du dépôt) : `install_path` pointe sur `models/magpie_tts_357m_8bit/config.json`, et le dossier du modèle est son parent (les poids sont dans des sous-dossiers, rangés par les `companions`).

- [ ] **Step 1 : tests qui échouent** — dans `src/models.rs`, module `tests`. Remplacer `kokoro_is_the_only_speech_model` par :

```rust
    /// Kokoro (MLX ou int8), puis les trois moteurs à texte du lot 3. Le
    /// terminal ne voit toujours que Kokoro.
    #[test]
    fn the_speech_models_are_kokoro_qwen3_and_magpie() {
        let ids: Vec<&str> = CATALOG.iter().filter(|p| p.kind == Kind::Speech).map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "kokoro_82m_mlx",
                "kokoro_82m_int8",
                "qwen3_tts_06b_base_8bit",
                "qwen3_tts_17b_customvoice_8bit",
                "magpie_tts_357m_8bit"
            ]
        );
        assert!(available().iter().filter(|p| p.kind == Kind::Speech).all(|p| p.family == "kokoro"));
        assert_eq!(
            available_with(true).iter().filter(|p| matches!(p.runtime, Runtime::Qwen3TtsMlx | Runtime::MagpieMlx)).count(),
            3
        );
    }

    const TEXT_MODELS: [&str; 3] = ["qwen3_tts_06b_base_8bit", "qwen3_tts_17b_customvoice_8bit", "magpie_tts_357m_8bit"];

    /// Tailles relevées sur les dépôts aux révisions figées (relevé §2.a,
    /// §2.b, §3.d) : seuls les fichiers que les chargeurs lisent.
    #[test]
    fn text_models_have_their_own_folder_and_only_the_files_they_read() {
        for (id, total) in [
            ("qwen3_tts_06b_base_8bit", 1_991_218_180),
            ("qwen3_tts_17b_customvoice_8bit", 3_080_066_433),
            ("magpie_tts_357m_8bit", 301_589_611 + 32),
        ] {
            let p = package(id);
            assert_eq!(p.family, p.id, "{id} : un dossier par paquet");
            assert_eq!(p.total_bytes(), total, "{id}");
            assert!(p.companions.iter().all(|c| c.repo == p.repo && c.revision == p.revision), "{id}");
            assert!(p.companions.iter().all(|c| c.dest == c.file), "{id} : même arborescence que le dépôt");
            assert!(p.description.contains("Suivi des mots approché"), "{id}");
            assert!(!p.recommended, "{id}");
        }
        let magpie = package("magpie_tts_357m_8bit");
        assert!(!magpie.companions.iter().any(|c| c.file.starts_with("decoder_step/")), "jamais lu par MLX");
        let qwen = package("qwen3_tts_17b_customvoice_8bit");
        assert!(qwen.companions.iter().any(|c| c.dest == "speech_tokenizer/model.safetensors" && c.bytes == 682_293_092));
        assert!(!qwen.companions.iter().any(|c| c.file.ends_with("index.json")), "jamais lu");
    }

    #[test]
    fn text_models_read_one_sentence_at_a_time_with_their_voice_and_language() {
        for id in TEXT_MODELS {
            assert_eq!(sessions_for_family(id), 1, "{id}");
            assert_eq!(honoured_options(id), &["voice", "language"], "{id}");
            assert!(voices_follow_language(id), "{id}");
        }
    }

    #[test]
    fn qwen3_and_magpie_name_their_voices() {
        let ids = |family: &str| voices_for_family(family).iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(ids("qwen3_tts_06b_base_8bit"), ["siwis"]);
        assert_eq!(
            ids("qwen3_tts_17b_customvoice_8bit"),
            ["vivian", "aiden", "serena", "uncle_fu", "dylan", "eric", "ryan", "ono_anna", "sohee"]
        );
        assert_eq!(ids("magpie_tts_357m_8bit"), ["sofia", "aria", "jason", "leo", "john_van_stan"]);
        assert_eq!(default_voice_for_language("qwen3_tts_17b_customvoice_8bit", "fr"), "vivian");
        assert_eq!(default_voice_for_language("qwen3_tts_17b_customvoice_8bit", "en"), "aiden");
        assert_eq!(default_voice_for_language("magpie_tts_357m_8bit", "fr"), "sofia");
        assert_eq!(default_voice_for_language("qwen3_tts_06b_base_8bit", "en"), "siwis");
    }

    /// Spec : voix choisie absente du nouveau modèle → sa voix par défaut,
    /// sans message (et sans réécrire la configuration).
    #[test]
    fn a_voice_of_another_model_gives_the_default() {
        assert_eq!(voice_for_language("qwen3_tts_17b_customvoice_8bit", "fr", Some("p-fenrir60")), "vivian");
        assert_eq!(voice_for_language("magpie_tts_357m_8bit", "fr", Some("Thomas")), "sofia");
        assert_eq!(voice_for_language("magpie_tts_357m_8bit", "en", Some("leo")), "leo");
    }

    #[test]
    fn the_heavy_model_says_so_in_its_pace() {
        assert!(package("qwen3_tts_17b_customvoice_8bit").pace.starts_with("Lourd : lecture hachée au-delà de"));
    }

    #[test]
    fn magpie_is_installed_with_its_root_config() {
        let Some(path) = install_path(package("magpie_tts_357m_8bit")) else { return };
        assert!(path.ends_with("magpie_tts_357m_8bit/config.json"), "{path:?}");
    }
```

Dans `src/reader/library.rs`, module `tests` :

```rust
    #[test]
    fn the_new_voices_say_their_accent_and_gender() {
        assert_eq!(voice_traits("qwen3_tts_06b_base_8bit", "siwis"), ("française", "femme"));
        assert_eq!(voice_traits("qwen3_tts_17b_customvoice_8bit", "vivian"), ("chinoise", "femme"));
        assert_eq!(voice_traits("qwen3_tts_17b_customvoice_8bit", "aiden"), ("américaine", "homme"));
        assert_eq!(voice_traits("qwen3_tts_17b_customvoice_8bit", "eric"), ("sichuanaise", "homme"));
        assert_eq!(voice_traits("magpie_tts_357m_8bit", "sofia"), ("américaine", "femme"));
        assert_eq!(voice_traits("magpie_tts_357m_8bit", "john_van_stan"), ("américaine", "homme"));
        assert_eq!(voice_traits("kokoro", "bf_emma"), ("britannique", "femme"));
    }

    #[test]
    fn an_installed_text_model_lists_its_voices_under_its_short_name() {
        let c = Config::default();
        let installed = [models::CATALOG.iter().find(|p| p.id == "magpie_tts_357m_8bit").unwrap()];
        let entries = library_entries(&c, &installed, SYSTEM, &[], "fr_FR");
        let magpie: Vec<_> = entries.iter().filter(|v| v.model_id == "magpie_tts_357m_8bit").collect();
        assert_eq!(magpie.len(), 5);
        assert!(magpie.iter().all(|v| v.model_label == "Magpie"));
        assert!(magpie.iter().any(|v| v.voice_id == "sofia" && v.recommended && v.gender == "femme"));
    }
```

Dans `src/reader/migration.rs`, module `tests` (adapter le constructeur de configuration au nom réel du fichier, par ex. `Config::default()`) :

```rust
    /// Un lecteur déjà sur un moteur à texte du catalogue n'est pas basculé
    /// vers Kokoro par le passage unique : la réponse est seulement retenue.
    #[test]
    fn a_catalogued_text_model_is_not_migrated_to_kokoro() {
        let mut c = Config::default();
        c.engine.family = "magpie_tts_357m_8bit".into();
        c.engine.model_path = "/m/magpie_tts_357m_8bit/config.json".into();
        let available = models::available_with(true);
        assert_eq!(plan(&c, &available, |_| true), Step::Record);
    }
```

(si `Step` n'implémente pas `PartialEq`/`Debug`, tester avec `matches!(…, Step::Record)`.)

- [ ] **Step 2 : RUST** — attendu : échecs (compilation : variantes et ids inconnus).

- [ ] **Step 3 : implémenter `src/models.rs`.**

`Runtime` : ajouter

```rust
    /// Lecture à partir du texte, dans l'app seulement : Qwen3-TTS en MLX
    /// (calcul Swift, `TextSpeechCompute`).
    Qwen3TtsMlx,
    /// Lecture à partir du texte, dans l'app seulement : MagpieTTS en MLX.
    MagpieMlx,
```

`available_with` : ajouter `| Runtime::Qwen3TtsMlx | Runtime::MagpieMlx` au bras `=> mlx`.

Avant `CATALOG` :

```rust
/// Un compagnon rangé sous le même chemin que dans le dépôt.
const fn hf(repo: &'static str, revision: &'static str, file: &'static str, bytes: u64) -> Companion {
    Companion { repo, revision, file, dest: file, bytes }
}

const QWEN3_06B_REPO: &str = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit";
const QWEN3_06B_REVISION: &str = "50f45ef0047cde7e84c2ef04326acb8ada2436a7";
const QWEN3_17B_REPO: &str = "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit";
const QWEN3_17B_REVISION: &str = "41d3337e8b7f2843a75841595fc14e4b9a7a4b96";
const MAGPIE_REPO: &str = "aufklarer/Magpie-TTS-Multilingual-357M-MLX-8bit";
const MAGPIE_REVISION: &str = "260839c0a2c46993735d72082c3fba177710d477";

/// Ce que lit le chargeur de Qwen3 (relevé §1.e) à côté de
/// `model.safetensors` : configuration, vocabulaire BPE, codec. Ni
/// `model.safetensors.index.json`, ni `generation_config.json`, ni
/// `preprocessor_config.json`, que personne ne lit.
const QWEN3_06B_FILES: &[Companion] = &[
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "config.json", 5_522),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "vocab.json", 2_776_833),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "merges.txt", 1_671_839),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "tokenizer_config.json", 7_344),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "speech_tokenizer/config.json", 2_336),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "speech_tokenizer/model.safetensors", 682_293_092),
];

const QWEN3_17B_FILES: &[Companion] = &[
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "config.json", 6_058),
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "vocab.json", 2_776_833),
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "merges.txt", 1_671_839),
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "tokenizer_config.json", 7_344),
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "speech_tokenizer/config.json", 2_336),
    hf(QWEN3_17B_REPO, QWEN3_17B_REVISION, "speech_tokenizer/model.safetensors", 682_293_092),
];

/// Magpie (relevé §3.d) : encodeur de texte, décodeur (`decoder_prefill`
/// seulement : `decoder_step` porte les mêmes poids et n'est pas lu), codec,
/// et les vocabulaires français et anglais.
const MAGPIE_FILES: &[Companion] = &[
    hf(MAGPIE_REPO, MAGPIE_REVISION, "text_encoder/config.json", 2_461),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "text_encoder/model.safetensors", 108_912_472),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "decoder_prefill/config.json", 9_650),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "decoder_prefill/model.safetensors", 129_439_762),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "nanocodec_decoder/config.json", 466),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "nanocodec_decoder/model.safetensors", 63_165_546),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "tokenizer/fr.json", 29_555),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "tokenizer/en.json", 29_699),
];
```

Ajouter à la fin de `CATALOG` (après les paquets de dictée ; mettre à jour la doc de `CATALOG` : « Kokoro, et dans l'app Qwen3-TTS et MagpieTTS (lot 3) ») :

```rust
    ModelPackage {
        id: "qwen3_tts_06b_base_8bit",
        family: "qwen3_tts_06b_base_8bit",
        display_name: "Qwen3 0.6B · voix française",
        description: "Une voix française fixe, au timbre tiré de SIWIS, calculée par la puce graphique. Suivi des mots approché.",
        precision: "8 bits",
        languages: &["fr", "en"],
        repo: QWEN3_06B_REPO,
        revision: QWEN3_06B_REVISION,
        file: "model.safetensors",
        languages_label: "10 langues, dont français et anglais",
        bytes: 1_304_461_214,
        recommended: false,
        pace: "Cadence pas encore mesurée sur ce Mac.",
        companions: QWEN3_06B_FILES,
        runtime: Runtime::Qwen3TtsMlx,
        kind: Kind::Speech,
        voices_label: "1 voix : siwis (française, fixe)",
        licenses: "Poids Apache-2.0 · code MLX MIT (mlx-audio-swift) · timbre tiré de SIWIS (CC-BY-4.0, Idiap)",
    },
    ModelPackage {
        id: "qwen3_tts_17b_customvoice_8bit",
        family: "qwen3_tts_17b_customvoice_8bit",
        display_name: "Qwen3 1.7B CustomVoice",
        description: "Grand modèle expressif, neuf voix, aucune française. Suivi des mots approché.",
        precision: "8 bits",
        languages: &["fr", "en"],
        repo: QWEN3_17B_REPO,
        revision: QWEN3_17B_REVISION,
        file: "model.safetensors",
        languages_label: "10 langues, dont français et anglais",
        bytes: 2_393_308_931,
        recommended: false,
        pace: "Lourd : lecture hachée au-delà de 1×.",
        companions: QWEN3_17B_FILES,
        runtime: Runtime::Qwen3TtsMlx,
        kind: Kind::Speech,
        voices_label: "9 voix, aucune française : vivian (recommandée en français), aiden (recommandée en anglais)…",
        licenses: "Poids Apache-2.0 · code MLX MIT (mlx-audio-swift)",
    },
    ModelPackage {
        id: "magpie_tts_357m_8bit",
        family: "magpie_tts_357m_8bit",
        display_name: "MagpieTTS 357M",
        description: "Modèle multilingue de NVIDIA, cinq voix anglaises, le français lu tel quel. Suivi des mots approché.",
        precision: "8 bits",
        languages: &["fr", "en"],
        repo: MAGPIE_REPO,
        revision: MAGPIE_REVISION,
        file: "config.json",
        languages_label: "9 langues, dont français et anglais",
        bytes: 32,
        recommended: false,
        pace: "Cadence pas encore mesurée sur ce Mac.",
        companions: MAGPIE_FILES,
        runtime: Runtime::MagpieMlx,
        kind: Kind::Speech,
        voices_label: "5 voix anglaises : sofia (recommandée), aria, jason, leo, john_van_stan",
        licenses: "Poids NVIDIA Open Model License (téléchargés par vous) · code Apache-2.0 (speech-swift) · dictionnaire anglais CMUdict (BSD)",
    },
```

(`pace` des deux premiers : remplacé par le chiffre mesuré à la Tâche 9.)

Voix :

```rust
const QWEN3_BASE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "siwis", label: "siwis · française, femme, recommandée", language: "fr" },
];

/// Les neuf locuteurs de CustomVoice (`talker_config.spk_id`), langue
/// native d'après la carte du modèle. Aucun n'est français : vivian lit le
/// mieux le français mesuré (WER médian 3,3 %), aiden l'anglais.
const QWEN3_CUSTOM_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "vivian", label: "vivian · chinoise, femme, recommandée en français", language: "zh" },
    VoiceOption { id: "aiden", label: "aiden · américaine, homme, recommandée en anglais", language: "en" },
    VoiceOption { id: "serena", label: "serena · chinoise, femme", language: "zh" },
    VoiceOption { id: "uncle_fu", label: "uncle_fu · chinoise, homme", language: "zh" },
    VoiceOption { id: "dylan", label: "dylan · pékinoise, homme", language: "zh" },
    VoiceOption { id: "eric", label: "eric · sichuanaise, homme", language: "zh" },
    VoiceOption { id: "ryan", label: "ryan · anglaise, homme", language: "en" },
    VoiceOption { id: "ono_anna", label: "ono_anna · japonaise, femme", language: "ja" },
    VoiceOption { id: "sohee", label: "sohee · coréenne, femme", language: "ko" },
];

/// Les cinq voix du paquet MLX, dans l'ordre de `baked_speaker_names`
/// (relevé §3.d) ; `MagpieSpeaker(named:)` les reconnaît.
const MAGPIE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "sofia", label: "sofia · américaine, femme, recommandée", language: "en" },
    VoiceOption { id: "aria", label: "aria · américaine, femme", language: "en" },
    VoiceOption { id: "jason", label: "jason · américaine, homme", language: "en" },
    VoiceOption { id: "leo", label: "leo · américaine, homme", language: "en" },
    VoiceOption { id: "john_van_stan", label: "john_van_stan · américaine, homme (LibriVox)", language: "en" },
];
```

`voices_for_family` :

```rust
    match family {
        "kokoro" => KOKORO_VOICES,
        "qwen3_tts_06b_base_8bit" => QWEN3_BASE_VOICES,
        "qwen3_tts_17b_customvoice_8bit" => QWEN3_CUSTOM_VOICES,
        "magpie_tts_357m_8bit" => MAGPIE_VOICES,
        _ => &[],
    }
```

Une aide privée, et ses usages :

```rust
/// Les familles lues par un moteur à texte (lot 3) : une session, la voix et
/// la langue honorées.
fn reads_text(family: &str) -> bool {
    CATALOG
        .iter()
        .any(|p| p.family == family && matches!(p.runtime, Runtime::Qwen3TtsMlx | Runtime::MagpieMlx))
}
```

- `sessions_for_family` : `"kokoro" => 1,` devient `f if f == "kokoro" || reads_text(f) => 1,` (commentaire : « Kokoro, Qwen3 et Magpie calculent en série sous un seul verrou MLX : une session »).
- `honoured_options` : même garde, `&["voice", "language"]`.
- `voices_follow_language` : `!voices_for_family(family).is_empty()` (doc : « Vrai quand la famille nomme ses voix : la voix se range alors dans `voice.per_language`, une pour tout »). Kokoro reste vrai ; famille inconnue et `system` restent faux.

- [ ] **Step 4 : implémenter `src/reader/library.rs`.**

```rust
/// L'accent et le genre d'une voix nommée par le catalogue.
pub fn voice_traits(family: &str, id: &str) -> (&'static str, &'static str) {
    match family {
        "kokoro" => kokoro_traits(id),
        "qwen3_tts_06b_base_8bit" => ("française", "femme"),
        "qwen3_tts_17b_customvoice_8bit" => match id {
            "vivian" | "serena" => ("chinoise", "femme"),
            "uncle_fu" => ("chinoise", "homme"),
            "dylan" => ("pékinoise", "homme"),
            "eric" => ("sichuanaise", "homme"),
            "aiden" => ("américaine", "homme"),
            "ryan" => ("anglaise", "homme"),
            "ono_anna" => ("japonaise", "femme"),
            "sohee" => ("coréenne", "femme"),
            _ => ("", ""),
        },
        "magpie_tts_357m_8bit" => match id {
            "sofia" | "aria" => ("américaine", "femme"),
            "jason" | "leo" | "john_van_stan" => ("américaine", "homme"),
            _ => ("", ""),
        },
        _ => ("", ""),
    }
}
```

`model_label` : ajouter `"qwen3_tts_06b_base_8bit" => "Qwen3 0.6B"`, `"qwen3_tts_17b_customvoice_8bit" => "Qwen3 1.7B"`, `"magpie_tts_357m_8bit" => "Magpie"` (en `.to_string()`). Dans `library_entries`, `kokoro_traits(voice.id)` devient `voice_traits(package.family, voice.id)`.

- [ ] **Step 5 : implémenter `src/reader/migration.rs`.** Dans `plan`, remplacer `if config.engine.family == "kokoro" {` par :

```rust
    // Déjà sur un modèle de lecture du catalogue (Kokoro, Qwen3, Magpie) :
    // rien à basculer, la réponse est retenue.
    if models::CATALOG.iter().any(|p| p.kind == models::Kind::Speech && p.family == config.engine.family) {
```

(l'ancienne famille `magpie_tts` d'audio.cpp n'est pas au catalogue : elle reste migrée, comme avant.)

- [ ] **Step 6 : RUST** — attendu : vert. Si un test existant compte les familles nommant des voix ou les paquets de l'app (`every_catalogued_family_names_its_voices`, `kokoro_is_the_recommended_voice_wherever_it_runs`), il doit passer tel quel ; sinon lire le message et corriger la donnée, pas le test.
- [ ] **Step 7 : commit** `feat(moteur): Qwen3 0.6B (voix française), Qwen3 1.7B CustomVoice et MagpieTTS 357M au catalogue de l'app, un dossier chacun, avec leurs voix dans la bibliothèque`

---

### Task 2 : Le moteur à texte du cœur, et le pont `TextSpeechCompute`

**Files:**
- Create: `src/tts/text_compute.rs`
- Modify: `src/tts/mod.rs` (`pub mod text_compute;`, `AppCompute`, `build_engine_with`, `build_text_engine`, tests)
- Modify: `ffi/src/lib.rs` (`SpeechOutput`, `TextSpeechCompute`, `ForeignText`, `text_speech_bridge_check`, constructeur `with_mlx` à la place de `with_kokoro`, `app_reader`, tests)

**Interfaces:**
- Consumes: `Runtime::{Qwen3TtsMlx, MagpieMlx}`, `voice_for_language` (Tâche 1).
- Produces:
  - Rust : `pub trait TextCompute: Send + Sync { fn load(&self, model_dir: &Path, runtime: &str) -> Result<()>; fn warm_up(&self, model_dir: &Path) -> Result<()>; fn synthesize(&self, model_dir: &Path, text: &str, language: &str, voice: &str, cancel: &Cancel) -> Result<SpeechRaw>; fn release(&self, model_dir: &Path); }` ; `pub struct SpeechRaw { samples: Vec<f32>, sample_rate: u32 }` ; `pub fn runtime_name(Runtime) -> Option<&'static str>` (`"qwen3_tts_mlx"`, `"magpie_mlx"`) ; `pub fn trim_silence(&[f32], u32) -> &[f32]` ; `TextComputeEngine::load(compute, package, dir, fallback)`.
  - Rust : `#[derive(Clone, Default)] pub struct AppCompute { pub kokoro: Option<Arc<kokoro::SharedCompute>>, pub text: Option<Arc<dyn text_compute::TextCompute>> }` ; `build_engine_with(preference, settings, compute: &AppCompute)`.
  - FFI (vu de Swift) : `protocol TextSpeechCompute { func load(modelDir: String, runtime: String) throws; func warmUp(modelDir: String) throws; func synthesize(modelDir: String, text: String, language: String, voice: String, cancel: SynthesisCancel) throws -> SpeechOutput; func release(modelDir: String) }` ; `struct SpeechOutput { samples: [Float]; sampleRate: UInt32 }` ; `func textSpeechBridgeCheck(compute:modelDir:runtime:) throws -> UInt64` ; `SillonReader.withMlx(kokoro:text:)`.
  - `language` envoyé au calcul : toujours `"fr"` ou `"en"` ; `voice` : toujours une voix du catalogue de la famille (jamais vide en lecture ; vide seulement à la préparation).

**Écart avec la spec, voulu** : `model_dir` est passé à `warm_up`, `synthesize` et `release`. La bibliothèque des voix charge un second modèle pour l'aperçu pendant que la lecture tient le sien (`src/reader/library.rs`) : avec un seul emplacement, comme `SharedCompute`, l'aperçu remplacerait les poids sous le moteur de lecture. Pas de `words`/`word_starts` dans `SpeechOutput` : l'attention de Magpie n'est pas dans ce lot.

- [ ] **Step 1 : tests qui échouent** — créer `src/tts/text_compute.rs` avec seulement son module de tests (le code vient au Step 3) :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CATALOG;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::AtomicUsize;

    /// Un calcul en mémoire : rend `samples` à `rate`, retient chaque appel.
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<(PathBuf, String, String, String)>>,
        loads: AtomicUsize,
        releases: Mutex<Vec<PathBuf>>,
        samples: Vec<f32>,
        rate: u32,
        fail: bool,
        fail_warm_up: bool,
    }

    impl Fake {
        fn speaking(samples: Vec<f32>, rate: u32) -> Self {
            Self { samples, rate, ..Default::default() }
        }
    }

    impl TextCompute for Fake {
        fn load(&self, _: &Path, _: &str) -> Result<()> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn warm_up(&self, _: &Path) -> Result<()> {
            anyhow::ensure!(!self.fail_warm_up, "préparation ratée");
            Ok(())
        }
        fn synthesize(&self, dir: &Path, text: &str, language: &str, voice: &str, cancel: &Cancel) -> Result<SpeechRaw> {
            cancel.check()?;
            self.calls.locked().push((dir.to_path_buf(), text.into(), language.into(), voice.into()));
            anyhow::ensure!(!self.fail, "calcul en échec");
            Ok(SpeechRaw { samples: self.samples.clone(), sample_rate: self.rate })
        }
        fn release(&self, dir: &Path) {
            self.releases.locked().push(dir.to_path_buf());
        }
    }

    fn package(id: &str) -> &'static ModelPackage {
        CATALOG.iter().find(|p| p.id == id).unwrap()
    }

    fn engine(fake: &Arc<Fake>, id: &str, dir: &str) -> TextComputeEngine {
        let compute: Arc<dyn TextCompute> = Arc::clone(fake) as Arc<dyn TextCompute>;
        TextComputeEngine::load(compute, package(id), Path::new(dir), Arc::new(SilentEngine)).unwrap()
    }

    fn params(voice: &str, language: &str) -> TtsParams {
        TtsParams { voice: voice.into(), language: language.into(), ..TtsParams::default() }
    }

    /// 0,1 s de silence, 0,2 s de son, 0,1 s de silence, à 1 kHz.
    fn padded() -> Vec<f32> {
        let mut s = vec![0.0; 100];
        s.extend(std::iter::repeat(0.5).take(200));
        s.extend(vec![0.0; 100]);
        s
    }

    #[test]
    fn trims_leading_and_trailing_silence_keeping_30_ms() {
        let samples = padded();
        let kept = trim_silence(&samples, 1_000);
        assert_eq!(kept.len(), 30 + 200 + 30);
        assert_eq!(kept[30], 0.5);
    }

    #[test]
    fn a_quiet_but_audible_signal_is_kept_whole() {
        let samples = vec![0.01; 50];
        assert_eq!(trim_silence(&samples, 1_000).len(), 50);
        assert!(trim_silence(&[0.0; 50], 1_000).is_empty());
    }

    #[test]
    fn the_sentence_is_trimmed_keeps_its_rate_and_has_no_word_timing() {
        let fake = Arc::new(Fake::speaking(padded(), 1_000));
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let s = e.open_session().unwrap().synthesize_timed("Bonjour.", &["bonjour".into()], &params("sofia", "fr")).unwrap();
        assert_eq!(s.pcm.sample_rate, 1_000, "la fréquence du calcul, telle quelle");
        assert_eq!(s.pcm.samples.len(), 30 + 200 + 30, "30 ms gardés de chaque côté");
        assert!(s.words.is_none(), "estimation par lettres");
    }

    #[test]
    fn the_voice_of_another_model_reaches_the_compute_as_the_default() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "qwen3_tts_17b_customvoice_8bit", "/m/q17");
        let mut session = e.open_session().unwrap();
        session.synthesize("Bonjour.", &params("p-fenrir60", "fr-FR")).unwrap();
        session.synthesize("Hello.", &params("", "en")).unwrap();
        session.synthesize("Salut.", &params("eric", "de")).unwrap();
        let calls = fake.calls.locked().clone();
        assert_eq!((calls[0].2.as_str(), calls[0].3.as_str()), ("fr", "vivian"));
        assert_eq!((calls[1].2.as_str(), calls[1].3.as_str()), ("en", "aiden"));
        assert_eq!((calls[2].2.as_str(), calls[2].3.as_str()), ("fr", "eric"), "langue inconnue : le français");
    }

    #[test]
    fn each_engine_speaks_with_its_own_model() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let reading = engine(&fake, "qwen3_tts_17b_customvoice_8bit", "/m/q17");
        let preview = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        preview.open_session().unwrap().synthesize("Aperçu.", &params("leo", "fr")).unwrap();
        drop(preview);
        reading.open_session().unwrap().synthesize("Lecture.", &params("vivian", "fr")).unwrap();
        let calls = fake.calls.locked().clone();
        assert_eq!(calls[0].0, PathBuf::from("/m/magpie"));
        assert_eq!(calls[1].0, PathBuf::from("/m/q17"), "l'aperçu relâché ne touche pas la lecture");
        assert_eq!(*fake.releases.locked(), [PathBuf::from("/m/magpie")]);
    }

    #[test]
    fn dropping_the_engine_releases_its_model_once() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "qwen3_tts_06b_base_8bit", "/m/q06");
        let session = e.open_session().unwrap();
        drop(e);
        assert!(fake.releases.locked().is_empty(), "une session vit encore");
        drop(session);
        assert_eq!(fake.releases.locked().len(), 1);
    }

    #[test]
    fn a_failed_warm_up_releases_what_was_loaded() {
        let fake = Arc::new(Fake { fail_warm_up: true, ..Fake::speaking(vec![0.5], 24_000) });
        let compute: Arc<dyn TextCompute> = Arc::clone(&fake) as Arc<dyn TextCompute>;
        let err = TextComputeEngine::load(compute, package("magpie_tts_357m_8bit"), Path::new("/m/x"), Arc::new(SilentEngine));
        assert!(err.is_err());
        assert_eq!(fake.loads.load(Ordering::SeqCst), 1);
        assert_eq!(fake.releases.locked().len(), 1, "chaque load a son release");
    }

    #[test]
    fn a_cancelled_synthesis_never_counts_as_a_failure() {
        let fake = Arc::new(Fake::speaking(vec![0.5; 10], 24_000));
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let mut session = e.open_session().unwrap();
        for _ in 0..5 {
            let err = session
                .synthesize_cancellable("x", &[], &params("sofia", "fr"), &Cancel::when(|| true))
                .unwrap_err();
            assert!(is_cancelled(&err), "{err}");
        }
        assert_eq!(e.readiness(), Readiness::Ready);
    }

    #[test]
    fn three_failures_in_a_row_fall_back_to_the_system_voice() {
        let fake = Arc::new(Fake { fail: true, ..Fake::speaking(vec![0.5; 10], 24_000) });
        let e = engine(&fake, "magpie_tts_357m_8bit", "/m/magpie");
        let mut session = e.open_session().unwrap();
        for _ in 0..3 {
            assert!(session.synthesize("x", &params("sofia", "fr")).is_err());
        }
        assert!(matches!(e.readiness(), Readiness::Fallback(_)));
        assert!(e.name().contains("(repli)"));
        assert!(!e.splits_first_clause());
        assert_eq!(fake.releases.locked().len(), 1, "le modèle part avec le repli");
    }

    #[test]
    fn a_silent_or_invalid_output_is_a_failure() {
        let silent = Arc::new(Fake::speaking(vec![0.0; 100], 24_000));
        assert!(engine(&silent, "magpie_tts_357m_8bit", "/m/a").open_session().unwrap().synthesize("x", &params("", "fr")).is_err());
        let nan = Arc::new(Fake::speaking(vec![f32::NAN; 10], 24_000));
        assert!(engine(&nan, "magpie_tts_357m_8bit", "/m/b").open_session().unwrap().synthesize("x", &params("", "fr")).is_err());
    }

    #[test]
    fn the_engine_is_named_after_its_package() {
        let fake = Arc::new(Fake::speaking(vec![0.5], 24_000));
        assert_eq!(engine(&fake, "qwen3_tts_06b_base_8bit", "/m/q").name(), "Qwen3 0.6B · voix française · MLX");
        assert_eq!(runtime_name(Runtime::KokoroMlx), None);
    }
}
```

Dans `src/tts/mod.rs`, module `tests` (les appels existants de `build_engine_with(…, None)` deviennent `&AppCompute::default()`, et `Some(Arc::new(kokoro::SharedCompute::new(…)))` devient `&AppCompute { kokoro: Some(…), text: None }`) :

```rust
    fn text_settings(model_path: &str, family: &str) -> EngineSettings {
        EngineSettings { model_path: model_path.into(), family: family.into(), threads: 4, sessions: 1 }
    }

    fn text_files(name: &str, file: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-texte-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), b"x").unwrap();
        dir.join(file)
    }

    #[test]
    fn a_text_model_without_the_app_compute_falls_back_and_says_why() {
        let path = text_files("sans-calcul", "config.json");
        let (_, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings(&path.to_string_lossy(), "magpie_tts_357m_8bit"),
            &AppCompute::default(),
        );
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    fn a_text_model_without_its_files_falls_back_and_says_why() {
        struct Never;
        impl text_compute::TextCompute for Never {
            fn load(&self, _: &std::path::Path, _: &str) -> Result<()> { anyhow::bail!("non utilisé") }
            fn warm_up(&self, _: &std::path::Path) -> Result<()> { Ok(()) }
            fn synthesize(&self, _: &std::path::Path, _: &str, _: &str, _: &str, _: &Cancel) -> Result<text_compute::SpeechRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self, _: &std::path::Path) {}
        }
        let compute = AppCompute { kokoro: None, text: Some(Arc::new(Never)) };
        let (_, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings("/nulle/part/model.safetensors", "qwen3_tts_17b_customvoice_8bit"),
            &compute,
        );
        assert!(note.unwrap().contains("n'est pas installé"));
    }

    /// L'ancien Magpie d'audio.cpp (famille `magpie_tts`, un `.gguf`) n'est
    /// pas le nouveau : il n'est toujours pas pris en charge.
    #[test]
    fn an_old_magpie_tts_config_is_not_the_new_magpie() {
        let (_, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings("/ailleurs/magpie.gguf", "magpie_tts"),
            &AppCompute::default(),
        );
        assert!(note.unwrap().contains("non pris en charge"));
    }

    #[test]
    fn a_text_model_loads_in_the_background_and_splits_its_first_clause() {
        struct Slow;
        impl text_compute::TextCompute for Slow {
            fn load(&self, _: &std::path::Path, _: &str) -> Result<()> {
                std::thread::sleep(Duration::from_millis(200));
                Ok(())
            }
            fn warm_up(&self, _: &std::path::Path) -> Result<()> { Ok(()) }
            fn synthesize(&self, _: &std::path::Path, _: &str, _: &str, _: &str, _: &Cancel) -> Result<text_compute::SpeechRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self, _: &std::path::Path) {}
        }
        let path = text_files("charge", "model.safetensors");
        let compute = AppCompute { kokoro: None, text: Some(Arc::new(Slow)) };
        let (engine, note) = build_engine_with(
            EnginePreference::Auto,
            &text_settings(&path.to_string_lossy(), "qwen3_tts_06b_base_8bit"),
            &compute,
        );
        assert_eq!(note, None, "le chargement part en arrière-plan");
        assert!(engine.splits_first_clause());
    }
```

Dans `ffi/src/lib.rs`, module `tests` :

```rust
    /// Un calcul à texte « étranger » écrit en Rust : une seconde à 22 050 Hz.
    struct FakeText {
        fail: bool,
    }

    impl TextSpeechCompute for FakeText {
        fn load(&self, _: String, runtime: String) -> Result<(), SillonError> {
            if self.fail {
                return Err(SillonError::Failed { message: format!("{runtime} ne se charge pas") });
            }
            Ok(())
        }
        fn warm_up(&self, _: String) -> Result<(), SillonError> {
            Ok(())
        }
        fn synthesize(&self, _: String, _: String, _: String, _: String, cancel: Arc<SynthesisCancel>)
            -> Result<SpeechOutput, SillonError> {
            if cancel.is_cancelled() {
                return Err(SillonError::Cancelled);
            }
            Ok(SpeechOutput { samples: vec![0.25; 22_050], sample_rate: 22_050 })
        }
        fn release(&self, _: String) {}
    }

    #[test]
    fn a_foreign_text_compute_crosses_the_bridge() {
        assert_eq!(text_speech_bridge_check(Arc::new(FakeText { fail: false }), "/x".into(), "magpie_mlx".into()).unwrap(), 22_050);
        let err = text_speech_bridge_check(Arc::new(FakeText { fail: true }), "/x".into(), "magpie_mlx".into())
            .unwrap_err()
            .to_string();
        assert!(err.contains("magpie_mlx ne se charge pas"), "{err}");
    }

    #[test]
    fn a_text_abandon_crosses_the_bridge_as_an_abandon() {
        use sillon::tts::text_compute::TextCompute;
        let compute = ForeignText(Arc::new(FakeText { fail: false }));
        let err = compute
            .synthesize(std::path::Path::new("/x"), "x", "fr", "sofia", &tts::Cancel::when(|| true))
            .unwrap_err();
        assert!(tts::is_cancelled(&err), "{err}");
    }
```

- [ ] **Step 2 : RUST** — attendu : échecs (compilation).

- [ ] **Step 3 : implémenter `src/tts/text_compute.rs`** (au-dessus du module de tests) :

```rust
//! Les moteurs de lecture qui prennent le texte (Qwen3-TTS, MagpieTTS) : le
//! calcul est dans l'app (MLX, Swift), derrière `TextCompute` ; le cœur
//! garde le choix de la voix et de la langue, le repli, la coupe des
//! silences et le minutage estimé (`words: None` : le lecteur répartit la
//! durée selon les lettres de la forme parlée).

use anyhow::{bail, ensure, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::PcmBuffer;
use crate::models::{ModelPackage, Runtime};
use crate::sync::LockExt;
use crate::text::language::Lang;

use super::{is_cancelled, Cancel, Readiness, Synthesis, TtsEngine, TtsParams, TtsSession};

/// Ce que le calcul rend : l'audio mono et sa fréquence (24 000 Hz pour
/// Qwen3, 22 050 pour Magpie ; la lecture rééchantillonne).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechRaw {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Le calcul d'un modèle de lecture à texte, fourni par l'app.
///
/// Un modèle par dossier : la lecture et l'aperçu de la bibliothèque peuvent
/// tenir chacun le leur. `load` et `release` vont par paires, par dossier.
/// Les appels arrivent de fils d'arrière-plan et bloquent jusqu'au
/// résultat ; le calcul les sérialise lui-même (un seul verrou MLX).
pub trait TextCompute: Send + Sync {
    fn load(&self, model_dir: &Path, runtime: &str) -> Result<()>;
    /// Une phrase courte, pour compiler ce qui doit l'être.
    fn warm_up(&self, model_dir: &Path) -> Result<()>;
    /// `Err(Cancelled)` quand `cancel` le dit, avant ou en route.
    fn synthesize(&self, model_dir: &Path, text: &str, language: &str, voice: &str, cancel: &Cancel)
        -> Result<SpeechRaw>;
    fn release(&self, model_dir: &Path);
}

/// Le nom du moteur, passé au calcul de l'app avec le dossier.
pub fn runtime_name(runtime: Runtime) -> Option<&'static str> {
    match runtime {
        Runtime::Qwen3TtsMlx => Some("qwen3_tts_mlx"),
        Runtime::MagpieMlx => Some("magpie_mlx"),
        _ => None,
    }
}

/// Sous ce niveau (−50 dBFS), un échantillon est du silence.
const SILENCE: f32 = 0.003_162_3;
/// Le silence gardé de chaque côté de la parole.
const KEEP_SECONDS: f32 = 0.030;
/// Échecs du calcul d'affilée avant le repli, comme Kokoro.
const MAX_COMPUTE_FAILURES: usize = 3;

/// La parole sans le silence de tête et de queue (30 ms gardés de chaque
/// côté). Sans minutage par mot, le surlignage se répartit sur toute la
/// durée du son : un blanc de début le décalerait. Vide : rien d'audible.
pub fn trim_silence(samples: &[f32], sample_rate: u32) -> &[f32] {
    let keep = (sample_rate as f32 * KEEP_SECONDS) as usize;
    let Some(first) = samples.iter().position(|s| s.abs() > SILENCE) else {
        return &samples[..0];
    };
    let last = samples.iter().rposition(|s| s.abs() > SILENCE).unwrap_or(first);
    &samples[first.saturating_sub(keep)..(last + 1 + keep).min(samples.len())]
}

fn check(raw: &SpeechRaw) -> Result<()> {
    ensure!(raw.sample_rate > 0, "sortie invalide : fréquence nulle");
    ensure!(raw.samples.iter().all(|s| s.is_finite()), "sortie invalide : valeur non finie");
    Ok(())
}

struct Inner {
    compute: Arc<dyn TextCompute>,
    package: &'static ModelPackage,
    dir: PathBuf,
    /// Pourquoi la voix est passée en repli, si elle l'est.
    broken: Mutex<Option<String>>,
    fallback: Arc<dyn TtsEngine>,
    failures: AtomicUsize,
    /// La part du calcul est rendue (repli, ou moteur relâché) : une fois.
    released: AtomicBool,
}

impl Inner {
    fn broken(&self) -> Option<String> {
        self.broken.locked().clone()
    }

    fn release_compute(&self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.compute.release(&self.dir);
        }
    }

    /// Compte un échec du calcul ; au troisième d'affilée, le repli.
    fn compute_failed(&self) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= MAX_COMPUTE_FAILURES {
            eprintln!("{} : {failures} échecs du calcul d'affilée, passage en repli", self.package.id);
            *self.broken.locked() = Some(format!("calcul {} en échec", self.package.display_name));
            self.release_compute();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.release_compute();
    }
}

/// Un moteur à texte : une session (le calcul est en série).
pub struct TextComputeEngine {
    inner: Arc<Inner>,
}

impl TextComputeEngine {
    /// Charge le modèle et le prépare (« Bonjour. ») : quelques secondes,
    /// dans le fil de chargement différé. Une préparation ratée rend le
    /// modèle chargé.
    pub fn load(
        compute: Arc<dyn TextCompute>,
        package: &'static ModelPackage,
        dir: &Path,
        fallback: Arc<dyn TtsEngine>,
    ) -> Result<Self> {
        let Some(runtime) = runtime_name(package.runtime) else {
            bail!("{} ne lit pas à partir du texte", package.display_name);
        };
        compute.load(dir, runtime)?;
        if let Err(err) = compute.warm_up(dir) {
            compute.release(dir);
            return Err(err);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                compute,
                package,
                dir: dir.to_path_buf(),
                broken: Mutex::new(None),
                fallback,
                failures: AtomicUsize::new(0),
                released: AtomicBool::new(false),
            }),
        })
    }
}

impl TtsEngine for TextComputeEngine {
    fn name(&self) -> String {
        match self.inner.broken() {
            Some(_) => format!("{} (repli)", self.inner.fallback.name()),
            None => format!("{} · MLX", self.inner.package.display_name),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(TextComputeSession { inner: Arc::clone(&self.inner), fallback: None }))
    }

    fn max_sessions(&self) -> usize {
        1
    }

    fn readiness(&self) -> Readiness {
        match self.inner.broken() {
            Some(reason) => Readiness::Fallback(reason),
            None => Readiness::Ready,
        }
    }

    /// La tête de phrase d'abord : le premier son arrive plus tôt sur un
    /// modèle lent. En repli, la voix de repli parle d'un seul tenant.
    fn splits_first_clause(&self) -> bool {
        self.inner.broken().is_none()
    }
}

struct TextComputeSession {
    inner: Arc<Inner>,
    fallback: Option<Box<dyn TtsSession>>,
}

impl TextComputeSession {
    fn fallback(&mut self) -> Result<&mut Box<dyn TtsSession>> {
        if self.fallback.is_none() {
            self.fallback = Some(self.inner.fallback.open_session()?);
        }
        Ok(self.fallback.as_mut().expect("ouverte juste au-dessus"))
    }
}

impl TtsSession for TextComputeSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        Ok(self.synthesize_timed(text, &[], params)?.pcm)
    }

    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        self.synthesize_cancellable(text, words, params, &Cancel::never())
    }

    /// L'abandon va au calcul, qui le consulte en route ; il ne compte pas
    /// vers le repli.
    fn synthesize_cancellable(
        &mut self,
        text: &str,
        words: &[String],
        params: &TtsParams,
        cancel: &Cancel,
    ) -> Result<Synthesis> {
        if self.inner.broken().is_some() {
            return self.fallback()?.synthesize_cancellable(text, words, params, cancel);
        }
        cancel.check()?;
        let language = Lang::parse(&params.language).unwrap_or(Lang::Fr).code();
        let family = self.inner.package.family;
        let voice = crate::models::voice_for_language(family, language, Some(params.voice.as_str()));
        let raw = self
            .inner
            .compute
            .synthesize(&self.inner.dir, text, language, voice, cancel)
            .and_then(|raw| check(&raw).map(|()| raw));
        let raw = match raw {
            Ok(raw) => raw,
            Err(err) if is_cancelled(&err) => return Err(err),
            Err(err) => {
                self.inner.compute_failed();
                return Err(err);
            }
        };
        let kept = trim_silence(&raw.samples, raw.sample_rate);
        if kept.is_empty() {
            self.inner.compute_failed();
            bail!("{} n'a rien dit", self.inner.package.display_name);
        }
        self.inner.failures.store(0, Ordering::SeqCst);
        Ok(Synthesis { pcm: PcmBuffer::new(kept.to_vec(), raw.sample_rate, 1), words: None })
    }
}
```

(si `Readiness` n'implémente pas `PartialEq` pour le test `a_cancelled_synthesis_never_counts_as_a_failure`, utiliser `matches!(e.readiness(), Readiness::Ready)` ; il dérive déjà `PartialEq` aujourd'hui.)

- [ ] **Step 4 : implémenter `src/tts/mod.rs`.** Ajouter `pub mod text_compute;`. Puis :

```rust
/// Les calculs que l'app fournit : Kokoro MLX (phonèmes) et les modèles à
/// texte (Qwen3, Magpie). Le terminal n'en a aucun.
#[derive(Clone, Default)]
pub struct AppCompute {
    /// Enveloppé **une fois** : chaque moteur reçoit le même `Arc`.
    pub kokoro: Option<Arc<kokoro::SharedCompute>>,
    pub text: Option<Arc<dyn text_compute::TextCompute>>,
}
```

`build_engine` appelle `build_engine_with(preference, settings, &AppCompute::default())`. `build_engine_with(preference, settings, compute: &AppCompute)` : `build_kokoro(settings, compute.kokoro.clone())` ; avant le repli « non pris en charge », insérer :

```rust
    let text_package = crate::models::CATALOG.iter().find(|p| {
        p.family == settings.family
            && matches!(p.runtime, crate::models::Runtime::Qwen3TtsMlx | crate::models::Runtime::MagpieMlx)
    });
    if let Some(package) = text_package {
        return build_text_engine(package, settings, compute.text.clone());
    }
```

et la fonction :

```rust
/// Un modèle à texte (Qwen3, Magpie) : le calcul de l'app, le dossier du
/// fichier principal ; le chargement part en arrière-plan.
fn build_text_engine(
    package: &'static crate::models::ModelPackage,
    settings: &EngineSettings,
    text: Option<Arc<dyn text_compute::TextCompute>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let fallback = fallback::fallback_engine(settings);
    let Some(compute) = text else {
        return (
            fallback,
            Some(format!("Calcul Metal indisponible — {} ne tourne que dans l'app", package.display_name)),
        );
    };
    let main = std::path::Path::new(&settings.model_path);
    let dir = main.parent().filter(|_| main.is_file()).map(std::path::Path::to_path_buf);
    let Some(dir) = dir else {
        return (fallback, Some(format!("{} n'est pas installé — voir les réglages", package.display_name)));
    };
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn(package.display_name.to_string(), 1, fallback, move || {
        let engine = text_compute::TextComputeEngine::load(compute, package, &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    (Arc::new(engine), None)
}
```

Mettre à jour le commentaire « Kokoro est le seul modèle » (« Kokoro, Qwen3 et Magpie sont les seuls modèles : une autre famille… ») et la doc de `build_engine_with`.

- [ ] **Step 5 : implémenter `ffi/src/lib.rs`.** À côté de `KokoroCompute` :

```rust
/// Un modèle de lecture à texte (Qwen3-TTS, MagpieTTS), fourni par l'app
/// (MLX). Le moteur Rust garde voix, langue, repli et coupe des silences.
///
/// Contrat pour l'implémentation étrangère : celui de `KokoroCompute`
/// (fils d'arrière-plan, appels synchrones parfois simultanés, sérialisés
/// par l'app, jamais d'attente du fil principal), avec en plus :
/// - un modèle **par dossier** : `load`/`release` vont par paires pour un
///   même `model_dir`, et deux dossiers peuvent être tenus à la fois (la
///   lecture et l'aperçu de la bibliothèque) ;
/// - `runtime` : `qwen3_tts_mlx` ou `magpie_mlx` ;
/// - `language` : `fr` ou `en` ; `voice` : une voix du catalogue (vide à la
///   préparation : la voix par défaut du modèle) ;
/// - `synthesize` consulte `cancel` avant et pendant la génération (toutes
///   les ~12 trames Qwen, ~20 trames Magpie) ; s'il dit vrai, elle rend
///   `SillonError::Cancelled` (jamais `Failed`) et ne garde pas `cancel`.
#[uniffi::export(with_foreign)]
pub trait TextSpeechCompute: Send + Sync {
    fn load(&self, model_dir: String, runtime: String) -> Result<(), SillonError>;
    fn warm_up(&self, model_dir: String) -> Result<(), SillonError>;
    fn synthesize(
        &self,
        model_dir: String,
        text: String,
        language: String,
        voice: String,
        cancel: Arc<SynthesisCancel>,
    ) -> Result<SpeechOutput, SillonError>;
    fn release(&self, model_dir: String);
}

#[derive(uniffi::Record)]
pub struct SpeechOutput {
    pub samples: Vec<f32>,
    /// 24 000 (Qwen3) ou 22 050 (Magpie).
    pub sample_rate: u32,
}

/// `TextSpeechCompute` vu par le moteur.
struct ForeignText(Arc<dyn TextSpeechCompute>);

fn from_foreign(e: SillonError) -> anyhow::Error {
    match e {
        SillonError::Cancelled => anyhow::Error::new(tts::Cancelled),
        e => anyhow::anyhow!(e.to_string()),
    }
}

impl tts::text_compute::TextCompute for ForeignText {
    fn load(&self, model_dir: &std::path::Path, runtime: &str) -> anyhow::Result<()> {
        self.0.load(model_dir.to_string_lossy().into_owned(), runtime.to_string()).map_err(from_foreign)
    }
    fn warm_up(&self, model_dir: &std::path::Path) -> anyhow::Result<()> {
        self.0.warm_up(model_dir.to_string_lossy().into_owned()).map_err(from_foreign)
    }
    fn synthesize(
        &self,
        model_dir: &std::path::Path,
        text: &str,
        language: &str,
        voice: &str,
        cancel: &tts::Cancel,
    ) -> anyhow::Result<tts::text_compute::SpeechRaw> {
        let cancel = Arc::new(SynthesisCancel(cancel.clone()));
        let out = self
            .0
            .synthesize(model_dir.to_string_lossy().into_owned(), text.into(), language.into(), voice.into(), cancel)
            .map_err(from_foreign)?;
        Ok(tts::text_compute::SpeechRaw { samples: out.samples, sample_rate: out.sample_rate })
    }
    fn release(&self, model_dir: &std::path::Path) {
        self.0.release(model_dir.to_string_lossy().into_owned());
    }
}

/// Vérifie le pont vers un calcul à texte étranger : chargement, « Bonjour. »,
/// libération, sortie contrôlée. Rend le nombre d'échantillons.
#[uniffi::export]
pub fn text_speech_bridge_check(
    compute: Arc<dyn TextSpeechCompute>,
    model_dir: String,
    runtime: String,
) -> Result<u64, SillonError> {
    use tts::text_compute::TextCompute;
    let backend = ForeignText(compute);
    let dir = std::path::Path::new(&model_dir);
    backend.load(dir, &runtime).map_err(failed)?;
    let raw = backend.synthesize(dir, "Bonjour.", "fr", "", &tts::Cancel::never()).map_err(failed);
    backend.release(dir);
    let raw = raw?;
    if raw.samples.is_empty() || raw.samples.iter().any(|s| !s.is_finite()) {
        return Err(failed("sortie invalide"));
    }
    Ok(raw.samples.len() as u64)
}
```

`ForeignKokoro::infer` peut reprendre `from_foreign`. `app_reader(loaded, compute: tts::AppCompute)` : `let available = compute.kokoro.is_some() || compute.text.is_some();` et la fabrique appelle `tts::build_engine_with(config.engine_preference(), &config.engine_settings(), &compute)` (cloner `compute` dans la fermeture). `new()` passe `tts::AppCompute::default()`. Remplacer le constructeur `with_kokoro` par :

```rust
    /// Le lecteur réel avec les calculs MLX de l'app : Kokoro (enveloppé une
    /// seule fois, voir `SharedCompute`) et les modèles à texte.
    #[uniffi::constructor]
    pub fn with_mlx(kokoro: Arc<dyn KokoroCompute>, text: Arc<dyn TextSpeechCompute>) -> Arc<Self> {
        let compute = tts::AppCompute {
            kokoro: Some(Arc::new(kokoro::SharedCompute::new(Arc::new(ForeignKokoro(kokoro))))),
            text: Some(Arc::new(ForeignText(text))),
        };
        Arc::new(Self { handle: ReaderHandle::spawn(move || app_reader(Config::load(), compute)) })
    }
```

et la doc du test `the_kokoro_engine_on_the_app_compute_is_named_kokoro_mlx` (`SillonReader::with_mlx`). L'app ne compile plus jusqu'à la Tâche 8 (`AppDelegate` appelle `withKokoro`) : dans cette tâche, remplacer seulement l'appel de `macos/Sources/AppDelegate.swift` par `SillonReader.withMlx(kokoro: compute, text: UnavailableTextSpeech())` avec, dans le même fichier, un calcul qui refuse tout (remplacé à la Tâche 8) :

```swift
/// Remplacé par `TextSpeechMLXCompute` (lot 3, Tâche 8).
final class UnavailableTextSpeech: TextSpeechCompute, @unchecked Sendable {
    func load(modelDir: String, runtime: String) throws {
        throw SillonError.Failed(message: "Calcul Metal indisponible")
    }
    func warmUp(modelDir: String) throws {}
    func synthesize(modelDir: String, text: String, language: String, voice: String,
                    cancel: SynthesisCancel) throws -> SpeechOutput {
        throw SillonError.Failed(message: "Calcul Metal indisponible")
    }
    func release(modelDir: String) {}
}
```

- [ ] **Step 6 : RUST**, **FFI**, puis **UIBUILD** — attendu : vert, `** TEST BUILD SUCCEEDED **`.
- [ ] **Step 7 : commit** `feat(moteur): un moteur de lecture à texte (Qwen3, Magpie) derrière TextSpeechCompute — un modèle par dossier, silences de tête et de queue coupés, suivi des mots estimé, repli après trois échecs`

---

### Task 3 : La voix française fixe — x-vector SIWIS précalculé

**Files:**
- Create: `scripts/qwen3-voix-siwis.py`
- Create: `assets/voices/qwen3-06b-siwis.safetensors` (produit par le script, 1 024 flottants, clé `speaker_embedding`)
- Create: `assets/voices/qwen3-06b-siwis.json` (provenance, produit par le script)
- Modify: `NOTICE` (crédit SIWIS)

**Interfaces:**
- Produces: `assets/voices/qwen3-06b-siwis.safetensors` : un tenseur `speaker_embedding`, `float32`, forme `[1024]` (= `talker_config.hidden_size` du 0.6B, = `speaker_encoder_config.enc_dim`). Lu par `Qwen3TTSRunner` (Tâche 5) comme ressource du framework.

Ce qu'il faut savoir (relevé §2.e, rapport du banc `.superpowers/research/eval/rapport.md` §8) :
- l'extrait : **SIWIS French Speech Synthesis Database**, partie 5, `expr_chap_s01_0011.wav` (lecture expressive, 8,6 s ; texte : « Seulement, Conseil avait un défaut. Formaliste enragé, il ne me parlait jamais qu'à la troisième personne, au point d'en être agaçant. »). Copie locale : `~/Library/Caches/sillon-bench/eval/refs/siwis/expr_chap_s01_0011.wav`, sha256 `54f7cef4cc995d718fcadcdfe5b3c69c2a7563d195e00a77fa72df2ecfe980dc`. Source : Honnet, Lazaridis, Garner, Yamagishi (2017), Idiap / Université d'Édimbourg, DataShare, `https://doi.org/10.7488/ds/1705`, licence **CC-BY-4.0** (crédit obligatoire ; ni l'extrait ni ses dérivés ne sont restreints).
- la préparation est celle du banc (`prep_refs.py`) : 24 kHz mono, fondu de 30 ms à la fin, RMS ramené à −23 dBFS, écrêtage ±0,98 (pas de coupe : < 10,2 s). C'est cette préparation qui a été mesurée meilleure en français (accent +4,7 sur Vivian, mode x-vector).
- le calcul : l'encodeur de locuteur du **0.6B** Base (`enc_dim` 1024 ; celui du 1.7B mesuré au banc donnait 2 048), par mlx-audio 0.5.8, `extract_speaker_embedding` (déterministe : aucun tirage).
- dans le modèle, le vecteur prend la place de l'embedding `spk_id` dans le préfixe du codec (mode `x_vector_only` de l'officiel, `py/modeling.py:2103-2104, 2166-2171`) ; ni audio de référence, ni encodeur de codec à l'exécution. Ce n'est pas du clonage par l'utilisateur.

- [ ] **Step 1 : vérifier la licence de la source.** `curl -sL https://doi.org/10.7488/ds/1705 | grep -io 'creative commons[^<]*\|cc[- ]by[^<]*' | head -3` — attendu : une mention CC BY 4.0. Si la page ne la montre pas (ou exige une autre licence), **s'arrêter** et le dire à l'utilisateur avant de livrer quoi que ce soit dérivé de SIWIS.

- [ ] **Step 2 : écrire `scripts/qwen3-voix-siwis.py`.**

```python
#!/usr/bin/env python3
"""La voix française fixe de Qwen3-TTS 0.6B Base, livrée avec Sillon.

Le x-vector de l'extrait SIWIS expr_chap_s01_0011 (CC-BY-4.0, Idiap / U.
d'Édimbourg, https://doi.org/10.7488/ds/1705), calculé une fois par
l'encodeur de locuteur du modèle (mlx-audio 0.5.8, Python), écrit dans
assets/voices/qwen3-06b-siwis.safetensors (clé speaker_embedding, 1 024
flottants) avec sa provenance (qwen3-06b-siwis.json). Le port Swift le met
à la place du locuteur (mode x-vector) : une voix fixe, pas un clonage.

L'extrait est préparé comme au banc d'écoute (prep_refs.py) : 24 kHz mono,
fondu de 30 ms à la fin, RMS à -23 dBFS, écrêtage à +-0,98.

    HF_HOME=~/Library/Caches/sillon-bench/eval/hf \\
      ~/Library/Caches/sillon-bench/eval/venv/bin/python -I \\
      scripts/qwen3-voix-siwis.py ~/Library/Caches/sillon-bench/eval/refs/siwis/expr_chap_s01_0011.wav
"""
import hashlib, json, os, sys
import numpy as np, librosa, mlx.core as mx
from huggingface_hub import snapshot_download
from mlx_audio.tts.utils import load_model

REPO = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit"
REVISION = "50f45ef0047cde7e84c2ef04326acb8ada2436a7"
SOURCE_SHA256 = "54f7cef4cc995d718fcadcdfe5b3c69c2a7563d195e00a77fa72df2ecfe980dc"
TEXT = ("Seulement, Conseil avait un défaut. Formaliste enragé, il ne me parlait "
        "jamais qu'à la troisième personne, au point d'en être agaçant.")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "voices", "qwen3-06b-siwis")


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def prepare(wav):
    x, _ = librosa.load(wav, sr=24000, mono=True)
    fade = int(0.03 * 24000)
    x[-fade:] *= np.linspace(1, 0, fade)
    x = x * (10 ** (-23 / 20) / (np.sqrt(np.mean(x ** 2)) + 1e-9))
    return np.clip(x, -0.98, 0.98).astype(np.float32)


def main():
    wav = sys.argv[1]
    if sha256(wav) != SOURCE_SHA256:
        sys.exit(f"{wav} n'est pas l'extrait SIWIS attendu (sha256 {SOURCE_SHA256})")
    model = load_model(snapshot_download(REPO, revision=REVISION))
    audio = prepare(wav)
    first = np.array(model.extract_speaker_embedding(mx.array(audio)).astype(mx.float32)).reshape(-1)
    again = np.array(model.extract_speaker_embedding(mx.array(audio)).astype(mx.float32)).reshape(-1)
    assert first.shape == (1024,), first.shape
    assert np.all(np.isfinite(first)), "valeur non finie"
    assert np.array_equal(first, again), "l'encodeur de locuteur doit être déterministe"
    mx.save_safetensors(OUT + ".safetensors", {"speaker_embedding": mx.array(first)})
    json.dump({
        "name": "siwis",
        "model": f"{REPO}@{REVISION}",
        "source": "SIWIS French Speech Synthesis Database, partie 5, expr_chap_s01_0011.wav "
                  "(Honnet, Lazaridis, Garner, Yamagishi, 2017, https://doi.org/10.7488/ds/1705)",
        "license": "CC-BY-4.0 (crédit : Idiap / SIWIS)",
        "source_sha256": SOURCE_SHA256,
        "text": TEXT,
        "preparation": "24 kHz mono, fondu 30 ms en fin, RMS -23 dBFS, écrêtage 0,98 (prep_refs.py)",
        "method": "mlx-audio 0.5.8, Qwen3TTS.extract_speaker_embedding (mode x-vector)",
        "vector_sha256": sha256(OUT + ".safetensors"),
        "evaluation": ".superpowers/research/eval/rapport.md §8 (mesuré sur le 1.7B Base)",
    }, open(OUT + ".json", "w"), ensure_ascii=False, indent=2)
    print(OUT + ".safetensors", float(np.linalg.norm(first)))


if __name__ == "__main__":
    main()
```

- [ ] **Step 3 : lancer le script** (commande de sa docstring). Attendu : une ligne `…/qwen3-06b-siwis.safetensors <norme>` ; le fichier pèse ≈ 4,2 Ko. Le relancer : `shasum -a 256 assets/voices/qwen3-06b-siwis.safetensors` identique les deux fois (sinon : non déterministe, s'arrêter et le signaler). Si l'extrait n'est pas sur la machine, demander à l'utilisateur de le télécharger depuis le DOI (archive SIWIS, partie 5) — ne jamais en prendre un autre.

- [ ] **Step 4 : `NOTICE`** — après le paragraphe de p-fenrir60 :

```
La voix « siwis » de Qwen3-TTS 0.6B Base, livrée avec Sillon
(assets/voices/qwen3-06b-siwis.safetensors), est un vecteur de locuteur
calculé à partir d'un extrait de la SIWIS French Speech Synthesis Database
(P.-E. Honnet, A. Lazaridis, P. N. Garner, J. Yamagishi, Idiap Research
Institute et Université d'Édimbourg, 2017, https://doi.org/10.7488/ds/1705),
sous licence Creative Commons Attribution 4.0
(https://creativecommons.org/licenses/by/4.0/). Recette :
scripts/qwen3-voix-siwis.py et assets/voices/qwen3-06b-siwis.json.
```

- [ ] **Step 5 : commit** `feat(voix): la voix française fixe de Qwen3 0.6B — x-vector de l'extrait SIWIS expr_chap_s01_0011 (CC-BY-4.0, Idiap), calculé une fois hors ligne et livré avec sa recette`

---

### Task 4 : Un encodeur BPE de Qwen2 à nous, et ses ids de référence

**Files:**
- Create: `scripts/qwen3-bpe-ids.py`
- Create: `macos/Tests/Fixtures/qwen3-bpe.json`, `macos/Tests/Fixtures/qwen3-vocab.json`, `macos/Tests/Fixtures/qwen3-merges.txt`, `macos/Tests/Fixtures/qwen3-tokenizer_config.json` (produits par le script)
- Create: `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/Qwen2BPETokenizer.swift`
- Test: `macos/Tests/Qwen2BPETokenizerTests.swift`

**Interfaces:**
- Produces (module `KokoroMLX`) : `public final class Qwen2BPETokenizer` ; `public convenience init(directory: URL) throws` (lit `vocab.json`, `merges.txt`, `tokenizer_config.json`) ; `public init(vocab: URL, merges: URL, config: URL) throws` ; `public func encode(text: String) -> [Int]` (même signature que l'appel `tokenizer.encode(text:)` de `Qwen3TTS.swift`, relevé §1.c).
- Le vocabulaire (4,4 Mo, Apache-2.0, poids Qwen) est copié dans les fixtures **pour que le test d'identité tourne sur la CI** sans modèle. Décision à confirmer par l'utilisateur ; l'alternative (test ignoré sans `SILLON_TTS_MODELS`) ne tournerait jamais sur la CI.

Points d'algorithme (relevé §1.d, à suivre exactement) :
1. NFC (`precomposedStringWithCanonicalMapping`) ;
2. couper sur les jetons ajoutés littéraux (`added_tokens_decoder` de `tokenizer_config.json`), chacun donne son id ;
3. pré-découper le reste par la regex de `transformers.Qwen2Tokenizer` : `(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+`. **Attention** : le port écrit `\p{N}{1,3}` (`Qwen3TTS.swift:1336`) ; Qwen2 découpe les chiffres un à un (`\p{N}`). La fixture tranche (phrases avec « 2027 », « 3,14 ») : si elle donne raison à `{1,3}`, prendre `{1,3}`, et le noter dans `ORIGINE.md` ;
4. projeter chaque octet UTF-8 par la table GPT-2 `bytes_to_unicode` ;
5. fusionner par rang (`merges.txt`, première ligne `#version` ignorée) ;
6. chercher chaque morceau dans `vocab.json`.

- [ ] **Step 1 : écrire `scripts/qwen3-bpe-ids.py`.**

```python
#!/usr/bin/env python3
"""Ids de référence de l'encodeur BPE de Qwen3-TTS (macos/Vendor/KokoroMLX/
TTS/Qwen3TTS/Qwen2BPETokenizer.swift), par transformers.AutoTokenizer.

Télécharge vocab.json, merges.txt et tokenizer_config.json du dépôt
mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit (révision figée ; mêmes fichiers
dans le 1.7B CustomVoice), les copie dans macos/Tests/Fixtures (qwen3-*), et
écrit macos/Tests/Fixtures/qwen3-bpe.json : les ids de chaque phrase.

    HF_HOME=~/Library/Caches/sillon-bench/eval/hf \\
      ~/Library/Caches/sillon-bench/eval/venv/bin/python -I scripts/qwen3-bpe-ids.py
"""
import json, os, shutil, tempfile
import transformers
from huggingface_hub import hf_hub_download

REPO = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit"
REVISION = "50f45ef0047cde7e84c2ef04326acb8ada2436a7"
FILES = ["vocab.json", "merges.txt", "tokenizer_config.json"]
FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "macos", "Tests", "Fixtures")
TEXTS = [
    "Bonjour.",
    "Ce matin, le marché était plein.",
    "L’été dernier, j’ai lu « Vingt mille lieues sous les mers ».",
    "Qu'on m'excuse de dire ainsi que j'avais quarante ans.",
    "Il est arrivé en 2027, à 14 h 30… enfin, presque.",
    "Prix : 3,14 € ; remise de 15 %.",
    "Où ? Là-bas ! Déjà ?",
    "Œuvre, cœur, ægipan, naïf, Noël.",
    "été en forme décomposée",
    "Je me vêtis chaudement, bottes de mer, bonnet de loutre.",
    "« Allons-y », dit-il — sans attendre.",
    "C’est-à-dire : l’opération n’a pas réussi.",
    "ÇA VA ? Très bien, MERCI.",
    "deux mille vingt-sept",
    "Hello, world!",
    "It's a beautiful day, isn't it? We'll see.",
    "The quick brown fox jumps over 13 lazy dogs.",
    "She said: \"don't\" — and left.",
    "Mr. O'Neil's café opened in 1999.",
    "  deux  espaces  et une tabulation\tici.",
    "Ligne un\nLigne deux",
    "Emoji : 🙂 et symbole ≈.",
    "<|im_start|>assistant\n",
    "<|im_end|>\n<|im_start|>assistant\n",
    "<|im_start|>assistant\nBonjour.<|im_end|>\n<|im_start|>assistant\n",
]


def main():
    work = tempfile.mkdtemp()
    for name in FILES:
        src = hf_hub_download(REPO, name, revision=REVISION)
        shutil.copy(src, os.path.join(work, name))
        shutil.copy(src, os.path.join(FIXTURES, "qwen3-" + name))
    tok = transformers.AutoTokenizer.from_pretrained(work)
    cases = [{"text": t, "ids": tok.encode(t, add_special_tokens=False)} for t in TEXTS]
    assert len(cases[-3]["ids"]) == 3, cases[-3]
    assert len(cases[-2]["ids"]) == 5, cases[-2]
    json.dump({
        "source": f"transformers {transformers.__version__}, AutoTokenizer, {REPO}@{REVISION}",
        "cases": cases,
    }, open(os.path.join(FIXTURES, "qwen3-bpe.json"), "w"), ensure_ascii=False, indent=1)
    print(type(tok).__name__, len(cases), "phrases")


if __name__ == "__main__":
    main()
```

(les copies s'appellent `qwen3-vocab.json`, `qwen3-merges.txt`, `qwen3-tokenizer_config.json` : à plat dans les ressources du bundle de tests, des noms sans collision.)

- [ ] **Step 2 : lancer le script** (docstring). Attendu : `Qwen2TokenizerFast 25 phrases` (ou le nom de classe de transformers 5) ; les quatre fichiers dans `macos/Tests/Fixtures/`.

- [ ] **Step 3 : test qui échoue** — `macos/Tests/Qwen2BPETokenizerTests.swift` :

```swift
import XCTest
@testable import KokoroMLX

/// L'encodeur BPE de Qwen3-TTS, à nous (pas de swift-transformers) : mêmes
/// ids que `transformers.AutoTokenizer` (scripts/qwen3-bpe-ids.py), sur le
/// vrai vocabulaire (fixtures qwen3-*), sans modèle.
final class Qwen2BPETokenizerTests: XCTestCase {
    struct Reference: Decodable {
        struct Case: Decodable { let text: String; let ids: [Int] }
        let source: String
        let cases: [Case]
    }

    static func fixture(_ name: String, _ ext: String) -> URL {
        Bundle(for: Qwen2BPETokenizerTests.self).url(forResource: name, withExtension: ext)!
    }

    static let tokenizer: Qwen2BPETokenizer = try! Qwen2BPETokenizer(
        vocab: fixture("qwen3-vocab", "json"),
        merges: fixture("qwen3-merges", "txt"),
        config: fixture("qwen3-tokenizer_config", "json"))

    func testEverySentenceGivesTheIdsOfTransformers() throws {
        let data = try Data(contentsOf: Self.fixture("qwen3-bpe", "json"))
        let reference = try JSONDecoder().decode(Reference.self, from: data)
        XCTAssertEqual(reference.cases.count, 25)
        for c in reference.cases {
            XCTAssertEqual(Self.tokenizer.encode(text: c.text), c.ids, c.text)
        }
    }

    /// Le port coupe aux indices 3 et `count - 5` (relevé §1.d).
    func testTheChatTemplateHasThreeAndFiveTokens() {
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_start|>assistant\n").count, 3)
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_end|>\n<|im_start|>assistant\n").count, 5)
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_start|>").first, 151644)
    }

    /// Un texte en NFD (PDF) donne les ids du même texte en NFC.
    func testDecomposedTextIsComposedFirst() {
        XCTAssertEqual(Self.tokenizer.encode(text: "e\u{301}te\u{301}"), Self.tokenizer.encode(text: "été"))
    }

    func testEmptyTextGivesNothing() {
        XCTAssertEqual(Self.tokenizer.encode(text: ""), [])
    }
}
```

- [ ] **Step 4 : UIBUILD** — attendu : échec (`Qwen2BPETokenizer` inconnu).

- [ ] **Step 5 : implémenter `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/Qwen2BPETokenizer.swift`.**

```swift
import Foundation

/// L'encodeur BPE de Qwen2 (ByteLevel), écrit pour Sillon à la place de
/// swift-transformers : `vocab.json`, `merges.txt` et les jetons ajoutés de
/// `tokenizer_config.json`. Seul l'encodage sert (le texte à dire). Ne lit
/// que : n'écrit jamais dans le dossier du modèle. Parité :
/// `Qwen2BPETokenizerTests` (ids de `transformers`).
public final class Qwen2BPETokenizer: @unchecked Sendable {
    private let vocab: [String: Int]
    /// « a b » → rang de la fusion (plus petit = plus tôt).
    private let ranks: [String: Int]
    /// Jetons ajoutés, du plus long au plus court (contenu, id).
    private let added: [(String, Int)]
    private let split: NSRegularExpression
    private var cache: [String: [Int]] = [:]
    private let lock = NSLock()

    /// La pré-découpe de `transformers.Qwen2Tokenizer` (chiffres un à un).
    static let pattern =
        #"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+"#

    /// La table GPT-2 `bytes_to_unicode` : chaque octet devient un caractère
    /// imprimable.
    static let byteEncoder: [Character] = {
        var bytes = Array(33...126) + Array(161...172) + Array(174...255)
        var chars = bytes
        var n = 0
        for b in 0..<256 where !bytes.contains(b) {
            bytes.append(b)
            chars.append(256 + n)
            n += 1
        }
        var table = [Character](repeating: " ", count: 256)
        for (b, c) in zip(bytes, chars) {
            table[b] = Character(Unicode.Scalar(UInt32(c))!)
        }
        return table
    }()

    public convenience init(directory: URL) throws {
        try self.init(vocab: directory.appendingPathComponent("vocab.json"),
                      merges: directory.appendingPathComponent("merges.txt"),
                      config: directory.appendingPathComponent("tokenizer_config.json"))
    }

    public init(vocab vocabURL: URL, merges mergesURL: URL, config configURL: URL) throws {
        vocab = try JSONDecoder().decode([String: Int].self, from: Data(contentsOf: vocabURL))
        var ranks: [String: Int] = [:]
        let lines = try String(contentsOf: mergesURL, encoding: .utf8).split(separator: "\n", omittingEmptySubsequences: true)
        var rank = 0
        for (index, line) in lines.enumerated() {
            if index == 0, line.hasPrefix("#version") { continue }
            let parts = line.split(separator: " ", omittingEmptySubsequences: false)
            guard parts.count == 2 else { continue }
            ranks["\(parts[0]) \(parts[1])"] = rank
            rank += 1
        }
        self.ranks = ranks
        struct Config: Decodable {
            struct Added: Decodable { let content: String }
            let added_tokens_decoder: [String: Added]?
        }
        let config = try JSONDecoder().decode(Config.self, from: Data(contentsOf: configURL))
        added = (config.added_tokens_decoder ?? [:])
            .compactMap { key, value in Int(key).map { (value.content, $0) } }
            .sorted { $0.0.count > $1.0.count }
        split = try NSRegularExpression(pattern: Self.pattern)
    }

    public func encode(text: String) -> [Int] {
        let text = text.precomposedStringWithCanonicalMapping
        var ids: [Int] = []
        var rest = Substring(text)
        while !rest.isEmpty {
            // Le jeton ajouté le plus proche (le plus long à égalité).
            var next: (range: Range<Substring.Index>, id: Int)?
            for (content, id) in added {
                if let r = rest.range(of: content), r.lowerBound < (next?.range.lowerBound ?? rest.endIndex) {
                    next = (r, id)
                }
            }
            guard let found = next else {
                ids += encodeOrdinary(String(rest))
                break
            }
            ids += encodeOrdinary(String(rest[..<found.range.lowerBound]))
            ids.append(found.id)
            rest = rest[found.range.upperBound...]
        }
        return ids
    }

    private func encodeOrdinary(_ text: String) -> [Int] {
        guard !text.isEmpty else { return [] }
        let ns = text as NSString
        var ids: [Int] = []
        for match in split.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            ids += encodePiece(ns.substring(with: match.range))
        }
        return ids
    }

    private func encodePiece(_ piece: String) -> [Int] {
        if let hit = lock.withLock({ cache[piece] }) { return hit }
        let symbols = piece.utf8.map { String(Self.byteEncoder[Int($0)]) }
        let ids = bpe(symbols).compactMap { vocab[$0] }
        lock.withLock { cache[piece] = ids }
        return ids
    }

    /// Fusionne la paire de plus petit rang, toutes ses occurrences de gauche
    /// à droite, jusqu'à ce qu'aucune paire ne soit connue (GPT-2).
    private func bpe(_ symbols: [String]) -> [String] {
        var parts = symbols
        while parts.count > 1 {
            var best: (rank: Int, first: String, second: String)?
            for i in 0..<(parts.count - 1) {
                if let r = ranks["\(parts[i]) \(parts[i + 1])"], r < (best?.rank ?? Int.max) {
                    best = (r, parts[i], parts[i + 1])
                }
            }
            guard let best else { break }
            var merged: [String] = []
            merged.reserveCapacity(parts.count)
            var i = 0
            while i < parts.count {
                if i < parts.count - 1, parts[i] == best.first, parts[i + 1] == best.second {
                    merged.append(best.first + best.second)
                    i += 2
                } else {
                    merged.append(parts[i])
                    i += 1
                }
            }
            parts = merged
        }
        return parts
    }
}
```

(si le compilateur Swift 5 du framework refuse `lock.withLock` avec valeur de retour, écrire `lock.lock(); defer { lock.unlock() }`.)

- [ ] **Step 6 : UIBUILD** — attendu : compile. Les tests tournent sur la CI ; en local, on ne lance jamais `xcodebuild test`. Pour gagner une boucle, l'implémenteur **peut** vérifier hors Xcode la même logique avec un petit script Swift en ligne de commande (`swift -e` n'importe pas le framework : copier la classe dans un fichier temporaire hors du dépôt, avec un `main` qui compare à `qwen3-bpe.json`) ; ne rien ajouter au dépôt pour cela.
- [ ] **Step 7 : commit** `feat(macos): un encodeur BPE de Qwen2 à nous (sans swift-transformers), vérifié jeton pour jeton contre transformers sur 25 phrases françaises et anglaises`

---

### Task 5 : Copie de Qwen3-TTS dans `KokoroMLX`, défauts du port corrigés

**Files:**
- Create: `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/{Qwen3TTS,Qwen3TTSTalker,Qwen3TTSCodePredictor,Qwen3TTSConfig,Qwen3TTSSpeechTokenizer}.swift` (copiés, taillés)
- Create: `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/EuclideanCodebook.swift` (copié de `mas/Sources/MLXAudioCodecs/Mimi/Quantization.swift:5-54`)
- Create: `macos/Vendor/KokoroMLX/TTS/KVCache.swift` (copié de `mlm/Libraries/MLXLMCommon/KVCache.swift:408-497`, plus le protocole)
- Create: `macos/Vendor/KokoroMLX/TTS/StableSeed.swift`, `macos/Vendor/KokoroMLX/TTS/Qwen3TTSRunner.swift` (ajouts de Sillon)
- Create: `patches/qwen3tts-mlx.patch`
- Modify: `macos/project.yml` (la voix `assets/voices/qwen3-06b-siwis.safetensors` en ressource de `KokoroMLX`)
- Modify: `macos/Vendor/KokoroMLX/ORIGINE.md`, `NOTICE` (KVCache : deux fichiers dérivés de mlx-swift-lm)
- Test: `macos/Tests/Qwen3TTSRunnerTests.swift`, `macos/Tests/Qwen3TTSSmokeTests.swift`

**Interfaces:**
- Consumes: `Qwen2BPETokenizer` (Tâche 4), `assets/voices/qwen3-06b-siwis.safetensors` (Tâche 3), `BaseConfiguration`, `computeMelSpectrogram` non requis.
- Produces (publics, module `KokoroMLX`) :
  - `public enum StableSeed { public static func of(_ text: String) -> UInt64 }` (FNV-1a 64 bits sur l'UTF-8 NFC).
  - `public final class Qwen3TTSRunner` : `public static let sampleRate = 24_000` ; `public var sampleRate: Int` ; `public let voices: [String]` ; `public static func checkFiles(_ directory: URL) throws` ; `public static func load(directory: URL) throws -> Qwen3TTSRunner` ; `public func synthesize(text: String, language: String, voice: String, shouldStop: @escaping () -> Bool) throws -> [Float]?` (`nil` : arrêtée en route) ; `public static func languageName(_ code: String) -> String`.
  - `public enum Qwen3TTSError: Error, LocalizedError { case missingFile(String), modelNotInitialized(String), invalidInput(String), unknownVoice(String), stopped }`.
  - Interne, testé : `Qwen3TTSModel.effectiveLanguage(requested: String, dialect: String?) -> String`.

- [ ] **Step 1 : tests qui échouent** — `macos/Tests/Qwen3TTSRunnerTests.swift` :

```swift
import XCTest
@testable import KokoroMLX

/// La porte de Qwen3-TTS, sans modèle : langue, dialecte, graine, et un
/// chargeur qui ne touche jamais au dossier du modèle.
final class Qwen3TTSRunnerTests: XCTestCase {
    func testLanguagesAreThoseOfTheCodec() {
        XCTAssertEqual(Qwen3TTSRunner.languageName("fr"), "french")
        XCTAssertEqual(Qwen3TTSRunner.languageName("fr-FR"), "french")
        XCTAssertEqual(Qwen3TTSRunner.languageName("en"), "english")
        XCTAssertEqual(Qwen3TTSRunner.languageName("de"), "auto")
    }

    /// Bogue du port (relevé §1.f) : le dialecte d'Eric ou de Dylan ne
    /// remplace la langue que pour `chinese` ou `auto`, comme l'officiel.
    func testADialectSpeakerKeepsTheRequestedLanguage() {
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "french", dialect: "sichuan_dialect"), "french")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "english", dialect: "beijing_dialect"), "english")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "chinese", dialect: "sichuan_dialect"), "sichuan_dialect")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "auto", dialect: "beijing_dialect"), "beijing_dialect")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "french", dialect: nil), "french")
    }

    /// Même phrase, même graine, d'un lancement à l'autre (`hashValue`
    /// change à chaque processus : interdit ici).
    func testTheSeedIsStableAndIgnoresUnicodeForm() {
        XCTAssertEqual(StableSeed.of("Bonjour."), 7_611_860_131_407_550_282)
        XCTAssertEqual(StableSeed.of(""), 14_695_981_039_346_656_037)
        XCTAssertEqual(StableSeed.of("e\u{301}"), StableSeed.of("é"))
        XCTAssertEqual(StableSeed.of("é"), 775_207_407_765_167_617)
    }

    /// Relevé §1.e : un `speech_tokenizer` qui n'est pas un dossier faisait
    /// effacer le dossier du modèle. Ici : refusé avant toute lecture, rien
    /// d'écrit, rien d'effacé.
    func testAMalformedFolderIsRefusedAndLeftUntouched() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("qwen3-intact-\(UUID())")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        for name in ["config.json", "model.safetensors", "vocab.json", "merges.txt", "tokenizer_config.json", "speech_tokenizer"] {
            try Data("x".utf8).write(to: dir.appendingPathComponent(name))
        }
        let before = try FileManager.default.contentsOfDirectory(atPath: dir.path).sorted()
        XCTAssertThrowsError(try Qwen3TTSRunner.load(directory: dir))
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.path), "le dossier n'est jamais effacé")
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: dir.path).sorted(), before,
                       "pas de tokenizer.json écrit")
    }

    func testTheFixedFrenchVoiceShipsInTheFramework() {
        XCTAssertNotNil(Bundle(for: Qwen3TTSRunner.self).url(forResource: "qwen3-06b-siwis", withExtension: "safetensors"))
    }
}
```

`macos/Tests/Qwen3TTSSmokeTests.swift` :

```swift
import XCTest
import KokoroMLX

/// Fumée de Qwen3-TTS copié dans le framework : le vrai modèle (MLX, Metal),
/// seulement avec `SILLON_TTS_MODELS` (dossier contenant
/// `qwen3_tts_06b_base_8bit/` et `qwen3_tts_17b_customvoice_8bit/`, copie
/// de ~/Library/Application Support/sillon/models ;
/// `TEST_RUNNER_SILLON_TTS_MODELS` pour xcodebuild). Sans lui : ignoré.
final class Qwen3TTSSmokeTests: XCTestCase {
    func runner(_ id: String) throws -> Qwen3TTSRunner {
        guard let root = ProcessInfo.processInfo.environment["SILLON_TTS_MODELS"] else {
            throw XCTSkip("SILLON_TTS_MODELS absent")
        }
        let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
        guard FileManager.default.fileExists(atPath: dir.path) else { throw XCTSkip("\(id) absent") }
        return try Qwen3TTSRunner.load(directory: dir)
    }

    func check(_ samples: [Float]?, file: StaticString = #filePath, line: UInt = #line) {
        guard let samples else { return XCTFail("arrêté sans raison", file: file, line: line) }
        let seconds = Double(samples.count) / Double(Qwen3TTSRunner.sampleRate)
        XCTAssertTrue((0.3...6).contains(seconds), "\(seconds) s", file: file, line: line)
        XCTAssertTrue(samples.allSatisfy(\.isFinite), file: file, line: line)
    }

    func testTheFixedFrenchVoiceSaysHello() throws {
        let r = try runner("qwen3_tts_06b_base_8bit")
        XCTAssertEqual(r.voices, ["siwis"])
        check(try r.synthesize(text: "Bonjour.", language: "fr", voice: "siwis", shouldStop: { false }))
    }

    func testCustomVoiceSaysHelloAndStopsWhenAsked() throws {
        let r = try runner("qwen3_tts_17b_customvoice_8bit")
        XCTAssertEqual(r.voices.count, 9)
        check(try r.synthesize(text: "Bonjour.", language: "fr", voice: "vivian", shouldStop: { false }))
        XCTAssertNil(try r.synthesize(text: "Une phrase qui ne finira pas.", language: "fr", voice: "eric",
                                      shouldStop: { true }))
        XCTAssertThrowsError(try r.synthesize(text: "x", language: "fr", voice: "inconnue", shouldStop: { false }))
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec (types inconnus).

- [ ] **Step 3 : CLONE** (mas, mlm). Copier `Q/{Qwen3TTS,Qwen3TTSTalker,Qwen3TTSCodePredictor,Qwen3TTSConfig,Qwen3TTSSpeechTokenizer}.swift` dans `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/`. **Ne pas** copier `Qwen3TTSSpeakerEncoder.swift` (le x-vector est précalculé). Copier `EuclideanCodebook` seul (`mas/Sources/MLXAudioCodecs/Mimi/Quantization.swift:5-54`) dans `TTS/Qwen3TTS/EuclideanCodebook.swift` (rien d'autre de Mimi : conflits de noms, relevé §1.a). Écrire `TTS/KVCache.swift` : en tête, le protocole

```swift
import Foundation
@preconcurrency import MLX

/// Le sous-ensemble du cache KV de mlx-swift-lm 3.32.3
/// (`Libraries/MLXLMCommon/KVCache.swift`, MIT, ml-explore) que lit
/// Qwen3-TTS : `offset`, `update`, `trim` (relevé §1.c).
public protocol KVCache: AnyObject {
    var offset: Int { get }
    func update(keys: MLXArray, values: MLXArray) -> (MLXArray, MLXArray)
    @discardableResult func trim(_ n: Int) -> Int
}
```

puis la classe `KVCacheSimple` recopiée de `mlm/Libraries/MLXLMCommon/KVCache.swift:408-497` (allocation par pas de 256), conformée à ce protocole, sans ce qui sert aux autres caches (états, méta-données) si cela ne compile pas sans le reste de MLXLMCommon.

- [ ] **Step 4 : tailler les copies** (relevé §1.c, ligne à ligne) :
  - imports : retirer `MLXLMCommon`, `MLXAudioCore`, `MLXAudioCodecs`, `HuggingFace`, `Tokenizers` partout ; garder `MLX`, `MLXNN`, `Foundation`.
  - `Qwen3TTSConfig.swift` : `ropeScaling` **est lu** (`mropeSection`, utilisé par `Qwen3TTSTalker.swift:272` ; le relevé dit le contraire) : remplacer `[String: StringOrNumber]?` par une petite structure locale, mêmes valeurs dans les deux dépôts (`mrope_section: [24, 20, 20]`) :

```swift
/// `rope_scaling` : seul `mrope_section` est lu (les autres clés sont
/// ignorées par le décodage).
struct Qwen3RopeScaling: Codable, Sendable {
    var mropeSection: [Int]?
    enum CodingKeys: String, CodingKey { case mropeSection = "mrope_section" }
}
```

    et `var mropeSection: [Int]? { ropeScaling?.mropeSection }` (talker et prédicteur de codes). `BaseConfiguration.Quantization`/`.PerLayerQuantization` : déjà dans `STT/BaseConfiguration.swift`.
  - `Qwen3TTS.swift` : retirer la conformité `SpeechGenerationModel`, `defaultGenerationParameters`, `generate(…)`, `generateStream`, `AudioGenerationInfo`, `SendingBox`, tout `GenerateParameters` (`:30-37, 59, 88, 107, 141, 167, 181, 653-666`), `fromPretrained` (`:1124-1136`), `generateTokenizerJson` (`:1178-1200`, `:1285-1372`), le clonage (ICL, `prepareReferenceConditioning`, `extractSpeakerEmbedding`, `speakerEncoder`, cache de référence), et tous les `print` (`:275, 278, 924, 926, 934, 961, 1193, 1237, 1243, 1277`). `var tokenizer: Qwen2BPETokenizer?`. Remplacer `AudioGenerationError` par :

```swift
/// Les erreurs de la copie de Qwen3-TTS (à la place de celles de
/// MLXAudioCore).
public enum Qwen3TTSError: Error, LocalizedError {
    case missingFile(String)
    case modelNotInitialized(String)
    case invalidInput(String)
    case unknownVoice(String)
    /// `shouldStop` a dit vrai en route : pas un échec.
    case stopped

    public var errorDescription: String? {
        switch self {
        case .missingFile(let f): return "Qwen3-TTS : fichier manquant \(f)"
        case .modelNotInitialized(let m): return "Qwen3-TTS : modèle incomplet (\(m))"
        case .invalidInput(let m): return "Qwen3-TTS : entrée invalide (\(m))"
        case .unknownVoice(let v): return "Qwen3-TTS : voix inconnue « \(v) »"
        case .stopped: return "Qwen3-TTS : synthèse arrêtée"
        }
    }
}
```

  - `fromModelDirectory(_:)` devient **synchrone** (`throws`) et ne lit que `model.safetensors` (pas « tous les `*.safetensors` ») ; tokenizer : `model.tokenizer = try Qwen2BPETokenizer(directory: modelDir)` ; codec : `try loadSpeechTokenizer(model:path:)` si `speech_tokenizer` est un dossier, sinon `throw Qwen3TTSError.missingFile("speech_tokenizer/")` — **supprimer** la branche `try? fm.removeItem(at: modelDir)` (`:1214-1221`) et la génération de `tokenizer.json`. Dans `loadSpeechTokenizer`, ne lire que `speech_tokenizer/model.safetensors`, et filtrer les clés `encoder_model.*` avant `update(verify: .all)` (relevé §1.c, retrait de l'encodeur) ; dans `Qwen3TTSSpeechTokenizer.init` (`:1044-1055`) ne pas créer `encoderModel`, et retirer l'encodeur (`:794-887`) ; `hasEncoder` rend `false`.
  - Après `eval(model.talker.parameters())` : l'embedding texte (622 Mo en BF16) passe en 8 bits, comme « allégé » au banc (relevé §1.g ; `QuantizedEmbedding` hérite d'`Embedding` en MLXNN 0.32.3, vérifié) :

```swift
        // Sillon : l'embedding texte, non quantifié dans les dépôts (622 Mo
        // en BF16), passe en 8 bits au chargement, comme au banc d'écoute.
        quantize(model: model.talker, groupSize: 64, bits: 8, mode: .affine) { path, module in
            path.hasSuffix("text_embedding") && !(module is QuantizedEmbedding)
        }
        eval(model.talker.parameters())
```

  - `prepareGenerationInputs` : ajouter le paramètre `speakerEmbedding: MLXArray? = nil` ; si `speaker == nil` et `speakerEmbedding != nil`, `speakerEmbed = speakerEmbedding!.asType(codecEmbed.dtype)` (mode x-vector de l'officiel ; même place que l'embedding `spk_id`, `:959-960`). Un locuteur CustomVoice inconnu lève `Qwen3TTSError.unknownVoice` au lieu d'avertir. Remplacer le bloc du dialecte (`:928-936`) par `languageId = langMap[Self.effectiveLanguage(requested: language.lowercased(), dialect: dialectName)]` avec :

```swift
    /// Sillon, correction du port : comme l'officiel (`modeling.py:2118-2122`),
    /// le dialecte d'un locuteur (Eric : sichuanais, Dylan : pékinois) ne
    /// remplace la langue que si elle est `chinese` ou `auto`. Le port le
    /// faisait toujours : Eric lisait le français avec l'id d'un dialecte.
    static func effectiveLanguage(requested: String, dialect: String?) -> String {
        guard let dialect, requested == "chinese" || requested == "auto" else { return requested }
        return dialect
    }
```

  - `generateVoiceDesign` : retirer `conditioning`, `refAudio`, `refText`, `streamingInterval`, `onToken`, `onInfo`, `onAudioChunk` et le flux (`:216-233` reste : `streamingDecode` par paquets de 300 trames pour la phrase entière) ; ajouter `speakerEmbedding: MLXArray? = nil` (passé à `prepareGenerationInputs`) et `shouldStop: (() -> Bool)? = nil`. Remplacer les trois `try Task.checkCancellation()` (`:415, 514, 532`) par : dans la boucle, `if step % 12 == 0, shouldStop?() == true { throw Qwen3TTSError.stopped }` (≈ 1 s de son, une synchronisation `item` par trame existe déjà) ; après la boucle, `if shouldStop?() == true { throw Qwen3TTSError.stopped }` avant le décodage.
  - `Qwen3TTSSpeechTokenizer.swift:973, 1013` : si `-> sending` ne compile pas en Swift 5, l'enlever.
- [ ] **Step 5 : écrire `TTS/StableSeed.swift`.**

```swift
import Foundation

/// Une graine par phrase, la même d'un lancement à l'autre : une phrase
/// resynthétisée (saut, cache vidé) sonne pareil. FNV-1a 64 bits sur
/// l'UTF-8 de la forme NFC (`hashValue` change à chaque processus).
/// Ajout de Sillon.
public enum StableSeed {
    public static func of(_ text: String) -> UInt64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in text.precomposedStringWithCanonicalMapping.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return hash
    }
}
```

- [ ] **Step 6 : écrire `TTS/Qwen3TTSRunner.swift`.**

```swift
import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon pour Qwen3-TTS, comme `KokoroRunner` pour Kokoro :
/// l'app ne lie pas MLX. Ajout de Sillon, hors des fichiers repris. Tout
/// s'appelle sous `MLXGate` (dans l'app) : une synthèse à la fois.
public final class Qwen3TTSRunner: @unchecked Sendable {
    public static let sampleRate = 24_000
    public var sampleRate: Int { Self.sampleRate }

    private let model: Qwen3TTSModel
    /// Base : le x-vector figé de la voix « siwis ». CustomVoice : `nil`.
    private let speakerEmbedding: MLXArray?
    /// Les voix acceptées : `siwis` (Base), ou les locuteurs de `spk_id`.
    public let voices: [String]

    /// Les fichiers que le chargement lit (relevé §1.e). Refusé avant toute
    /// lecture s'il en manque un : rien n'est créé, modifié ni effacé dans
    /// `directory` (le catalogue Rust en est seul maître).
    public static func checkFiles(_ directory: URL) throws {
        let names = ["config.json", "model.safetensors", "vocab.json", "merges.txt", "tokenizer_config.json",
                     "speech_tokenizer/config.json", "speech_tokenizer/model.safetensors"]
        for name in names {
            var isDirectory: ObjCBool = false
            let path = directory.appendingPathComponent(name).path
            guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), !isDirectory.boolValue else {
                throw Qwen3TTSError.missingFile(name)
            }
        }
    }

    public static func load(directory: URL) throws -> Qwen3TTSRunner {
        try checkFiles(directory)
        let model = try Qwen3TTSModel.fromModelDirectory(directory)
        guard let talker = model.config.talkerConfig else {
            throw Qwen3TTSError.modelNotInitialized("talker_config")
        }
        if model.config.ttsModelType == "base" {
            guard let url = Bundle(for: Qwen3TTSRunner.self).url(forResource: "qwen3-06b-siwis",
                                                                 withExtension: "safetensors") else {
                throw Qwen3TTSError.missingFile("qwen3-06b-siwis.safetensors")
            }
            guard let vector = try MLX.loadArrays(url: url)["speaker_embedding"],
                  vector.dim(-1) == talker.hiddenSize else {
                throw Qwen3TTSError.unknownVoice("siwis")
            }
            return Qwen3TTSRunner(model: model, speakerEmbedding: vector.reshaped([1, -1]), voices: ["siwis"])
        }
        let speakers = (talker.spkId ?? [:]).keys.sorted()
        guard !speakers.isEmpty else { throw Qwen3TTSError.modelNotInitialized("spk_id vide") }
        return Qwen3TTSRunner(model: model, speakerEmbedding: nil, voices: speakers)
    }

    private init(model: Qwen3TTSModel, speakerEmbedding: MLXArray?, voices: [String]) {
        self.model = model
        self.speakerEmbedding = speakerEmbedding
        self.voices = voices
    }

    /// `fr` → `french`, `en` → `english` (clés de `codec_language_id`) ;
    /// une autre langue : `auto`.
    public static func languageName(_ code: String) -> String {
        switch code.prefix(2).lowercased() {
        case "fr": return "french"
        case "en": return "english"
        default: return "auto"
        }
    }

    /// Une phrase entière, 24 kHz mono. `voice` vide : la première voix.
    /// `nil` : `shouldStop` a dit vrai en route. Réglages de
    /// `generation_config.json` : température 0,9, top-k 50, top-p 1,
    /// pénalité 1,05 ; graine tirée de la phrase.
    public func synthesize(text: String, language: String, voice: String,
                           shouldStop: @escaping () -> Bool) throws -> [Float]? {
        var speaker: String?
        if speakerEmbedding == nil {
            let name = voice.isEmpty ? voices[0] : voice.lowercased()
            guard voices.contains(name) else { throw Qwen3TTSError.unknownVoice(voice) }
            speaker = name
        }
        MLXRandom.seed(StableSeed.of(text))
        do {
            let audio = try model.generateVoiceDesign(
                text: text, instruct: speaker, language: Self.languageName(language),
                temperature: 0.9, topK: 50, topP: 1.0, repetitionPenalty: 1.05, minP: 0.0,
                maxTokens: 4096, speakerEmbedding: speakerEmbedding, shouldStop: shouldStop)
            eval(audio)
            return audio.reshaped([-1]).asType(.float32).asArray(Float.self)
        } catch Qwen3TTSError.stopped {
            return nil
        }
    }
}
```

(adapter les labels de `generateVoiceDesign` à la signature taillée au Step 4 ; `MLXRandom.seed` est un espace de noms de `MLX` en 0.32.3, relevé du lot 2 — sinon `MLX.seed`.)

- [ ] **Step 7 : `macos/project.yml`** — dans `targets.KokoroMLX.sources`, ajouter :

```yaml
      # La voix française fixe de Qwen3 0.6B (Tâche 3 du lot 3).
      - path: ../assets/voices/qwen3-06b-siwis.safetensors
        buildPhase: resources
```

- [ ] **Step 8 : vérifier le dossier.** `grep -rn "removeItem\|write(to\|createFile\|print(" macos/Vendor/KokoroMLX/TTS/` — attendu : aucune ligne. `grep -rln "import MLXLMCommon\|import Tokenizers\|import HuggingFace\|import MLXAudio" macos/Vendor/KokoroMLX/TTS/` — attendu : aucune.
- [ ] **Step 9 : UIBUILD** — attendu : compile.
- [ ] **Step 10 : traces.** `ORIGINE.md` : nouvelle section « TTS : Qwen3-TTS » (fichiers, commit `244458d`, chaque changement du Step 4 avec sa raison, `EuclideanCodebook` de Mimi, `KVCache` de mlx-swift-lm 3.32.3, ajouts de Sillon `Qwen2BPETokenizer`, `StableSeed`, `Qwen3TTSRunner`, la correction du dialecte, la quantification de l'embedding texte, `\p{N}` contre `{1,3}` tel que tranché à la Tâche 4). `patches/qwen3tts-mlx.patch` : `diff -ruN a b` où `a/` = les fichiers d'origine rangés comme ici (les 5 de `Q`, et `EuclideanCodebook` extrait des lignes 5-54), `b/` = `macos/Vendor/KokoroMLX/TTS/Qwen3TTS/` ; la recette dans `ORIGINE.md`, comme pour Kokoro. `NOTICE` : « un fichier dérivé de mlx-swift-lm » devient « deux fichiers dérivés de mlx-swift-lm 3.32.3 (…/STT/BaseConfiguration.swift, …/TTS/KVCache.swift) ».
- [ ] **Step 11 : commit** `feat(macos): Qwen3-TTS copié de mlx-audio-swift dans le framework MLX — encodeur BPE à nous, voix française fixe en x-vector, dialecte d'Eric et Dylan seulement en chinois, un chargeur qui n'écrit ni n'efface rien`

---

### Task 6 : Copie de MagpieTTS (français, et l'anglais avec son dictionnaire)

**Files:**
- Create: `macos/Vendor/KokoroMLX/TTS/Magpie/{Configuration,Transformer,TextEncoder,Decoder,LocalTransformer,NanoCodec,Sampling,WeightLoader,SubVocab,ByT5Encoder,Tokenizer,MagpieTTS,EnglishG2P}.swift` (copiés de `M`, taillés)
- Create: `macos/Vendor/KokoroMLX/TTS/Magpie/Resources/cmudict_ipa_en.txt` (copié de `M/Resources/`, 3,1 Mo)
- Create: `macos/Vendor/KokoroMLX/TTS/MagpieRunner.swift` (ajout de Sillon)
- Create: `third_party/licences/speech-swift-LICENSE` (copie de `ss/LICENSE`), `third_party/licences/cmudict-LICENSE` (l'en-tête de licence de `cmudict_ipa_en.txt`, lignes `;;;` du début, sans le préfixe)
- Create: `patches/magpie-mlx.patch`
- Modify: `macos/project.yml` (`joindre` des deux licences), `scripts/build-macos.sh` (liste `licences`), `NOTICE`, `macos/Vendor/KokoroMLX/ORIGINE.md`
- Test: `macos/Tests/MagpieRunnerTests.swift`, `macos/Tests/MagpieSmokeTests.swift`

**Interfaces:**
- Consumes: `StableSeed` (Tâche 5).
- Produces (publics, module `KokoroMLX`) : `public final class MagpieRunner` : `public var sampleRate: Int` (22 050) ; `public static func load(directory: URL) throws -> MagpieRunner` ; `public func synthesize(text: String, language: String, voice: String, shouldStop: @escaping () -> Bool) throws -> [Float]?` ; `public static func prepare(_ text: String) -> String` (NFC) ; `public static func maxSteps(textBytes: Int, maxLen: Int, bakedT: Int) -> Int` ; `MagpieTTS.load(directory: URL) throws -> MagpieTTS`.

- [ ] **Step 1 : tests qui échouent** — `macos/Tests/MagpieRunnerTests.swift` :

```swift
import XCTest
@testable import KokoroMLX

/// La porte de Magpie, sans modèle.
final class MagpieRunnerTests: XCTestCase {
    /// Relevé §3.c : 500 trames (≈ 23 s) coupaient une phrase longue. La
    /// borne suit le texte, sans dépasser le contexte du décodeur.
    func testMaxStepsGrowsWithTheText() {
        XCTAssertEqual(MagpieRunner.maxSteps(textBytes: 10, maxLen: 2048, bakedT: 110), 90)
        XCTAssertGreaterThan(MagpieRunner.maxSteps(textBytes: 400, maxLen: 2048, bakedT: 110), 500,
                             "400 octets ≈ 30 s de parole : plus que les 500 trames d'origine")
        XCTAssertEqual(MagpieRunner.maxSteps(textBytes: 10_000, maxLen: 2048, bakedT: 110), 2048 - 110 - 1)
    }

    /// Un « é » décomposé (PDF) donnerait deux octets inattendus en byT5.
    func testFrenchIsComposedBeforeItsBytes() {
        let nfd = MagpieRunner.prepare("E\u{301}te\u{301}").lowercased()
        XCTAssertEqual(MagpieByT5Encoder.encode(nfd, language: .french),
                       MagpieByT5Encoder.encode("été", language: .french))
    }

    func testTheFiveVoicesAreKnownByTheirCatalogIds() {
        for id in ["sofia", "aria", "jason", "leo", "john_van_stan"] {
            XCTAssertNotNil(MagpieSpeaker(named: id), id)
        }
        XCTAssertNil(MagpieSpeaker(named: "vivian"))
    }

    func testTheEnglishDictionaryShipsInTheFramework() {
        XCTAssertNotNil(Bundle(for: MagpieRunner.self).url(forResource: "cmudict_ipa_en", withExtension: "txt"))
    }
}
```

`macos/Tests/MagpieSmokeTests.swift` :

```swift
import XCTest
import KokoroMLX

/// Fumée de Magpie copié dans le framework : le vrai modèle, seulement avec
/// `SILLON_TTS_MODELS` (dossier contenant `magpie_tts_357m_8bit/`).
final class MagpieSmokeTests: XCTestCase {
    func runner() throws -> MagpieRunner {
        guard let root = ProcessInfo.processInfo.environment["SILLON_TTS_MODELS"] else {
            throw XCTSkip("SILLON_TTS_MODELS absent")
        }
        let dir = URL(fileURLWithPath: root).appendingPathComponent("magpie_tts_357m_8bit")
        guard FileManager.default.fileExists(atPath: dir.path) else { throw XCTSkip("magpie absent") }
        return try MagpieRunner.load(directory: dir)
    }

    func testSaysHelloInFrenchAndEnglish() throws {
        let r = try runner()
        for (text, language) in [("Bonjour.", "fr"), ("Hello.", "en")] {
            let samples = try XCTUnwrap(try r.synthesize(text: text, language: language, voice: "sofia", shouldStop: { false }))
            let seconds = Double(samples.count) / Double(r.sampleRate)
            XCTAssertTrue((0.2...5).contains(seconds), "\(text) : \(seconds) s")
            XCTAssertTrue(samples.allSatisfy(\.isFinite))
        }
        XCTAssertNil(try r.synthesize(text: "Une phrase arrêtée.", language: "fr", voice: "leo", shouldStop: { true }))
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec.

- [ ] **Step 3 : CLONE** (ss, au commit `28aef99`). Copier les 13 fichiers listés (relevé §3.a), **pas** `Downloader.swift`, `ChineseG2P.swift`, `JapaneseG2P.swift`, ni les dictionnaires autres que `cmudict_ipa_en.txt`. Tailler :
  - `import MLXRandom` → retirer (espace de noms de `MLX` en 0.32.3 ; aucune dépendance ajoutée à `project.yml`) ; `import AudioCommon`, `import NaturalLanguage` → retirer.
  - `Sampling.swift` : `topKFilter`, `forbidIds`, `sampleTopK` passent de `public` à `internal` (relevé §3.a, collisions).
  - `MagpieTTS.swift` : retirer `fromPretrained`, `synthesizeStream`, `runStreaming` (le flux rejoue le codec, RTF 0,93 au lieu de 0,24) ; `load(from paths: MagpieTTSDownloader.Paths)` devient `load(directory: URL)` qui lit `text_encoder/`, `decoder_prefill/`, `nanocodec_decoder/`, `tokenizer/` sous `directory` ; retirer le bloc `MAGPIE_DEBUG` (`:197-199`) ; `synthesize(…)` et `sampleCodeSequence(…)` prennent `shouldStop: (() -> Bool)? = nil`, consulté `if step % 20 == 0, shouldStop?() == true { throw MagpieTTSError.stopped }` dans la boucle de `sampleCodeSequence` et une fois avant le codec.
  - `Configuration.swift` : `MagpieTTSError` gagne `case stopped` (« Magpie : synthèse arrêtée ») ; `MagpieTTSVariant` et ce qui ne sert qu'au téléchargement : retirer.
  - `Tokenizer.swift` : retirer les cas `.chinese`, `.japanese` (G2P non copiés) — ils tombent sur `charLookup(text)` ; garder anglais (dictionnaire), espagnol et allemand peuvent rester sur `charLookup` (dictionnaires non copiés) ; `.french` : `MagpieByT5Encoder.encode(MagpieRunner.prepare(text).lowercased(), language: .french)`.
  - `EnglishG2P.swift` : `Bundle.module` → `Bundle(for: MagpieRunner.self)` ; `spanish`/`german` : retirer.
- [ ] **Step 4 : écrire `TTS/MagpieRunner.swift`.**

```swift
import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon pour MagpieTTS (speech-swift, Apache-2.0). Ajout de
/// Sillon, hors des fichiers repris. Sous `MLXGate` dans l'app : le cache
/// des vocabulaires et la graine de MLX sont partagés.
public final class MagpieRunner: @unchecked Sendable {
    private let model: MagpieTTS
    public var sampleRate: Int { MagpieTTS.sampleRate }

    public static func load(directory: URL) throws -> MagpieRunner {
        MagpieRunner(model: try MagpieTTS.load(directory: directory))
    }

    private init(model: MagpieTTS) {
        self.model = model
    }

    /// NFC : en byT5, un « é » décomposé ferait deux octets inattendus.
    public static func prepare(_ text: String) -> String {
        text.precomposedStringWithCanonicalMapping
    }

    /// Trames permises : 4 par octet de texte, plus 50, sans dépasser le
    /// contexte du décodeur (`max_len` moins les trames du locuteur et BOS).
    /// L'amont coupait à 500 trames (≈ 23 s).
    public static func maxSteps(textBytes: Int, maxLen: Int, bakedT: Int) -> Int {
        min(maxLen - bakedT - 1, 4 * textBytes + 50)
    }

    /// Une phrase entière, 22 050 Hz mono. `voice` vide : sofia. `nil` :
    /// arrêtée en route. Échantillonnage d'origine (température 0,6,
    /// top-k 80), graine tirée de la phrase.
    public func synthesize(text: String, language: String, voice: String,
                           shouldStop: @escaping () -> Bool) throws -> [Float]? {
        let speaker: MagpieSpeaker
        if voice.isEmpty {
            speaker = .sofia
        } else if let named = MagpieSpeaker(named: voice) {
            speaker = named
        } else {
            throw MagpieTTSError.invalidConfig("voix inconnue « \(voice) »")
        }
        let lang: MagpieLanguage = language.prefix(2).lowercased() == "en" ? .english : .french
        let text = Self.prepare(text)
        var params = MagpieTTSParams()
        params.seed = StableSeed.of(text)
        params.maxSteps = Self.maxSteps(textBytes: text.utf8.count,
                                        maxLen: model.decoderConfig.maxLen,
                                        bakedT: model.decoderConfig.bakedT)
        do {
            return try model.synthesize(text: text, speaker: speaker, language: lang,
                                        params: params, shouldStop: shouldStop)
        } catch MagpieTTSError.stopped {
            return nil
        }
    }
}
```

- [ ] **Step 5 : licences.** `third_party/licences/speech-swift-LICENSE` = `ss/LICENSE` (Apache-2.0 ; pas de `NOTICE` à la racine de speech-swift, relevé §3.e). `third_party/licences/cmudict-LICENSE` = l'en-tête de `cmudict_ipa_en.txt` (CMUdict 0.07, Carnegie Mellon University, licence de type BSD), précédé d'une ligne « Dictionnaire IPA anglais de NVIDIA NeMo (cmudict_ipa_en.txt), dérivé de CMUdict 0.07 : ». `macos/project.yml`, script « Joindre NOTICE et les licences » :

```sh
          # MagpieTTS (Vendor/KokoroMLX/TTS/Magpie) vient de speech-swift.
          joindre "$SRCROOT/../third_party/licences/speech-swift-LICENSE" speech-swift-LICENSE
          joindre "$SRCROOT/../third_party/licences/cmudict-LICENSE" cmudict-LICENSE
```

`scripts/build-macos.sh` : ajouter `speech-swift-LICENSE cmudict-LICENSE` à `licences=(…)`. `NOTICE`, dans la liste de l'app :

```
- des fichiers de speech-swift (macos/Vendor/KokoroMLX/TTS/Magpie, commit
  28aef99510e438bcc5014df2582820d9fdba80e0), licence Apache 2.0, soniqo —
  speech-swift-LICENSE ; les changements sont décrits dans ORIGINE.md et
  patches/magpie-mlx.patch ;
- le dictionnaire IPA anglais de NVIDIA NeMo (cmudict_ipa_en.txt), dérivé
  de CMUdict 0.07, Copyright (C) 1993-2015 Carnegie Mellon University,
  licence de type BSD — cmudict-LICENSE ;
```

et, après le paragraphe de Kokoro : « Les poids de MagpieTTS Multilingual 357M (NVIDIA) sont téléchargés à la demande depuis aufklarer/Magpie-TTS-Multilingual-357M-MLX-8bit (révision 260839c0a2c46993735d72082c3fba177710d477), sous NVIDIA Open Model License : Licensed by NVIDIA Corporation under the NVIDIA Open Model License. Sillon ne les livre pas. Ceux de Qwen3-TTS (Alibaba, Apache 2.0) le sont depuis mlx-community (révisions dans src/models.rs). »
- [ ] **Step 6 : vérifier.** `grep -rn "Bundle.module\|import AudioCommon\|import MLXRandom\|MAGPIE_DEBUG\|public func topKFilter" macos/Vendor/KokoroMLX/TTS/Magpie/` — attendu : rien.
- [ ] **Step 7 : UIBUILD** — attendu : compile ; puis `find macos/build/derived -path "*KokoroMLX.framework*" -name cmudict_ipa_en.txt` — attendu : un chemin (le dictionnaire est dans le framework) ; idem `qwen3-06b-siwis.safetensors`.
- [ ] **Step 8 : traces.** `ORIGINE.md` : section « TTS : MagpieTTS » (dépôt, commit `28aef99`, licence Apache-2.0 jointe, fichiers, chaque changement du Step 3 et sa raison, ajout `MagpieRunner`). `patches/magpie-mlx.patch` : `diff -ruN` de `M` (les 13 fichiers) vers `TTS/Magpie/`, recette dans `ORIGINE.md`.
- [ ] **Step 9 : commit** `feat(macos): MagpieTTS copié de speech-swift dans le framework MLX — français en octets (NFC), anglais par le dictionnaire de NeMo, phrases longues non tronquées, arrêt en route ; licences jointes`

---

### Task 7 : `MLXBudget` — les plafonds de MLX sont la somme des modèles chargés

**Files:**
- Create: `macos/Sources/Voice/MLXBudget.swift`
- Modify: `macos/Sources/Voice/KokoroMLX.swift` (`KokoroMLXCompute` inscrit et retire son besoin)
- Modify: `macos/Sources/Dictation/MLXDictationEngine.swift` (`SpeechModels.load/release/applyLimits` passent par le registre)
- Test: `macos/Tests/MLXBudgetTests.swift`

**Interfaces:**
- Produces: `final class MLXBudget: @unchecked Sendable` : `struct Need: Equatable { var memory: Int; var cache: Int }` ; `static let shared` ; `init(apply: @escaping (Int, Int) -> Void = { DictationMLX.setLimits(memory: $0, cache: $1) })` ; `func set(_ key: String, _ need: Need)` ; `func remove(_ key: String)` ; `var total: Need` ; `func applyNow()` ; `static func weightBytes(_ directory: URL) -> Int` (tous les `*.safetensors` du dossier **et de ses sous-dossiers**). Clés : `"kokoro"`, `"dictee"`, `"lecture:<dossier>"`. Appelé sous `MLXGate`.

- [ ] **Step 1 : tests qui échouent** — `macos/Tests/MLXBudgetTests.swift` :

```swift
import XCTest
@testable import Sillon

final class MLXBudgetTests: XCTestCase {
    final class Recorder: @unchecked Sendable {
        var applied: [(Int, Int)] = []
    }

    func budget() -> (MLXBudget, Recorder) {
        let r = Recorder()
        return (MLXBudget(apply: { r.applied.append(($0, $1)) }), r)
    }

    func testTheLimitsAreTheSumOfTheLoadedModels() {
        let (b, r) = budget()
        b.set("kokoro", .init(memory: 1 << 30, cache: 64 << 20))
        b.set("dictee", .init(memory: 3 << 30, cache: 256 << 20))
        XCTAssertEqual(b.total, .init(memory: 4 << 30, cache: 320 << 20))
        XCTAssertEqual(r.applied.last?.0, 4 << 30)
        b.set("kokoro", .init(memory: 2 << 30, cache: 64 << 20))
        XCTAssertEqual(b.total.memory, 5 << 30, "une clé remplace son besoin, ne l'ajoute pas deux fois")
    }

    /// Spec (« Mémoire ») et Review Focus 5 : une dictée libérée pendant
    /// que Qwen 1.7B lit ne lui impose plus 1 Go.
    func testReleasingDictationKeepsTheReadingModelsNeed() {
        let (b, r) = budget()
        b.set("lecture:/m/q17", .init(memory: 4 << 30, cache: 256 << 20))
        b.set("dictee", .init(memory: 5 << 30, cache: 256 << 20))
        b.remove("dictee")
        XCTAssertEqual(r.applied.last?.0, 4 << 30)
        XCTAssertEqual(r.applied.last?.1, 256 << 20)
    }

    func testNothingLoadedAppliesNothing() {
        let (b, r) = budget()
        b.set("kokoro", .init(memory: 1 << 30, cache: 64 << 20))
        let count = r.applied.count
        b.remove("kokoro")
        b.remove("inconnue")
        XCTAssertEqual(r.applied.count, count, "rien de chargé : plafonds laissés tels quels")
        XCTAssertEqual(b.total, .init(memory: 0, cache: 0))
    }

    func testWeightBytesCountSubfolders() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("budget-\(UUID())")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("speech_tokenizer"),
                                                withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try Data(count: 10).write(to: dir.appendingPathComponent("model.safetensors"))
        try Data(count: 5).write(to: dir.appendingPathComponent("speech_tokenizer/model.safetensors"))
        try Data(count: 7).write(to: dir.appendingPathComponent("vocab.json"))
        XCTAssertEqual(MLXBudget.weightBytes(dir), 15)
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter `macos/Sources/Voice/MLXBudget.swift`.**

```swift
import Foundation
import KokoroMLX
import os

/// Les plafonds de MLX (`Memory.memoryLimit`, `cacheLimit`) sont globaux au
/// processus. Chaque modèle chargé (Kokoro, la dictée, un modèle de lecture
/// à texte) inscrit ici son besoin ; les plafonds valent **la somme** des
/// besoins inscrits, reposée à chaque changement. Plus de plafonds remis
/// « en dur » : une dictée libérée pendant que Qwen 1.7B lit ne lui impose
/// pas 1 Go. Appelé sous `MLXGate` (les plafonds changent entre deux calculs).
final class MLXBudget: @unchecked Sendable {
    struct Need: Equatable {
        var memory: Int
        var cache: Int
    }

    static let shared = MLXBudget()

    private let lock = OSAllocatedUnfairLock()
    private var needs: [String: Need] = [:]
    private let apply: (Int, Int) -> Void

    init(apply: @escaping (Int, Int) -> Void = { DictationMLX.setLimits(memory: $0, cache: $1) }) {
        self.apply = apply
    }

    var total: Need {
        lock.withLockUnchecked {
            needs.values.reduce(Need(memory: 0, cache: 0)) {
                Need(memory: $0.memory + $1.memory, cache: $0.cache + $1.cache)
            }
        }
    }

    func set(_ key: String, _ need: Need) {
        lock.withLockUnchecked { needs[key] = need }
        applyNow()
    }

    func remove(_ key: String) {
        lock.withLockUnchecked { needs[key] = nil }
        applyNow()
    }

    /// Repose la somme ; rien de chargé : rien à poser.
    func applyNow() {
        let t = total
        guard t.memory > 0 else { return }
        apply(t.memory, t.cache)
    }

    /// Octets des poids (`*.safetensors`) du dossier et de ses sous-dossiers.
    static func weightBytes(_ directory: URL) -> Int {
        guard let files = FileManager.default.enumerator(at: directory, includingPropertiesForKeys: [.fileSizeKey]) else {
            return 0
        }
        var total = 0
        for case let url as URL in files where url.pathExtension == "safetensors" {
            total += (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        }
        return total
    }
}
```

- [ ] **Step 4 : brancher Kokoro et la dictée.**
  - `KokoroMLXCompute` : `init(…, budget: MLXBudget = .shared)` ; dans `load`, juste avant `loadForward`, `budget.set("kokoro", .init(memory: Self.memoryLimit, cache: Self.cacheLimit))` (et `budget.remove("kokoro")` si le chargement échoue) ; dans `release`, au dernier détenteur, `budget.remove("kokoro")` après `clearCache()`. `loadRunner` passe la somme : `let t = MLXBudget.shared.total; return try KokoroRunner.load(directory: directory, memoryLimit: max(t.memory, memoryLimit), cacheLimit: max(t.cache, cacheLimit))`. Mettre à jour le commentaire de `memoryLimit` (« son besoin dans `MLXBudget` »).
  - `SpeechModels` : `load` fait `MLXBudget.shared.set("dictee", .init(memory: weightBytes(directory) + memoryMargin, cache: cacheLimit))` au lieu de `DictationMLX.setLimits` ; `release()` fait `KokoroRunner.clearCache()` puis `MLXBudget.shared.remove("dictee")` (plus de `KokoroMLXCompute.memoryLimit` en dur) ; `applyLimits(memory:)` devient `MLXBudget.shared.applyNow()` (garder le nom de l'appel ou renommer en `applyLimits()` et mettre à jour les quatre adaptateurs). `weightBytes` peut appeler `MLXBudget.weightBytes`.
  - `grep -rn "setLimits" macos/Sources` — attendu : seulement le défaut de `MLXBudget.init`.
- [ ] **Step 5 : UIBUILD** — attendu : compile (dont `KokoroMLXComputeTests` et `SpeechModelRunnerTests`, inchangés).
- [ ] **Step 6 : commit** `feat(macos): les plafonds de MLX valent la somme des modèles chargés (Kokoro, dictée, lecture) ; une dictée libérée ne remet plus ceux de Kokoro en dur`

---

### Task 8 : Le calcul Swift `TextSpeechMLXCompute`, branché sur l'app

**Files:**
- Create: `macos/Sources/Voice/TextSpeechMLX.swift`
- Modify: `macos/Sources/AppDelegate.swift` (`TextSpeechMLXCompute` à la place de `UnavailableTextSpeech`, qui disparaît ; garder une référence `textCompute` comme `kokoroCompute`)
- Test: `macos/Tests/TextSpeechMLXComputeTests.swift`

**Interfaces:**
- Consumes: FFI `TextSpeechCompute`, `SpeechOutput`, `textSpeechBridgeCheck` (Tâche 2) ; `Qwen3TTSRunner` (Tâche 5) ; `MagpieRunner` (Tâche 6) ; `MLXGate`, `MLXBudget` (Tâche 7).
- Produces: `protocol TextSpeechModel: AnyObject { var sampleRate: Int { get }; func synthesize(text: String, language: String, voice: String, shouldStop: @escaping () -> Bool) throws -> [Float]? }` ; `final class TextSpeechMLXCompute: TextSpeechCompute` : `init(loader: @escaping (URL, String) throws -> TextSpeechModel = TextSpeechMLXCompute.loadModel, clearCache: @escaping () -> Void = KokoroRunner.clearCache, gate: MLXGate = .shared, budget: MLXBudget = .shared)` ; `static func need(runtime: String, weightBytes: Int) -> MLXBudget.Need` ; `func synthesize(modelDir:text:language:voice:isCancelled:) throws -> SpeechOutput` (pour la mesure).

- [ ] **Step 1 : tests qui échouent** — `macos/Tests/TextSpeechMLXComputeTests.swift` :

```swift
import XCTest
@testable import Sillon

/// Un modèle factice : jamais MLX dans les tests unitaires.
final class FakeTextModel: TextSpeechModel {
    let name: String
    let sampleRate: Int
    var stopsOnTheWay = false
    private(set) var spoken: [(String, String, String)] = []

    init(name: String, sampleRate: Int = 22_050) {
        self.name = name
        self.sampleRate = sampleRate
    }

    func synthesize(text: String, language: String, voice: String,
                    shouldStop: @escaping () -> Bool) throws -> [Float]? {
        spoken.append((text, language, voice))
        if stopsOnTheWay, shouldStop() { return nil }
        return [Float](repeating: 0.25, count: sampleRate / 10)
    }
}

final class TextSpeechMLXComputeTests: XCTestCase {
    final class Loads: @unchecked Sendable {
        var count: [String: Int] = [:]
        var models: [String: FakeTextModel] = [:]
        var cleared = 0
    }

    func compute(_ loads: Loads, budget: MLXBudget = MLXBudget(apply: { _, _ in })) -> TextSpeechMLXCompute {
        TextSpeechMLXCompute(
            loader: { url, runtime in
                guard runtime == TextSpeechMLXCompute.qwenRuntime || runtime == TextSpeechMLXCompute.magpieRuntime else {
                    throw SillonError.Failed(message: "inconnu")
                }
                loads.count[url.path, default: 0] += 1
                let model = FakeTextModel(name: url.path)
                loads.models[url.path] = model
                return model
            },
            clearCache: { loads.cleared += 1 },
            gate: MLXGate(), budget: budget)
    }

    func testLoadsOncePerDirectoryAndReleasesAtTheLastUser() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertEqual(loads.count["/m/q"], 1)
        c.release(modelDir: "/m/q")
        XCTAssertEqual(loads.cleared, 0, "un détenteur reste")
        XCTAssertNoThrow(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "vivian",
                                          cancel: SynthesisCancel.fixed(cancelled: false)))
        c.release(modelDir: "/m/q")
        XCTAssertEqual(loads.cleared, 1)
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              cancel: SynthesisCancel.fixed(cancelled: false)))
    }

    /// Review Focus 2 : la lecture et l'aperçu tiennent chacun leur modèle.
    func testTwoDirectoriesKeepTheirOwnModel() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q17", runtime: "qwen3_tts_mlx")
        try c.load(modelDir: "/m/magpie", runtime: "magpie_mlx")
        _ = try c.synthesize(modelDir: "/m/magpie", text: "aperçu", language: "fr", voice: "leo",
                             cancel: SynthesisCancel.fixed(cancelled: false))
        c.release(modelDir: "/m/magpie")
        let out = try c.synthesize(modelDir: "/m/q17", text: "lecture", language: "fr", voice: "vivian",
                                   cancel: SynthesisCancel.fixed(cancelled: false))
        XCTAssertEqual(loads.models["/m/q17"]?.spoken.map(\.0), ["lecture"])
        XCTAssertEqual(loads.models["/m/magpie"]?.spoken.map(\.0), ["aperçu"])
        XCTAssertEqual(out.sampleRate, 22_050)
    }

    func testCancelledBeforeTheRunIsCancelledNotFailed() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              cancel: SynthesisCancel.fixed(cancelled: true))) { error in
            guard case SillonError.Cancelled = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(loads.models["/m/q"]?.spoken.count, 0, "rien calculé")
    }

    func testStoppedOnTheWayIsCancelled() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "magpie_mlx")
        loads.models["/m/q"]?.stopsOnTheWay = true
        var calls = 0
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              isCancelled: { calls += 1; return calls > 1 })) { error in
            guard case SillonError.Cancelled = error else { return XCTFail("\(error)") }
        }
    }

    func testAnUnknownRuntimeFailsWithItsName() {
        let c = compute(Loads())
        XCTAssertThrowsError(try c.load(modelDir: "/m/x", runtime: "autre"))
    }

    func testTheNeedOfEachModelIsInTheBudgetWhileLoaded() throws {
        let budget = MLXBudget(apply: { _, _ in })
        let c = compute(Loads(), budget: budget)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertEqual(budget.total, TextSpeechMLXCompute.need(runtime: "qwen3_tts_mlx", weightBytes: 0))
        c.release(modelDir: "/m/q")
        XCTAssertEqual(budget.total.memory, 0)
    }

    func testNeedsFollowTheSurvey() {
        let qwen = TextSpeechMLXCompute.need(runtime: "qwen3_tts_mlx", weightBytes: 3_075_602_023)
        XCTAssertEqual(qwen.memory, 3_075_602_023 + (1 << 30), "1.7B : ≈ 4,1 Go pour un pic mesuré de 3,5–3,7 Go")
        let magpie = TextSpeechMLXCompute.need(runtime: "magpie_mlx", weightBytes: 301_517_780)
        XCTAssertEqual(magpie.memory, 4 * 301_517_780 + (512 << 20), "8 bits déquantifiés en FP32")
    }

    /// Le pont généré par UniFFI, de bout en bout, sur un modèle factice.
    func testTheComputeCrossesTheBridge() throws {
        let c = compute(Loads())
        XCTAssertEqual(try textSpeechBridgeCheck(compute: c, modelDir: "/m/q", runtime: "magpie_mlx"), 2_205)
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter `macos/Sources/Voice/TextSpeechMLX.swift`.**

```swift
import Foundation
import KokoroMLX
import os

/// Un modèle de lecture qui prend le texte : `Qwen3TTSRunner` ou
/// `MagpieRunner` (framework MLX) dans l'app, un faux dans les tests.
/// `nil` : `shouldStop` a dit vrai en route.
protocol TextSpeechModel: AnyObject {
    var sampleRate: Int { get }
    func synthesize(text: String, language: String, voice: String,
                    shouldStop: @escaping () -> Bool) throws -> [Float]?
}

extension Qwen3TTSRunner: TextSpeechModel {}
extension MagpieRunner: TextSpeechModel {}

/// Le calcul des modèles de lecture à texte (Qwen3-TTS, MagpieTTS), appelé
/// par le moteur Rust depuis ses fils (voir `TextSpeechCompute` dans
/// ffi/src/lib.rs).
///
/// Un modèle **par dossier**, compté par ses détenteurs : la lecture et
/// l'aperçu de la bibliothèque peuvent tenir chacun le leur. Chargement,
/// synthèse et libération passent sous `MLXGate` (une inférence MLX à la
/// fois dans l'app, avec Kokoro et la dictée) ; chaque modèle chargé inscrit
/// son besoin dans `MLXBudget`.
final class TextSpeechMLXCompute: TextSpeechCompute, @unchecked Sendable {
    typealias Loader = (URL, String) throws -> TextSpeechModel

    static let qwenRuntime = "qwen3_tts_mlx"
    static let magpieRuntime = "magpie_mlx"
    /// La phrase de préparation : compile `compile(shapeless:)` et les
    /// noyaux Metal (≈ 12 trames Qwen, 20 trames Magpie).
    static let warmUpText = "Bonjour."

    private let loader: Loader
    private let clearCache: () -> Void
    private let gate: MLXGate
    private let budget: MLXBudget
    /// Gardés par `gate`.
    private var loaded: [String: (model: TextSpeechModel, users: Int)] = [:]

    init(loader: @escaping Loader = TextSpeechMLXCompute.loadModel,
         clearCache: @escaping () -> Void = KokoroRunner.clearCache,
         gate: MLXGate = .shared, budget: MLXBudget = .shared) {
        self.loader = loader
        self.clearCache = clearCache
        self.gate = gate
        self.budget = budget
    }

    /// Qwen3 : les poids (le codec compris) plus 1 Go de marge (pic mesuré
    /// du 1.7B : 3,5–3,7 Go). Magpie : ses poids 8 bits sont déquantifiés
    /// en FP32 au chargement (×4), plus 512 Mo.
    static func need(runtime: String, weightBytes: Int) -> MLXBudget.Need {
        switch runtime {
        case magpieRuntime: return .init(memory: 4 * weightBytes + (512 << 20), cache: 128 << 20)
        default: return .init(memory: weightBytes + (1 << 30), cache: 256 << 20)
        }
    }

    static func loadModel(_ directory: URL, runtime: String) throws -> TextSpeechModel {
        switch runtime {
        case qwenRuntime: return try Qwen3TTSRunner.load(directory: directory)
        case magpieRuntime: return try MagpieRunner.load(directory: directory)
        default: throw SillonError.Failed(message: "moteur de lecture inconnu : \(runtime)")
        }
    }

    private func key(_ modelDir: String) -> String { "lecture:" + modelDir }

    func load(modelDir: String, runtime: String) throws {
        try gate.run {
            if let entry = loaded[modelDir] {
                loaded[modelDir] = (entry.model, entry.users + 1)
                return
            }
            let url = URL(fileURLWithPath: modelDir)
            budget.set(key(modelDir), Self.need(runtime: runtime, weightBytes: MLXBudget.weightBytes(url)))
            do {
                loaded[modelDir] = (try loader(url, runtime), 1)
            } catch {
                budget.remove(key(modelDir))
                throw SillonError.Failed(message: "Le modèle de lecture ne se charge pas : \(error.localizedDescription)")
            }
        }
    }

    func warmUp(modelDir: String) throws {
        _ = try synthesize(modelDir: modelDir, text: Self.warmUpText, language: "fr", voice: "",
                           isCancelled: { false })
    }

    /// Swift ne garde jamais `cancel` au-delà de l'appel.
    func synthesize(modelDir: String, text: String, language: String, voice: String,
                    cancel: SynthesisCancel) throws -> SpeechOutput {
        try synthesize(modelDir: modelDir, text: text, language: language, voice: voice,
                       isCancelled: { cancel.isCancelled() })
    }

    func synthesize(modelDir: String, text: String, language: String, voice: String,
                    isCancelled: @escaping () -> Bool) throws -> SpeechOutput {
        try gate.run {
            // L'abandon d'abord, après l'attente du verrou.
            if isCancelled() { throw SillonError.Cancelled }
            guard let entry = loaded[modelDir] else {
                throw SillonError.Failed(message: "modèle de lecture non chargé")
            }
            let samples: [Float]?
            do {
                samples = try entry.model.synthesize(text: text, language: language, voice: voice,
                                                     shouldStop: isCancelled)
            } catch {
                throw SillonError.Failed(message: error.localizedDescription)
            }
            guard let samples else { throw SillonError.Cancelled }
            return SpeechOutput(samples: samples, sampleRate: UInt32(entry.model.sampleRate))
        }
    }

    func release(modelDir: String) {
        gate.run {
            guard let entry = loaded[modelDir] else { return }
            guard entry.users <= 1 else {
                loaded[modelDir] = (entry.model, entry.users - 1)
                return
            }
            loaded[modelDir] = nil
            clearCache()
            budget.remove(key(modelDir))
        }
    }
}
```

- [ ] **Step 4 : `AppDelegate`** — supprimer `UnavailableTextSpeech` ; créer `let text = TextSpeechMLXCompute()`, le garder (`textCompute`), et `reader = SillonReader.withMlx(kokoro: compute, text: text)`.
- [ ] **Step 5 : UIBUILD** — attendu : compile.
- [ ] **Step 6 : commit** `feat(macos): l'app fait lire Qwen3 et Magpie — un modèle par dossier sous le verrou MLX, son besoin mémoire inscrit, arrêt en route rendu comme un abandon`

---

### Task 9 : Mesurer la cadence sur le port Swift, et la dire dans la fiche

**Files:**
- Modify: `macos/Sources/App/Measure.swift` (`Measure.rtf`, sous `#if DEBUG || SILLON_MESURE`)
- Modify: `macos/Sources/AppDelegate.swift` (argument `--measure-rtf <id>`)
- Modify: `scripts/measure-macos.sh` (option `--rtf <id>`)
- Modify: `src/models.rs` (`pace` des trois paquets, d'après la mesure)
- Test: `macos/Tests/MeasureRTFTests.swift`

**Interfaces:**
- Consumes: `TextSpeechMLXCompute.synthesize(modelDir:text:language:voice:isCancelled:)` (Tâche 8), FFI `modelDirectory(id:)`.
- Produces: `Measure.fluentSpeed(rtf: Double) -> Double?` (plus grande vitesse parmi 3, 2,5, 2, 1,5, 1,25, 1 telle que `rtf × vitesse ≤ 0,8` ; `nil` si même 1× ne tient pas) ; lignes `MESURE rtf <id> <langue> <valeur>`, `MESURE cadence_max <id> <vitesse>`, `MESURE empreinte <id> <Mo> Mo`.

Pourquoi 0,8 : à la vitesse v, la voix est occupée `rtf × v` du temps ; au-delà de 80 %, le moindre à-coup (premier mot, saut, autre calcul MLX) fait attendre la lecture. Le banc Python donnait pour le 1.7B 64 % à 1× (lecture tenue) et un échec à 2× ; le port Swift fait une synchronisation par trame et 15 passages du prédicteur : il faut **son** chiffre.

- [ ] **Step 1 : test qui échoue** — `macos/Tests/MeasureRTFTests.swift` :

```swift
import XCTest
@testable import Sillon

final class MeasureRTFTests: XCTestCase {
    func testTheFluentSpeedKeepsTwentyPercentOfMargin() {
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.25), 3)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.40), 2)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.60), 1.25)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.79), 1)
        XCTAssertNil(Measure.fluentSpeed(rtf: 0.85))
    }

    func testThePaceSentenceSaysTheMeasuredFigures() {
        XCTAssertEqual(Measure.paceSentence(rtf: 0.60),
                       "Lourd : lecture hachée au-delà de 1,25× (occupé 60 % du temps à 1× sur ce Mac).")
        XCTAssertEqual(Measure.paceSentence(rtf: 0.25),
                       "Quatre fois plus vite que la parole sur ce Mac : lecture fluide jusqu'à 3×.")
        XCTAssertEqual(Measure.paceSentence(rtf: 0.90),
                       "Lourd : lecture hachée même à 1× (occupé 90 % du temps sur ce Mac).")
    }
}
```

- [ ] **Step 2 : UIBUILD** — attendu : échec.
- [ ] **Step 3 : implémenter.** Dans `Measure.swift` :

```swift
    // MARK: - Cadence d'un modèle à texte (lot 3)

    /// Huit phrases françaises, quatre anglaises : longueurs de lecture
    /// ordinaires (formes parlées, sans chiffres).
    static let rtfPhrases: [(String, String)] = [
        ("fr", "Les monades permettent d'enchaîner des calculs qui portent un contexte."),
        ("fr", "On les retrouve partout : options, listes, promesses."),
        ("fr", "Ce matin, le marché était plein, et les étals débordaient de fruits."),
        ("fr", "Il nous fallut deux heures pour atteindre le sommet de ce pic."),
        ("fr", "La pluie tombait sans bruit sur les toits de la vieille ville."),
        ("fr", "Elle referma le livre, puis regarda longtemps par la fenêtre."),
        ("fr", "Une phrase plus courte."),
        ("fr", "Enfin, une dernière phrase, un peu plus longue que les autres, pour finir la série."),
        ("en", "The quick brown fox jumps over the lazy dog."),
        ("en", "Monads let you chain computations that carry a context."),
        ("en", "It was raining softly over the roofs of the old town."),
        ("en", "She closed the book and looked out of the window for a long time."),
    ]

    /// La plus grande vitesse tenue avec 20 % de marge.
    nonisolated static func fluentSpeed(rtf: Double) -> Double? {
        [3, 2.5, 2, 1.5, 1.25, 1].first { rtf * $0 <= 0.8 }
    }

    /// La phrase de la fiche (`pace`), d'après la mesure.
    nonisolated static func paceSentence(rtf: Double) -> String {
        let busy = Int((rtf * 100).rounded())
        guard let speed = fluentSpeed(rtf: rtf) else {
            return "Lourd : lecture hachée même à 1× (occupé \(busy) % du temps sur ce Mac)."
        }
        let x = speed.formatted(.number.precision(.fractionLength(0...2)).locale(Locale(identifier: "fr_FR")))
        if speed >= 3 {
            let times = Int((1 / rtf).rounded(.down))
            let words = [2: "Deux", 3: "Trois", 4: "Quatre", 5: "Cinq", 6: "Six", 7: "Sept", 8: "Huit", 9: "Neuf"]
            let lead = words[times].map { "\($0) fois" } ?? "\(times) fois"
            return "\(lead) plus vite que la parole sur ce Mac : lecture fluide jusqu'à \(x)×."
        }
        return "Lourd : lecture hachée au-delà de \(x)× (occupé \(busy) % du temps à 1× sur ce Mac)."
    }

    /// Charge le modèle `id` (installé), le prépare, puis synthétise les
    /// phrases une à une ; écrit le RTF soutenu par langue (calcul / durée
    /// du son), la vitesse tenue, et l'empreinte après la série.
    static func rtf(reader: SillonReader, compute: TextSpeechMLXCompute, id: String) async {
        guard let dir = reader.modelDirectory(id: id) else {
            print("MESURE erreur \(id) non installé"); fflush(stdout); return
        }
        let runtime = id.hasPrefix("magpie") ? TextSpeechMLXCompute.magpieRuntime : TextSpeechMLXCompute.qwenRuntime
        let voice = id.hasPrefix("magpie") ? "sofia" : (id.contains("customvoice") ? "vivian" : "siwis")
        do {
            try compute.load(modelDir: dir, runtime: runtime)
            defer { compute.release(modelDir: dir) }
            try compute.warmUp(modelDir: dir)
            var calcul: [String: Double] = [:], son: [String: Double] = [:]
            for (language, text) in rtfPhrases {
                let start = ContinuousClock.now
                let out = try compute.synthesize(modelDir: dir, text: text, language: language, voice: voice,
                                                 isCancelled: { false })
                let elapsed = ContinuousClock.now - start
                calcul[language, default: 0] += Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18
                son[language, default: 0] += Double(out.samples.count) / Double(out.sampleRate)
            }
            for language in ["fr", "en"] {
                let value = calcul[language, default: 0] / max(son[language, default: 1], 0.001)
                print(String(format: "MESURE rtf %@ %@ %.3f", id, language, value))
            }
            let fr = calcul["fr", default: 0] / max(son["fr", default: 1], 0.001)
            print("MESURE cadence_max \(id) \(fluentSpeed(rtf: fr).map { String($0) } ?? "aucune")")
            print("MESURE fiche \(id) \(paceSentence(rtf: fr))")
            print("MESURE empreinte \(id) \(residentMB()) Mo")
        } catch {
            print("MESURE erreur \(id) \(error.localizedDescription)")
        }
        fflush(stdout)
    }
```

(`residentMB()` est `private` : le rendre `static func` interne si besoin. `reader.modelDirectory(id:)` : le nom Swift de `model_directory`, déjà au FFI.)

`AppDelegate`, dans le bloc `#if DEBUG || SILLON_MESURE`, avant `--measure` :

```swift
        if let i = ProcessInfo.processInfo.arguments.firstIndex(of: "--measure-rtf"), !isUnitTesting,
           i + 1 < ProcessInfo.processInfo.arguments.count {
            let id = ProcessInfo.processInfo.arguments[i + 1]
            Task { @MainActor [reader = reader!, text = textCompute!] in
                await Measure.rtf(reader: reader, compute: text, id: id)
                NSApp.terminate(nil)
            }
            return
        }
```

`scripts/measure-macos.sh` : option `--rtf <id>` (ajoutée à `usage`) qui, après la construction, lance `"$APP/Contents/MacOS/Sillon" --measure-rtf "$ID" 2>"$JOURNAL" | { grep '^MESURE' || true; } | tee target/runs/sillon-rtf-$ID.txt` et sort en erreur s'il y a une ligne `MESURE erreur`.

- [ ] **Step 4 : UIBUILD** — attendu : compile.
- [ ] **Step 5 : mesurer.** Machine calme (pas de lecture, pas de dictée, secteur branché). Pour chaque id installé dans `~/Library/Application Support/sillon/models/` : `./scripts/measure-macos.sh --rtf qwen3_tts_17b_customvoice_8bit` (puis `--sans-construire --rtf qwen3_tts_06b_base_8bit`, `--sans-construire --rtf magpie_tts_357m_8bit`). Deux passages par modèle ; garder le plus lent. Si un modèle n'est pas installé, demander à l'utilisateur de le télécharger depuis la table Lecture (ou de lancer la mesure lui-même) — **ne jamais** remplir `pace` sans ligne `MESURE fiche`.
- [ ] **Step 6 : la fiche.** Dans `src/models.rs`, `pace` de chaque paquet mesuré = la phrase de `MESURE fiche`, en remplaçant « sur ce Mac » par « sur M5 » (l'unique Mac de mesure ; comme Kokoro). Le 1.7B doit commencer par « Lourd : lecture hachée au-delà de » : si la mesure le dit fluide à 3×, ne pas l'écrire, le signaler (le banc Python disait l'inverse : quelque chose cloche). Joindre les lignes `MESURE` au message de commit. **RUST** — vert (`the_heavy_model_says_so_in_its_pace`).
- [ ] **Step 7 : commit** `feat(moteur): la cadence de Qwen3 et Magpie mesurée sur le port Swift (M5) et dite dans leur fiche` (corps : les lignes `MESURE`).

---

### Task 10 : Vérification

- [ ] **Step 1 :** **RUST**, **FFI**, **UIBUILD** — verts (hors `clipboard_input` si le presse-papiers est vide). `grep -rn "xcodebuild.* test\b" scripts` — rien de nouveau.
- [ ] **Step 2 :** construire l'app (`./scripts/build-macos.sh`) — la vérification des licences passe (`speech-swift-LICENSE`, `cmudict-LICENSE` présents dans `Contents/Resources/licences`).
- [ ] **Step 3 : écoute, par l'utilisateur, à la main** (lecture bimodale : écouter en suivant le surlignage). Pour chaque modèle : le télécharger depuis la table Lecture, l'utiliser, ouvrir un texte français d'une page (avec un nombre, une date, une citation entre « », une phrase de plus de 30 s), lire 2 minutes à 1× :
  - Qwen3 0.6B : une seule voix, la même à chaque phrase (le timbre ne change pas d'une phrase à l'autre), accent français ;
  - Qwen3 1.7B : vivian puis eric en français (Eric lit avec un accent étranger mais en français, pas en dialecte chinois) ; aiden sur un texte anglais ;
  - Magpie : sofia en français, leo en anglais ; la phrase de plus de 30 s est lue jusqu'au bout ;
  - pour tous : le surlignage démarre avec la voix (pas d'avance due à un blanc de tête), sauter de phrase en pleine lecture coupe aussitôt, revenir sur une phrase la redit à l'identique ;
  - bibliothèque des voix : écouter une voix Magpie pendant que Qwen 1.7B est le modèle de lecture, fermer, reprendre la lecture — Qwen parle toujours avec sa voix ;
  - dicter une phrase (Whisper tiny) pendant que Qwen 1.7B est chargé, puis relire : pas de plantage, pas de lecture hachée nouvelle ;
  - supprimer le modèle utilisé : la lecture passe à la voix de macOS avec la confirmation de la spec.
- [ ] **Step 4 : cadence (procédure RTF)** — si la Tâche 9 n'a pas pu mesurer un modèle, la faire maintenant : `./scripts/measure-macos.sh --rtf <id>` pour `qwen3_tts_06b_base_8bit`, `qwen3_tts_17b_customvoice_8bit`, `magpie_tts_357m_8bit`, machine calme, deux passages, le plus lent retenu. Lire : `MESURE rtf <id> fr|en <rtf>` (calcul / durée du son, ≤ 0,8 × vitesse pour une lecture tenue), `MESURE cadence_max`, `MESURE empreinte` (comparer à ≈ 3,5–3,7 Go pour le 1.7B, ≈ 2,5 Go pour le 0.6B, ≈ 1,6 Go pour Magpie : un écart de plus de 30 % se signale). Puis, pour le 1.7B, lire 5 minutes à 1× et 2 minutes à 1,5× et noter les attentes (`MESURE`-libre : à l'oreille, ou `./scripts/measure-macos.sh --charge` si l'utilisateur veut le filtre de charge complet).
- [ ] **Step 5 :** pousser pour la CI seulement avec l'accord de l'utilisateur ; sur la CI, vérifier que `Qwen2BPETokenizerTests`, `Qwen3TTSRunnerTests`, `MagpieRunnerTests`, `MLXBudgetTests`, `TextSpeechMLXComputeTests`, `MeasureRTFTests` passent, et que les tests de fumée sont **ignorés** (pas en échec).
