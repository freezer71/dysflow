# La voix Kokoro-82M dans Sillon — plan d'implémentation

> **Pour les agents :** sous-skill requis : superpowers:subagent-driven-development
> (recommandé) ou superpowers:executing-plans, tâche par tâche. Les étapes
> utilisent des cases à cocher (`- [ ]`).

**Objectif :** Kokoro-82M devient la voix de Sillon — `p-fenrir60` en
français, `af_bella` en anglais —, en MLX dans l'app macOS et en ONNX
Runtime au terminal, avec une langue par document (détectée ou choisie), un
surlignage minuté par mot, un premier son en deux morceaux, et un passage
unique des configurations existantes à Kokoro.

**Architecture :** le moteur Rust garde tout ce qui n'est pas du calcul : la
langue du document (`text::language`, `whatlang`), la normalisation française
réservée aux documents français, la phonétisation par un processus espeak-ng
séparé (`tts::kokoro::phonemes`), les voix 510 × 256 (`voices`), le minutage
des mots par alignement des phonèmes (`align`), la coupe en tête et suite de la
phrase attendue (pool + lecteur), le catalogue multi-fichiers et le passage à
Kokoro. Le calcul passe par le trait `KokoroBackend` : `onnx.rs` (feature
`kokoro-onnx`, `ort` en `load-dynamic`, ORT ≥ 1.31) au terminal, et dans l'app
une interface étrangère UniFFI `KokoroCompute` implémentée en Swift par MLX
(fichiers Kokoro de mlx-audio-swift vendorisés dans un framework `KokoroMLX`).

**Pile :** Rust 2021 (`sillon`, `sillon-ffi`, UniFFI 0.32), `whatlang` 0.18,
`ort` 2.0.0-rc.13 + `libloading` 0.8 (feature `kokoro-onnx`), espeak-ng 1.52.0
(binaire séparé, construit par `scripts/build-espeak.sh`), Swift en mode de
langage 5, mlx-swift 0.32.3 (MLX, MLXNN, MLXFast, MLXFFT), XcodeGen, XCTest /
XCUITest, Python du banc (`.superpowers/research/eval/light_run.py`).

**Spec :** `docs/superpowers/specs/2026-10-11-voix-kokoro-design.md` (mesures :
`.superpowers/research/eval/rapport.md` §10–11,
`.superpowers/research/2026-10-11-spike-kokoro-integration.md`,
`.superpowers/research/2026-10-09-vitesse-sonic-rapport.md`). La lire avant
toute tâche.

## Contraintes globales

- macOS 14 minimum, Apple Silicon (`arm64`) seul ; Swift en **mode de langage
  5** (`SWIFT_VERSION: "5.0"`), y compris pour le framework vendorisé.
- Rust : `cargo test` (crate `sillon`) et `cargo test -p sillon-ffi`. Sous
  `cfg(test)`, `paths::config_dir`, `data_dir` et `cache_dir` sont des dossiers
  temporaires propres au processus : **aucun test ne lit ni n'écrit la
  configuration ou les données réelles** de l'utilisateur.
- Tests unitaires macOS :
  `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
  (filtrer avec `-only-testing:SillonTests/<Classe>` pendant une tâche, puis la
  suite entière avant le commit). Après un changement du FFI :
  `./scripts/build-ffi.sh` d'abord.
- Tests d'interface : **compilés seulement** (même commande avec
  `build-for-testing`), **exécutés uniquement avec le feu vert explicite de
  l'utilisateur**.
- Critères de poids (spec, chemin MLX de l'app, mesurés sur le M5) : **≤ 10 %
  du temps occupé à 1×**, **2× sans coupure**, **≤ 2 Go** d'empreinte physique,
  **premier son ≤ 0,3 s voix chaude**, **0 % CPU en pause** (aucun appel à MLX),
  après « Garder la voix chargée » l'empreinte revient à celle de l'app sans
  voix (± 50 Mo). Vérifiés par la Tâche 15 avec la méthode du banc
  (`.superpowers/research/eval/`), **seulement sur une machine calme** (load
  1 min < 4) et avec l'accord de l'utilisateur.
- Valeurs figées : `mlx-community/Kokoro-82M-bf16` révision
  `a71e4d38b236d968966a2002c4c895dbd12b1c3c` ;
  `onnx-community/Kokoro-82M-v1.0-ONNX-timestamped` révision
  `dd4401a9add81ac692d20e240d22ec9dda82cc29`, `onnx/model_quantized.onnx`
  = 92 361 055 octets ; `kokoro-v1_0.safetensors` = 327 115 152 octets ;
  `config.json` (MLX, porte le vocabulaire de 114 symboles) = 2 351 octets ;
  chaque voix `voices/<nom>.safetensors` = 522 320 octets ; espeak-ng 1.52.0 ;
  mlx-swift 0.32.3 ; mlx-audio-swift commit `244458d` ; ORT ≥ 1.31.
- Fils ORT : `min(engine.threads, 4)`. **Pas d'ORT dans l'app macOS** : le
  crate `sillon-ffi` n'active jamais `kokoro-onnx`.
- Une voix par document (langue majoritaire en lettres) ; « Document mêlé »
  au-delà de 30 % ; un document anglais ne passe pas par la normalisation
  française.
- Les modèles ne se téléchargent dans le vrai dossier des modèles **que par
  l'app ou le moteur à l'exécution**, jamais dans un test. Les tests
  `#[ignore]` qui ont besoin des voix lisent un dossier passé par variable
  d'environnement (`SILLON_KOKORO_DIR`), rempli à la main.
- Ne jamais journaliser un texte lu ni une touche : les messages d'erreur et
  du journal ne citent ni phrase ni phonèmes, seulement des nombres et des
  noms de fichiers.
- `git add` de chemins explicites uniquement ; **ne jamais indexer
  `vendor/audio.cpp`** (il porte un correctif local non versionné), ni
  `macos/Generated`, `macos/Libraries`, `macos/*.xcodeproj`, `macos/build`,
  `target/`.
- Les 4 fichiers macOS non commités au départ (`SettingKey.swift`,
  `VoiceTab.swift`, `SettingsDiffTests.swift`, `SettingsStoreTests.swift` : la
  vitesse affichée pour toutes les familles, puisque Sonic l'applique) sont
  relus et commités dans la **Tâche 14**, la première qui touche l'onglet Voix.
  Les tâches précédentes ne les indexent pas.
- Textes, commentaires et noms d'interface en **français**, comme le code
  existant. Messages de commit en français, style du dépôt
  (`feat(moteur): …`, `feat(macos): …`), terminés par
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Points d'attention

Cinq situations que la spec implique sans les tester, les plus susceptibles de
gêner quelqu'un qui lit avec Sillon ; chacune a reçu son test dans la tâche
indiquée.

1. **Une courte sélection anglaise** (« Hello world. », un titre, du code
   seul) : moins de 40 lettres, rien de fiable. Attendu : la langue par défaut,
   et l'onglet Voix dit « non détectée » au lieu de prétendre l'avoir
   reconnue. → Tâche 3, `a_short_text_takes_the_default_language_and_says_so`.
2. **Un saut à un mot de la fin de la phrase pendant qu'elle se synthétise en
   deux morceaux** : le mot visé est dans la suite. Attendu : la tête ne joue
   pas, la lecture part du mot, sans redire le début. → Tâche 5,
   `a_jump_into_the_tail_skips_the_head`.
3. **espeak-ng qui s'arrête en pleine lecture** (paquet désinstallé, binaire
   tué). Attendu : une relance ; au second échec, la phrase est sautée une
   fois et la voix de repli prend la suite, annoncée. → Tâche 9,
   `a_phonemizer_that_dies_twice_hands_over_to_the_fallback`.
4. **Supprimer un des deux paquets Kokoro quand l'autre est installé** (terminal
   et app partagent `models/kokoro/`). Attendu : les voix et le `config.json`
   communs restent. → Tâche 6,
   `removing_one_kokoro_package_keeps_what_the_other_needs`.
5. **Revenir à l'ancien modèle après le passage à Kokoro, puis relancer.**
   Attendu : le choix tient ; la migration ne se refait pas. → Tâche 11,
   `going_back_to_the_old_model_holds_after_a_relaunch`.

Couverts aussi : voix inconnue dans `config.toml` (Tâche 1), ponctuation
française autour des guillemets et parenthèses vers espeak (Tâche 7), phrase
de plus de 510 phonèmes (Tâche 9), document mêlé (Tâches 3 et 14).

## Structure des fichiers

| Fichier | Rôle |
|---|---|
| `Cargo.toml` | `whatlang` ; feature `kokoro-onnx` (`ort`, `libloading`) ; exemples Kokoro |
| `src/text/language.rs` (nouveau) | `Lang`, `Detection`, `detect`, `detect_markdown`, `resolve` |
| `src/config.rs` | `voice.language = "auto"`, `[voice.per_language]`, `app.voice_migration`, `default_language`, `tts_params_in` |
| `src/models.rs` | voix par langue, `Companion`, `Runtime`, paquets Kokoro, `available_with`, installation et suppression multi-fichiers |
| `src/doc/mod.rs` | `WordSpan`, `Sentence::word_fraction_timed`, `word_start_timed`, `spoken_words` |
| `src/doc/builder.rs` | `DocumentBuilder::with_language`, normalisation française réservée au français, `parse_document_in` |
| `src/reader/language.rs` (nouveau) | `LanguageChoices` (choix par document), `DocumentLanguage`, `Reader::set_document_language` |
| `src/reader/migration.rs` (nouveau) | passage unique à Kokoro (`plan`, `Reader::migrate_voice`) |
| `src/reader/{mod,settings,view,preview,data}.rs` | langue à l'ouverture, effets, instantané, aperçu par langue, catalogue avec MLX |
| `src/tts/mod.rs` | `Synthesis`, `synthesize_timed`, `splits_first_clause`, `build_engine_with` |
| `src/tts/pool.rs` | mots, minutages, `Part`, coupe en tête et suite, temps occupé |
| `src/tts/lazy.rs` | coupe annoncée d'avance, état du moteur chargé relayé |
| `src/tts/fallback.rs` (nouveau) | repli à la demande : premier audio.cpp installé, sinon la voix du système |
| `src/tts/kokoro/{mod,backend,phonemes,voices,align,onnx}.rs` (nouveaux) | le moteur Kokoro |
| `src/audio/player.rs` | `enqueue_part`, segments numérotés |
| `src/app/mod.rs` | paramètres dans la langue du document, minutages, attente de la suite, coupures |
| `src/app/config.rs`, `src/launch.rs` | « auto » au terminal, langue détectée, Kokoro installé adopté |
| `assets/voices/p-fenrir60.safetensors`, `.json` (nouveaux) | la voix française livrée |
| `examples/kokoro_blend.rs`, `kokoro_fixtures.rs`, `kokoro_serve.rs` (nouveaux) | recette de `p-fenrir60`, fixtures d'alignement, serveur du banc |
| `tests/fixtures/kokoro/*.json` (nouveaux) | phonèmes misaki de référence, fixtures d'alignement |
| `scripts/build-espeak.sh` (nouveau), `build-ffi.sh`, `build-macos.sh`, `package.sh`, `measure-macos.sh`, `mesure-charge.py` (nouveau) | construction et mesure |
| `NOTICE` | licences (Kokoro, mlx-audio-swift, mlx-swift, GPL d'espeak-ng et offre de source, ONNX Runtime) |
| `ffi/src/lib.rs` | `KokoroCompute` (interface étrangère), `with_kokoro`, nouveaux champs |
| `macos/Vendor/KokoroMLX/` (nouveau) | fichiers Kokoro de mlx-audio-swift, licence MIT |
| `macos/Sources/Voice/KokoroMLX.swift`, `KokoroWarmUp.swift` (nouveaux) | le calcul MLX, la préparation des noyaux Metal |
| `macos/Sources/Settings/VoiceTab.swift`, `SettingKey.swift`, `ModelsTab.swift`, `SettingsDiff.swift`, `SettingsDefaults.swift` | onglet Voix Kokoro, licences |
| `macos/Sources/AppDelegate.swift`, `Model/ReaderModel.swift`, `App/Measure.swift` | lecteur avec Kokoro, avis, mesure de charge |
| `macos/project.yml` | mlx-swift figé, cible `KokoroMLX`, espeak-ng et metallib dans l'app |
| `macos/Tests/*`, `macos/UITests/KokoroVoiceUITests.swift` (nouveau) | tests |

---

### Tâche 1 : langues — détection, « auto », une voix par langue

La détection de la langue (`whatlang`, restreint au français et à l'anglais),
la valeur `auto` qui devient le défaut de `voice.language`, la table
`[voice.per_language]`, et les voix Kokoro rangées par langue. Rien ne change
encore à l'ouverture d'un document : `tts_params()` parle la langue par défaut.

**Fichiers :**
- Modifier : `Cargo.toml` (dépendance `whatlang`)
- Créer : `src/text/language.rs`
- Modifier : `src/text/mod.rs`
- Modifier : `src/models.rs` (`VoiceOption.language`, `KOKORO_VOICES`, fonctions par langue)
- Modifier : `src/config.rs` (`per_language`, défaut `auto`, `default_language`, `tts_params_in`, `align_voice_with_family`)
- Modifier : `src/app/config.rs` (« auto » dans le cycle des langues)

**Interfaces :**
- Consomme : rien.
- Produit :
  - `pub enum Lang { Fr, En }` ; `Lang::code(self) -> &'static str` ; `Lang::parse(&str) -> Option<Lang>`
  - `pub struct Detection { pub language: Lang, pub minority_share: f32 }`
  - `pub const MIN_LETTERS: usize = 40; pub const MIXED_THRESHOLD: f32 = 0.30; pub const AUTO: &str = "auto";`
  - `pub fn detect(text: &str) -> Option<Detection>` ; `pub fn detect_markdown(markdown: &str) -> Option<Detection>`
  - `pub fn resolve(configured: &str, chosen: Option<&str>, detected: Option<Detection>) -> String`
  - `models::VoiceOption { id, label, language: &'static str }` (`""` : toutes les langues du modèle)
  - `models::voices_follow_language(family: &str) -> bool` ; `models::default_voice_for_language(family, language) -> &'static str` ; `models::voice_for_language(family, language, configured: Option<&str>) -> &'static str`
  - `VoiceConfig.per_language: BTreeMap<String, String>` (dernier champ de `[voice]`)
  - `Config::default_language(&self) -> String` ; `Config::tts_params_in(&self, language: &str) -> TtsParams` ; `tts_params()` = `tts_params_in(&default_language())`

- [ ] **Étape 1 : ajouter la dépendance**

Dans `Cargo.toml`, section `[dependencies]`, après `unicode-width = "0.2"` :

```toml
# Langue d'un document (français ou anglais), MIT.
whatlang = "0.18"
```

Run: `cargo build`
Expected: compilation sans erreur.

- [ ] **Étape 2 : écrire les tests qui échouent**

Créer `src/text/language.rs` avec seulement le module de tests (le code vient
à l'étape 4) :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FR: &str = "Les monades permettent d'enchaîner des calculs qui portent un contexte, \
                      sans le manipuler à la main à chaque étape du programme.";
    const EN: &str = "Monads let you chain computations that carry a context, without handling \
                      it by hand at every single step of the program.";

    #[test]
    fn a_french_paragraph_is_french() {
        let d = detect(FR).expect("assez de lettres");
        assert_eq!(d.language, Lang::Fr);
        assert!(d.minority_share < 0.05, "{}", d.minority_share);
    }

    #[test]
    fn an_english_paragraph_is_english() {
        assert_eq!(detect(EN).map(|d| d.language), Some(Lang::En));
    }

    #[test]
    fn under_forty_letters_nothing_is_said() {
        assert_eq!(detect("Hello world."), None);
        assert_eq!(detect(""), None);
    }

    #[test]
    fn a_mixed_document_takes_its_majority_and_says_how_much_is_other() {
        let text = format!("{FR}\n\n{FR}\n\n{EN}");
        let d = detect(&text).unwrap();
        assert_eq!(d.language, Lang::Fr);
        assert!((0.2..0.45).contains(&d.minority_share), "{}", d.minority_share);
    }

    #[test]
    fn code_blocks_say_nothing_about_the_language() {
        let code = "```rust\nfn main() { println!(\"the quick brown fox jumps over the lazy dog\"); }\n\
                    // another english comment line that is quite long indeed\n```";
        let markdown = format!("{FR}\n\n{code}\n\nUn `inline english code span here` aussi.");
        let d = detect_markdown(&markdown).unwrap();
        assert_eq!(d.language, Lang::Fr);
        assert!(d.minority_share < 0.05, "{}", d.minority_share);
    }

    #[test]
    fn codes_are_read_loosely() {
        assert_eq!(Lang::parse("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::parse(" EN_us "), Some(Lang::En));
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::En.code(), "en");
    }

    #[test]
    fn a_choice_beats_the_configuration_which_beats_the_detection() {
        let en = Some(Detection { language: Lang::En, minority_share: 0.0 });
        assert_eq!(resolve("auto", None, en), "en");
        assert_eq!(resolve("auto", None, None), "fr", "rien de détecté : le français");
        assert_eq!(resolve("fr", None, en), "fr", "langue imposée par la configuration");
        assert_eq!(resolve("de", None, en), "de", "une langue d'un autre modèle reste telle quelle");
        assert_eq!(resolve("fr", Some("en"), None), "en", "le choix fait pour ce document");
    }
}
```

Dans `src/text/mod.rs`, après `pub mod dates;` :

```rust
pub mod language;
```

Run: `cargo test --lib text::language`
Expected: FAIL — `cannot find function detect`, `Lang`, `resolve` introuvables.

- [ ] **Étape 3 : écrire le code de la détection**

En tête de `src/text/language.rs`, au-dessus du module de tests :

```rust
//! La langue d'un document : français ou anglais, d'après ses lettres.
//!
//! Une seule langue par document, celle qui a le plus de lettres. Les blocs de
//! code n'en disent rien et sont écartés. Un texte trop court, ou dont la
//! détection hésite, prend la langue par défaut : se tromper de voix sur une
//! phrase gêne plus que garder celle qu'on attend.

use whatlang::{Detector, Lang as Detected};

/// Les deux langues de la voix Kokoro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// `fr`, `fr-FR`, `en_US`… ; toute autre langue : `None`.
    pub fn parse(code: &str) -> Option<Lang> {
        let base = code.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        match base.as_str() {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// Ce que la détection a vu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub language: Lang,
    /// Part des lettres dans l'autre langue, de 0 à 0,5.
    pub minority_share: f32,
}

/// Lettres minimum pour se prononcer.
pub const MIN_LETTERS: usize = 40;
/// Au-delà, l'onglet Voix signale un document mêlé.
pub const MIXED_THRESHOLD: f32 = 0.30;
/// Valeur de `voice.language` qui laisse chaque document décider.
pub const AUTO: &str = "auto";

fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

fn detector() -> Detector {
    Detector::with_allowlist(vec![Detected::Fra, Detected::Eng])
}

fn lang_of(detected: Detected) -> Option<Lang> {
    match detected {
        Detected::Fra => Some(Lang::Fr),
        Detected::Eng => Some(Lang::En),
        _ => None,
    }
}

/// La langue d'un texte brut, ou `None` s'il est trop court ou ambigu.
///
/// La fiabilité se juge sur le texte entier ; la part de chaque langue se
/// compte paragraphe par paragraphe, la détection globale ne disant pas
/// combien de lettres sont dans l'autre.
pub fn detect(text: &str) -> Option<Detection> {
    if letters(text) < MIN_LETTERS {
        return None;
    }
    let detector = detector();
    let whole = detector.detect(text)?;
    if !whole.is_reliable() {
        return None;
    }
    let (mut fr, mut en) = (0usize, 0usize);
    for paragraph in text.split("\n\n") {
        let n = letters(paragraph);
        if n == 0 {
            continue;
        }
        match detector.detect(paragraph).and_then(|info| lang_of(info.lang())) {
            Some(Lang::Fr) => fr += n,
            Some(Lang::En) => en += n,
            None => {}
        }
    }
    let total = fr + en;
    if total == 0 {
        return lang_of(whole.lang()).map(|language| Detection { language, minority_share: 0.0 });
    }
    let language = if fr >= en { Lang::Fr } else { Lang::En };
    Some(Detection { language, minority_share: fr.min(en) as f32 / total as f32 })
}

/// La langue d'un markdown, blocs et extraits de code écartés.
pub fn detect_markdown(markdown: &str) -> Option<Detection> {
    detect(&without_code(markdown))
}

fn without_code(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut fence: Option<&'static str> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            out.push('\n');
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            out.push('\n');
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            out.push('\n');
            continue;
        }
        let mut inside = false;
        for c in line.chars() {
            if c == '`' {
                inside = !inside;
            } else if !inside {
                out.push(c);
            }
        }
        out.push('\n');
    }
    out
}

/// La langue d'un document : le choix fait pour lui, sinon la langue imposée
/// par la configuration (tout sauf `auto`), sinon celle détectée, sinon le
/// français.
pub fn resolve(configured: &str, chosen: Option<&str>, detected: Option<Detection>) -> String {
    if let Some(choice) = chosen.map(str::trim).filter(|c| !c.is_empty()) {
        return choice.to_string();
    }
    let configured = configured.trim();
    if !configured.is_empty() && configured != AUTO {
        return configured.to_string();
    }
    detected.map_or(Lang::Fr, |d| d.language).code().to_string()
}
```

Run: `cargo test --lib text::language`
Expected: PASS (7 tests).

- [ ] **Étape 4 : écrire les tests des voix par langue et de la configuration**

Dans `src/models.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn kokoro_names_its_voices_by_language() {
        assert!(voices_follow_language("kokoro"));
        assert!(!voices_follow_language("supertonic"));
        let voices = voices_for_family("kokoro");
        assert_eq!(voices.iter().filter(|v| v.language == "fr").count(), 2);
        assert_eq!(voices.iter().filter(|v| v.language == "en").count(), 28);
        assert_eq!(default_voice_for_language("kokoro", "fr"), "p-fenrir60");
        assert_eq!(default_voice_for_language("kokoro", "en-US"), "af_bella");
        assert!(voices.iter().any(|v| v.id == "af_nicole" && v.label.contains("chuchotée")));
    }

    #[test]
    fn an_unknown_voice_gives_the_default_of_its_language() {
        assert_eq!(voice_for_language("kokoro", "fr", Some("ff_siwis")), "ff_siwis");
        assert_eq!(voice_for_language("kokoro", "fr", Some("af_bella")), "p-fenrir60", "voix d'une autre langue");
        assert_eq!(voice_for_language("kokoro", "en", Some("F1")), "af_bella");
        assert_eq!(voice_for_language("kokoro", "en", None), "af_bella");
    }
```

Dans `src/config.rs`, module `tests`, remplacer dans
`partial_config_keeps_defaults_for_missing_fields` la ligne
`assert_eq!(parsed.voice.language, "fr");` par :

```rust
        assert_eq!(parsed.voice.language, "auto");
```

et ajouter :

```rust
    #[test]
    fn auto_is_the_default_and_speaks_french_until_a_document_says_otherwise() {
        let config = Config::default();
        assert_eq!(config.voice.language, "auto");
        assert_eq!(config.default_language(), "fr");
        assert_eq!(config.tts_params().language, "fr");
        assert_eq!(config.tts_params_in("en").language, "en");
    }

    #[test]
    fn kokoro_takes_its_voice_from_the_document_language() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.per_language.insert("en".into(), "af_sky".into());
        config.voice.per_language.insert("fr".into(), "F1".into());
        assert_eq!(config.tts_params_in("en").voice, "af_sky");
        // Inconnue : la voix par défaut de la langue, sans réécrire la valeur.
        assert_eq!(config.tts_params_in("fr").voice, "p-fenrir60");
        assert_eq!(config.voice.per_language["fr"], "F1");
    }

    #[test]
    fn voices_per_language_round_trip_through_toml() {
        let mut config = Config::default();
        config.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        config.voice.per_language.insert("en".into(), "af_bella".into());
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("[voice.per_language]"), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn switching_to_kokoro_keeps_the_former_voice_for_a_way_back() {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.voice = "F2".into();
        assert!(config.align_voice_with_family());
        assert_eq!(config.voice.voice, "F2", "la voix de Supertonic reste pour y revenir");
        assert_eq!(config.voice.per_language["fr"], "p-fenrir60");
        assert_eq!(config.voice.per_language["en"], "af_bella");
        assert!(!config.align_voice_with_family(), "rien à refaire");
    }
```

Dans `src/app/config.rs`, test `languages_cycle_through_those_of_the_model`,
remplacer `for _ in 0..31 {` par `for _ in 0..32 {`, et `for _ in 0..11 {`
par `for _ in 0..12 {`, puis les deux messages par
`"trente et une langues et « auto », un tour complet"` et
`"onze langues et « auto » chez MagpieTTS"`.

Run: `cargo test --lib config models app::config`
Expected: FAIL — `voices_follow_language`, `per_language`, `default_language`, `tts_params_in` introuvables.

- [ ] **Étape 5 : les voix Kokoro rangées par langue**

Dans `src/models.rs`, remplacer la structure `VoiceOption` et les deux
tables existantes par :

```rust
/// Une voix intégrée, proposée au choix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: &'static str,
    pub label: &'static str,
    /// Langue de la voix ; vide : toutes celles du modèle.
    pub language: &'static str,
}

/// Voix de Supertonic 3 : cinq masculines, cinq féminines.
///
/// Le modèle n'accepte pas de référence externe — pas de clonage : seuls ces
/// identifiants ont un effet.
const SUPERTONIC_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "M1", label: "M1 · masculine 1", language: "" },
    VoiceOption { id: "M2", label: "M2 · masculine 2", language: "" },
    VoiceOption { id: "M3", label: "M3 · masculine 3", language: "" },
    VoiceOption { id: "M4", label: "M4 · masculine 4", language: "" },
    VoiceOption { id: "M5", label: "M5 · masculine 5", language: "" },
    VoiceOption { id: "F1", label: "F1 · féminine 1", language: "" },
    VoiceOption { id: "F2", label: "F2 · féminine 2", language: "" },
    VoiceOption { id: "F3", label: "F3 · féminine 3", language: "" },
    VoiceOption { id: "F4", label: "F4 · féminine 4", language: "" },
    VoiceOption { id: "F5", label: "F5 · féminine 5", language: "" },
];
```

et, plus bas, `MAGPIE_VOICES` :

```rust
const MAGPIE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "Aria", label: "Aria", language: "" },
    VoiceOption { id: "Jason", label: "Jason", language: "" },
    VoiceOption { id: "John", label: "John", language: "" },
    VoiceOption { id: "Leo", label: "Leo", language: "" },
    VoiceOption { id: "Sofia", label: "Sofia", language: "" },
];

/// Voix de Kokoro : deux françaises, les 28 anglaises officielles (US, GB).
///
/// `p-fenrir60` est livrée avec Sillon (timbre de `ff_siwis`, prosodie mêlée à
/// celle d'`am_fenrir`, §11 du rapport) ; les autres viennent du paquet.
const KOKORO_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr" },
    VoiceOption { id: "ff_siwis", label: "ff_siwis · française", language: "fr" },
    VoiceOption { id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en" },
    VoiceOption { id: "af_alloy", label: "af_alloy · américaine, femme", language: "en" },
    VoiceOption { id: "af_aoede", label: "af_aoede · américaine, femme", language: "en" },
    VoiceOption { id: "af_heart", label: "af_heart · américaine, femme", language: "en" },
    VoiceOption { id: "af_jessica", label: "af_jessica · américaine, femme", language: "en" },
    VoiceOption { id: "af_kore", label: "af_kore · américaine, femme", language: "en" },
    VoiceOption { id: "af_nicole", label: "af_nicole · américaine, femme, chuchotée", language: "en" },
    VoiceOption { id: "af_nova", label: "af_nova · américaine, femme", language: "en" },
    VoiceOption { id: "af_river", label: "af_river · américaine, femme", language: "en" },
    VoiceOption { id: "af_sarah", label: "af_sarah · américaine, femme", language: "en" },
    VoiceOption { id: "af_sky", label: "af_sky · américaine, femme", language: "en" },
    VoiceOption { id: "am_adam", label: "am_adam · américaine, homme", language: "en" },
    VoiceOption { id: "am_echo", label: "am_echo · américaine, homme", language: "en" },
    VoiceOption { id: "am_eric", label: "am_eric · américaine, homme", language: "en" },
    VoiceOption { id: "am_fenrir", label: "am_fenrir · américaine, homme", language: "en" },
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

Dans `voices_for_family`, ajouter le bras `"kokoro" => KOKORO_VOICES,` avant
`_ => &[],`. Après `default_voice`, ajouter :

```rust
/// Vrai quand la famille choisit sa voix selon la langue du document.
pub fn voices_follow_language(family: &str) -> bool {
    family == "kokoro"
}

fn language_code(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

/// Voix par défaut d'une famille dans une langue ; sa voix par défaut tout
/// court si aucune n'est rangée sous cette langue.
pub fn default_voice_for_language(family: &str, language: &str) -> &'static str {
    let code = language_code(language);
    voices_for_family(family)
        .iter()
        .find(|v| v.language == code)
        .map_or_else(|| default_voice(family), |v| v.id)
}

/// La voix qui parle `language` : celle de la configuration si la famille la
/// connaît dans cette langue, sinon sa voix par défaut — sans réécrire la
/// configuration (règle des valeurs non listées).
pub fn voice_for_language(family: &str, language: &str, configured: Option<&str>) -> &'static str {
    let code = language_code(language);
    configured
        .map(str::trim)
        .and_then(|c| {
            voices_for_family(family)
                .iter()
                .find(|v| v.id == c && (v.language.is_empty() || v.language == code))
        })
        .map_or_else(|| default_voice_for_language(family, code), |v| v.id)
}
```

- [ ] **Étape 6 : `auto`, `[voice.per_language]` et les paramètres par langue**

Dans `src/config.rs`, `VoiceConfig` : ajouter **en dernier champ** (une table
TOML s'écrit après les valeurs) :

```rust
    /// Voix par langue de document, pour les familles qui en ont une par
    /// langue (Kokoro) : `fr = "p-fenrir60"`, `en = "af_bella"`.
    pub per_language: BTreeMap<String, String>,
```

Dans `impl Default for VoiceConfig`, remplacer `language: "fr".to_string(),`
par `language: crate::text::language::AUTO.to_string(),` et ajouter
`per_language: BTreeMap::new(),` après `volume: 1.0,`. Le commentaire du champ
`language` devient :

```rust
    /// `auto` : chaque document prend sa langue détectée. `fr`, `en`… : tous
    /// les documents parlent cette langue, sauf choix fait pour l'un d'eux.
    pub language: String,
```

Remplacer `tts_params` par :

```rust
    /// La langue des documents quand rien d'autre ne décide : celle de la
    /// configuration, le français pour `auto`.
    pub fn default_language(&self) -> String {
        crate::text::language::resolve(&self.voice.language, None, None)
    }

    /// Paramètres de synthèse dans la langue par défaut.
    pub fn tts_params(&self) -> TtsParams {
        self.tts_params_in(&self.default_language())
    }

    /// Paramètres de synthèse pour un document dans `language`.
    pub fn tts_params_in(&self, language: &str) -> TtsParams {
        let family = &self.engine.family;
        let voice = if crate::models::voices_follow_language(family) {
            let code = language.split(['-', '_']).next().unwrap_or(language);
            let configured = self.voice.per_language.get(code).map(String::as_str);
            crate::models::voice_for_language(family, code, configured).to_string()
        } else {
            // Une voix que la famille ne connaît pas ne doit jamais atteindre
            // le moteur : elle y provoque une erreur illisible. Un chemin de
            // WAV sous `~` est développé : le moteur ne le ferait pas.
            expand_tilde(&crate::models::sanitised_voice(family, &self.voice.voice))
        };
        TtsParams {
            voice,
            language: language.to_string(),
            pitch_shift: self.voice.pitch.clamp(-1.0, 1.0),
            energy: self.voice.energy.clamp(0.0, 2.0),
            emotion: self.voice.emotion.clone(),
            instruction: self.voice.instruction.clone(),
            temperature: self.sampling.temperature,
            top_p: self.sampling.top_p,
            top_k: self.sampling.top_k,
            guidance_scale: self.sampling.guidance_scale,
            repetition_penalty: self.sampling.repetition_penalty,
            inference_steps: self.sampling.inference_steps,
            seed: self.sampling.seed,
            extra: self.extra.clone(),
        }
    }
```

Remplacer `align_voice_with_family` par :

```rust
    /// Fait suivre la voix quand la famille change.
    ///
    /// Les préréglages sont propres à chaque modèle : garder celui du
    /// précédent afficherait un nom que le nouveau ne saurait pas honorer.
    /// Kokoro choisit sa voix par langue : `voice.voice` reste alors celle de
    /// l'ancien modèle, pour qui voudra y revenir, et seules les langues sans
    /// voix reçoivent la leur. Renvoie vrai si quelque chose a changé.
    pub fn align_voice_with_family(&mut self) -> bool {
        let family = self.engine.family.clone();
        if crate::models::voices_follow_language(&family) {
            let mut changed = false;
            for language in ["fr", "en"] {
                if !self.voice.per_language.contains_key(language) {
                    let voice = crate::models::default_voice_for_language(&family, language);
                    self.voice.per_language.insert(language.to_string(), voice.to_string());
                    changed = true;
                }
            }
            return changed;
        }
        if crate::models::knows_voice(&family, &self.voice.voice) {
            return false;
        }
        self.voice.voice = crate::models::default_voice(&family).to_string();
        true
    }
```

Dans `src/app/config.rs`, bras `Field::Language` de `adjust` :

```rust
            Field::Language => {
                // « auto » d'abord : chaque document prend alors sa langue.
                // La normalisation en dépend : effet à la prochaine ouverture.
                let mut languages = vec![crate::text::language::AUTO];
                languages.extend(models::languages_for_family(&config.engine.family));
                cycle_string(&mut config.voice.language, &languages, delta);
                Effect::Reparse
            }
```

et dans `help` (ligne `Field::Language => "Langue du texte, transmise au modèle.",`) :

```rust
            Field::Language => "Langue des documents : « auto » suit chacun d'eux.",
```

Run: `cargo test --lib text::language config models app::config`
Expected: PASS.

- [ ] **Étape 7 : la suite entière**

Run: `cargo test`
Expected: PASS. Si `reader::settings::tests::each_setting_triggers_the_effect_of_its_kind`
échoue sur `language = "en"`, c'est attendu à la Tâche 3 seulement : ici, la
langue reste un paramètre de synthèse (`Resynthesize`) côté moteur ; ne rien
y changer.

- [ ] **Étape 8 : commit**

```bash
git add Cargo.toml Cargo.lock src/text/language.rs src/text/mod.rs src/models.rs src/config.rs src/app/config.rs
git commit -m "$(cat <<'EOF'
feat(moteur): langue détectée (français, anglais), « auto » par défaut, une voix Kokoro par langue

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 2 : la normalisation suit la langue du document

`DocumentBuilder` connaît la langue du document. Dates, nombres, sigles et
rangs de titres ne sont mis en mots qu'en français ; un document anglais garde
ses chiffres (espeak-ng les lit). L'annonce du code et les lignes de tableau
parlent la langue du document. `App` synthétise dans cette langue.

**Fichiers :**
- Modifier : `src/doc/builder.rs`
- Modifier : `src/app/mod.rs` (paramètres de synthèse dans la langue du document)
- Modifier : `src/tts/preview.rs` (`start` reçoit la langue)

**Interfaces :**
- Consomme : `text::language::Lang::parse`, `Config::tts_params_in` (Tâche 1).
- Produit :
  - `DocumentBuilder::with_language(code_policy: CodePolicy, language: &str) -> Self` ; `DocumentBuilder::new(p)` = `with_language(p, "fr")` ; `DocumentBuilder::language(&self) -> &str`
  - `pub fn parse_document_in(markdown: &str, code_policy: CodePolicy, language: &str) -> Document` ; `parse_document(m, p)` = `parse_document_in(m, p, "fr")`
  - `App::synthesis_params(&self) -> TtsParams` (langue du document) ; `App::request_preview_in(&mut self, language: &str) -> Result<(), String>`
  - `tts::preview::start(previewer, player, requires_reference, config, language: &str, label)`

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/doc/builder.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn an_english_document_keeps_its_figures() {
        let d = parse_document_in("The report has 37 pages, dated 2024.\n", CodePolicy::Announce, "en");
        let s = &d.sentences[0];
        assert_eq!(s.text, "The report has 37 pages, dated 2024.");
        assert!(s.words.iter().all(|w| w.spoken.is_none()), "{:?}", s.words);
    }

    #[test]
    fn a_french_document_still_says_its_figures_in_words() {
        let d = parse_document_in("Le rapport a 37 pages.\n", CodePolicy::Announce, "fr");
        assert!(d.sentences[0].text.contains("trente-sept"), "{}", d.sentences[0].text);
        assert_eq!(parse_document("Le rapport a 37 pages.\n", CodePolicy::Announce), d);
    }

    #[test]
    fn an_english_heading_keeps_its_number_as_written() {
        let d = parse_document_in("## 1. Context\n\nSome text.", CodePolicy::Announce, "en");
        assert_eq!(d.sentences[0].text, "1. Context");
    }

    #[test]
    fn code_and_tables_are_announced_in_the_document_language() {
        let d = parse_document_in("```rust\nfn a() {}\n```", CodePolicy::Announce, "en");
        assert_eq!(d.sentences[0].text, "Code block rust.");
        let t = parse_document_in("| Name | Port |\n|---|---|\n| alpha | 8080 |\n", CodePolicy::Announce, "en");
        let spoken: Vec<&str> = t.sentences.iter().map(|s| s.text.as_str()).collect();
        assert!(spoken.contains(&"Name: alpha."), "{spoken:?}");
    }

    #[test]
    fn the_builder_remembers_its_language() {
        assert_eq!(DocumentBuilder::new(CodePolicy::Announce).language(), "fr");
        assert_eq!(DocumentBuilder::with_language(CodePolicy::Skip, "en").language(), "en");
    }
```

Dans `src/app/mod.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn the_synthesis_speaks_the_document_language() {
        let mut builder = DocumentBuilder::with_language(CodePolicy::Announce, "en");
        builder.push_block("The report has 37 pages.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::new(
            builder,
            None,
            Arc::new(SilentEngine),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        assert_eq!(app.synth.params().language, "en");
        assert_eq!(app.synthesis_params().language, "en");
    }
```

Run: `cargo test --lib doc::builder app::tests::the_synthesis_speaks_the_document_language`
Expected: FAIL — `parse_document_in`, `with_language`, `language`, `synthesis_params` introuvables.

- [ ] **Étape 2 : la langue dans `DocumentBuilder`**

Dans `src/doc/builder.rs` :

1. Dans `pub struct DocumentBuilder`, après `word_counter: usize,` :

```rust
    /// Langue du document : la mise en mots des nombres, dates et sigles est
    /// française, et ne vaut que pour un document français.
    language: String,
```

2. Remplacer `pub fn new(code_policy: CodePolicy) -> Self {` et son corps par :

```rust
    /// Un document français : la normalisation complète.
    pub fn new(code_policy: CodePolicy) -> Self {
        Self::with_language(code_policy, "fr")
    }

    /// Un document dans `language` (`fr`, `en`, ou un code d'un autre modèle).
    pub fn with_language(code_policy: CodePolicy, language: &str) -> Self {
        let mut doc = Document::default();
        // Section racine : accueille le texte qui précède tout titre.
        doc.sections.push(Section {
            id: 0,
            level: 0,
            title: "Introduction".to_string(),
            first_sentence: None,
            block_ids: Vec::new(),
        });
        Self {
            doc,
            code_policy,
            current_section: 0,
            list_stack: Vec::new(),
            quote_depth: 0,
            pending: None,
            table_cells: Vec::new(),
            table_rows: Vec::new(),
            in_table_head: false,
            in_table: false,
            emphases: Emphases::default(),
            word_counter: 0,
            language: language.trim().to_string(),
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    fn french(&self) -> bool {
        crate::text::language::Lang::parse(&self.language) == Some(crate::text::language::Lang::Fr)
    }
```

3. Dans `flush`, remplacer le bras `CodePolicy::Announce => match language { … },` par :

```rust
                    CodePolicy::Announce => {
                        let (named, bare) = if self.french() {
                            ("Bloc de code", "Bloc de code.")
                        } else {
                            ("Code block", "Code block.")
                        };
                        match language {
                            Some(lang) => format!("{named} {lang}."),
                            None => bare.to_string(),
                        }
                    }
```

4. Dans la construction des lignes de tableau, remplacer
`Some(name) if !name.is_empty() => format!("{name} : {cell}."),` par :

```rust
                        Some(name) if !name.is_empty() && french => format!("{name} : {cell}."),
                        Some(name) if !name.is_empty() => format!("{name}: {cell}."),
```

et ajouter, juste avant `for (header, cells) in rows {` :

```rust
        let french = self.french();
```

5. Dans `commit`, remplacer :

```rust
            let text = if titled {
                verbalize(&sanitize(&verbalize_section_number(raw_text)))
            } else {
                verbalize(&written)
            };
```

par :

```rust
            // Un document anglais garde ses chiffres : espeak-ng les lit, et
            // la mise en mots française les dirait en français.
            let text = match (titled, self.french()) {
                (true, true) => verbalize(&sanitize(&verbalize_section_number(raw_text))),
                (true, false) => shown.clone(),
                (false, true) => verbalize(&written),
                (false, false) => written.clone(),
            };
```

6. Dans `tokenize`, remplacer `let said = verbalize(token);` par :

```rust
            let said = if self.french() { verbalize(token) } else { token.to_string() };
```

7. Remplacer `parse_document` par :

```rust
/// Raccourci : analyse un document français complet en une fois.
pub fn parse_document(markdown: &str, code_policy: CodePolicy) -> Document {
    parse_document_in(markdown, code_policy, "fr")
}

/// Analyse un document complet dans `language`.
pub fn parse_document_in(markdown: &str, code_policy: CodePolicy, language: &str) -> Document {
    let mut builder = DocumentBuilder::with_language(code_policy, language);
    for block in crate::input::split_blocks(markdown) {
        builder.push_block(&block);
    }
    builder.into_document()
}
```

Run: `cargo test --lib doc::builder`
Expected: PASS (tests existants compris : un document français ne change pas).

- [ ] **Étape 3 : `App` synthétise dans la langue du document**

Dans `src/tts/preview.rs`, remplacer la signature et la dernière instruction de
`start` :

```rust
pub fn start(
    previewer: &mut Previewer,
    player: &mut crate::audio::Player,
    requires_reference: bool,
    config: &crate::config::Config,
    language: &str,
    label: String,
) -> Result<(), String> {
    previewer.cancel();
    player.stop_preview();
    if let Some(reason) = refusal(requires_reference, &config.voice.voice, player.is_available()) {
        return Err(reason);
    }
    player.pause();
    previewer
        .request(sample_text(language).to_string(), config.tts_params_in(language), label)
        .map_err(|err| format!("aperçu impossible : {err}"))
}
```

Dans `src/app/mod.rs` :

1. Dans `App::new`, remplacer `config.tts_params(),` par
`config.tts_params_in(builder.language()),`.
2. Dans `from_parts`, remplacer `synth.set_params(config.tts_params());` par
`synth.set_params(config.tts_params_in(builder.language()));`.
3. Remplacer `apply_tts_params` par :

```rust
    /// Paramètres de synthèse, dans la langue du document.
    pub fn synthesis_params(&self) -> TtsParams {
        self.config.tts_params_in(self.builder.language())
    }

    fn apply_tts_params(&mut self) {
        let params = self.synthesis_params();
        self.synth.set_params(params);
    }
```

4. Dans `reload_engine`, remplacer `self.config.tts_params(),` par
`self.synthesis_params(),`.
5. Remplacer `request_preview` par :

```rust
    pub fn request_preview(&mut self) -> Result<(), String> {
        let language = self.builder.language().to_string();
        self.request_preview_in(&language)
    }

    /// L'aperçu dans une langue donnée : chaque voix de l'onglet Voix
    /// s'écoute dans la sienne.
    pub fn request_preview_in(&mut self, language: &str) -> Result<(), String> {
        self.preview_error = None;
        let label = self.preview_label();
        preview::start(
            &mut self.previewer,
            &mut self.player,
            self.synth.requires_reference(),
            &self.config,
            language,
            label,
        )
    }
```

Dans `src/reader/preview.rs`, appel `sample::start(…)` de `preview_voice` :
ajouter l'argument `&self.config.default_language(),` entre `&self.config,`
et `label,`.

Run: `cargo test --lib`
Expected: PASS.

- [ ] **Étape 4 : commit**

```bash
git add src/doc/builder.rs src/app/mod.rs src/tts/preview.rs src/reader/preview.rs
git commit -m "$(cat <<'EOF'
feat(moteur): un document anglais garde ses chiffres, la synthèse parle la langue du document

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 3 : la langue de chaque document dans le lecteur

À l'ouverture, `Reader` détecte la langue (hors code), applique le choix fait
pour ce document (retenu sous son identité, suivi au renommage) ou la langue
imposée par la configuration. Changer la langue du document ou
`voice.language` remet le texte en forme à la même phrase ; changer la voix de
l'autre langue ne resynthétise rien. L'instantané dit la langue détectée,
choisie et la part de l'autre langue.

**Fichiers :**
- Créer : `src/reader/language.rs`
- Modifier : `src/reader/mod.rs` (champs, `open`, `open_file`, `rest`)
- Modifier : `src/reader/settings.rs` (`SettingsView`, `SettingsPatch`, `SettingsEffect::between_in`, `reparse` en `pub(super)`)
- Modifier : `src/reader/view.rs` (`Snapshot.document_language`)
- Modifier : `src/reader/preview.rs` (`preview_voice_in`)
- Modifier : `src/launch.rs` (langue détectée au terminal)

**Interfaces :**
- Consomme : `detect_markdown`, `resolve`, `Lang`, `MIXED_THRESHOLD` (Tâche 1) ; `DocumentBuilder::with_language`, `parse_document_in`, `App::request_preview_in` (Tâche 2).
- Produit :
  - `pub struct LanguageChoices { pub items: Vec<LanguageChoice> }` avec `load()`, `save()`, `get(&self, document: &str) -> Option<&str>`, `set(&mut self, document: &str, language: Option<&str>)`, `adopt(&mut self, former: &str, current: &str) -> bool`
  - `#[derive(Debug, Clone, PartialEq, Default)] pub struct DocumentLanguage { pub detected: Option<String>, pub chosen: Option<String>, pub effective: String, pub minority_share: f32 }` avec `fn mixed(&self) -> bool`
  - `Reader::document_language(&self) -> Option<&DocumentLanguage>` ; `Reader::current_language(&self) -> String` ; `Reader::set_document_language(&mut self, language: Option<&str>, now: Instant) -> anyhow::Result<()>` ; `Reader::preview_voice_in(&mut self, language: &str, now: Instant) -> Result<(), String>`
  - `Snapshot.document_language: Option<DocumentLanguage>`
  - `SettingsView.voices_by_language: BTreeMap<String, String>` ; `SettingsPatch.voice_for_language: Option<(String, String)>`
  - `SettingsEffect::between_in(before, after, language: &str) -> SettingsEffect` (`between` = `between_in(…, &before.default_language())`)

- [ ] **Étape 1 : écrire les tests qui échouent**

Créer `src/reader/language.rs` avec le module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::reader::settings::{SettingsEffect, SettingsPatch};
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    const EN: &str = "Monads let you chain computations that carry a context, without handling \
                      it by hand at every single step. The report has 37 pages.";
    const FR: &str = "Les monades permettent d'enchaîner des calculs qui portent un contexte, \
                      sans le manipuler à la main. Le rapport a 37 pages.";

    fn reader_with(config: Config) -> Reader {
        let mut config = config;
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    fn reader() -> Reader {
        reader_with(Config::default())
    }

    #[test]
    fn an_english_text_is_read_in_english_with_its_figures() {
        let mut r = reader();
        r.open_text(EN, "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.effective, "en");
        assert_eq!(lang.detected.as_deref(), Some("en"));
        let app = r.session().unwrap();
        assert_eq!(app.synth.params().language, "en");
        assert!(app.doc().sentences.iter().any(|s| s.text.contains("37")), "chiffres gardés");
    }

    #[test]
    fn a_short_text_takes_the_default_language_and_says_so() {
        let mut r = reader();
        r.open_text("Hello world.", "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.detected, None, "rien de fiable sous 40 lettres");
        assert_eq!(lang.effective, "fr");
        let snapshot = crate::reader::view::snapshot(&r);
        assert_eq!(snapshot.document_language.unwrap().detected, None);
    }

    #[test]
    fn a_configured_language_beats_the_detection() {
        let mut config = Config::default();
        config.voice.language = "en".into();
        let mut r = reader_with(config);
        r.open_text(FR, "sélection", Instant::now());
        assert_eq!(r.document_language().unwrap().effective, "en");
        assert_eq!(r.document_language().unwrap().detected.as_deref(), Some("fr"));
    }

    #[test]
    fn a_manual_choice_reparses_at_the_same_sentence_and_is_remembered() {
        let mut r = reader();
        r.open_text(&format!("{FR} {FR} {FR}"), "sélection", Instant::now());
        r.with_session(Instant::now(), |app| app.seek_to(2));
        r.set_document_language(Some("en"), Instant::now()).unwrap();
        assert_eq!(r.document_language().unwrap().effective, "en");
        assert_eq!(r.document_language().unwrap().chosen.as_deref(), Some("en"));
        assert_eq!(r.session().unwrap().current_sentence(), 2);
        // Le même texte, recapturé, retrouve son choix.
        r.open_text(&format!("{FR} {FR} {FR}"), "sélection", Instant::now());
        assert_eq!(r.document_language().unwrap().effective, "en");
        // Revenir à « automatique » oublie le choix.
        r.set_document_language(None, Instant::now()).unwrap();
        assert_eq!(r.document_language().unwrap().effective, "fr");
        assert!(r.set_document_language(Some("de"), Instant::now()).is_err());
    }

    #[test]
    fn a_choice_follows_a_file_from_its_former_name() {
        let mut store = LanguageChoices::default();
        store.set("notes.md", Some("en"));
        assert!(store.adopt("notes.md", "/docs/notes.md"));
        assert_eq!(store.get("/docs/notes.md"), Some("en"));
        assert_eq!(store.get("notes.md"), None);
        assert!(!store.adopt("absent.md", "/docs/absent.md"));
    }

    #[test]
    fn choices_round_trip_through_their_file() {
        let path = std::env::temp_dir().join(format!("sillon-langues-{}.json", std::process::id()));
        let mut store = LanguageChoices::default();
        store.set("/a.md", Some("en"));
        store.save_to(&path).unwrap();
        assert_eq!(LanguageChoices::load_from(&path).get("/a.md"), Some("en"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_mixed_document_reports_its_share() {
        let mut r = reader();
        r.open_text(&format!("{FR}\n\n{FR}\n\n{EN}"), "sélection", Instant::now());
        let lang = r.document_language().unwrap();
        assert_eq!(lang.effective, "fr");
        assert!(lang.mixed(), "{}", lang.minority_share);
    }

    #[test]
    fn the_language_and_the_voices_trigger_the_right_effect() {
        use SettingsEffect::*;
        let mut before = Config::default();
        before.engine.family = "kokoro".into();
        let mut after = before.clone();
        after.voice.language = "en".into();
        assert_eq!(SettingsEffect::between_in(&before, &after, "fr"), Reparse);
        let mut other = before.clone();
        other.voice.per_language.insert("en".into(), "af_sky".into());
        assert_eq!(SettingsEffect::between_in(&before, &other, "fr"), Display, "voix de l'autre langue : rien à resynthétiser");
        assert_eq!(SettingsEffect::between_in(&before, &other, "en"), Resynthesize);
    }

    #[test]
    fn a_voice_per_language_round_trips_through_a_patch() {
        let mut r = reader();
        let patch = SettingsPatch { voice_for_language: Some(("en".into(), "af_sky".into())), ..Default::default() };
        let applied = r.apply_patch(&patch, Instant::now());
        assert_eq!(applied.voices_by_language.get("en").map(String::as_str), Some("af_sky"));
        assert_eq!(r.settings().voices_by_language.len(), 1);
    }
}
```

Dans `src/reader/mod.rs`, après `pub mod handle;` :

```rust
pub mod language;
```

Run: `cargo test --lib reader::language`
Expected: FAIL — `LanguageChoices`, `document_language`, `set_document_language`, `between_in`, `voice_for_language`, `voices_by_language` introuvables.

- [ ] **Étape 2 : le magasin des choix et la langue du document**

En tête de `src/reader/language.rs` :

```rust
//! La langue de chaque document : détectée à l'ouverture, ou choisie à la
//! main et retenue sous la même identité que la position de lecture.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::text::language::{self, Lang, MIXED_THRESHOLD};

use super::Reader;

/// La langue choisie pour un document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageChoice {
    pub document: String,
    pub language: String,
}

/// Les choix de langue, dans `languages.json` à côté des positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageChoices {
    #[serde(default)]
    pub items: Vec<LanguageChoice>,
}

impl LanguageChoices {
    pub fn path() -> Option<PathBuf> {
        crate::paths::data_dir().map(|d| d.join("languages.json"))
    }

    /// Relit les choix, ou repart de zéro sans le dire : un choix perdu se
    /// refait d'un geste.
    pub fn load() -> Self {
        Self::path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("répertoire de données introuvable")?;
        self.save_to(&path)
    }

    pub(crate) fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save_to(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        crate::paths::write_atomic(path, text.as_bytes())
            .with_context(|| format!("écriture de {}", path.display()))
    }

    pub fn get(&self, document: &str) -> Option<&str> {
        self.items.iter().find(|c| c.document == document).map(|c| c.language.as_str())
    }

    /// Pose (ou, avec `None`, oublie) le choix d'un document.
    pub fn set(&mut self, document: &str, language: Option<&str>) {
        self.items.retain(|c| c.document != document);
        if let Some(language) = language {
            self.items.push(LanguageChoice { document: document.to_string(), language: language.to_string() });
        }
    }

    /// Fait passer un choix retenu sous l'ancien nom d'un fichier à son
    /// identité courante. Vrai si quelque chose a bougé.
    pub fn adopt(&mut self, former: &str, current: &str) -> bool {
        if former == current || self.get(former).is_none() {
            return false;
        }
        if self.get(current).is_some() {
            self.set(former, None);
            return true;
        }
        for item in &mut self.items {
            if item.document == former {
                item.document = current.to_string();
            }
        }
        true
    }
}

/// La langue du document ouvert, telle que l'onglet Voix la montre.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentLanguage {
    /// Langue détectée (`fr`, `en`), ou rien de fiable.
    pub detected: Option<String>,
    /// Langue choisie à la main pour ce document.
    pub chosen: Option<String>,
    /// Langue lue : le choix, sinon la configuration, sinon la détection.
    pub effective: String,
    /// Part des lettres dans l'autre langue.
    pub minority_share: f32,
}

impl DocumentLanguage {
    /// « Document mêlé » : plus de 30 % des lettres dans l'autre langue.
    pub fn mixed(&self) -> bool {
        self.minority_share > MIXED_THRESHOLD
    }
}

impl Reader {
    /// La langue d'un texte qu'on ouvre.
    pub(super) fn language_of(&self, markdown: &str, source: &str) -> DocumentLanguage {
        let detection = language::detect_markdown(markdown);
        let chosen = self.choices.get(source).map(str::to_string);
        DocumentLanguage {
            detected: detection.map(|d| d.language.code().to_string()),
            effective: language::resolve(&self.config.voice.language, chosen.as_deref(), detection),
            chosen,
            minority_share: detection.map_or(0.0, |d| d.minority_share),
        }
    }

    pub fn document_language(&self) -> Option<&DocumentLanguage> {
        self.text.as_ref().and(self.language.as_ref())
    }

    /// La langue lue : celle du document gardé, sinon la langue par défaut.
    pub fn current_language(&self) -> String {
        self.document_language()
            .map(|l| l.effective.clone())
            .unwrap_or_else(|| self.current_config().default_language())
    }

    /// Choisit la langue du document gardé (`None` : automatique). Le texte
    /// est remis en forme à la même phrase : la normalisation en dépend.
    pub fn set_document_language(&mut self, language: Option<&str>, now: Instant) -> Result<()> {
        if let Some(code) = language {
            if Lang::parse(code).is_none() {
                bail!("langue inconnue : {code}");
            }
        }
        let Some(identity) = self.document_identity().map(str::to_string) else {
            bail!("aucun document ouvert");
        };
        self.choices.set(&identity, language.map(|c| Lang::parse(c).map_or(c, Lang::code)));
        if self.persist {
            self.choices.save()?;
        }
        self.reparse(now);
        Ok(())
    }
}
```

- [ ] **Étape 3 : `Reader` ouvre chaque texte dans sa langue**

Dans `src/reader/mod.rs` :

1. Dans `pub struct Reader`, après `preview_error: Option<String>,` :

```rust
    /// Choix de langue faits à la main, par document.
    choices: language::LanguageChoices,
    /// Langue du document gardé.
    language: Option<language::DocumentLanguage>,
```

2. Dans `Reader::new`, après `preview_error: None,` :

```rust
            choices: language::LanguageChoices::load(),
            language: None,
```

3. Dans `open`, remplacer `let mut builder = DocumentBuilder::new(self.config.code_policy());` par :

```rust
        let language = self.language_of(&text.markdown, &text.source);
        let mut builder = DocumentBuilder::with_language(self.config.code_policy(), &language.effective);
        self.language = Some(language);
```

4. Dans `open_file`, remplacer les deux premières lignes du corps par :

```rust
        let markdown = crate::source::load(path)?;
        let source = crate::launch::document_identity(path);
        // Le choix de langue suit le fichier, comme sa position : il doit
        // être là avant l'ouverture, qui en dépend.
        if let Some(name) = path.file_name() {
            if self.choices.adopt(&name.to_string_lossy(), &source) && self.persist {
                let _ = self.choices.save();
            }
        }
```

5. Dans `rest`, remplacer la reconstruction du document au repos par :

```rust
        let language = self.current_language();
        self.resting_doc = self.text.as_ref().map(|t| {
            crate::doc::builder::parse_document_in(&t.markdown, self.config.code_policy(), &language)
        });
```

Dans `src/reader/settings.rs` :

1. `fn reparse(&mut self, now: Instant)` devient `pub(super) fn reparse(&mut self, now: Instant)`,
et dans sa branche « au repos », remplacer
`let doc = crate::doc::builder::parse_document(&text.markdown, self.config.code_policy());` par :

```rust
        let language = self.language_of(&text.markdown, &text.source);
        let doc = crate::doc::builder::parse_document_in(&text.markdown, self.config.code_policy(), &language.effective);
        self.language = Some(language);
```

Si l'emprunt de `text` (`self.text.as_ref()`) gêne l'appel `self.language_of`,
cloner d'abord : `let text = text.clone();` en tête de cette branche.

2. `SettingsView` : ajouter après `extra: BTreeMap<String, String>,` :

```rust
    /// `[voice.per_language]` : la voix de chaque langue (Kokoro).
    pub voices_by_language: BTreeMap<String, String>,
```

et dans `SettingsView::of`, après `extra: c.extra.clone(),` :
`voices_by_language: c.voice.per_language.clone(),`.

3. `SettingsPatch` : ajouter après `extra_remove: Vec<String>,` :

```rust
    /// La voix d'une langue : `("en", "af_sky")`.
    pub voice_for_language: Option<(String, String)>,
```

et dans `write_into`, avant la boucle `extra_set` :

```rust
        if let Some((language, voice)) = &self.voice_for_language {
            c.voice.per_language.insert(language.clone(), voice.clone());
        }
```

4. Remplacer `SettingsEffect::between` par :

```rust
    /// L'effet le plus fort, pour un document dans la langue par défaut.
    pub fn between(before: &Config, after: &Config) -> Self {
        Self::between_in(before, after, &before.default_language())
    }

    /// L'effet le plus fort de ce qui change de `before` à `after`, pour un
    /// document dans `language`.
    ///
    /// Mesuré sur la configuration, pas sur les champs présents : renvoyer
    /// une valeur déjà en place ne relance rien. La langue change la
    /// normalisation, donc le découpage ; la voix de l'autre langue ne change
    /// rien à ce qu'on entend.
    pub fn between_in(before: &Config, after: &Config, language: &str) -> Self {
        if before.engine != after.engine {
            return Self::Reload;
        }
        if before.reading.code_blocks != after.reading.code_blocks
            || before.voice.language != after.voice.language
        {
            return Self::Reparse;
        }
        let mut voice = before.voice.clone();
        voice.volume = after.voice.volume;
        voice.speed = after.voice.speed;
        voice.per_language = after.voice.per_language.clone();
        let heard = before.tts_params_in(language).voice != after.tts_params_in(language).voice;
        if voice != after.voice || heard || before.sampling != after.sampling || before.extra != after.extra {
            return Self::Resynthesize;
        }
        if before.voice.volume != after.voice.volume || before.voice.speed != after.voice.speed {
            return Self::Immediate;
        }
        if before.reading != after.reading
            || before.app != after.app
            || before.voice.per_language != after.voice.per_language
        {
            return Self::Display;
        }
        Self::Nothing
    }
```

5. Dans `apply_patch`, remplacer `let effect = SettingsEffect::between(&before, &next);` par :

```rust
        let effect = SettingsEffect::between_in(&before, &next, &self.current_language());
```

6. Dans les tests de `settings.rs` : dans `each_setting_triggers_the_effect_of_its_kind`,
remplacer `assert_eq!(effect_of(|c| c.voice.language = "en".into()), Resynthesize);` par
`assert_eq!(effect_of(|c| c.voice.language = "en".into()), Reparse, "la normalisation suit la langue");` ;
dans `every_setting_round_trips_through_a_patch`, ajouter
`voice_for_language: Some(("en".into(), "af_sky".into())),` après `extra_remove: vec![],`
du premier patch, et `voices_by_language: BTreeMap::from([("en".to_string(), "af_sky".to_string())]),`
après `extra: …,` de `expected`.

Dans `src/reader/view.rs` : ajouter au `Snapshot`, après `preview` :

```rust
    /// Langue du document gardé : détectée, choisie, part de l'autre langue.
    pub document_language: Option<super::language::DocumentLanguage>,
```

et dans `snapshot`, ajouter `document_language: reader.document_language().cloned(),`
aux deux constructions (`Snapshot { … ..Snapshot::default() }` au repos, et la
construction complète).

Dans `src/reader/preview.rs`, remplacer `preview_voice` par :

```rust
    pub fn preview_voice(&mut self, now: Instant) -> Result<(), String> {
        let language = self.current_language();
        self.preview_voice_in(&language, now)
    }

    /// La phrase d'exemple dans `language` : chaque voix s'écoute dans la
    /// sienne.
    pub fn preview_voice_in(&mut self, language: &str, now: Instant) -> Result<(), String> {
        self.last_activity = now;
        self.preview_error = None;
        self.wake(now);
        if let Some(app) = self.session.as_mut() {
            return app.request_preview_in(language);
        }
        let engine = self.ensure_engine();
        if self.lone_preview.is_none() {
            let player = (self.players)();
            self.lone_preview = Some(LonePreview { previewer: Previewer::new(Arc::clone(&engine)), player });
        }
        let lone = self.lone_preview.as_mut().expect("posé juste au-dessus");
        lone.player.set_volume(self.config.voice.volume);
        let label = format!("{} · {:.2}×", self.config.voice.voice, self.config.voice.speed);
        sample::start(
            &mut lone.previewer,
            &mut lone.player,
            engine.requires_reference(),
            &self.config,
            language,
            label,
        )
    }
```

(`wake` et `ensure_engine` sont privés à `reader/mod.rs` : les passer en
`pub(super)` s'ils ne le sont pas déjà.)

- [ ] **Étape 4 : le terminal lit aussi dans la langue du document**

Dans `src/launch.rs`, remplacer `read_input` par :

```rust
/// Un document prêt à recevoir `text` : la langue vient du choix retenu pour
/// lui, de la configuration, ou de la détection.
fn builder_for(config: &Config, text: &str, identity: Option<&str>) -> DocumentBuilder {
    let detected = crate::text::language::detect_markdown(text);
    let choices = crate::reader::language::LanguageChoices::load();
    let chosen = identity.and_then(|id| choices.get(id)).map(str::to_string);
    let language = crate::text::language::resolve(&config.voice.language, chosen.as_deref(), detected);
    DocumentBuilder::with_language(config.code_policy(), &language)
}

/// Ce qu'il y a à lire : le document assemblé, le flux qui l'alimente encore,
/// et le nom sous lequel il se retient.
fn read_input(
    args: &Args,
    config: &Config,
) -> Result<(DocumentBuilder, Option<Receiver<InputEvent>>, String)> {
    if args.clip {
        let text = read_clipboard()?;
        let mut builder = builder_for(config, &text, None);
        for block in input::split_blocks(&text) {
            builder.push_block(&block);
        }
        return Ok((builder, None, "presse-papiers".to_string()));
    }

    match &args.file {
        Some(path) => {
            // Un fichier est analysé d'emblée : sommaire et durée sont
            // disponibles dès l'ouverture.
            let text = crate::source::load_with_cache(path, !args.no_cache)?;
            let identity = document_identity(path);
            let mut builder = builder_for(config, &text, Some(&identity));
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            Ok((builder, None, identity))
        }
        None if std::io::stdin().is_terminal() => {
            // Lancé sans rien : il n'y a rien à lire, mais tout à régler.
            Ok((DocumentBuilder::with_language(config.code_policy(), &config.default_language()), None, "configuration".to_string()))
        }
        None => {
            // Le tube est mis de côté et le descripteur 0 rendu au terminal,
            // faute de quoi le clavier resterait muet en mode pipe.
            let piped = input::detach_piped_stdin().context("récupération du flux d'entrée")?;
            let stream = match piped {
                Some(file) => input::spawn_reader(file),
                None => input::spawn_reader(std::io::stdin()),
            };
            // Un flux se lit au fil de l'eau : sa langue est celle par défaut.
            Ok((DocumentBuilder::with_language(config.code_policy(), &config.default_language()), Some(stream), "stdin".to_string()))
        }
    }
}
```

Run: `cargo test`
Expected: PASS (dont `reader::language`, 9 tests).

- [ ] **Étape 5 : commit**

```bash
git add src/reader/language.rs src/reader/mod.rs src/reader/settings.rs src/reader/view.rs src/reader/preview.rs src/launch.rs
git commit -m "$(cat <<'EOF'
feat(reader): une langue par document, détectée ou choisie et retenue, remise en forme à la même phrase

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 4 : la synthèse minutée et le surlignage qui la suit

Le contrat de synthèse peut rendre le temps de chaque mot affiché. Le pool
transmet les mots, garde le minutage avec l'audio et compte son temps occupé ;
le lecteur surligne d'après le minutage quand il existe, d'après les poids
sinon, et un saut au mot tombe sur le début mesuré du mot. Les moteurs
existants ne changent pas (implémentation par défaut, sans minutage).

**Fichiers :**
- Modifier : `src/doc/mod.rs` (`WordSpan`, `spoken_words`, `word_fraction_timed`)
- Modifier : `src/tts/mod.rs` (`Synthesis`, `synthesize_timed`, `splits_first_clause`)
- Modifier : `src/tts/pool.rs` (mots, `Rendered.words`, `Part::Whole`, temps occupé)
- Modifier : `src/app/mod.rs` (mots publiés, minutages, décalage du saut)

**Interfaces :**
- Consomme : rien de nouveau.
- Produit :
  - `crate::doc::WordSpan { pub start: f32, pub end: f32 }` (secondes de synthèse, 1×), réexporté `crate::tts::WordSpan`
  - `Sentence::spoken_words(&self) -> Vec<String>` ; `Sentence::word_fraction_timed(&self, elapsed: f32, spans: &[WordSpan]) -> Option<(usize, f32)>`
  - `pub struct Synthesis { pub pcm: PcmBuffer, pub words: Option<Vec<WordSpan>> }`
  - `TtsSession::synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis>` (défaut : `synthesize`, sans minutage)
  - `TtsEngine::splits_first_clause(&self) -> bool` (défaut `false`)
  - `Synthesizer::push_with_words(&self, text: String, words: Vec<String>, pause_ms: u64)`
  - `Rendered { pub audio, pub speech, pub words: Option<Arc<[WordSpan]>>, pub part: Part }` ; `pub enum Part { Whole }` (la Tâche 5 ajoute `Head { words: usize }`)
  - `SynthesisStats.busy: Duration` (temps passé à synthétiser, cumulé)
  - `fn start_offset(start: &StartAt, rendered: &Rendered) -> Duration` (privée à `app`)

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/doc/mod.rs`, module `tests`, ajouter (le module a déjà un document
de test ; ces tests bâtissent leur phrase eux-mêmes) :

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
    fn a_measured_timing_drives_the_highlight() {
        let s = sentence_of(&["un", "deux", "trois"]);
        let spans = [
            WordSpan { start: 0.0, end: 0.5 },
            WordSpan { start: 0.5, end: 0.6 },
            WordSpan { start: 0.6, end: 1.0 },
        ];
        assert_eq!(s.word_fraction_timed(0.25, &spans), Some((0, 0.5)));
        assert_eq!(s.word_fraction_timed(0.55, &spans).map(|(w, _)| w), Some(1));
        assert_eq!(s.word_fraction_timed(5.0, &spans), Some((2, 1.0)));
        // Un minutage qui ne couvre pas les mots ne sert à rien.
        assert_eq!(s.word_fraction_timed(0.1, &spans[..2]), None);
    }

    #[test]
    fn spoken_words_prefer_the_spoken_form() {
        let mut s = sentence_of(&["en", "2027"]);
        s.words[1].spoken = Some("deux mille vingt-sept".into());
        assert_eq!(s.spoken_words(), ["en", "deux mille vingt-sept"]);
    }
```

Dans `src/tts/pool.rs`, module `tests`, ajouter :

```rust
    /// Un mot = 100 ms ; le minutage rendu suit les mots reçus. Avec
    /// `wrong`, il en oublie un.
    struct TimedEngine {
        wrong: bool,
    }
    struct TimedSession {
        wrong: bool,
    }

    impl TtsEngine for TimedEngine {
        fn name(&self) -> String {
            "minuté".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(TimedSession { wrong: self.wrong }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for TimedSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            let words = text.split_whitespace().count().max(1);
            Ok(PcmBuffer::new(vec![0.1; words * 100], 1000, 1))
        }
        fn synthesize_timed(&mut self, text: &str, words: &[String], p: &TtsParams) -> Result<super::super::Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let count = if self.wrong { words.len().saturating_sub(1) } else { words.len() };
            let spans = (0..count)
                .map(|i| crate::doc::WordSpan { start: i as f32 * 0.1, end: (i + 1) as f32 * 0.1 })
                .collect();
            Ok(super::super::Synthesis { pcm, words: Some(spans) })
        }
    }

    #[test]
    fn the_timing_of_each_word_reaches_the_rendered_audio() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: false }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux trois".into(), vec!["un".into(), "deux".into(), "trois".into()], 0);
        let rendered = wait_ready(&synth, 0);
        let words = rendered.words.expect("minutage rendu");
        assert_eq!(words.len(), 3);
        assert_eq!(words[2].end, 0.3);
        assert_eq!(rendered.part, Part::Whole);
    }

    #[test]
    fn a_timing_that_does_not_cover_the_words_is_dropped() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: true }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux".into(), vec!["un".into(), "deux".into()], 0);
        assert!(wait_ready(&synth, 0).words.is_none());
    }

    #[test]
    fn the_time_spent_synthesizing_is_counted() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("une phrase".into(), 0);
        wait_ready(&synth, 0);
        assert!(synth.stats().busy >= Duration::from_millis(25), "{:?}", synth.stats().busy);
    }
```

Dans `src/app/mod.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn a_measured_timing_wins_over_the_weights() {
        let mut app = app_reading("Un mot très long ici.");
        let spans: Arc<[crate::doc::WordSpan]> = (0..5)
            .map(|i| crate::doc::WordSpan { start: i as f32 * 0.2, end: (i + 1) as f32 * 0.2 })
            .collect();
        app.timings.insert(0, Arc::clone(&spans));
        app.player.simulate(0, Duration::from_millis(500), Duration::from_secs(1));
        assert_eq!(app.current_word_fraction().map(|(w, _)| w), Some(2));
        app.timings.clear();
        // Sans minutage, la répartition par poids reprend la main.
        assert!(app.current_word_fraction().is_some());
    }

    #[test]
    fn a_jump_to_a_word_starts_at_its_measured_time() {
        let audio = Arc::new(crate::tts::PcmBuffer::new(vec![0.0; 1000], 1000, 1));
        let spans: Arc<[crate::doc::WordSpan]> =
            vec![crate::doc::WordSpan { start: 0.0, end: 0.25 }, crate::doc::WordSpan { start: 0.25, end: 1.0 }].into();
        let rendered = crate::tts::pool::Rendered {
            audio,
            speech: Duration::from_secs(1),
            words: Some(spans),
            part: crate::tts::pool::Part::Whole,
        };
        let start = StartAt { sentence: 0, word: 1, ratio: 0.5, consumed: false };
        assert_eq!(start_offset(&start, &rendered), Duration::from_millis(250));
        let untimed = crate::tts::pool::Rendered { words: None, ..rendered };
        assert_eq!(start_offset(&start, &untimed), Duration::from_millis(500), "les poids, faute de mieux");
    }
```

Run: `cargo test --lib doc:: tts::pool app::tests`
Expected: FAIL — `WordSpan`, `Synthesis`, `push_with_words`, `Part`, `timings`, `start_offset` introuvables.

- [ ] **Étape 2 : `WordSpan` et la lecture d'un minutage**

Dans `src/doc/mod.rs`, après `pub use builder::DocumentBuilder;` :

```rust
/// Le temps d'un mot dans sa phrase, en secondes de synthèse (1×) : l'unité
/// du chronomètre du lecteur, quelle que soit la vitesse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordSpan {
    pub start: f32,
    pub end: f32,
}
```

Dans `impl Sentence`, après `word_start` :

```rust
    /// Les mots tels que la voix les dit : forme parlée, sinon écrite.
    pub fn spoken_words(&self) -> Vec<String> {
        self.words
            .iter()
            .map(|w| w.spoken.clone().unwrap_or_else(|| w.text.clone()))
            .collect()
    }

    /// Le mot prononcé à `elapsed` d'après le minutage mesuré de chaque mot,
    /// et la part déjà dite. `None` si le minutage ne couvre pas les mots.
    pub fn word_fraction_timed(&self, elapsed: f32, spans: &[WordSpan]) -> Option<(usize, f32)> {
        if spans.is_empty() || spans.len() != self.words.len() {
            return None;
        }
        for (i, span) in spans.iter().enumerate() {
            if elapsed < span.end {
                let length = (span.end - span.start).max(f32::EPSILON);
                return Some((i, ((elapsed - span.start) / length).clamp(0.0, 1.0)));
            }
        }
        Some((spans.len() - 1, 1.0))
    }
```

- [ ] **Étape 3 : le contrat minuté**

Dans `src/tts/mod.rs`, après `pub use pool::{SynthesisStats, Synthesizer};` :

```rust
pub use crate::doc::WordSpan;

/// Une phrase synthétisée, avec le temps de chacun de ses mots affichés
/// quand le moteur sait le dire.
#[derive(Debug, Clone)]
pub struct Synthesis {
    pub pcm: PcmBuffer,
    pub words: Option<Vec<WordSpan>>,
}
```

Dans le trait `TtsEngine`, après `readiness` :

```rust
    /// Vrai quand le moteur gagne à synthétiser la phrase attendue en deux
    /// morceaux (tête jusqu'à la première virgule, puis la suite).
    fn splits_first_clause(&self) -> bool {
        false
    }
```

Remplacer le trait `TtsSession` par :

```rust
/// Une session de synthèse, utilisée par un seul thread.
pub trait TtsSession: Send {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer>;

    /// Comme `synthesize`, avec le temps de chaque mot de `words` — les
    /// formes parlées des mots affichés, dans l'ordre. Sans minutage par
    /// défaut : le lecteur répartit alors la durée selon les lettres.
    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        let _ = words;
        Ok(Synthesis { pcm: self.synthesize(text, params)?, words: None })
    }
}
```

- [ ] **Étape 4 : le pool transmet les mots et garde le minutage**

Dans `src/tts/pool.rs` :

1. `use super::{PcmBuffer, TtsEngine, TtsParams};` devient
`use super::{PcmBuffer, Synthesis, TtsEngine, TtsParams, WordSpan};`.

2. `Job` :

```rust
#[derive(Debug, Clone)]
struct Job {
    text: String,
    /// Formes parlées des mots affichés, pour le minutage.
    words: Vec<String>,
    pause_ms: u64,
}

/// Ce qu'un worker prend : la phrase, ses mots, et sous quels paramètres.
struct Picked {
    id: usize,
    text: String,
    words: Vec<String>,
    params: TtsParams,
    generation: u64,
    pause_ms: u64,
}
```

3. `Rendered` :

```rust
/// Quelle partie d'une phrase un audio porte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// La phrase entière.
    Whole,
}

/// Audio prêt à jouer pour une phrase.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Parole suivie du silence de fin de bloc.
    pub audio: Arc<PcmBuffer>,
    /// Durée de la parole seule, hors silence.
    ///
    /// Le curseur de mots s'appuie dessus : inclure le silence ferait traîner le
    /// dernier mot jusqu'au bloc suivant.
    pub speech: Duration,
    /// Temps de chaque mot affiché, quand le moteur le donne.
    pub words: Option<Arc<[WordSpan]>>,
    pub part: Part,
}
```

4. `SynthesisStats` : ajouter `pub busy: Duration,` après `pub workers: usize,`.
`State` : ajouter `busy: Duration,` après `failures`, initialisé à
`Duration::ZERO` dans `Synthesizer::new`. Dans `stats()`, ajouter
`busy: state.busy,`.

5. `State::next_job` rend un `Picked` :

```rust
    fn next_job(&self) -> Option<Picked> {
        let end = (self.cursor + self.window).min(self.jobs.len());
        for id in self.cursor..end {
            if self.in_flight.contains(&id) || self.failures.contains_key(&id) {
                continue;
            }
            if self.cache.get(&id).is_some_and(|e| e.generation == self.generation) {
                continue;
            }
            let job = &self.jobs[id];
            return Some(Picked {
                id,
                text: job.text.clone(),
                words: job.words.clone(),
                params: self.params.clone(),
                generation: self.generation,
                pause_ms: job.pause_ms,
            });
        }
        None
    }
```

6. `push` et `push_with_words` :

```rust
    /// Ajoute une phrase à la fin du document, sans mots à minuter.
    pub fn push(&self, text: String, pause_ms: u64) {
        self.push_with_words(text, Vec::new(), pause_ms);
    }

    /// Ajoute une phrase et les formes parlées de ses mots affichés.
    pub fn push_with_words(&self, text: String, words: Vec<String>, pause_ms: u64) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.push(Job { text, words, pause_ms });
        drop(state);
        self.shared.wake.notify_all();
    }
```

7. Dans `worker_loop`, remplacer la prise du travail et la synthèse :

```rust
        let job = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(job) = state.next_job() {
                    state.in_flight.insert(job.id);
                    break job;
                }
                state = shared.wake.wait(state).unwrap();
            }
        };

        if session.is_none() {
            match shared.engine.open_session() {
                Ok(s) => session = Some(s),
                Err(err) => {
                    finish_generation(
                        &shared,
                        job.id,
                        job.generation,
                        Err(format!("ouverture de session : {err}")),
                    );
                    continue;
                }
            }
        }

        let session = session.as_mut().expect("session ouverte juste au-dessus");
        let outcome = synthesize(&shared, session.as_mut(), &job.text, &job.words, &job.params)
            .map(|s| with_trailing_silence(s, job.words.len(), job.pause_ms));
        finish_generation(&shared, job.id, job.generation, outcome);
```

et ajouter, avant `with_trailing_silence` :

```rust
/// Une synthèse, chronométrée : le temps occupé est la mesure du filtre de
/// charge. Un son vide est un échec.
fn synthesize(
    shared: &Arc<Shared>,
    session: &mut dyn super::TtsSession,
    text: &str,
    words: &[String],
    params: &TtsParams,
) -> Result<Synthesis, String> {
    let started = std::time::Instant::now();
    let outcome = session.synthesize_timed(text, words, params);
    shared.state.lock().unwrap().busy += started.elapsed();
    match outcome {
        Ok(s) if s.pcm.is_empty() => Err("aucun échantillon produit".to_string()),
        Ok(s) => Ok(s),
        Err(e) => Err(e.to_string()),
    }
}
```

8. `with_trailing_silence` reçoit la synthèse et le nombre de mots attendus :

```rust
/// Ajoute le silence de fin de bloc à la parole.
///
/// Ce silence porte la hiérarchie à l'oreille : une pause plus longue après un
/// titre qu'après un item de liste, sans jamais prononcer le moindre marqueur.
/// Un minutage qui ne couvre pas exactement les mots est écarté.
fn with_trailing_silence(synthesis: Synthesis, expected_words: usize, pause_ms: u64) -> Rendered {
    let speech = synthesis.pcm;
    let words = synthesis
        .words
        .filter(|w| expected_words > 0 && w.len() == expected_words)
        .map(Arc::from);
    let speech_duration = speech.duration;
    if pause_ms == 0 {
        return Rendered { audio: Arc::new(speech), speech: speech_duration, words, part: Part::Whole };
    }
    let frames = (speech.sample_rate as u64 * pause_ms / 1000) as usize;
    let mut samples = Vec::with_capacity(speech.samples.len() + frames * speech.channels as usize);
    samples.extend_from_slice(&speech.samples);
    samples.resize(samples.len() + frames * speech.channels as usize, 0.0);
    Rendered {
        audio: Arc::new(PcmBuffer::new(samples, speech.sample_rate, speech.channels)),
        speech: speech_duration,
        words,
        part: Part::Whole,
    }
}
```

Le test existant `trailing_silence_extends_audio_but_not_speech` devient :

```rust
    #[test]
    fn trailing_silence_extends_audio_but_not_speech() {
        let pcm = PcmBuffer::new(vec![0.5; 1000], 1000, 1);
        let rendered = with_trailing_silence(Synthesis { pcm, words: None }, 0, 500);
        assert_eq!(rendered.speech, Duration::from_millis(1000));
        assert_eq!(rendered.audio.duration, Duration::from_millis(1500));
        assert_eq!(rendered.audio.samples.len(), 1500);
    }
```

Run: `cargo test --lib tts::pool`
Expected: PASS.

- [ ] **Étape 5 : le lecteur publie les mots et surligne d'après le minutage**

Dans `src/app/mod.rs` :

1. Dans `pub struct App`, après `start_at: Option<StartAt>,` :

```rust
    /// Minutage des mots des phrases en file, par phrase.
    timings: std::collections::HashMap<usize, Arc<[crate::doc::WordSpan]>>,
```

et dans `from_parts`, après `start_at: None,` : `timings: std::collections::HashMap::new(),`.

2. `publish_new_sentences` :

```rust
    /// Transmet à la synthèse les phrases apparues depuis le dernier tour,
    /// avec leurs mots : un moteur qui sait minuter les mots s'en sert.
    fn publish_new_sentences(&mut self) {
        let total = self.builder.sentence_count();
        while self.published < total {
            let (text, words, pause) = {
                let sentence = &self.builder.document().sentences[self.published];
                (sentence.text.clone(), sentence.spoken_words(), sentence.trailing_pause_ms)
            };
            self.synth.push_with_words(text, words, pause);
            self.published += 1;
        }
    }
```

et, dans `reload_engine`, la boucle de réinjection :

```rust
        for sentence in &self.builder.document().sentences {
            synth.push_with_words(sentence.text.clone(), sentence.spoken_words(), sentence.trailing_pause_ms);
        }
```

suivie, après `self.player.flush();` en fin de fonction, de `self.timings.clear();`.

3. `current_word_fraction` :

```rust
    /// Mot prononcé dans la phrase courante, et la part de ce mot déjà dite :
    /// d'après le minutage mesuré s'il existe, d'après les lettres sinon.
    pub fn current_word_fraction(&self) -> Option<(usize, f32)> {
        let position = self.player.position()?;
        let sentence = self.doc().sentence(position.sentence_id)?;
        let elapsed = position.elapsed.as_secs_f32();
        if let Some(found) = self
            .timings
            .get(&position.sentence_id)
            .and_then(|spans| sentence.word_fraction_timed(elapsed, spans))
        {
            return Some(found);
        }
        sentence.word_fraction(elapsed, position.speech.as_secs_f32())
    }
```

4. Après `fn relay_done`, ajouter :

```rust
/// Où commencer une phrase après un saut au mot : au début mesuré du mot si
/// le minutage existe, sinon à sa part de la parole selon les poids.
fn start_offset(start: &StartAt, rendered: &crate::tts::pool::Rendered) -> Duration {
    match rendered.words.as_deref().and_then(|spans| spans.get(start.word)) {
        Some(span) => Duration::from_secs_f32(span.start.max(0.0)),
        None => rendered.speech.mul_f32(start.ratio),
    }
}
```

5. Dans `fill_output`, remplacer le calcul de `offset` et l'empilement par :

```rust
                let offset = match self.start_at.as_mut() {
                    Some(start) if start.sentence == self.queue_head && !start.consumed => {
                        start.consumed = true;
                        start_offset(start, &rendered)
                    }
                    _ => Duration::ZERO,
                };
                if let Some(spans) = &rendered.words {
                    self.timings.insert(self.queue_head, Arc::clone(spans));
                }
                self.player.enqueue_from(
                    self.queue_head,
                    &rendered.audio,
                    rendered.speech,
                    offset,
                );
```

6. Dans `advance_playback`, après `let finished = self.player.reconcile();` :

```rust
        for id in &finished {
            self.timings.remove(id);
        }
```

7. Dans `seek_to`, après `self.player.flush();` : `self.timings.clear();`.

Run: `cargo test`
Expected: PASS.

- [ ] **Étape 6 : commit**

```bash
git add src/doc/mod.rs src/tts/mod.rs src/tts/pool.rs src/app/mod.rs
git commit -m "$(cat <<'EOF'
feat(moteur): synthèse minutée par mot, surlignage et saut au mot sur le temps mesuré

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 5 : le premier son en deux morceaux

Quand la phrase du curseur n'est pas prête (démarrage, saut, resynthèse) et
que le moteur le demande, le pool la synthétise en deux morceaux : la tête
jusqu'à la première virgule, point-virgule ou deux-points (au moins 3 mots),
puis la suite. Le lecteur joue la tête dès qu'elle est prête et enchaîne la
suite sans blanc ; un saut à un mot de la suite ne fait pas entendre la tête.
Le lecteur compte aussi les coupures (sortie vide pendant la lecture).

**Fichiers :**
- Modifier : `src/tts/pool.rs` (`Part::Head`, `split_first_clause`, `tail`)
- Modifier : `src/tts/lazy.rs` (`splitting_first_clause`)
- Modifier : `src/audio/player.rs` (`enqueue_part`, segments numérotés)
- Modifier : `src/app/mod.rs` (attente de la suite, coupures)

**Interfaces :**
- Consomme : `Synthesis`, `Rendered`, `Part`, `push_with_words`, `splits_first_clause` (Tâche 4).
- Produit :
  - `Part::Head { words: usize }` (nombre de mots affichés dans la tête)
  - `pub(crate) struct Split { pub head_text: String, pub head_words: Vec<String>, pub tail_text: String, pub tail_words: Vec<String> }` ; `pub(crate) fn split_first_clause(words: &[String]) -> Option<Split>`
  - `Synthesizer::tail(&self, id: usize) -> Option<Rendered>` (minutage relatif au début de la suite)
  - `LazyEngine::splitting_first_clause(self) -> Self`
  - `Player::enqueue_part(&mut self, sentence_id: usize, audio: &Arc<PcmBuffer>, part_speech: Duration, base: Duration, skip: Duration)`
  - `App::stalls(&self) -> usize`

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/tts/pool.rs`, module `tests` :

```rust
    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn the_first_clause_is_cut_after_three_words_at_least() {
        let split = split_first_clause(&words("Le quatorze juillet, Camille harangua la foule.")).unwrap();
        assert_eq!(split.head_text, "Le quatorze juillet,");
        assert_eq!(split.head_words.len(), 3);
        assert_eq!(split.tail_text, "Camille harangua la foule.");
        assert!(split_first_clause(&words("Oui, bien sûr, tout à fait.")).is_none(), "tête trop courte");
        assert!(split_first_clause(&words("Une phrase sans virgule du tout.")).is_none());
        assert!(split_first_clause(&words("Trois mots ici,")).is_none(), "rien après");
        assert_eq!(split_first_clause(&words("Pour le corriger ; lancez les tests.")).unwrap().head_words.len(), 4);
    }

    /// Minute ses mots (100 ms chacun) et coupe la phrase attendue.
    struct SplittingEngine {
        texts: Arc<std::sync::Mutex<Vec<String>>>,
    }
    struct SplittingSession {
        texts: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TtsEngine for SplittingEngine {
        fn name(&self) -> String {
            "coupe".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(SplittingSession { texts: Arc::clone(&self.texts) }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn splits_first_clause(&self) -> bool {
            true
        }
    }

    impl TtsSession for SplittingSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            self.texts.lock().unwrap().push(text.to_string());
            let n = text.split_whitespace().count().max(1);
            Ok(PcmBuffer::new(vec![0.1; n * 100], 1000, 1))
        }
        fn synthesize_timed(&mut self, text: &str, words: &[String], p: &TtsParams) -> Result<Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let spans = (0..words.len())
                .map(|i| WordSpan { start: i as f32 * 0.1, end: (i + 1) as f32 * 0.1 })
                .collect();
            Ok(Synthesis { pcm, words: Some(spans) })
        }
    }

    fn wait_tail(synth: &Synthesizer, id: usize) -> Rendered {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = synth.tail(id) {
                return r;
            }
            assert!(std::time::Instant::now() < deadline, "suite {id} jamais prête");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn only_the_awaited_sentence_is_cut_in_two() {
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = Arc::new(SplittingEngine { texts: Arc::clone(&texts) });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        let second = "Paris bascula, en quelques heures, sans un mot.";
        synth.push_with_words(first.into(), words(first), 300);
        synth.push_with_words(second.into(), words(second), 0);

        let head = wait_ready(&synth, 0);
        assert_eq!(head.part, Part::Head { words: 3 });
        assert_eq!(head.words.as_ref().map(|w| w.len()), Some(3));
        let tail = wait_tail(&synth, 0);
        assert_eq!(tail.part, Part::Whole);
        assert_eq!(tail.words.as_ref().map(|w| w.len()), Some(4));
        assert!(tail.audio.duration > tail.speech, "la pause de fin de bloc va à la suite");
        assert_eq!(wait_ready(&synth, 1).part, Part::Whole, "les phrases suivantes restent entières");
        assert_eq!(texts.lock().unwrap()[..2], ["Le quatorze juillet,", "Camille harangua la foule."]);
    }
```

Dans `src/tts/lazy.rs`, module `tests` :

```rust
    #[test]
    fn the_cut_is_announced_before_the_model_is_there() {
        let counter = Arc::new(AtomicUsize::new(0));
        let engine = LazyEngine::spawn("Kokoro".into(), 1, fallback(&counter), || {
            thread::sleep(Duration::from_millis(200));
            Ok(Arc::new(Stub("réel", Arc::new(AtomicUsize::new(0)))) as Arc<dyn TtsEngine>)
        })
        .splitting_first_clause();
        assert!(engine.is_loading());
        assert!(engine.splits_first_clause());
    }
```

Dans `src/audio/player.rs`, module `tests` (le helper `active` reçoit
`seq: 0`, voir étape 4) :

```rust
    #[test]
    fn the_tail_of_a_sentence_continues_its_clock() {
        const RATE: u32 = 1000;
        let (mut player, mut output) = Player::idle(RATE);
        let head = Arc::new(PcmBuffer::new(vec![0.1; 1000], RATE, 1));
        let tail = Arc::new(PcmBuffer::new(vec![0.1; 500], RATE, 1));
        player.enqueue_part(0, &head, Duration::from_secs(1), Duration::ZERO, Duration::ZERO);
        player.enqueue_part(0, &tail, Duration::from_millis(500), Duration::from_secs(1), Duration::ZERO);
        player.play();
        player.reconcile();
        assert_eq!(player.position().unwrap().speech, Duration::from_secs(1));
        // La tête sort entière : la suite prend la main, au même mot près.
        let _: Vec<f32> = output.by_ref().take(1200).collect();
        player.reconcile();
        let p = player.position().expect("la suite sonne");
        assert_eq!(p.sentence_id, 0);
        assert!(p.elapsed >= Duration::from_secs(1), "{:?}", p.elapsed);
        assert_eq!(p.speech, Duration::from_millis(1500));
    }
```

Dans `src/app/mod.rs`, module `tests` :

```rust
    /// Le moteur de coupe des tests du pool, recopié : minute ses mots et
    /// coupe la phrase attendue.
    struct Cutting;
    struct CuttingSession;

    impl TtsEngine for Cutting {
        fn name(&self) -> String {
            "coupe".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn crate::tts::TtsSession>> {
            Ok(Box::new(CuttingSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn splits_first_clause(&self) -> bool {
            true
        }
    }

    impl crate::tts::TtsSession for CuttingSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> anyhow::Result<crate::tts::PcmBuffer> {
            let n = text.split_whitespace().count().max(1);
            // Une seconde par mot, en durées exactes.
            Ok(crate::tts::PcmBuffer::new(vec![0.1; n * 1024], 1024, 1))
        }
        fn synthesize_timed(
            &mut self,
            text: &str,
            words: &[String],
            p: &TtsParams,
        ) -> anyhow::Result<crate::tts::Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let spans = (0..words.len())
                .map(|i| crate::doc::WordSpan { start: i as f32, end: (i + 1) as f32 })
                .collect();
            Ok(crate::tts::Synthesis { pcm, words: Some(spans) })
        }
    }

    fn cutting_app(text: &str) -> (App, rodio::queue::SourcesQueueOutput<f32>) {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block(text);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let (player, output) = crate::audio::Player::idle(1024);
        let app = App::new(builder, None, Arc::new(Cutting), config, "t".into(), Vec::new(), player);
        (app, output)
    }

    fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(app) {
            assert!(Instant::now() < deadline, "jamais atteint");
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_tail_follows_the_head_in_the_queue() {
        let (mut app, _output) = cutting_app("Le quatorze juillet, Camille harangua la foule.");
        tick_until(&mut app, |a| a.queue_head >= 1);
        assert_eq!(app.player.queued_parts(), [(0, Duration::ZERO), (0, Duration::from_secs(3))]);
        assert_eq!(app.timings.get(&0).map(|t| t.len()), Some(7), "tête et suite mises bout à bout");
        assert_eq!(app.timings[&0][3].start, 3.0, "la suite commence à la fin de la tête");
    }

    #[test]
    fn a_jump_into_the_tail_skips_the_head() {
        let (mut app, _output) = cutting_app("Le quatorze juillet, Camille harangua la foule.");
        app.seek_to_word(0, 5);
        tick_until(&mut app, |a| a.queue_head >= 1);
        let parts = app.player.queued_parts();
        assert_eq!(parts.len(), 1, "la tête n'est pas jouée : {parts:?}");
        // Base 3 s (la tête) + 2 s (deux mots de la suite) : le mot 5.
        assert_eq!(parts[0], (0, Duration::from_secs(5)));
    }
```

et dans `src/audio/player.rs`, dans `impl Player`, la sonde de test :

```rust
    /// Les parties en file : phrase et position de départ dans sa parole.
    #[cfg(test)]
    pub fn queued_parts(&self) -> Vec<(usize, Duration)> {
        self.queued.iter().map(|q| (q.sentence_id, q.offset)).collect()
    }
```

Run: `cargo test --lib tts::pool tts::lazy audio::player app::tests`
Expected: FAIL — `split_first_clause`, `Part::Head`, `tail`, `splitting_first_clause`, `enqueue_part`, `queued_parts` introuvables.

- [ ] **Étape 2 : la coupe dans le pool**

Dans `src/tts/pool.rs` :

1. `Part` reçoit la tête :

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// La phrase entière.
    Whole,
    /// La tête de la phrase attendue ; la suite vient par `Synthesizer::tail`.
    Head { words: usize },
}
```

2. `Entry` garde la suite :

```rust
struct Entry {
    rendered: Rendered,
    /// La suite d'une phrase coupée, quand elle est prête.
    tail: Option<Rendered>,
    generation: u64,
}
```

(dans `finish_generation`, `Entry { rendered, generation }` devient
`Entry { rendered, tail: None, generation }`).

3. `State` : ajouter `split_cursor: bool,` (initialisé dans `Synthesizer::new`
à `engine.splits_first_clause()`, lu **avant** de déplacer `engine` dans
`Shared`), et `Picked` : `split: bool`. Dans `next_job`, renseigner
`split: self.split_cursor && id == self.cursor,`.

4. Après `Picked`, ajouter :

```rust
/// Une phrase coupée en tête et suite.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Split {
    pub head_text: String,
    pub head_words: Vec<String>,
    pub tail_text: String,
    pub tail_words: Vec<String>,
}

/// Coupe après le premier mot qui finit par une virgule, un point-virgule
/// ou deux-points, à condition que la tête ait au moins trois mots et que
/// la suite ne soit pas vide. La coupe peut toucher l'intonation : elle est
/// réservée à la phrase qui conditionne l'attente.
pub(crate) fn split_first_clause(words: &[String]) -> Option<Split> {
    let cut = words.iter().position(|w| w.ends_with([',', ';', ':']))?;
    if cut < 2 || cut + 1 >= words.len() {
        return None;
    }
    let (head, tail) = words.split_at(cut + 1);
    Some(Split {
        head_text: head.join(" "),
        head_words: head.to_vec(),
        tail_text: tail.join(" "),
        tail_words: tail.to_vec(),
    })
}
```

5. Dans `impl Synthesizer`, après `get` :

```rust
    /// La suite d'une phrase coupée, si elle est prête et à jour. Son
    /// minutage part du début de la suite.
    pub fn tail(&self, id: usize) -> Option<Rendered> {
        let state = self.shared.state.lock().unwrap();
        state
            .cache
            .get(&id)
            .filter(|e| e.generation == state.generation)
            .and_then(|e| e.tail.clone())
    }
```

6. Dans `worker_loop`, remplacer les trois dernières instructions (de
`let session = …` à `finish_generation(…)`) par :

```rust
        let session = session.as_mut().expect("session ouverte juste au-dessus");
        let split = if job.split { split_first_clause(&job.words) } else { None };
        let Some(split) = split else {
            let outcome = synthesize(&shared, session.as_mut(), &job.text, &job.words, &job.params)
                .map(|s| with_trailing_silence(s, job.words.len(), job.pause_ms));
            finish_generation(&shared, job.id, job.generation, outcome);
            continue;
        };
        // La tête d'abord, jouable aussitôt ; la phrase reste « en cours »
        // jusqu'à la suite.
        let head = synthesize(&shared, session.as_mut(), &split.head_text, &split.head_words, &job.params)
            .map(|s| {
                let mut rendered = with_trailing_silence(s, split.head_words.len(), 0);
                rendered.part = Part::Head { words: split.head_words.len() };
                rendered
            });
        match head {
            Ok(rendered) => finish_head(&shared, job.id, job.generation, rendered),
            Err(message) => {
                finish_generation(&shared, job.id, job.generation, Err(message));
                continue;
            }
        }
        let tail = synthesize(&shared, session.as_mut(), &split.tail_text, &split.tail_words, &job.params)
            .map(|s| with_trailing_silence(s, split.tail_words.len(), job.pause_ms));
        finish_tail(&shared, job.id, job.generation, tail);
```

7. Après `finish_generation`, ajouter :

```rust
/// Range la tête d'une phrase coupée ; la phrase reste en cours.
fn finish_head(shared: &Arc<Shared>, id: usize, generation: u64, rendered: Rendered) {
    let mut state = shared.state.lock().unwrap();
    if generation == state.generation {
        state.cache.insert(id, Entry { rendered, tail: None, generation });
    }
    drop(state);
    shared.wake.notify_all();
}

/// Range la suite d'une phrase coupée. Un échec de la suite se dit comme
/// celui d'une phrase : la lecture passe à la suivante.
fn finish_tail(shared: &Arc<Shared>, id: usize, generation: u64, outcome: Result<Rendered, String>) {
    let mut state = shared.state.lock().unwrap();
    state.in_flight.remove(&id);
    if generation == state.generation {
        match outcome {
            Ok(rendered) => {
                if let Some(entry) = state.cache.get_mut(&id) {
                    entry.tail = Some(rendered);
                }
            }
            Err(message) => {
                state.failures.insert(id, message);
            }
        }
    }
    drop(state);
    shared.wake.notify_all();
}
```

Dans `src/tts/lazy.rs` : ajouter le champ `splits: bool` à `LazyEngine`
(initialisé à `false` dans `spawn`), puis :

```rust
    /// Annonce d'avance que le moteur chargé coupera la phrase attendue : le
    /// pool en décide à sa création, avant la fin du chargement.
    pub fn splitting_first_clause(mut self) -> Self {
        self.splits = true;
        self
    }
```

et dans `impl TtsEngine for LazyEngine` :

```rust
    fn splits_first_clause(&self) -> bool {
        self.splits
    }
```

Run: `cargo test --lib tts::pool tts::lazy`
Expected: PASS.

- [ ] **Étape 3 : le lecteur joue une phrase en plusieurs parties**

Dans `src/audio/player.rs` :

1. `Queued` et `Active` reçoivent `seq: u64` (« numéro de la partie dans
la file »), et `Player` un compteur `next_seq: u64` (initialisé à 0 dans
`new`, `silent`). Le helper de test `active` pose `seq: 0`, et `simulate`
aussi.

2. Remplacer `enqueue_from` par :

```rust
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
        self.enqueue_part(sentence_id, audio, speech, Duration::ZERO, offset);
    }

    /// Empile une partie de phrase : `base` est la parole de la phrase qui
    /// la précède (la tête, pour une suite), `skip` ce qu'on passe dans
    /// cette partie. Le chronomètre de la phrase part de `base + skip`.
    pub fn enqueue_part(
        &mut self,
        sentence_id: usize,
        audio: &Arc<PcmBuffer>,
        part_speech: Duration,
        base: Duration,
        skip: Duration,
    ) {
        let Some(sink) = &self.sink else { return };
        let skip = skip.min(part_speech);
        // Le son reste celui de la synthèse, à 1× ; la vitesse s'applique en
        // le jouant, et peut changer en route.
        sink.append(Tempo::new(
            Arc::clone(&audio.samples),
            audio.channels,
            audio.sample_rate,
            self.output_rate.unwrap_or(audio.sample_rate),
            audio.sample_index(skip),
            audio.sample_index(part_speech),
            self.speed.clone(),
        ));
        self.next_seq += 1;
        self.queued.push_back(Queued {
            sentence_id,
            seq: self.next_seq,
            total: base + audio.duration,
            speech: base + part_speech,
            offset: base + skip,
        });
        if self.playing {
            sink.play();
        }
    }
```

3. Dans `reconcile`, la nouvelle partie se reconnaît à son numéro :

```rust
            Some(front) => {
                let changed = self.active.as_ref().map(|a| a.seq) != Some(front.seq);
                if changed {
                    self.active = Some(Active {
                        sentence_id: front.sentence_id,
                        seq: front.seq,
                        total: front.total,
                        speech: front.speech,
                        accumulated: front.offset,
                        resumed_at: self.playing.then(Instant::now),
                        speed: self.speed.get(),
                    });
                }
            }
```

Run: `cargo test --lib audio::player`
Expected: PASS.

- [ ] **Étape 4 : `App` attend la suite et compte les coupures**

Dans `src/app/mod.rs` :

1. Après `struct StartAt`, ajouter :

```rust
/// La tête d'une phrase coupée est partie (ou a été passée) : sa suite est
/// attendue avant toute autre phrase.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AwaitingTail {
    sentence: usize,
    /// Parole de la tête : là où la suite reprend le chronomètre.
    base: Duration,
    head_words: usize,
}
```

2. Dans `pub struct App`, après `timings` :

```rust
    awaiting_tail: Option<AwaitingTail>,
    /// Coupures : la sortie s'est vidée en pleine lecture faute de son prêt.
    stalls: usize,
    /// La sortie est déjà vide : une coupure ne compte qu'une fois.
    starved: bool,
```

initialisés dans `from_parts` à `None`, `0`, `true` (rien n'a encore joué).

3. Remplacer `fill_output` par :

```rust
    /// Alimente la sortie audio, sans jamais la laisser prendre trop d'avance.
    ///
    /// Deux parties en file suffisent à enchaîner sans blanc, tout en gardant
    /// la possibilité de sauter avec une latence imperceptible. Une phrase
    /// coupée entre en deux fois : sa tête dès qu'elle est prête, puis sa
    /// suite, au même chronomètre.
    fn fill_output(&mut self) {
        if !self.player.is_available() {
            return;
        }
        while self.player.queue_len() < 2 {
            if let Some(waiting) = self.awaiting_tail {
                if let Some(tail) = self.synth.tail(waiting.sentence) {
                    self.enqueue_tail(waiting, &tail);
                    continue;
                }
                if self.synth.failure(waiting.sentence).is_some() {
                    self.awaiting_tail = None;
                    self.queue_head = waiting.sentence + 1;
                    continue;
                }
                break;
            }
            let total = self.builder.sentence_count();
            if self.queue_head >= total {
                break;
            }
            if let Some(rendered) = self.synth.get(self.queue_head) {
                let words = self
                    .builder
                    .document()
                    .sentence(self.queue_head)
                    .map(|s| s.words.len())
                    .unwrap_or(0);
                if let crate::tts::pool::Part::Head { words: head_words } = rendered.part {
                    self.enqueue_head(head_words, &rendered);
                    continue;
                }
                self.pace.record(rendered.speech.as_secs_f64(), words);
                let offset = match self.start_at.as_mut() {
                    Some(start) if start.sentence == self.queue_head && !start.consumed => {
                        start.consumed = true;
                        start_offset(start, &rendered)
                    }
                    _ => Duration::ZERO,
                };
                if let Some(spans) = &rendered.words {
                    self.timings.insert(self.queue_head, Arc::clone(spans));
                }
                self.player.enqueue_from(self.queue_head, &rendered.audio, rendered.speech, offset);
                self.starved = false;
                self.queue_head += 1;
                continue;
            }
            // Une phrase que le moteur refuse ne doit pas bloquer la lecture.
            if self.synth.failure(self.queue_head).is_some() {
                self.skipped.insert(self.queue_head);
                self.queue_head += 1;
                continue;
            }
            break; // Synthèse en cours : on attend.
        }
        let reading = self.player.is_playing() && self.queue_head < self.builder.sentence_count();
        if reading && self.player.queue_len() == 0 && !self.starved {
            self.stalls += 1;
            self.starved = true;
        }
    }

    /// La tête d'une phrase coupée : jouée, sauf si le saut en cours vise un
    /// mot de la suite.
    fn enqueue_head(&mut self, head_words: usize, head: &crate::tts::pool::Rendered) {
        let id = self.queue_head;
        if let Some(spans) = &head.words {
            self.timings.insert(id, Arc::clone(spans));
        }
        let start = self.start_at.filter(|s| s.sentence == id && !s.consumed);
        match start {
            Some(s) if s.word >= head_words => {}
            Some(s) => {
                if let Some(start) = self.start_at.as_mut() {
                    start.consumed = true;
                }
                self.player.enqueue_part(id, &head.audio, head.speech, Duration::ZERO, start_offset(&s, head));
                self.starved = false;
            }
            None => {
                self.player.enqueue_part(id, &head.audio, head.speech, Duration::ZERO, Duration::ZERO);
                self.starved = false;
            }
        }
        self.awaiting_tail = Some(AwaitingTail { sentence: id, base: head.speech, head_words });
    }

    /// La suite d'une phrase coupée, au chronomètre de la tête ; son
    /// minutage est mis bout à bout avec celui de la tête.
    fn enqueue_tail(&mut self, waiting: AwaitingTail, tail: &crate::tts::pool::Rendered) {
        let id = waiting.sentence;
        let shift = waiting.base.as_secs_f32();
        let head = self.timings.remove(&id);
        if let (Some(head), Some(rest)) = (head, &tail.words) {
            let joined: Vec<crate::doc::WordSpan> = head
                .iter()
                .copied()
                .chain(rest.iter().map(|w| crate::doc::WordSpan { start: w.start + shift, end: w.end + shift }))
                .collect();
            self.timings.insert(id, joined.into());
        }
        let skip = match self.start_at.as_mut() {
            Some(start) if start.sentence == id && !start.consumed => {
                start.consumed = true;
                let word = start.word.saturating_sub(waiting.head_words);
                tail.words
                    .as_deref()
                    .and_then(|w| w.get(word))
                    .map_or(Duration::ZERO, |w| Duration::from_secs_f32(w.start.max(0.0)))
            }
            _ => Duration::ZERO,
        };
        let words = self.builder.document().sentence(id).map_or(0, |s| s.words.len());
        self.pace.record((waiting.base + tail.speech).as_secs_f64(), words);
        self.player.enqueue_part(id, &tail.audio, tail.speech, waiting.base, skip);
        self.starved = false;
        self.awaiting_tail = None;
        self.queue_head = id + 1;
    }

    /// Coupures comptées depuis l'ouverture (mesure du filtre de charge).
    pub fn stalls(&self) -> usize {
        self.stalls
    }
```

4. Dans `seek_to`, après `self.timings.clear();` :

```rust
        self.awaiting_tail = None;
        // La sortie vidée par un saut n'est pas une coupure.
        self.starved = true;
```

et dans `reload_engine`, après `self.timings.clear();` :
`self.awaiting_tail = None; self.starved = true;`.

Run: `cargo test`
Expected: PASS.

- [ ] **Étape 5 : commit**

```bash
git add src/tts/pool.rs src/tts/lazy.rs src/audio/player.rs src/app/mod.rs
git commit -m "$(cat <<'EOF'
feat(moteur): la phrase attendue se synthétise en tête et suite, enchaînées sans blanc ; coupures comptées

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 6 : le catalogue multi-fichiers et les paquets Kokoro

Un paquet peut porter des fichiers compagnons (configuration, voix),
téléchargés dans le dossier de sa famille ; il n'est installé que complet.
Les deux paquets Kokoro partagent `models/kokoro/config.json` et
`models/kokoro/voices/`. Le paquet MLX n'est proposé que si l'app a fourni le
calcul MLX ; le paquet int8 seulement avec `kokoro-onnx`, et jamais à côté du
MLX. La famille `kokoro` déclare ses langues, ses voix par langue, ses
réglages honorés (voix, langue) et une session.

**Fichiers :**
- Modifier : `Cargo.toml` (feature `kokoro-onnx`, vide pour l'instant)
- Modifier : `src/models.rs`
- Modifier : `src/reader/mod.rs` (`mlx`, `set_mlx_compute`, `package`)
- Modifier : `src/reader/settings.rs` (catalogue selon le calcul, tailles totales, voix avec leur langue, fiche)
- Modifier : `src/reader/data.rs` (`self.package`)
- Modifier : `src/tui/ui.rs`, `src/app/mod.rs` (tailles totales affichées)

**Interfaces :**
- Consomme : `KOKORO_VOICES`, `VoiceOption.language` (Tâche 1).
- Produit :
  - `pub struct Companion { repo, revision, file, dest: &'static str, bytes: u64 }` ; `Companion::url(&self) -> String`
  - `pub enum Runtime { AudioCpp, KokoroMlx, KokoroOnnx }`
  - `ModelPackage` gagne `companions: &'static [Companion]`, `runtime: Runtime`, `voices_label: &'static str`, `licenses: &'static str` ; `ModelPackage::total_bytes(&self) -> u64`
  - `pub fn available_with(mlx: bool) -> Vec<&'static ModelPackage>` (`available()` = `available_with(false)`)
  - `pub fn is_installed_in(root: &Path, p: &ModelPackage) -> bool`, `pub fn remove_in(root: &Path, p: &ModelPackage) -> Result<()>` (les versions sans `_in` passent `models_dir()`)
  - `pub fn kokoro_dir(model_path: &str) -> Option<PathBuf>` (dossier d'un paquet Kokoro complet : `config.json` présent)
  - `Reader::set_mlx_compute(&mut self, available: bool)` ; `Reader::package(&self, id: &str) -> Result<&'static ModelPackage>`
  - `VoiceChoice { id, label, language }` ; `VoiceModel` gagne `voices_label`, `licenses`

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `src/models.rs`, module `tests`, remplacer `exactly_one_package_is_recommended`
et `catalog_is_empty_without_a_compiled_family` par :

```rust
    #[test]
    fn kokoro_is_the_recommended_voice_wherever_it_runs() {
        assert!(CATALOG.iter().filter(|p| p.recommended).all(|p| p.family == "kokoro"));
        for mlx in [true, false] {
            let recommended = available_with(mlx).into_iter().filter(|p| p.recommended).count();
            assert!(recommended <= 1, "deux paquets recommandés côte à côte (mlx = {mlx})");
        }
    }

    #[test]
    fn audio_cpp_packages_need_a_compiled_family() {
        let audio_cpp: Vec<_> = available().into_iter().filter(|p| p.runtime == Runtime::AudioCpp).collect();
        assert_eq!(audio_cpp.is_empty(), compiled_families().is_empty());
    }

    #[test]
    fn the_mlx_package_needs_the_app_and_never_sits_next_to_onnx() {
        let with_mlx: Vec<&str> = available_with(true).iter().map(|p| p.id).collect();
        assert!(with_mlx.contains(&"kokoro_82m_mlx"));
        assert!(!with_mlx.contains(&"kokoro_82m_int8"), "pas d'ORT dans l'app");
        let without: Vec<&str> = available_with(false).iter().map(|p| p.id).collect();
        assert!(!without.contains(&"kokoro_82m_mlx"));
        assert_eq!(without.contains(&"kokoro_82m_int8"), cfg!(feature = "kokoro-onnx"));
    }

    #[test]
    fn kokoro_packages_are_pinned_and_carry_their_voices() {
        let mlx = package("kokoro_82m_mlx");
        assert_eq!(mlx.revision, "a71e4d38b236d968966a2002c4c895dbd12b1c3c");
        assert_eq!(mlx.bytes, 327_115_152);
        let int8 = package("kokoro_82m_int8");
        assert_eq!(int8.revision, "dd4401a9add81ac692d20e240d22ec9dda82cc29");
        assert_eq!(int8.file, "onnx/model_quantized.onnx");
        assert_eq!(int8.bytes, 92_361_055);
        // config.json et 29 voix : ff_siwis et les 28 anglaises.
        assert_eq!(mlx.companions.len(), 30);
        assert_eq!(mlx.companions, int8.companions, "voix et configuration communes");
        assert_eq!(mlx.total_bytes(), 327_115_152 + 2_351 + 29 * 522_320);
        assert!(mlx.companions.iter().any(|c| c.dest == "voices/af_bella.safetensors"));
        assert!(mlx.companions.iter().all(|c| c.repo == "mlx-community/Kokoro-82M-bf16"));
        assert_eq!(
            mlx.companions[0].url(),
            "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/config.json"
        );
    }

    #[test]
    fn kokoro_reads_only_its_voice_and_language() {
        for ignored in ["pitch_shift", "energy", "emotion", "temperature", "seed", "instruction"] {
            assert!(!honours("kokoro", ignored), "{ignored}");
        }
        assert!(honours("kokoro", "voice") && honours("kokoro", "language"));
        assert_eq!(sessions_for_family("kokoro"), 1);
        assert_eq!(languages_for_family("kokoro"), ["fr", "en"]);
    }

    /// Crée les fichiers d'un paquet, creux (taille annoncée, aucun octet
    /// écrit) : 327 Mo ne coûtent rien sur APFS ni ext4.
    fn sparse(root: &Path, package: &ModelPackage, with_companions: bool) {
        let mut files = vec![(main_path_in(root, package), package.bytes)];
        if with_companions {
            files.extend(package.companions.iter().map(|c| (companion_path_in(root, package, c), c.bytes)));
        }
        for (path, bytes) in files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        }
    }

    #[test]
    fn a_package_is_installed_only_when_every_file_is_there() {
        let root = scratch("complet-kokoro");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, false);
        assert!(!is_installed_in(&root, mlx), "voix absentes");
        sparse(&root, mlx, true);
        assert!(is_installed_in(&root, mlx));
        let voice = companion_path_in(&root, mlx, &mlx.companions[3]);
        std::fs::File::create(&voice).unwrap().set_len(10).unwrap();
        assert!(!is_installed_in(&root, mlx), "une voix tronquée");
        assert!(is_truncated_in(&root, mlx));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_one_kokoro_package_keeps_what_the_other_needs() {
        let root = scratch("partage-kokoro");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, mlx, true);
        sparse(&root, int8, true);
        remove_in(&root, mlx).unwrap();
        assert!(!main_path_in(&root, mlx).exists());
        assert!(is_installed_in(&root, int8), "voix et configuration restent pour l'autre");
        remove_in(&root, int8).unwrap();
        assert!(!companion_path_in(&root, int8, &int8.companions[0]).exists(), "plus personne n'en a besoin");
        assert!(!root.join("kokoro").exists(), "dossier de famille retiré");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_kokoro_dir_is_one_with_its_configuration() {
        let root = scratch("dossier-kokoro");
        let main = root.join("kokoro").join("kokoro-v1_0.safetensors");
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        std::fs::write(&main, b"x").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), None, "config.json manque");
        std::fs::write(root.join("kokoro").join("config.json"), b"{}").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), Some(root.join("kokoro")));
        std::fs::remove_dir_all(&root).ok();
    }
```

Dans `src/reader/settings.rs`, module `tests`, dans
`capabilities_follow_the_active_family`, remplacer
`VoiceChoice { id: "M1".into(), label: "M1 · masculine 1".into() }` par
`VoiceChoice { id: "M1".into(), label: "M1 · masculine 1".into(), language: String::new() }`,
puis ajouter :

```rust
    #[test]
    fn kokoro_lists_its_voices_with_their_language() {
        let caps = reader_on("kokoro").capabilities();
        assert_eq!(caps.voices.len(), 30);
        assert_eq!(caps.voices[0], VoiceChoice {
            id: "p-fenrir60".into(),
            label: "p-fenrir60 · française, recommandée".into(),
            language: "fr".into(),
        });
        assert_eq!(caps.honoured, ["voice", "language"]);
        assert_eq!(caps.languages, ["fr", "en"]);
    }

    #[test]
    fn the_mlx_package_is_offered_only_once_the_app_provides_the_compute() {
        let mut r = reader();
        assert!(!r.voices().iter().any(|v| v.id == "kokoro_82m_mlx"));
        assert!(r.package("kokoro_82m_mlx").is_err());
        r.set_mlx_compute(true);
        let mlx = r.voices().into_iter().find(|v| v.id == "kokoro_82m_mlx").expect("proposé");
        assert!(mlx.recommended);
        assert_eq!(mlx.bytes, crate::models::CATALOG[0].total_bytes());
        assert!(mlx.licenses.contains("GPL-3.0"));
        assert!(mlx.voices_label.contains("p-fenrir60"));
        assert!(!r.voices().iter().any(|v| v.id == "kokoro_82m_int8"));
    }
```

Run: `cargo test --lib models reader::settings`
Expected: FAIL — `Runtime`, `available_with`, `companions`, `is_installed_in`, `set_mlx_compute`… introuvables.

- [ ] **Étape 2 : déclarer la feature**

Dans `Cargo.toml`, section `[features]`, après `ocr = …` :

```toml
# Kokoro au terminal par ONNX Runtime (≥ 1.31, chargé dynamiquement). Jamais
# dans l'app macOS, qui calcule Kokoro en MLX. Ses dépendances arrivent avec
# le moteur ONNX.
kokoro-onnx = []
```

- [ ] **Étape 3 : compagnons, environnement d'exécution, paquets Kokoro**

Dans `src/models.rs` :

1. Avant `pub struct ModelPackage`, ajouter :

```rust
/// Un fichier de plus, téléchargé dans le dossier de la famille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Companion {
    pub repo: &'static str,
    pub revision: &'static str,
    /// Chemin dans le dépôt.
    pub file: &'static str,
    /// Chemin sous `models/<famille>/`. Deux paquets d'une famille qui
    /// nomment le même `dest` le partagent.
    pub dest: &'static str,
    pub bytes: u64,
}

impl Companion {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.revision, self.file)
    }
}

/// Ce qu'un paquet demande pour se charger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// Une famille compilée dans audio.cpp.
    AudioCpp,
    /// Kokoro en MLX : seulement dans l'app, qui fournit le calcul.
    KokoroMlx,
    /// Kokoro par ONNX Runtime : seulement avec la feature `kokoro-onnx`.
    KokoroOnnx,
}
```

2. Dans `pub struct ModelPackage`, après `pub pace: &'static str,` :

```rust
    /// Fichiers téléchargés à côté du fichier principal.
    pub companions: &'static [Companion],
    pub runtime: Runtime,
    /// Les voix, pour la fiche (« Française : … · Anglaise : … »).
    pub voices_label: &'static str,
    /// Les licences, pour la fiche.
    pub licenses: &'static str,
```

et dans `impl ModelPackage` :

```rust
    /// Taille annoncée du paquet entier, compagnons compris.
    pub fn total_bytes(&self) -> u64 {
        self.bytes + self.companions.iter().map(|c| c.bytes).sum::<u64>()
    }
```

3. Les trois paquets existants reçoivent, après `pace: …,` :

```rust
        companions: &[],
        runtime: Runtime::AudioCpp,
        voices_label: "",
        licenses: "",
```

et `magpie_tts_q8_0` passe à `recommended: false,`.

4. Avant `pub const CATALOG`, ajouter :

```rust
const KOKORO_MLX_REPO: &str = "mlx-community/Kokoro-82M-bf16";
const KOKORO_MLX_REVISION: &str = "a71e4d38b236d968966a2002c4c895dbd12b1c3c";
const KOKORO_ONNX_REPO: &str = "onnx-community/Kokoro-82M-v1.0-ONNX-timestamped";
const KOKORO_ONNX_REVISION: &str = "dd4401a9add81ac692d20e240d22ec9dda82cc29";
const KOKORO_LANGUAGES: &[&str] = &["fr", "en"];
/// Une voix Kokoro : 510 × 256 flottants, en safetensors.
pub const KOKORO_VOICE_BYTES: u64 = 522_320;

macro_rules! kokoro_voice {
    ($name:literal) => {
        Companion {
            repo: KOKORO_MLX_REPO,
            revision: KOKORO_MLX_REVISION,
            file: concat!("voices/", $name, ".safetensors"),
            dest: concat!("voices/", $name, ".safetensors"),
            bytes: KOKORO_VOICE_BYTES,
        }
    };
}

/// Ce que les deux paquets Kokoro partagent : la configuration (elle porte
/// le vocabulaire des phonèmes) et les voix. `p-fenrir60` n'y est pas : elle
/// est livrée avec Sillon.
const KOKORO_SHARED: &[Companion] = &[
    Companion {
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "config.json",
        dest: "config.json",
        bytes: 2_351,
    },
    kokoro_voice!("ff_siwis"),
    kokoro_voice!("af_bella"),
    kokoro_voice!("af_alloy"),
    kokoro_voice!("af_aoede"),
    kokoro_voice!("af_heart"),
    kokoro_voice!("af_jessica"),
    kokoro_voice!("af_kore"),
    kokoro_voice!("af_nicole"),
    kokoro_voice!("af_nova"),
    kokoro_voice!("af_river"),
    kokoro_voice!("af_sarah"),
    kokoro_voice!("af_sky"),
    kokoro_voice!("am_adam"),
    kokoro_voice!("am_echo"),
    kokoro_voice!("am_eric"),
    kokoro_voice!("am_fenrir"),
    kokoro_voice!("am_liam"),
    kokoro_voice!("am_michael"),
    kokoro_voice!("am_onyx"),
    kokoro_voice!("am_puck"),
    kokoro_voice!("am_santa"),
    kokoro_voice!("bf_alice"),
    kokoro_voice!("bf_emma"),
    kokoro_voice!("bf_isabella"),
    kokoro_voice!("bf_lily"),
    kokoro_voice!("bm_daniel"),
    kokoro_voice!("bm_fable"),
    kokoro_voice!("bm_george"),
    kokoro_voice!("bm_lewis"),
];

const KOKORO_VOICES_LABEL: &str = "Française : p-fenrir60, ff_siwis · Anglaise : 28 voix";
```

5. En tête de `CATALOG` (avant MagpieTTS : `first_installed` préfère ainsi
Kokoro) :

```rust
    ModelPackage {
        id: "kokoro_82m_mlx",
        family: "kokoro",
        display_name: "Kokoro 82M · MLX",
        description: "Voix naturelle et légère, calculée par la puce graphique.",
        precision: "bf16",
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "kokoro-v1_0.safetensors",
        languages_label: "Français, anglais",
        bytes: 327_115_152,
        recommended: true,
        pace: "Quinze fois plus vite que la parole sur M5 : lecture fluide.",
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroMlx,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: "Poids et voix Apache-2.0 · code MLX MIT · espeak-ng GPL-3.0 (programme séparé)",
    },
    ModelPackage {
        id: "kokoro_82m_int8",
        family: "kokoro",
        display_name: "Kokoro 82M · int8",
        description: "Voix naturelle et légère, calculée par le processeur (ONNX Runtime ≥ 1.31).",
        precision: "int8",
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_ONNX_REPO,
        revision: KOKORO_ONNX_REVISION,
        file: "onnx/model_quantized.onnx",
        languages_label: "Français, anglais",
        bytes: 92_361_055,
        recommended: true,
        pace: "Huit à dix fois plus vite que la parole sur processeur : lecture fluide.",
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroOnnx,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: "Poids et voix Apache-2.0 · ONNX Runtime MIT · espeak-ng GPL-3.0 (programme séparé)",
    },
```

6. Remplacer `available` par :

```rust
/// Catalogue restreint à ce que ce binaire sait charger.
///
/// audio.cpp : les familles compilées. Kokoro MLX : seulement quand l'app
/// fournit le calcul (`mlx`), et alors jamais le paquet ONNX à côté. Kokoro
/// int8 : seulement avec la feature `kokoro-onnx`.
pub fn available_with(mlx: bool) -> Vec<&'static ModelPackage> {
    let families = compiled_families();
    CATALOG
        .iter()
        .filter(|p| match p.runtime {
            Runtime::AudioCpp => families.contains(&p.family),
            Runtime::KokoroMlx => mlx,
            Runtime::KokoroOnnx => !mlx && cfg!(feature = "kokoro-onnx"),
        })
        .collect()
}

/// Le catalogue du terminal : sans calcul MLX.
pub fn available() -> Vec<&'static ModelPackage> {
    available_with(false)
}
```

7. Remplacer le bloc de `install_path` à `remove` (inclus) par :

```rust
pub fn install_path(package: &ModelPackage) -> Option<PathBuf> {
    models_dir().map(|d| main_path_in(&d, package))
}

fn main_path_in(root: &Path, package: &ModelPackage) -> PathBuf {
    root.join(package.family).join(package.file_name())
}

fn companion_path_in(root: &Path, package: &ModelPackage, companion: &Companion) -> PathBuf {
    root.join(package.family).join(companion.dest)
}

/// Ce qu'il faut télécharger pour ce paquet : adresse, destination, taille.
pub(crate) fn planned_files_in(root: &Path, package: &ModelPackage) -> Vec<(String, PathBuf, u64)> {
    let mut files = vec![(package.url(), main_path_in(root, package), package.bytes)];
    files.extend(
        package
            .companions
            .iter()
            .map(|c| (c.url(), companion_path_in(root, package, c), c.bytes)),
    );
    files
}
```

(garder `is_active_path`, `is_active_path_with`, `is_active_package` tels quels), puis :

```rust
/// Taille sur disque du fichier principal, s'il est là.
///
/// Un téléchargement interrompu laisse un `.part`, qui n'est jamais confondu
/// avec un modèle complet.
pub fn installed_size(package: &ModelPackage) -> Option<u64> {
    let path = install_path(package)?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn size_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Installé : le fichier principal est là, et chaque compagnon a sa taille.
pub fn is_installed_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)).is_some()
        && package
            .companions
            .iter()
            .all(|c| size_of(&companion_path_in(root, package, c)) == Some(c.bytes))
}

pub fn is_installed(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_installed_in(&root, package))
}

/// Vrai si un fichier du paquet est là sans avoir la taille annoncée.
///
/// Un téléchargement interrompu ailleurs que par nous, un disque plein, une
/// copie tronquée : le fichier existe et se présente comme installé, puis le
/// moteur refuse de l'ouvrir sur un message illisible. Mieux vaut le dire.
pub fn is_truncated_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)).is_some_and(|s| s != package.bytes)
        || package.companions.iter().any(|c| {
            size_of(&companion_path_in(root, package, c)).is_some_and(|s| s != c.bytes)
        })
}

pub fn is_truncated(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_truncated_in(&root, package))
}

/// Premier modèle installé, celui que le lecteur choisit par défaut.
pub fn first_installed() -> Option<&'static ModelPackage> {
    available().into_iter().find(|p| is_installed(p))
}

/// Le dossier d'un paquet Kokoro complet, d'après son fichier principal :
/// celui où se trouvent `config.json` et `voices/`.
pub fn kokoro_dir(model_path: &str) -> Option<PathBuf> {
    let dir = Path::new(model_path).parent()?;
    (Path::new(model_path).is_file() && dir.join("config.json").is_file()).then(|| dir.to_path_buf())
}

pub fn remove(package: &ModelPackage) -> Result<()> {
    let root = models_dir().context("répertoire de données introuvable")?;
    remove_in(&root, package)
}

/// Supprime un paquet. Un compagnon partagé reste tant qu'un autre paquet de
/// la famille, présent, le nomme : retirer le paquet MLX ne casse pas l'int8.
pub fn remove_in(root: &Path, package: &ModelPackage) -> Result<()> {
    let path = main_path_in(root, package);
    std::fs::remove_file(&path).with_context(|| format!("suppression de {}", path.display()))?;
    for companion in package.companions {
        let shared = CATALOG.iter().any(|other| {
            other.id != package.id
                && other.family == package.family
                && main_path_in(root, other).exists()
                && other.companions.iter().any(|c| c.dest == companion.dest)
        });
        if !shared {
            let _ = std::fs::remove_file(companion_path_in(root, package, companion));
        }
    }
    // Laisse le dossier de famille propre s'il devient vide.
    let family = root.join(package.family);
    let _ = std::fs::remove_dir(family.join("voices"));
    let _ = std::fs::remove_dir(&family);
    Ok(())
}
```

8. Dans `voices_for_family`… (déjà fait Tâche 1). Dans `sessions_for_family`,
ajouter avant `_ =>` :

```rust
        // Kokoro calcule en série (MLX comme ORT) : une session.
        "kokoro" => 1,
```

et dans `honoured_options`, avant `_ =>` :

```rust
        // Kokoro : la voix et la langue ; la vitesse s'applique au son.
        "kokoro" => &["voice", "language"],
```

9. Le téléchargement couvre tous les fichiers. Remplacer `Download::start` par :

```rust
    /// Lance le téléchargement dans un thread dédié : fichier principal puis
    /// compagnons, en reprenant ce qui est déjà là.
    pub fn start(package: &'static ModelPackage) -> Result<Self> {
        let root = models_dir().context("répertoire de données introuvable")?;
        let files = planned_files_in(&root, package);
        let received = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(package.total_bytes()));
        let outcome = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = (Arc::clone(&received), Arc::clone(&outcome), Arc::clone(&cancel));
        thread::spawn(move || {
            let (received, outcome, cancel) = worker;
            let mut base = 0u64;
            let mut result = Ok(());
            for (url, destination, bytes) in files {
                if size_of(&destination) == Some(bytes) {
                    base += bytes;
                    received.store(base, Ordering::Relaxed);
                    continue;
                }
                result = fetch(&url, &destination, bytes, &received, base, &cancel);
                if result.is_err() || cancel.load(Ordering::Relaxed) {
                    break;
                }
                base += bytes;
            }
            let state = match result {
                Ok(()) if cancel.load(Ordering::Relaxed) => Progress::Cancelled,
                Ok(()) => Progress::Done,
                Err(err) => Progress::Failed(err.to_string()),
            };
            *outcome.lock().unwrap() = Some(state);
        });

        Ok(Self { package, received, total, outcome, cancel })
    }
```

`Download::fake` : `total: Arc::new(AtomicU64::new(package.total_bytes())),`.

10. `fetch` prend la taille attendue et le décalage déjà reçu (le total ne
bouge plus : il est celui du paquet) :

```rust
fn fetch(
    url: &str,
    destination: &Path,
    expected: u64,
    received: &AtomicU64,
    base: u64,
    cancel: &AtomicBool,
) -> Result<()> {
```

Dans son corps : supprimer `let expected = total.load(Ordering::Relaxed);` ;
`received.store(expected, …)` (cas `Resume::Complete`) devient
`received.store(base + expected, Ordering::Relaxed);` ; supprimer le bloc
`if let Some(length) = response.header("Content-Length") … { … total.store(full, …); }` ;
`received.store(already, …)` devient `received.store(base + already, …)`,
`received.store(0, …)` devient `received.store(base, …)`, et dans la boucle
`received.store(written, …)` devient `received.store(base + written, …)`.
Les créations de dossier (`create_dir_all(parent)`) couvrent déjà
`voices/`.

Run: `cargo test --lib models`
Expected: PASS.

- [ ] **Étape 4 : le lecteur et son catalogue**

Dans `src/reader/mod.rs` : champ `mlx: bool,` dans `Reader` (après
`language`), `mlx: false,` dans `new`, et :

```rust
    /// L'app fournit le calcul MLX : le paquet Kokoro MLX est proposé.
    pub fn set_mlx_compute(&mut self, available: bool) {
        self.mlx = available;
    }

    /// Un paquet du catalogue que ce lecteur sait charger.
    pub fn package(&self, id: &str) -> anyhow::Result<&'static crate::models::ModelPackage> {
        crate::models::available_with(self.mlx)
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("voix inconnue : {id}"))
    }
```

Dans `src/reader/settings.rs` :
- supprimer la fonction libre `pub(super) fn package(id: &str)` ; dans
  `install_voice` et `use_voice`, `let package = package(id)?;` devient
  `let package = self.package(id)?;` ;
- `voices()` : `models::available_with(self.mlx).into_iter().map(|p| voice_model(p, &active)).collect()` ;
- `VoiceModel` : ajouter `pub voices_label: String,` et `pub licenses: String,`
  après `pace`, remplis dans `voice_model` par `p.voices_label.to_string()` et
  `p.licenses.to_string()`, et `bytes: p.total_bytes(),` ;
- `VoiceChoice` : ajouter `pub language: String,` ; dans `capabilities`,
  `VoiceChoice { id: v.id.to_string(), label: v.label.to_string(), language: v.language.to_string() }` ;
- `download()` : les trois `d.package.bytes` deviennent `d.package.total_bytes()`.

Dans `src/reader/data.rs` : supprimer `use super::settings::package;` et
remplacer `self.remove_package(package(id)?)` par
`let package = self.package(id)?; self.remove_package(package)`.

Dans `src/tui/ui.rs` (deux occurrences) et `src/app/mod.rs` (une
occurrence), `models::human_size(package.bytes)` devient
`models::human_size(package.total_bytes())`.

Run: `cargo test`
Expected: PASS.

- [ ] **Étape 5 : commit**

```bash
git add Cargo.toml src/models.rs src/reader/mod.rs src/reader/settings.rs src/reader/data.rs src/tui/ui.rs src/app/mod.rs
git commit -m "$(cat <<'EOF'
feat(modeles): paquets multi-fichiers, Kokoro MLX (app) et int8 (terminal), voix et configuration partagées

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 7 : la phonétisation par espeak-ng, dans un processus séparé

espeak-ng 1.52.0 (GPL-3.0) est construit comme un programme à part, avec ses
données réduites au français et à l'anglais. Le moteur le lance une fois par
langue (`fr-fr`, `en-us`) et lui parle ligne à ligne ; une phrase sentinelle
marque la fin de chaque réponse. Le traitement de misaki (ponctuation gardée,
liens `^`, table de remplacements) est porté pour obtenir les phonèmes sur
lesquels Kokoro a été entraîné. La phrase et ses mots seuls partent dans la
même requête.

**Fichiers :**
- Créer : `scripts/build-espeak.sh`
- Créer : `src/tts/kokoro/mod.rs` (déclarations seulement, complété Tâche 9)
- Créer : `src/tts/kokoro/phonemes.rs`
- Modifier : `src/tts/mod.rs` (`pub mod kokoro;`)
- Créer : `tests/fixtures/kokoro/misaki-phonemes.json` (référence du spike)

**Interfaces :**
- Consomme : rien.
- Produit :
  - `pub trait Phonemizer: Send { fn phonemize(&mut self, language: &str, items: &[String]) -> anyhow::Result<Vec<String>>; fn stop(&mut self) {} }`
  - `pub struct EspeakProcess` ; `EspeakProcess::new(program: PathBuf, data: Option<PathBuf>) -> Self` ; `EspeakProcess::locate() -> anyhow::Result<Self>`
  - `pub fn locate_program() -> Option<PathBuf>` (`SILLON_ESPEAK`, à côté du binaire, `PATH`) ; `pub fn data_dir() -> Option<PathBuf>` (`SILLON_ESPEAK_DATA`)
  - `pub(crate) fn finish(language: &str, raw: &str) -> String` (traitement misaki) ; `pub(crate) const SENTINEL: &str = "Zyzomys."`
  - binaire `target/espeak/bin/espeak-ng`, données `target/espeak/share/espeak-ng-data/`, licence `target/espeak/COPYING`, adresse des sources `target/espeak/SOURCE.txt`

- [ ] **Étape 1 : construire espeak-ng**

Créer `scripts/build-espeak.sh` (exécutable) :

```bash
#!/usr/bin/env bash
# Construit espeak-ng (GPL-3.0) comme programme séparé, pour la phonétisation
# de Kokoro. Sillon ne s'y lie pas : il le lance et lui parle par ses entrées
# et sorties standard, ce qui fait de l'ensemble un agrégat.
#
#   ./scripts/build-espeak.sh
#
# Produit target/espeak/bin/espeak-ng, target/espeak/share/espeak-ng-data
# (français et anglais seulement), target/espeak/COPYING et SOURCE.txt.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=1.52.0
URL="https://github.com/espeak-ng/espeak-ng/archive/refs/tags/$VERSION.tar.gz"
OUT=target/espeak
SRC="$OUT/espeak-ng-$VERSION"
mkdir -p "$OUT"
if [ ! -d "$SRC" ]; then
    curl -fsSL "$URL" -o "$OUT/espeak-ng-$VERSION.tar.gz"
    tar -xzf "$OUT/espeak-ng-$VERSION.tar.gz" -C "$OUT"
fi

# Programme autonome : bibliothèque statique, ni son, ni MBROLA, ni Klatt.
cmake -S "$SRC" -B "$OUT/build" -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF -DUSE_ASYNC=OFF -DUSE_MBROLA=OFF -DUSE_LIBSONIC=OFF \
    -DUSE_LIBPCAUDIO=OFF -DUSE_KLATT=OFF -DUSE_SPEECHPLAYER=OFF \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DCMAKE_OSX_ARCHITECTURES=arm64
cmake --build "$OUT/build" -j

BIN=$(find "$OUT/build" -type f -name espeak-ng -perm -u+x | head -1)
DATA=$(find "$OUT/build" -type d -name espeak-ng-data | head -1)
for f in phontab phonindex phondata intonations fr_dict en_dict; do
    if [ ! -f "$DATA/$f" ]; then
        echo "Erreur : $DATA/$f manque (construction des données d'espeak-ng)" >&2
        exit 1
    fi
done

rm -rf "$OUT/bin" "$OUT/share"
mkdir -p "$OUT/bin" "$OUT/share/espeak-ng-data"
cp "$BIN" "$OUT/bin/espeak-ng"
for f in phontab phonindex phondata phondata-manifest intonations fr_dict en_dict; do
    [ -f "$DATA/$f" ] && cp "$DATA/$f" "$OUT/share/espeak-ng-data/"
done
cp -R "$DATA/lang" "$OUT/share/espeak-ng-data/"
[ -d "$DATA/voices" ] && cp -R "$DATA/voices" "$OUT/share/espeak-ng-data/"
cp "$SRC/COPYING" "$OUT/COPYING"
echo "espeak-ng $VERSION, sources exactes : $URL" > "$OUT/SOURCE.txt"

# Contrôle : une phrase française, une anglaise.
printf 'Bonjour.\n' | "$OUT/bin/espeak-ng" -q --ipa --tie -v fr-fr --path="$OUT/share"
printf 'Hello.\n' | "$OUT/bin/espeak-ng" -q --ipa --tie -v en-us --path="$OUT/share"
du -sh "$OUT/bin/espeak-ng" "$OUT/share/espeak-ng-data"
```

Run: `chmod +x scripts/build-espeak.sh && ./scripts/build-espeak.sh`
Expected: deux lignes de phonèmes (`bɔ̃ʒˈuʁ`, `həlˈo͡ʊ` : le lien U+0361
de `--tie` entre `o` et `ʊ`), puis les tailles (binaire de l'ordre du Mo,
données de quelques Mo). Sans lien dans la sortie anglaise, relire
`espeak-ng --help` (options `--ipa`, `--tie`) et corriger les arguments ici
et dans `EspeakProcess::spawn` (Étape 4) : misaki attend les liens. Si `cmake` refuse une
option `USE_…`, la retirer de la ligne (CMake d'espeak-ng 1.52.0 fait foi)
et relancer.

Vérifier ensuite que le programme répond **ligne à ligne sans attendre la fin
de l'entrée** (c'est ce que suppose le protocole) :

Run:
```bash
python3 - <<'EOF'
import subprocess, time
p = subprocess.Popen(["target/espeak/bin/espeak-ng", "-q", "--ipa", "--tie", "-v", "fr-fr", "--path=target/espeak/share"],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
t = time.time(); p.stdin.write("Bonjour.\n"); p.stdin.flush()
print(repr(p.stdout.readline()), round(time.time() - t, 3), "s")
p.stdin.close(); p.wait()
EOF
```
Expected: `' bɔ̃ʒˈuʁ\n'` (ou sans espace en tête) en moins de 0,1 s. Si la
lecture reste bloquée, espeak-ng garde sa sortie en mémoire tampon : s'arrêter
et le signaler (le protocole ne tiendrait pas).

- [ ] **Étape 2 : la référence du spike en fixture**

Run:
```bash
mkdir -p tests/fixtures/kokoro
python3 - <<'EOF'
import json, os
src = os.path.expanduser("~/Library/Caches/sillon-bench/spike-kokoro/ref/phonemes.json")
data = json.load(open(src))["phonemes"]
json.dump(data, open("tests/fixtures/kokoro/misaki-phonemes.json", "w"), ensure_ascii=False, indent=1)
print({k: len(v) for k, v in data.items()})
EOF
```
Expected: `{'fr': 16, 'en': 16}`. Ce fichier (phrase → phonèmes de misaki
avec espeak-ng 1.52.0) est la référence de parité.

- [ ] **Étape 3 : écrire les tests qui échouent**

Créer `src/tts/kokoro/mod.rs` :

```rust
//! Kokoro-82M : phonétisation, voix, minutage des mots et moteur. Le calcul
//! passe par `KokoroBackend` (MLX dans l'app, ONNX Runtime au terminal).

pub mod phonemes;
```

et, dans `src/tts/mod.rs`, après `pub mod lazy;` : `pub mod kokoro;`.

Créer `src/tts/kokoro/phonemes.rs` avec le module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Un faux espeak-ng : la sentinelle a sa réponse, toute autre ligne
    /// répond sur deux lignes (`<texte>` puis `x`). Avec `die_once`, la
    /// ligne `meurs` tue le processus la première fois seulement ; avec
    /// `die_always`, à chaque fois.
    fn fake(name: &str, die_once: bool, die_always: bool) -> EspeakProcess {
        let dir = std::env::temp_dir().join(format!("sillon-espeak-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("deja-mort");
        let _ = std::fs::remove_file(&marker);
        let script = dir.join("espeak-ng");
        let death = if die_always {
            "exit 1".to_string()
        } else if die_once {
            format!("if [ ! -f '{m}' ]; then : > '{m}'; exit 1; fi", m = marker.display())
        } else {
            ":".to_string()
        };
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nwhile IFS= read -r line; do\n  if [ \"$line\" = \"{SENTINEL}\" ]; then echo 'zizomis'; continue; fi\n  if [ \"$line\" = \"meurs\" ]; then {death}; fi\n  echo \"<$line>\"; echo x\ndone\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        EspeakProcess::new(script, None)
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_sentinel_separates_answers_spread_over_several_lines() {
        let mut espeak = fake("lignes", false, false);
        let out = espeak.phonemize("fr", &strings(&["bonjour", "le monde"])).unwrap();
        assert_eq!(out, ["<bonjour> x", "<le monde> x"]);
        // Un second aller-retour sur le même processus.
        assert_eq!(espeak.phonemize("fr", &strings(&["encore"])).unwrap(), ["<encore> x"]);
    }

    #[test]
    fn punctuation_stays_where_it_was() {
        let mut espeak = fake("ponctuation", false, false);
        let out = espeak.phonemize("fr", &strings(&["Peut-être : les notes, et (l'ordre)."])).unwrap();
        // Les morceaux entre ponctuations partent seuls ; « - » disparaît
        // comme chez misaki, et les parenthèses reviennent.
        assert_eq!(out, ["<Peutêtre> x : <les notes> x, <et> x (<l'ordre> x)."]);
    }

    #[test]
    fn an_empty_item_gives_an_empty_answer() {
        let mut espeak = fake("vide", false, false);
        assert_eq!(espeak.phonemize("fr", &strings(&["", "."])).unwrap(), ["", "."]);
    }

    #[test]
    fn a_dead_process_is_restarted_once() {
        let mut espeak = fake("relance", true, false);
        assert_eq!(espeak.phonemize("fr", &strings(&["meurs"])).unwrap(), ["<meurs> x"]);
        let mut doomed = fake("condamne", false, true);
        assert!(doomed.phonemize("fr", &strings(&["meurs"])).is_err(), "deux morts : échec");
        assert_eq!(doomed.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant> x"], "on repart");
    }

    #[test]
    fn ties_and_flags_are_turned_into_kokoro_symbols() {
        // espeak-ng écrit les liens en U+0361 ; misaki les note « ^ ».
        assert_eq!(finish("fr", "ad\u{361}ʒɛ-ktif"), "aʤɛktif");
        assert_eq!(finish("fr", "t\u{361}ʃˈa (en)ʃˈuː(fr)"), "ʧˈa ʃˈuː");
        assert_eq!(finish("en", "hə\u{2c8}lo\u{361}ʊ ɐ"), "hə\u{2c8}lO ə");
        assert_eq!(finish("en", "ɹˈɑːdɚ"), "ɹˈɑdəɹ");
    }

    #[test]
    fn each_language_has_its_espeak_voice() {
        assert_eq!(voice_of("fr"), "fr-fr");
        assert_eq!(voice_of("en-GB"), "en-us");
    }

    /// Parité avec misaki : `SILLON_ESPEAK=target/espeak/bin/espeak-ng
    /// SILLON_ESPEAK_DATA=target/espeak/share cargo test --lib kokoro::phonemes -- --ignored`
    #[test]
    #[ignore]
    fn french_phonemes_match_misaki() {
        let reference: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/misaki-phonemes.json")).unwrap();
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let fr = &reference["fr"];
        let texts: Vec<String> = fr.keys().cloned().collect();
        let ours = espeak.phonemize("fr", &texts).unwrap();
        for (text, mine) in texts.iter().zip(&ours) {
            assert_eq!(mine, &fr[text], "écart sur une phrase de référence");
        }
    }

    /// L'anglais de misaki passe d'abord par un lexique : espeak-ng seul
    /// s'en approche sans l'égaler. On mesure l'écart, on le borne.
    #[test]
    #[ignore]
    fn english_phonemes_stay_close_to_misaki() {
        let reference: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/misaki-phonemes.json")).unwrap();
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let en = &reference["en"];
        let texts: Vec<String> = en.keys().cloned().collect();
        let ours = espeak.phonemize("en", &texts).unwrap();
        let mut total = 0.0;
        for (text, mine) in texts.iter().zip(&ours) {
            total += similarity(mine, &en[text]);
        }
        let mean = total / texts.len() as f64;
        eprintln!("anglais : similarité moyenne avec misaki {mean:.3}");
        assert!(mean >= 0.80, "{mean}");
    }

    fn similarity(a: &str, b: &str) -> f64 {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut prev = row[0];
            row[0] = i;
            for j in 1..=b.len() {
                let cur = row[j];
                row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
                prev = cur;
            }
        }
        1.0 - row[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
    }

    /// Budget : ≤ 5 ms par phrase, phrase et mots seuls compris, à chaud.
    #[test]
    #[ignore]
    fn a_sentence_and_its_words_cost_under_five_milliseconds() {
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let sentence = "Le rapport de la SNCF, long de trente-sept pages, est arrivé un mardi.";
        let mut items = vec![sentence.to_string()];
        items.extend(sentence.split_whitespace().map(str::to_string));
        espeak.phonemize("fr", &items).unwrap();
        let mut times = Vec::new();
        for _ in 0..20 {
            let t = std::time::Instant::now();
            espeak.phonemize("fr", &items).unwrap();
            times.push(t.elapsed());
        }
        times.sort();
        eprintln!("phonétisation : médiane {:?}", times[10]);
        assert!(times[10] <= std::time::Duration::from_millis(5), "{:?}", times[10]);
    }
}
```

Run: `cargo test --lib kokoro::phonemes`
Expected: FAIL — `EspeakProcess`, `finish`, `SENTINEL`, `voice_of` introuvables.

- [ ] **Étape 4 : écrire la phonétisation**

En tête de `src/tts/kokoro/phonemes.rs` :

```rust
//! Phonétisation par espeak-ng, lancé comme programme séparé : la GPL-3.0
//! d'espeak-ng reste de son côté. Le traitement de misaki (`EspeakG2P` pour
//! le français, `EspeakFallback` pour l'anglais) est porté tel quel : Kokoro
//! a été entraîné sur ces phonèmes.
//!
//! Protocole : une ligne de texte, puis une ligne sentinelle ; tout ce que
//! espeak-ng écrit avant la réponse connue de la sentinelle appartient à la
//! ligne. Aucun texte n'est jamais journalisé.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Ce que la synthèse demande à la phonétisation.
pub trait Phonemizer: Send {
    /// Les phonèmes de chaque entrée, dans l'ordre, en une seule requête.
    fn phonemize(&mut self, language: &str, items: &[String]) -> Result<Vec<String>>;
    /// Arrête ce qui tourne : la voix est relâchée.
    fn stop(&mut self) {}
}

/// Une phrase qu'aucun texte ne contient, dont la réponse marque la fin.
pub(crate) const SENTINEL: &str = "Zyzomys.";
/// Au-delà, espeak-ng est tenu pour bloqué.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);
/// La ponctuation que misaki garde (phonemizer, `preserve_punctuation`).
const MARKS: &[char] = &[';', ':', ',', '.', '!', '?', '¡', '¿', '—', '…', '"', '«', '»', '“', '”'];

/// La voix espeak-ng d'une langue.
pub(crate) fn voice_of(language: &str) -> &'static str {
    match crate::text::language::Lang::parse(language) {
        Some(crate::text::language::Lang::En) => "en-us",
        _ => "fr-fr",
    }
}

/// Le programme espeak-ng : `SILLON_ESPEAK` (l'app y met le sien), à côté du
/// binaire, puis le `PATH`.
pub fn locate_program() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SILLON_ESPEAK").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("espeak-ng")));
    if let Some(path) = beside.filter(|p| p.is_file()) {
        return Some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|d| d.join("espeak-ng")).find(|p| p.is_file())
}

/// Le dossier qui contient `espeak-ng-data`, quand il n'est pas celui du
/// système (l'app : `Contents/Resources`).
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SILLON_ESPEAK_DATA").map(PathBuf::from)
}

/// Un morceau de texte entre deux ponctuations, ou la ponctuation elle-même
/// avec ses espaces.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Marks(String),
}

/// Prépare le texte comme misaki : guillemets français en guillemets
/// courbes, parenthèses en chevrons (que phonemizer garde), retours à la
/// ligne en espaces.
fn prepare(text: &str) -> String {
    text.replace('«', "\u{201c}")
        .replace('»', "\u{201d}")
        .replace('(', "«")
        .replace(')', "»")
        .replace(['\n', '\r'], " ")
}

/// Découpe aux ponctuations : un groupe de ponctuations et d'espaces qui
/// contient au moins un signe est gardé tel quel.
fn split_punctuation(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text_run = String::new();
    let mut gap = String::new();
    let mut gap_has_mark = false;
    for c in text.chars() {
        if MARKS.contains(&c) || c.is_whitespace() {
            gap.push(c);
            gap_has_mark |= MARKS.contains(&c);
            continue;
        }
        if !gap.is_empty() {
            if gap_has_mark {
                if !text_run.trim().is_empty() {
                    pieces.push(Piece::Text(text_run.trim().to_string()));
                }
                text_run.clear();
                pieces.push(Piece::Marks(std::mem::take(&mut gap)));
            } else {
                text_run.push_str(&gap);
                gap.clear();
            }
            gap_has_mark = false;
        }
        text_run.push(c);
    }
    if !text_run.trim().is_empty() {
        pieces.push(Piece::Text(text_run.trim().to_string()));
    }
    if gap_has_mark {
        pieces.push(Piece::Marks(gap));
    }
    pieces
}

/// Remplacements de misaki pour le français (`EspeakG2P`, version 1.0).
const FR_E2M: &[(&str, &str)] = &[
    ("a^ɪ", "I"), ("a^ʊ", "W"), ("d^z", "ʣ"), ("d^ʒ", "ʤ"), ("e^ɪ", "A"), ("o^ʊ", "O"),
    ("s^s", "S"), ("t^s", "ʦ"), ("t^ʃ", "ʧ"), ("ɔ^ɪ", "Y"), ("ə^ʊ", "Q"),
];

/// Remplacements de misaki pour l'anglais (`EspeakFallback`), du plus long
/// au plus court, dans l'ordre de misaki.
const EN_E2M: &[(&str, &str)] = &[
    ("ʔˌn\u{329}", "ʔn"),
    ("ʔn\u{329}", "ʔn"), ("a^ɪ", "I"), ("a^ʊ", "W"), ("d^ʒ", "ʤ"), ("e^ɪ", "A"), ("t^ʃ", "ʧ"),
    ("ɔ^ɪ", "Y"), ("ə^l", "ᵊl"),
    ("ʲo", "jo"), ("ʲə", "jə"),
    ("e", "A"), ("ʲ", ""), ("ɚ", "əɹ"), ("r", "ɹ"), ("x", "k"), ("ç", "k"), ("ɐ", "ə"),
    ("ɬ", "l"), ("\u{303}", ""),
];

/// La sortie brute d'espeak-ng mise aux symboles de Kokoro, comme misaki.
pub(crate) fn finish(language: &str, raw: &str) -> String {
    // Liens d'espeak-ng (U+0361) en « ^ », drapeaux de langue « (en) » retirés.
    let mut ps = raw.replace('\u{361}', "^");
    let flags = regex::Regex::new(r"\([a-z]{2,3}(-[a-z0-9]+)?\)").expect("motif fixe");
    ps = flags.replace_all(&ps, "").into_owned();
    ps = ps.split_whitespace().collect::<Vec<_>>().join(" ");
    if voice_of(language) == "en-us" {
        for (old, new) in EN_E2M {
            ps = ps.replace(old, new);
        }
        let syllabic = regex::Regex::new(r"(\S)\u{329}").expect("motif fixe");
        ps = syllabic.replace_all(&ps, "ᵊ$1").into_owned().replace('\u{329}', "");
        ps = ps.replace("o^ʊ", "O").replace("ɜːɹ", "ɜɹ").replace("ɜː", "ɜɹ").replace("ɪə", "iə").replace('ː', "");
        ps = ps.replace('o', "ɔ");
        ps = ps.replace('ɾ', "T").replace('ʔ', "t");
        ps = ps.replace('^', "");
    } else {
        for (old, new) in FR_E2M {
            ps = ps.replace(old, new);
        }
        ps = ps.replace('^', "").replace('-', "");
    }
    ps.replace('«', "(").replace('»', ")").trim().to_string()
}

struct Running {
    process: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    sentinel: String,
}

impl Running {
    fn read_line(&self) -> Result<String> {
        self.lines.recv_timeout(ANSWER_TIMEOUT).map_err(|_| anyhow!("espeak-ng ne répond plus"))
    }

    /// Une requête : chaque ligne suivie de la sentinelle, une réponse par
    /// ligne.
    fn ask(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let mut request = String::new();
        for line in lines {
            request.push_str(line);
            request.push('\n');
            request.push_str(SENTINEL);
            request.push('\n');
        }
        self.stdin.write_all(request.as_bytes()).context("écriture vers espeak-ng")?;
        self.stdin.flush().context("écriture vers espeak-ng")?;
        let mut answers = Vec::with_capacity(lines.len());
        for _ in lines {
            let mut parts = Vec::new();
            loop {
                let line = self.read_line()?;
                if line.trim() == self.sentinel {
                    break;
                }
                parts.push(line.trim().to_string());
            }
            answers.push(parts.join(" "));
        }
        Ok(answers)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// espeak-ng, un processus par langue, lancé à la demande.
pub struct EspeakProcess {
    program: PathBuf,
    data: Option<PathBuf>,
    running: HashMap<&'static str, Running>,
}

impl EspeakProcess {
    pub fn new(program: PathBuf, data: Option<PathBuf>) -> Self {
        Self { program, data, running: HashMap::new() }
    }

    /// Le programme trouvé, ou la raison de son absence.
    pub fn locate() -> Result<Self> {
        match locate_program() {
            Some(program) => Ok(Self::new(program, data_dir())),
            None if std::env::var_os("SILLON_ESPEAK").is_some() => {
                bail!("Installation de Sillon incomplète (espeak-ng)")
            }
            None => bail!("espeak-ng introuvable — installez le paquet `espeak-ng`"),
        }
    }

    fn spawn(&self, voice: &str) -> Result<Running> {
        let mut command = Command::new(&self.program);
        command.args(["-q", "--ipa", "--tie", "-v", voice]);
        if let Some(data) = &self.data {
            command.arg(format!("--path={}", data.display()));
        }
        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("lancement de {}", self.program.display()))?;
        let stdin = process.stdin.take().context("entrée d'espeak-ng")?;
        let stdout = process.stdout.take().context("sortie d'espeak-ng")?;
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name("espeak-ng".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .context("fil de lecture d'espeak-ng")?;
        let mut running = Running { process, stdin, lines, sentinel: String::new() };
        // La réponse de la sentinelle, apprise une fois.
        running.stdin.write_all(format!("{SENTINEL}\n").as_bytes())?;
        running.stdin.flush()?;
        running.sentinel = running.read_line()?.trim().to_string();
        Ok(running)
    }

    fn ask(&mut self, voice: &'static str, lines: &[String]) -> Result<Vec<String>> {
        if !self.running.contains_key(voice) {
            let running = self.spawn(voice)?;
            self.running.insert(voice, running);
        }
        let running = self.running.get_mut(voice).expect("lancé juste au-dessus");
        let outcome = running.ask(lines);
        if outcome.is_err() {
            self.running.remove(voice);
        }
        outcome
    }
}

impl Phonemizer for EspeakProcess {
    fn phonemize(&mut self, language: &str, items: &[String]) -> Result<Vec<String>> {
        let voice = voice_of(language);
        let pieces: Vec<Vec<Piece>> = items.iter().map(|t| split_punctuation(&prepare(t))).collect();
        let lines: Vec<String> = pieces
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Piece::Text(t) => Some(t.clone()),
                Piece::Marks(_) => None,
            })
            .collect();
        let answers = if lines.is_empty() {
            Vec::new()
        } else {
            // Une relance, pas deux : un programme qui meurt deux fois de
            // suite est en panne, et la voix passe en repli.
            match self.ask(voice, &lines) {
                Ok(answers) => answers,
                Err(_) => self.ask(voice, &lines)?,
            }
        };
        let mut answers = answers.into_iter();
        Ok(pieces
            .iter()
            .map(|item| {
                let mut raw = String::new();
                for piece in item {
                    match piece {
                        Piece::Text(_) => raw.push_str(&answers.next().unwrap_or_default()),
                        Piece::Marks(m) => raw.push_str(m),
                    }
                }
                finish(language, &raw)
            })
            .collect())
    }

    fn stop(&mut self) {
        self.running.clear();
    }
}
```

Run: `cargo test --lib kokoro::phonemes`
Expected: PASS (6 tests, 3 ignorés).

- [ ] **Étape 5 : parité et budget avec le vrai espeak-ng**

Run:
```bash
SILLON_ESPEAK=$PWD/target/espeak/bin/espeak-ng SILLON_ESPEAK_DATA=$PWD/target/espeak/share \
  cargo test --lib kokoro::phonemes -- --ignored --nocapture
```
Expected: `french_phonemes_match_misaki` PASS (16 phrases identiques),
`english_phonemes_stay_close_to_misaki` PASS avec sa similarité affichée,
budget ≤ 5 ms. Un écart en français se corrige dans `finish` ou
`split_punctuation` en relisant
`~/Library/Caches/sillon-bench/light/venv-mlx/lib/python3.12/site-packages/misaki/espeak.py`
et `phonemizer/punctuation.py` du même dossier, jamais en retouchant la
fixture.

- [ ] **Étape 6 : commit**

```bash
git add scripts/build-espeak.sh src/tts/kokoro/mod.rs src/tts/kokoro/phonemes.rs src/tts/mod.rs tests/fixtures/kokoro/misaki-phonemes.json
git commit -m "$(cat <<'EOF'
feat(kokoro): phonétisation par espeak-ng en processus séparé, traitement de misaki porté, construction d'espeak-ng 1.52.0

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 8 : les voix (dont `p-fenrir60` livrée) et le minutage des mots

Une voix Kokoro est un tableau 510 × 256 ; la ligne de style d'une phrase est
`len(phonèmes) − 1`. `p-fenrir60` est calculée par sa recette (timbre
`ff_siwis`, prosodie 0,4 `ff_siwis` + 0,6 `am_fenrir`) et livrée dans le
binaire. Le minutage des mots aligne les phonèmes de la phrase entière sur
ceux de chaque mot affiché (distance d'édition, alignement monotone) ; une
consonne de liaison va au mot qui suit ; la moitié des espaces et
ponctuations voisines va à chaque mot ; les jetons de bord au premier et au
dernier mot. Un alignement trop mauvais rend la main aux poids.

**Fichiers :**
- Créer : `src/tts/kokoro/voices.rs`
- Créer : `src/tts/kokoro/align.rs`
- Modifier : `src/tts/kokoro/mod.rs`
- Créer : `examples/kokoro_blend.rs`, `examples/kokoro_fixtures.rs`
- Créer : `assets/voices/p-fenrir60.safetensors`, `assets/voices/p-fenrir60.json`
- Créer : `tests/fixtures/kokoro/align-fr.json` (généré)

**Interfaces :**
- Consomme : `Phonemizer`, `EspeakProcess::locate` (Tâche 7) ; `WordSpan`, `Sentence::spoken_words` (Tâche 4) ; `parse_document_in` (Tâche 2).
- Produit :
  - `pub const ROWS: usize = 510; pub const WIDTH: usize = 256;`
  - `pub struct VoicePack` : `from_values(Vec<f32>) -> Result<Self>`, `values(&self) -> &[f32]`, `row(&self, phonemes: usize) -> &[f32]`, `from_safetensors(&[u8]) -> Result<Self>`, `to_safetensors(&self) -> Vec<u8>`, `blend_recipe(siwis: &VoicePack, fenrir: &VoicePack) -> VoicePack`
  - `pub fn load_voice(model_dir: &Path, name: &str) -> Result<VoicePack>` (livrée d'abord, puis `model_dir/voices/<nom>.safetensors`)
  - `pub const FRAME_SECONDS: f32 = 0.025;` ; `pub enum AlignFailure { NoPhonemes, LengthMismatch, TooFar { cost: usize, length: usize }, EmptyWord(usize) }` ; `pub fn word_spans(tokens: &[char], durations: &[u32], words: &[String]) -> Result<Vec<WordSpan>, AlignFailure>`

- [ ] **Étape 1 : écrire les tests des voix**

Dans `src/tts/kokoro/mod.rs`, après `pub mod phonemes;` : `pub mod align;`
et `pub mod voices;`.

Créer `src/tts/kokoro/voices.rs` avec le module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> VoicePack {
        // La ligne r porte la valeur r : la ligne choisie se lit d'un coup d'œil.
        VoicePack::from_values((0..ROWS * WIDTH).map(|i| (i / WIDTH) as f32).collect()).unwrap()
    }

    #[test]
    fn the_style_row_is_the_phoneme_count_minus_one() {
        let v = ramp();
        assert_eq!(v.row(1)[0], 0.0);
        assert_eq!(v.row(37)[0], 36.0);
        assert_eq!(v.row(0)[0], 0.0, "rien à dire : la première ligne");
        assert_eq!(v.row(900)[0], 509.0, "au-delà : la dernière");
        assert_eq!(v.row(37).len(), WIDTH);
    }

    #[test]
    fn a_voice_round_trips_through_safetensors() {
        let v = ramp();
        let bytes = v.to_safetensors();
        assert_eq!(bytes.len() as u64, crate::models::KOKORO_VOICE_BYTES, "même taille que les voix du dépôt");
        assert_eq!(VoicePack::from_safetensors(&bytes).unwrap(), v);
    }

    #[test]
    fn a_broken_file_is_refused_with_a_reason() {
        assert!(VoicePack::from_safetensors(b"court").is_err());
        let mut bytes = ramp().to_safetensors();
        bytes.truncate(bytes.len() - 4);
        assert!(VoicePack::from_safetensors(&bytes).is_err());
        assert!(VoicePack::from_values(vec![0.0; 10]).is_err());
    }

    #[test]
    fn the_recipe_keeps_the_timbre_and_mixes_the_prosody() {
        let siwis = VoicePack::from_values(vec![1.0; ROWS * WIDTH]).unwrap();
        let fenrir = VoicePack::from_values(vec![2.0; ROWS * WIDTH]).unwrap();
        let blend = VoicePack::blend_recipe(&siwis, &fenrir);
        let row = blend.row(10);
        assert_eq!(row[0], 1.0);
        assert_eq!(row[127], 1.0, "timbre : ff_siwis");
        assert!((row[128] - 1.6).abs() < 1e-6, "prosodie : 0,4 × 1 + 0,6 × 2");
        assert!((row[255] - 1.6).abs() < 1e-6);
    }

    #[test]
    fn the_french_voice_ships_with_sillon() {
        let dir = std::env::temp_dir().join("sillon-sans-voix");
        let v = load_voice(&dir, "p-fenrir60").expect("livrée dans le binaire");
        assert_eq!(v.values().len(), ROWS * WIDTH);
        assert!(load_voice(&dir, "af_bella").is_err(), "les autres viennent du paquet");
    }

    /// `SILLON_KOKORO_DIR=<dossier avec voices/ff_siwis.safetensors et
    /// voices/am_fenrir.safetensors> cargo test --lib kokoro::voices -- --ignored`
    #[test]
    #[ignore]
    fn the_shipped_voice_matches_its_recipe() {
        let dir = std::path::PathBuf::from(std::env::var("SILLON_KOKORO_DIR").expect("SILLON_KOKORO_DIR"));
        let read = |name: &str| {
            VoicePack::from_safetensors(&std::fs::read(dir.join("voices").join(format!("{name}.safetensors"))).unwrap())
                .unwrap()
        };
        let recomputed = VoicePack::blend_recipe(&read("ff_siwis"), &read("am_fenrir"));
        let shipped = load_voice(&dir, "p-fenrir60").unwrap();
        let worst = recomputed
            .values()
            .iter()
            .zip(shipped.values())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst <= 1e-6, "{worst}");
    }
}
```

- [ ] **Étape 2 : écrire les voix (sans le fichier livré)**

En tête de `src/tts/kokoro/voices.rs` :

```rust
//! Les voix de Kokoro : 510 × 256 flottants, une ligne de style par longueur
//! de phrase en phonèmes. Le format safetensors (Apache-2.0) est lu et écrit
//! à la main : un en-tête JSON, puis les octets.

use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

pub const ROWS: usize = 510;
pub const WIDTH: usize = 256;
/// Colonnes 0–127 : le timbre ; 128–255 : la prosodie.
const TIMBRE: usize = 128;

/// Les voix livrées avec Sillon : nom et contenu.
const SHIPPED: &[(&str, &[u8])] = &[(
    "p-fenrir60",
    include_bytes!("../../../assets/voices/p-fenrir60.safetensors"),
)];

#[derive(Debug, Clone, PartialEq)]
pub struct VoicePack {
    values: Vec<f32>,
}

impl VoicePack {
    pub fn from_values(values: Vec<f32>) -> Result<Self> {
        ensure!(values.len() == ROWS * WIDTH, "voix illisible : {} valeurs au lieu de {}", values.len(), ROWS * WIDTH);
        Ok(Self { values })
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// La ligne de style d'une suite de `phonemes` phonèmes : `len − 1`,
    /// bornée au tableau.
    pub fn row(&self, phonemes: usize) -> &[f32] {
        let index = phonemes.saturating_sub(1).min(ROWS - 1);
        &self.values[index * WIDTH..(index + 1) * WIDTH]
    }

    /// Lit le seul tenseur d'un fichier safetensors, en flottants 32 bits.
    pub fn from_safetensors(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 8, "voix illisible : fichier trop court");
        let header_len = u64::from_le_bytes(bytes[..8].try_into().expect("8 octets")) as usize;
        ensure!(bytes.len() >= 8 + header_len, "voix illisible : en-tête tronqué");
        let header: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&bytes[8..8 + header_len]).context("voix illisible : en-tête")?;
        let (_, info) = header
            .iter()
            .find(|(name, _)| name.as_str() != "__metadata__")
            .context("voix illisible : aucun tenseur")?;
        let dtype = info["dtype"].as_str().unwrap_or("");
        if dtype != "F32" {
            bail!("voix illisible : type {dtype} au lieu de F32");
        }
        let offsets: Vec<usize> = info["data_offsets"]
            .as_array()
            .context("voix illisible : positions")?
            .iter()
            .filter_map(|v| v.as_u64().map(|v| v as usize))
            .collect();
        ensure!(offsets.len() == 2 && offsets[0] <= offsets[1], "voix illisible : positions");
        let start = 8 + header_len + offsets[0];
        let end = 8 + header_len + offsets[1];
        ensure!(end <= bytes.len() && end - start == ROWS * WIDTH * 4, "voix illisible : données tronquées");
        let values = bytes[start..end]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Self::from_values(values)
    }

    /// Écrit la voix comme celles du dépôt : un tenseur `voice` [510, 1, 256].
    pub fn to_safetensors(&self) -> Vec<u8> {
        let size = ROWS * WIDTH * 4;
        let mut header = format!(
            r#"{{"voice":{{"dtype":"F32","shape":[{ROWS},1,{WIDTH}],"data_offsets":[0,{size}]}}}}"#
        )
        .into_bytes();
        // L'en-tête se complète d'espaces jusqu'à un multiple de 8 octets.
        while header.len() % 8 != 0 {
            header.push(b' ');
        }
        let mut out = Vec::with_capacity(8 + header.len() + size);
        out.extend_from_slice(&(header.len() as u64).to_le_bytes());
        out.extend_from_slice(&header);
        for v in &self.values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// La recette de `p-fenrir60` : le timbre de `ff_siwis` (colonnes
    /// 0–127), la prosodie à 0,4 `ff_siwis` + 0,6 `am_fenrir` (128–255).
    pub fn blend_recipe(siwis: &VoicePack, fenrir: &VoicePack) -> VoicePack {
        let mut values = siwis.values.clone();
        for row in 0..ROWS {
            for col in TIMBRE..WIDTH {
                let i = row * WIDTH + col;
                values[i] = 0.4 * siwis.values[i] + 0.6 * fenrir.values[i];
            }
        }
        VoicePack { values }
    }
}

/// Une voix : livrée avec Sillon, sinon dans `model_dir/voices/`.
pub fn load_voice(model_dir: &Path, name: &str) -> Result<VoicePack> {
    if let Some((_, bytes)) = SHIPPED.iter().find(|(shipped, _)| *shipped == name) {
        return VoicePack::from_safetensors(bytes);
    }
    let path = model_dir.join("voices").join(format!("{name}.safetensors"));
    let bytes = std::fs::read(&path).with_context(|| format!("voix absente : {}", path.display()))?;
    VoicePack::from_safetensors(&bytes)
}
```

- [ ] **Étape 3 : calculer `p-fenrir60` et la livrer**

Créer `examples/kokoro_blend.rs` :

```rust
//! Calcule `p-fenrir60` à partir de `ff_siwis` et `am_fenrir`.
//!
//!   cargo run --example kokoro_blend -- <dossier contenant voices/> <sortie.safetensors>

use sillon::tts::kokoro::voices::VoicePack;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(args.len() == 3, "usage : kokoro_blend <dossier> <sortie>");
    let dir = std::path::Path::new(&args[1]).join("voices");
    let read = |name: &str| -> anyhow::Result<VoicePack> {
        VoicePack::from_safetensors(&std::fs::read(dir.join(format!("{name}.safetensors")))?)
    };
    let voice = VoicePack::blend_recipe(&read("ff_siwis")?, &read("am_fenrir")?);
    std::fs::write(&args[2], voice.to_safetensors())?;
    println!("écrit : {}", args[2]);
    Ok(())
}
```

Le fichier livré n'existe pas encore : `include_bytes!` empêcherait de
compiler l'exemple. Créer d'abord un fichier provisoire de la bonne forme,
puis le remplacer par le vrai :

Run:
```bash
mkdir -p assets/voices target/kokoro-voix/voices
python3 -c "import struct,json;h=json.dumps({'voice':{'dtype':'F32','shape':[510,1,256],'data_offsets':[0,510*256*4]}},separators=(',',':')).encode();h+=b' '*((8-len(h)%8)%8);open('assets/voices/p-fenrir60.safetensors','wb').write(struct.pack('<Q',len(h))+h+b'\0'*(510*256*4))"
for v in ff_siwis am_fenrir; do
  curl -fsSL "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/voices/$v.safetensors" -o target/kokoro-voix/voices/$v.safetensors
done
cargo run --example kokoro_blend -- target/kokoro-voix assets/voices/p-fenrir60.safetensors
ls -l assets/voices/p-fenrir60.safetensors
```
Expected: `écrit : assets/voices/p-fenrir60.safetensors`, 522 320 octets. Les
deux voix sources vont dans `target/` (jamais dans le dossier des modèles de
l'app). Contrôle de cohérence avec le spike (le même mélange en f32 brut) :

Run:
```bash
python3 - <<'EOF'
import struct, os
ref = os.path.expanduser("~/Library/Caches/sillon-bench/spike-kokoro/ref/style-fr.f32")
b = open("assets/voices/p-fenrir60.safetensors", "rb").read()
n = struct.unpack("<Q", b[:8])[0]; ours = struct.unpack(f"<{510*256}f", b[8+n:])
if os.path.exists(ref):
    theirs = struct.unpack(f"<{510*256}f", open(ref, "rb").read())
    print("écart max avec le spike :", max(abs(a - c) for a, c in zip(ours, theirs)))
else:
    print("référence du spike absente : contrôle sauté")
EOF
```
Expected: écart ≤ 1e-6 (ou contrôle sauté).

Créer `assets/voices/p-fenrir60.json` :

```json
{
  "name": "p-fenrir60",
  "license": "Apache-2.0",
  "source": "hexgrad/Kokoro-82M, voix ff_siwis et am_fenrir via mlx-community/Kokoro-82M-bf16 (révision a71e4d38b236d968966a2002c4c895dbd12b1c3c)",
  "timbre": { "voice": "ff_siwis", "columns": [0, 128] },
  "prosody": {
    "columns": [128, 256],
    "mix": [
      { "voice": "ff_siwis", "weight": 0.4 },
      { "voice": "am_fenrir", "weight": 0.6 }
    ]
  },
  "evaluation": ".superpowers/research/eval/rapport.md §11 : composite 78,3, écart-type F0 3,5 demi-tons"
}
```

Run: `cargo test --lib kokoro::voices && SILLON_KOKORO_DIR=target/kokoro-voix cargo test --lib kokoro::voices -- --ignored`
Expected: PASS (5 tests, puis le test ignoré).

- [ ] **Étape 4 : écrire les tests de l'alignement**

Créer `src/tts/kokoro/align.rs` avec le module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_liaison_consonant_belongs_to_the_next_word() {
        // « est arrivé » : espeak lie le t, que le mot seul n'a pas.
        let tokens = chars("ɛt aʁivˈe.");
        let durations = vec![2u32; tokens.len() + 2];
        let spans = word_spans(&tokens, &durations, &words(&["ɛ", "aʁivˈe."])).unwrap();
        // Mot 1 : ɛ (2 trames) + le jeton de bord du début (2) = 4 trames.
        assert_eq!(spans[0].start, 0.0);
        assert!((spans[0].end - 0.1).abs() < 1e-6, "{:?}", spans[0]);
        // Tout le reste, liaison comprise, va au mot 2 : 24 trames au total.
        assert_eq!(spans[1].start, spans[0].end);
        assert!((spans[1].end - 0.6).abs() < 1e-6, "{:?}", spans[1]);
    }

    #[test]
    fn spaces_and_punctuation_are_shared_between_neighbours() {
        let tokens = chars("ab, cd");
        // jeton de bord 0, a b , ␠ c d, jeton de bord 0
        let durations = vec![0, 1, 1, 2, 2, 1, 1, 0];
        let spans = word_spans(&tokens, &durations, &words(&["ab,", "cd"])).unwrap();
        // a b = 2 trames, plus la moitié de « , » et de l'espace = 2 : 4 trames.
        assert!((spans[0].end - 4.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
        assert!((spans[1].end - 8.0 * FRAME_SECONDS).abs() < 1e-6);
    }

    #[test]
    fn a_hopeless_alignment_gives_up() {
        let tokens = chars("bɔ̃ʒuʁ");
        let durations = vec![1u32; tokens.len() + 2];
        assert!(matches!(
            word_spans(&tokens, &durations, &words(&["xyzw", "qqq"])),
            Err(AlignFailure::TooFar { .. })
        ));
        assert_eq!(
            word_spans(&tokens, &durations, &words(&["bɔ̃ʒuʁ", "..."])),
            Err(AlignFailure::EmptyWord(1)),
            "un mot sans phonème"
        );
        assert_eq!(word_spans(&tokens, &[1, 1], &words(&["bɔ̃ʒuʁ"])), Err(AlignFailure::LengthMismatch));
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        sentence: String,
        words: Vec<String>,
        word_phonemes: Vec<String>,
    }

    /// Les phrases françaises du spike, phonétisées par espeak-ng
    /// (`examples/kokoro_fixtures.rs`) : le test tourne sans espeak.
    #[test]
    fn every_spike_sentence_is_aligned_in_order_and_in_full() {
        let fixtures: Vec<Fixture> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/align-fr.json")).unwrap();
        assert!(fixtures.len() >= 16, "{}", fixtures.len());
        let mismatched = fixtures
            .iter()
            .filter(|f| f.sentence.split_whitespace().count() != f.words.len())
            .count();
        assert!(mismatched >= 4, "les cas où les groupes ne suivent pas les mots : {mismatched}");
        for f in &fixtures {
            let tokens = chars(&f.sentence);
            let durations: Vec<u32> = (0..tokens.len() + 2).map(|i| 2 + (i % 3) as u32).collect();
            let total: u32 = durations.iter().sum();
            let spans = word_spans(&tokens, &durations, &f.word_phonemes).unwrap();
            assert_eq!(spans.len(), f.words.len());
            for pair in spans.windows(2) {
                assert!((pair[0].end - pair[1].start).abs() < 1e-6, "mots dans l'ordre, sans trou");
            }
            for s in &spans {
                assert!(s.end - s.start >= FRAME_SECONDS - 1e-6, "chaque mot dure au moins une trame");
            }
            assert!((spans.last().unwrap().end - total as f32 * FRAME_SECONDS).abs() < 1e-4, "somme = durée");
        }
    }
}
```

- [ ] **Étape 5 : écrire l'alignement**

En tête de `src/tts/kokoro/align.rs` :

```rust
//! Le temps de chaque mot affiché, d'après les durées de phonèmes de Kokoro.
//!
//! La phrase est phonétisée entière (liaisons, nombres en contexte) et chaque
//! mot seul. espeak-ng fusionne parfois des mots : une distance d'édition,
//! à alignement monotone, rattache chaque phonème de la phrase à un mot
//! affiché. Aligner sur les mots affichés règle les formes parlées : « 1789 »
//! est un mot à l'écran et quatre dans la voix.

use crate::doc::WordSpan;

/// Une trame de Kokoro : 600 échantillons à 24 kHz.
pub const FRAME_SECONDS: f32 = 600.0 / 24_000.0;
const PUNCTUATION: &[char] = &[';', ':', ',', '.', '!', '?', '¡', '¿', '—', '…', '"', '«', '»', '“', '”', '(', ')'];

/// Pourquoi une phrase garde la répartition par poids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignFailure {
    /// Ni la phrase ni les mots n'ont de phonème.
    NoPhonemes,
    /// Les durées ne correspondent pas aux jetons (deux jetons de bord en plus).
    LengthMismatch,
    /// Le coût dépasse la moitié de la longueur.
    TooFar { cost: usize, length: usize },
    /// Ce mot n'a reçu aucun phonème.
    EmptyWord(usize),
}

fn is_phone(c: char) -> bool {
    !c.is_whitespace() && !PUNCTUATION.contains(&c)
}

/// Le temps de chaque mot. `tokens` : les phonèmes envoyés au modèle (sans
/// jetons de bord) ; `durations` : une durée par jeton, bords compris, en
/// trames ; `words` : les phonèmes de chaque mot affiché, seul.
pub fn word_spans(tokens: &[char], durations: &[u32], words: &[String]) -> Result<Vec<WordSpan>, AlignFailure> {
    if durations.len() != tokens.len() + 2 {
        return Err(AlignFailure::LengthMismatch);
    }
    let sentence: Vec<usize> = (0..tokens.len()).filter(|&i| is_phone(tokens[i])).collect();
    let target: Vec<(char, usize)> = words
        .iter()
        .enumerate()
        .flat_map(|(w, word)| word.chars().filter(|&c| is_phone(c)).map(move |c| (c, w)))
        .collect();
    if sentence.is_empty() || target.is_empty() {
        return Err(AlignFailure::NoPhonemes);
    }
    let (n, m) = (sentence.len(), target.len());
    let at = |i: usize, j: usize| i * (m + 1) + j;
    let mut cost = vec![0usize; (n + 1) * (m + 1)];
    for i in 0..=n {
        cost[at(i, 0)] = i;
    }
    for j in 0..=m {
        cost[at(0, j)] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let differ = usize::from(tokens[sentence[i - 1]] != target[j - 1].0);
            cost[at(i, j)] = (cost[at(i - 1, j - 1)] + differ)
                .min(cost[at(i - 1, j)] + 1)
                .min(cost[at(i, j - 1)] + 1);
        }
    }
    let total = cost[at(n, m)];
    let length = n.max(m);
    if total * 2 > length {
        return Err(AlignFailure::TooFar { cost: total, length });
    }

    // Retour en arrière : chaque phonème de la phrase reçoit son mot. Un
    // phonème sans pendant (liaison) va au mot qui suit le point d'alignement.
    let mut label = vec![0usize; n];
    let (mut i, mut j) = (n, m);
    while i > 0 {
        if j > 0 {
            let differ = usize::from(tokens[sentence[i - 1]] != target[j - 1].0);
            if cost[at(i, j)] == cost[at(i - 1, j - 1)] + differ {
                label[i - 1] = target[j - 1].1;
                i -= 1;
                j -= 1;
                continue;
            }
            if cost[at(i, j)] == cost[at(i, j - 1)] + 1 {
                j -= 1;
                continue;
            }
        }
        label[i - 1] = if j < m { target[j].1 } else { target[m - 1].1 };
        i -= 1;
    }

    // En demi-trames, pour couper les espaces en deux sans arrondi.
    let mut halves = vec![0u64; words.len()];
    let mut phones = vec![0usize; words.len()];
    let mut owner: Vec<Option<usize>> = vec![None; tokens.len()];
    for (k, &t) in sentence.iter().enumerate() {
        owner[t] = Some(label[k]);
        phones[label[k]] += 1;
    }
    let previous: Vec<Option<usize>> = (0..tokens.len())
        .scan(None, |last, t| {
            let before = *last;
            if owner[t].is_some() {
                *last = owner[t];
            }
            Some(before)
        })
        .collect();
    let mut next: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut upcoming = None;
    for t in (0..tokens.len()).rev() {
        next[t] = upcoming;
        if owner[t].is_some() {
            upcoming = owner[t];
        }
    }
    for t in 0..tokens.len() {
        let d = durations[t + 1] as u64;
        match (owner[t], previous[t], next[t]) {
            (Some(w), _, _) => halves[w] += 2 * d,
            (None, Some(p), Some(q)) if p != q => {
                halves[p] += d;
                halves[q] += d;
            }
            (None, Some(p), _) => halves[p] += 2 * d,
            (None, None, Some(q)) => halves[q] += 2 * d,
            (None, None, None) => {}
        }
    }
    let last = words.len() - 1;
    halves[0] += 2 * durations[0] as u64;
    halves[last] += 2 * durations[tokens.len() + 1] as u64;
    if let Some(empty) = (0..words.len()).find(|&w| phones[w] == 0 || halves[w] == 0) {
        return Err(AlignFailure::EmptyWord(empty));
    }

    let mut spans = Vec::with_capacity(words.len());
    let mut start = 0u64;
    for h in halves {
        let end = start + h;
        spans.push(WordSpan {
            start: start as f32 * FRAME_SECONDS / 2.0,
            end: end as f32 * FRAME_SECONDS / 2.0,
        });
        start = end;
    }
    Ok(spans)
}
```

Note sur le test `spaces_and_punctuation_are_shared_between_neighbours` : la
virgule est collée à `b` et suivie d'un espace ; les deux jetons sont entre
le mot 0 et le mot 1, chacun partagé en deux : (2 + 2) / 2 = 2 trames de plus
pour chaque mot.

- [ ] **Étape 6 : générer les fixtures d'alignement**

Créer `examples/kokoro_fixtures.rs` :

```rust
//! Phonétise les phrases françaises du spike comme le moteur le fera : la
//! phrase parlée entière, puis chaque mot affiché seul. Écrit les fixtures
//! d'alignement (le test tourne ensuite sans espeak-ng).
//!
//!   SILLON_ESPEAK=target/espeak/bin/espeak-ng SILLON_ESPEAK_DATA=target/espeak/share \
//!     cargo run --example kokoro_fixtures -- tests/fixtures/kokoro/align-fr.json

use std::collections::BTreeMap;

use sillon::doc::builder::{parse_document_in, CodePolicy};
use sillon::tts::kokoro::phonemes::{EspeakProcess, Phonemizer};

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "tests/fixtures/kokoro/align-fr.json".into());
    let reference: BTreeMap<String, BTreeMap<String, String>> =
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/kokoro/misaki-phonemes.json")?)?;
    let mut espeak = EspeakProcess::locate()?;
    let mut fixtures = Vec::new();
    for text in reference["fr"].keys() {
        let doc = parse_document_in(text, CodePolicy::Announce, "fr");
        for sentence in &doc.sentences {
            let words = sentence.spoken_words();
            let mut items = vec![sentence.text.clone()];
            items.extend(words.iter().cloned());
            let ps = espeak.phonemize("fr", &items)?;
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
```

Run:
```bash
SILLON_ESPEAK=$PWD/target/espeak/bin/espeak-ng SILLON_ESPEAK_DATA=$PWD/target/espeak/share \
  cargo run --example kokoro_fixtures -- tests/fixtures/kokoro/align-fr.json
cargo test --lib kokoro::align
```
Expected: « 16 phrases écrites » (ou un peu plus si une entrée se découpe en
deux phrases), puis PASS (4 tests). `parse_document_in` doit être `pub` dans
`sillon::doc::builder` (il l'est) et `CodePolicy` aussi.

- [ ] **Étape 7 : commit**

```bash
git add src/tts/kokoro/mod.rs src/tts/kokoro/voices.rs src/tts/kokoro/align.rs examples/kokoro_blend.rs examples/kokoro_fixtures.rs assets/voices/p-fenrir60.safetensors assets/voices/p-fenrir60.json tests/fixtures/kokoro/align-fr.json
git commit -m "$(cat <<'EOF'
feat(kokoro): voix 510 × 256 et p-fenrir60 livrée, minutage des mots par alignement des phonèmes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 9 : le moteur Kokoro, son contrat de calcul et son repli

`KokoroEngine` garde tout sauf le calcul : phonèmes (phrase et mots en une
requête), identifiants (vocabulaire de `config.json`, jeton 0 aux deux bords),
ligne de style, coupe au-delà de 510 phonèmes, contrôle de la sortie (pas de
NaN, échantillons = somme des durées × 600), minutage des mots. Le calcul
passe par `KokoroBackend`. Le moteur coupe la phrase attendue en tête et
suite. espeak-ng qui meurt deux fois : la phrase échoue, la voix passe en
repli et le dit. Au repos, le moteur relâché libère le calcul et arrête
espeak-ng. `build_engine_with` choisit Kokoro pour la famille `kokoro` ; le
repli est le premier modèle audio.cpp installé (chargé à la demande), sinon la
voix du système.

**Fichiers :**
- Créer : `src/tts/kokoro/backend.rs`
- Modifier : `src/tts/kokoro/mod.rs` (`Vocab`, `KokoroEngine`, `KokoroSession`)
- Créer : `src/tts/fallback.rs`
- Modifier : `src/tts/mod.rs` (`build_engine_with`, `pub mod fallback;`)
- Modifier : `src/tts/lazy.rs` (l'état du moteur chargé est relayé)

**Interfaces :**
- Consomme : `Phonemizer`, `EspeakProcess` (Tâche 7) ; `VoicePack`, `load_voice`, `word_spans` (Tâche 8) ; `Synthesis`, `splits_first_clause` (Tâche 4) ; `splitting_first_clause` (Tâche 5) ; `kokoro_dir`, `voice_for_language`, `default_voice_for_language` (Tâches 1, 6).
- Produit :
  - `pub struct KokoroRaw { pub samples: Vec<f32>, pub durations: Vec<u32> }`
  - `pub trait KokoroBackend: Send + Sync { fn label(&self) -> &'static str; fn load(&self, model_dir: &Path) -> Result<()>; fn warm_up(&self) -> Result<()>; fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw>; fn release(&self); }`
  - `pub fn check(raw: &KokoroRaw, tokens: usize) -> Result<()>`
  - `pub const SAMPLE_RATE: u32 = 24_000; pub const FRAME: usize = 600; pub const MAX_PHONEMES: usize = 510;`
  - `pub struct Vocab` : `from_config(&Path) -> Result<Self>`, `from_pairs(impl IntoIterator<Item = (char, i64)>) -> Self`, `encode(&self, &str) -> (Vec<char>, Vec<i64>)`
  - `KokoroEngine::load(backend: Arc<dyn KokoroBackend>, phonemizer: Box<dyn Phonemizer>, model_dir: &Path, fallback: Arc<dyn TtsEngine>) -> Result<Self>` ; `KokoroEngine::with_parts(backend, phonemizer, vocab: Vocab, model_dir: PathBuf, fallback) -> Self`
  - `pub fn build_engine_with(preference, settings: &EngineSettings, mlx: Option<Arc<dyn kokoro::KokoroBackend>>) -> (Arc<dyn TtsEngine>, Option<String>)` ; `build_engine(p, s)` = `build_engine_with(p, s, None)`
  - `fallback::fallback_engine(settings: &EngineSettings) -> Arc<dyn TtsEngine>` ; `fallback::OnDemand::new(label, fallback, load) -> OnDemand`

- [ ] **Étape 1 : écrire les tests qui échouent**

Créer `src/tts/kokoro/backend.rs` :

```rust
//! Le calcul de Kokoro, derrière un trait : MLX dans l'app (interface
//! étrangère implémentée en Swift), ONNX Runtime au terminal.

use anyhow::{ensure, Result};
use std::path::Path;

/// Ce que le calcul rend : l'audio à 24 kHz, et une durée par jeton
/// d'entrée, en trames de 600 échantillons.
#[derive(Debug, Clone, PartialEq)]
pub struct KokoroRaw {
    pub samples: Vec<f32>,
    pub durations: Vec<u32>,
}

/// Le calcul seul : identifiants et ligne de style en entrée, audio et
/// durées en sortie. Les appels arrivent sur le fil de synthèse et bloquent
/// jusqu'au résultat. `speed` vaut toujours 1 : la vitesse est appliquée au
/// son, par Sonic.
pub trait KokoroBackend: Send + Sync {
    /// « MLX », « int8 » : pour le nom du moteur.
    fn label(&self) -> &'static str;
    fn load(&self, model_dir: &Path) -> Result<()>;
    /// Une courte synthèse, pour compiler ce qui doit l'être.
    fn warm_up(&self) -> Result<()>;
    fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw>;
    /// Libère poids et caches : la voix part au repos.
    fn release(&self);
}

/// Une sortie utilisable : une durée par jeton, aucun NaN, et exactement
/// la somme des durées × 600 échantillons (vérifié au spike).
pub fn check(raw: &KokoroRaw, tokens: usize) -> Result<()> {
    ensure!(raw.durations.len() == tokens, "sortie Kokoro invalide : {} durées pour {tokens} jetons", raw.durations.len());
    ensure!(raw.samples.iter().all(|s| s.is_finite()), "sortie Kokoro invalide : valeur non finie");
    let expected: usize = raw.durations.iter().map(|&d| d as usize).sum::<usize>() * super::FRAME;
    ensure!(
        raw.samples.len() == expected,
        "sortie Kokoro invalide : {} échantillons au lieu de {expected}",
        raw.samples.len()
    );
    Ok(())
}
```

Dans `src/tts/kokoro/mod.rs`, ajouter `pub mod backend;` et, à la fin, le
module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use crate::tts::{Readiness, TtsParams};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Good,
        Short,
        Nan,
    }

    /// Deux trames par jeton, du silence de la bonne longueur ; retient les
    /// appels (identifiants, première valeur de la ligne de style).
    struct FakeBackend {
        mode: Mode,
        calls: Mutex<Vec<(Vec<i64>, f32)>>,
        released: AtomicUsize,
    }

    impl FakeBackend {
        fn new(mode: Mode) -> Arc<Self> {
            Arc::new(Self { mode, calls: Mutex::new(Vec::new()), released: AtomicUsize::new(0) })
        }
    }

    impl KokoroBackend for FakeBackend {
        fn label(&self) -> &'static str {
            "faux"
        }
        fn load(&self, _: &Path) -> Result<()> {
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw> {
            self.calls.lock().unwrap().push((ids.to_vec(), style[0]));
            let durations = vec![2u32; ids.len()];
            let mut samples = vec![0.0; ids.len() * 2 * FRAME];
            match self.mode {
                Mode::Good => {}
                Mode::Short => {
                    samples.pop();
                }
                Mode::Nan => samples[0] = f32::NAN,
            }
            Ok(KokoroRaw { samples, durations })
        }
        fn release(&self) {
            self.released.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Les phonèmes sont le texte lui-même ; `dead` : chaque appel échoue.
    struct FakePhonemizer {
        dead: bool,
        stopped: Arc<AtomicUsize>,
    }

    impl Phonemizer for FakePhonemizer {
        fn phonemize(&mut self, _: &str, items: &[String]) -> Result<Vec<String>> {
            anyhow::ensure!(!self.dead, "espeak-ng ne répond plus");
            Ok(items.to_vec())
        }
        fn stop(&mut self) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn vocab() -> Vocab {
        Vocab::from_pairs(('a'..='z').zip(43..).chain([(' ', 16), (',', 3), ('.', 4)]))
    }

    /// Un dossier de modèle avec une voix « rampe » : la ligne r vaut r.
    fn model_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-kokoro-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("voices")).unwrap();
        let ramp = VoicePack::from_values(
            (0..voices::ROWS * voices::WIDTH).map(|i| (i / voices::WIDTH) as f32).collect(),
        )
        .unwrap();
        std::fs::write(dir.join("voices").join("rampe.safetensors"), ramp.to_safetensors()).unwrap();
        dir
    }

    fn engine(backend: Arc<FakeBackend>, dead: bool, name: &str) -> (KokoroEngine, Arc<AtomicUsize>) {
        let stopped = Arc::new(AtomicUsize::new(0));
        let phonemizer = Box::new(FakePhonemizer { dead, stopped: Arc::clone(&stopped) });
        let engine = KokoroEngine::with_parts(backend, phonemizer, vocab(), model_dir(name), Arc::new(SilentEngine));
        (engine, stopped)
    }

    fn params(voice: &str) -> TtsParams {
        TtsParams { voice: voice.into(), language: "fr".into(), ..TtsParams::default() }
    }

    fn strings(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn a_sentence_is_framed_by_edge_tokens_and_takes_its_style_row() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), false, "bords");
        let mut session = engine.open_session().unwrap();
        let out = session.synthesize_timed("un deux", &strings("un deux"), &params("rampe")).unwrap();
        let calls = backend.calls.lock().unwrap();
        let (ids, style) = &calls[0];
        assert_eq!(ids.first(), Some(&0));
        assert_eq!(ids.last(), Some(&0));
        assert_eq!(ids.len(), 7 + 2, "« un deux » : 7 symboles");
        assert_eq!(*style, 6.0, "ligne de style : 7 phonèmes − 1");
        assert_eq!(out.pcm.sample_rate, SAMPLE_RATE);
        assert_eq!(out.pcm.samples.len(), 9 * 2 * FRAME);
        let words = out.words.expect("minutage");
        assert_eq!(words.len(), 2);
        assert!((words[1].end - out.pcm.duration.as_secs_f32()).abs() < 1e-4);
    }

    #[test]
    fn an_invalid_output_fails_the_sentence() {
        for mode in [Mode::Short, Mode::Nan] {
            let (engine, _) = engine(FakeBackend::new(mode), false, "invalide");
            let mut session = engine.open_session().unwrap();
            assert!(session.synthesize("un deux", &params("rampe")).is_err());
        }
    }

    #[test]
    fn a_sentence_over_510_phonemes_is_cut_at_a_clause_and_glued_back() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, _) = engine(Arc::clone(&backend), false, "longue");
        let mut session = engine.open_session().unwrap();
        let text = "abcd efgh, ".repeat(60); // 660 symboles
        let words = strings(&text);
        let out = session.synthesize_timed(text.trim(), &words, &params("rampe")).unwrap();
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "deux morceaux");
        assert!(calls.iter().all(|(ids, _)| ids.len() <= MAX_PHONEMES + 2));
        let spans = out.words.expect("minutage recollé");
        assert_eq!(spans.len(), words.len());
        assert!((spans.last().unwrap().end - out.pcm.duration.as_secs_f32()).abs() < 1e-3);
    }

    #[test]
    fn the_cut_of_the_first_clause_is_asked_for() {
        let (engine, _) = engine(FakeBackend::new(Mode::Good), false, "coupe");
        assert!(engine.splits_first_clause());
        assert_eq!(engine.max_sessions(), 1);
        assert!(engine.name().contains("Kokoro"));
    }

    #[test]
    fn dropping_the_voice_releases_the_compute_and_stops_espeak() {
        let backend = FakeBackend::new(Mode::Good);
        let (engine, stopped) = engine(Arc::clone(&backend), false, "repos");
        let session = engine.open_session().unwrap();
        drop(engine);
        assert_eq!(backend.released.load(Ordering::SeqCst), 0, "une session vit encore");
        drop(session);
        assert_eq!(backend.released.load(Ordering::SeqCst), 1);
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_phonemizer_that_dies_twice_hands_over_to_the_fallback() {
        let (engine, _) = engine(FakeBackend::new(Mode::Good), true, "repli");
        let mut session = engine.open_session().unwrap();
        assert!(session.synthesize("un deux", &params("rampe")).is_err(), "la phrase est sautée");
        assert!(matches!(engine.readiness(), Readiness::Fallback(_)));
        assert!(engine.name().contains("repli"));
        assert!(session.synthesize("trois", &params("rampe")).is_ok(), "la voix de repli parle");
    }

    #[test]
    fn the_vocabulary_comes_from_the_configuration() {
        let dir = std::env::temp_dir().join(format!("sillon-vocab-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), r#"{"vocab": {"ɔ": 76, "̃": 17, " ": 16}, "n_token": 178}"#).unwrap();
        let vocab = Vocab::from_config(&dir.join("config.json")).unwrap();
        // Les voyelles nasales sont deux symboles : la base et le tilde.
        let (kept, ids) = vocab.encode("ɔ̃ x");
        assert_eq!(kept, ['ɔ', '\u{303}', ' ']);
        assert_eq!(ids, [76, 17, 16], "le x, inconnu, tombe");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_resting_reader_releases_kokoro() {
        use crate::audio::Player;
        use crate::config::Config;
        use crate::reader::Reader;
        let backend = FakeBackend::new(Mode::Good);
        let shared = Arc::clone(&backend);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut reader = Reader::new(
            config,
            Box::new(move |_| {
                let (engine, _) = engine(Arc::clone(&shared), false, "lecteur");
                Arc::new(engine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            std::time::Instant::now(),
        );
        reader.open_text("Une phrase.", "test", std::time::Instant::now());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while reader.session().is_some_and(|a| a.synth.get(0).is_none()) {
            assert!(std::time::Instant::now() < deadline, "jamais synthétisée");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        reader.rest();
        assert_eq!(backend.released.load(Ordering::SeqCst), 1, "poids et caches libérés au repos");
    }
}
```

Dans `src/tts/lazy.rs`, module `tests` :

```rust
    #[test]
    fn a_loaded_engine_that_falls_back_says_so() {
        struct Broken;
        impl TtsEngine for Broken {
            fn name(&self) -> String {
                "cassé".into()
            }
            fn open_session(&self) -> Result<Box<dyn TtsSession>> {
                anyhow::bail!("non")
            }
            fn max_sessions(&self) -> usize {
                1
            }
            fn readiness(&self) -> crate::tts::Readiness {
                crate::tts::Readiness::Fallback("espeak-ng s'est arrêté".into())
            }
        }
        let counter = Arc::new(AtomicUsize::new(0));
        let engine = LazyEngine::spawn("Kokoro".into(), 1, fallback(&counter), || Ok(Arc::new(Broken) as Arc<dyn TtsEngine>));
        let _ = engine.open_session();
        assert_eq!(engine.readiness(), crate::tts::Readiness::Fallback("espeak-ng s'est arrêté".into()));
    }
```

Créer `src/tts/fallback.rs` avec ses tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_fallback_loads_once_and_only_when_it_speaks() {
        let loads = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&loads);
        let engine = OnDemand::new("Supertonic 3 · F16".into(), Arc::new(SilentEngine), move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(SilentEngine) as Arc<dyn TtsEngine>)
        });
        assert_eq!(loads.load(Ordering::SeqCst), 0, "rien chargé d'avance");
        assert_eq!(engine.name(), "Supertonic 3 · F16");
        engine.open_session().unwrap();
        engine.open_session().unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_fallback_that_fails_to_load_gives_the_system_voice() {
        let engine = OnDemand::new("audio.cpp".into(), Arc::new(SilentEngine), || anyhow::bail!("fichier manquant"));
        engine.open_session().unwrap();
        assert_eq!(engine.name(), "silencieux");
    }
}
```

Dans `src/tts/mod.rs`, module `tests` :

```rust
    fn kokoro_settings(model_path: &str, backend: &str) -> EngineSettings {
        EngineSettings {
            model_path: model_path.into(),
            family: "kokoro".into(),
            backend: backend.into(),
            threads: 4,
            sessions: 1,
        }
    }

    fn kokoro_files(name: &str, file: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-build-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        std::fs::write(dir.join(file), b"x").unwrap();
        dir.join(file)
    }

    #[test]
    fn kokoro_without_its_files_falls_back_and_says_why() {
        let settings = kokoro_settings("/nulle/part/kokoro-v1_0.safetensors", "metal");
        let (_, note) = build_engine_with(EnginePreference::Auto, &settings, None);
        assert!(note.unwrap().contains("Kokoro n'est pas installé"));
    }

    #[test]
    fn kokoro_mlx_needs_the_compute_of_the_app() {
        let path = kokoro_files("mlx", "kokoro-v1_0.safetensors");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "metal"), None);
        assert!(note.unwrap().contains("Calcul Metal indisponible"));
    }

    #[test]
    #[cfg(not(feature = "kokoro-onnx"))]
    fn kokoro_int8_needs_the_onnx_feature() {
        let path = kokoro_files("onnx", "model_quantized.onnx");
        let (_, note) = build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "cpu"), None);
        assert!(note.unwrap().contains("kokoro-onnx"));
    }

    #[test]
    fn kokoro_with_its_compute_loads_in_the_background() {
        struct Idle;
        impl kokoro::KokoroBackend for Idle {
            fn label(&self) -> &'static str {
                "MLX"
            }
            fn load(&self, _: &std::path::Path) -> Result<()> {
                std::thread::sleep(Duration::from_millis(200));
                Ok(())
            }
            fn warm_up(&self) -> Result<()> {
                Ok(())
            }
            fn infer(&self, _: &[i64], _: &[f32]) -> Result<kokoro::KokoroRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self) {}
        }
        let path = kokoro_files("charge", "kokoro-v1_0.safetensors");
        let (engine, note) =
            build_engine_with(EnginePreference::Auto, &kokoro_settings(&path.to_string_lossy(), "metal"), Some(Arc::new(Idle)));
        assert_eq!(note, None, "rien à dire d'avance : le chargement part en arrière-plan");
        // Le nom dépend de l'issue du chargement (espeak-ng présent ou non
        // sur la machine de test) : seule la coupe, annoncée d'avance, est sûre.
        assert!(engine.splits_first_clause(), "annoncé avant la fin du chargement");
    }
```

Run: `cargo test --lib tts::`
Expected: FAIL — `KokoroEngine`, `Vocab`, `OnDemand`, `build_engine_with` introuvables.

- [ ] **Étape 2 : le moteur Kokoro**

Remplacer `src/tts/kokoro/mod.rs` (garder le module de tests de l'étape 1 à
la fin) par :

```rust
//! Kokoro-82M : phonétisation, voix, minutage des mots et moteur. Le calcul
//! passe par `KokoroBackend` (MLX dans l'app, ONNX Runtime au terminal).

pub mod align;
pub mod backend;
pub mod phonemes;
pub mod voices;

pub use backend::{check, KokoroBackend, KokoroRaw};

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use self::phonemes::Phonemizer;
use self::voices::{load_voice, VoicePack};
use super::{PcmBuffer, Readiness, Synthesis, TtsEngine, TtsParams, TtsSession};
use crate::text::language::Lang;

pub const SAMPLE_RATE: u32 = 24_000;
/// Échantillons par trame de durée.
pub const FRAME: usize = 600;
/// Au-delà, la phrase est coupée à une proposition.
pub const MAX_PHONEMES: usize = 510;

/// Le vocabulaire des phonèmes, tiré de `config.json` : un symbole (point de
/// code) par identifiant. Un symbole inconnu est écarté.
#[derive(Debug, Clone)]
pub struct Vocab(HashMap<char, i64>);

impl Vocab {
    pub fn from_config(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("lecture de {}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&text).context("config.json illisible")?;
        let vocab = json["vocab"].as_object().context("config.json sans vocabulaire")?;
        Ok(Self::from_pairs(vocab.iter().filter_map(|(symbol, id)| {
            let mut chars = symbol.chars();
            match (chars.next(), chars.next(), id.as_i64()) {
                (Some(c), None, Some(id)) => Some((c, id)),
                _ => None,
            }
        })))
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (char, i64)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// Les symboles gardés et leurs identifiants, point de code par point
    /// de code : une voyelle nasale est la base puis le tilde.
    pub fn encode(&self, phonemes: &str) -> (Vec<char>, Vec<i64>) {
        phonemes.chars().filter_map(|c| self.0.get(&c).map(|&id| (c, id))).unzip()
    }
}

/// Coupe une suite de phonèmes trop longue : à la dernière fin de
/// proposition avant la limite, sinon au dernier espace, sinon à la limite.
fn split_phonemes(ps: &str, max: usize) -> Vec<String> {
    let mut rest: Vec<char> = ps.chars().collect();
    let mut parts = Vec::new();
    while rest.len() > max {
        let window = &rest[..max];
        let cut = window
            .iter()
            .rposition(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | ',' | '—' | '…'))
            .map(|i| i + 1)
            .or_else(|| window.iter().rposition(|c| *c == ' '))
            .filter(|&i| i > 0)
            .unwrap_or(max);
        parts.push(rest[..cut].iter().collect::<String>().trim().to_string());
        rest.drain(..cut);
    }
    let last = rest.iter().collect::<String>().trim().to_string();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

struct Inner {
    backend: Arc<dyn KokoroBackend>,
    phonemizer: Mutex<Box<dyn Phonemizer>>,
    vocab: Vocab,
    model_dir: PathBuf,
    voices: Mutex<HashMap<String, Arc<VoicePack>>>,
    /// Pourquoi la voix est passée en repli, si elle l'est.
    broken: Mutex<Option<String>>,
    fallback: Arc<dyn TtsEngine>,
}

impl Inner {
    fn voice(&self, name: &str) -> Result<Arc<VoicePack>> {
        let mut voices = self.voices.lock().unwrap();
        if let Some(voice) = voices.get(name) {
            return Ok(Arc::clone(voice));
        }
        let voice = Arc::new(load_voice(&self.model_dir, name)?);
        voices.insert(name.to_string(), Arc::clone(&voice));
        Ok(voice)
    }

    fn broken(&self) -> Option<String> {
        self.broken.lock().unwrap().clone()
    }
}

impl Drop for Inner {
    /// Au repos : poids et caches du calcul libérés, espeak-ng arrêté.
    fn drop(&mut self) {
        self.backend.release();
        if let Ok(mut phonemizer) = self.phonemizer.lock() {
            phonemizer.stop();
        }
    }
}

/// Le moteur Kokoro : une session (le calcul est en série).
pub struct KokoroEngine {
    inner: Arc<Inner>,
}

impl KokoroEngine {
    /// Lit le vocabulaire et charge le calcul : quelques secondes, appelé
    /// dans le fil de chargement différé.
    pub fn load(
        backend: Arc<dyn KokoroBackend>,
        phonemizer: Box<dyn Phonemizer>,
        model_dir: &Path,
        fallback: Arc<dyn TtsEngine>,
    ) -> Result<Self> {
        let vocab = Vocab::from_config(&model_dir.join("config.json"))?;
        backend.load(model_dir)?;
        Ok(Self::with_parts(backend, phonemizer, vocab, model_dir.to_path_buf(), fallback))
    }

    pub fn with_parts(
        backend: Arc<dyn KokoroBackend>,
        phonemizer: Box<dyn Phonemizer>,
        vocab: Vocab,
        model_dir: PathBuf,
        fallback: Arc<dyn TtsEngine>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend,
                phonemizer: Mutex::new(phonemizer),
                vocab,
                model_dir,
                voices: Mutex::new(HashMap::new()),
                broken: Mutex::new(None),
                fallback,
            }),
        }
    }
}

impl TtsEngine for KokoroEngine {
    fn name(&self) -> String {
        match self.inner.broken() {
            Some(_) => format!("{} (repli)", self.inner.fallback.name()),
            None => format!("Kokoro · {}", self.inner.backend.label()),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        Ok(Box::new(KokoroSession { inner: Arc::clone(&self.inner), fallback: None }))
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

    fn splits_first_clause(&self) -> bool {
        true
    }
}

struct KokoroSession {
    inner: Arc<Inner>,
    /// Session de la voix de repli, ouverte au premier besoin.
    fallback: Option<Box<dyn TtsSession>>,
}

impl KokoroSession {
    fn fallback(&mut self) -> Result<&mut Box<dyn TtsSession>> {
        if self.fallback.is_none() {
            self.fallback = Some(self.inner.fallback.open_session()?);
        }
        Ok(self.fallback.as_mut().expect("ouverte juste au-dessus"))
    }
}

impl TtsSession for KokoroSession {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        Ok(self.synthesize_timed(text, &[], params)?.pcm)
    }

    fn synthesize_timed(&mut self, text: &str, words: &[String], params: &TtsParams) -> Result<Synthesis> {
        if self.inner.broken().is_some() {
            return self.fallback()?.synthesize_timed(text, words, params);
        }
        let language = Lang::parse(&params.language).unwrap_or(Lang::Fr).code();
        let voice_name = match params.voice.trim() {
            "" => crate::models::default_voice_for_language("kokoro", language),
            name => name,
        };
        let voice = self.inner.voice(voice_name)?;

        // La phrase et ses mots seuls : un seul aller-retour vers espeak-ng.
        let mut items = Vec::with_capacity(words.len() + 1);
        items.push(text.to_string());
        items.extend(words.iter().cloned());
        let phonemes = match self.inner.phonemizer.lock().unwrap().phonemize(language, &items) {
            Ok(p) => p,
            Err(err) => {
                *self.inner.broken.lock().unwrap() = Some(format!("espeak-ng s'est arrêté ({err})"));
                bail!("phonétisation impossible : {err}");
            }
        };

        let mut samples = Vec::new();
        let mut kept_all: Vec<char> = Vec::new();
        let mut durations_all: Vec<u32> = Vec::new();
        for part in split_phonemes(&phonemes[0], MAX_PHONEMES) {
            let (kept, ids) = self.inner.vocab.encode(&part);
            if ids.is_empty() {
                continue;
            }
            let mut tokens = Vec::with_capacity(ids.len() + 2);
            tokens.push(0);
            tokens.extend(ids);
            tokens.push(0);
            let raw = self.inner.backend.infer(&tokens, voice.row(part.chars().count()))?;
            check(&raw, tokens.len())?;
            samples.extend_from_slice(&raw.samples);
            if durations_all.is_empty() {
                durations_all = raw.durations;
            } else {
                // Les jetons de bord entre deux morceaux deviennent un espace.
                kept_all.push(' ');
                let last = durations_all.pop().unwrap_or(0);
                durations_all.push(last + raw.durations[0]);
                durations_all.extend_from_slice(&raw.durations[1..]);
            }
            kept_all.extend(kept);
        }
        if samples.is_empty() {
            bail!("phrase sans phonème connu");
        }
        let timing = if words.is_empty() {
            None
        } else {
            match align::word_spans(&kept_all, &durations_all, &phonemes[1..]) {
                Ok(spans) => Some(spans),
                Err(failure) => {
                    // Aucun texte au journal : seulement la raison chiffrée.
                    eprintln!("kokoro : minutage par mot abandonné ({failure:?})");
                    None
                }
            }
        };
        Ok(Synthesis { pcm: PcmBuffer::new(samples, SAMPLE_RATE, 1), words: timing })
    }
}
```

Dans `src/tts/lazy.rs`, `readiness` : le bras `State::Ready(_) => super::Readiness::Ready,`
devient `State::Ready(engine) => engine.readiness(),`.

- [ ] **Étape 3 : le repli à la demande et la construction**

Créer `src/tts/fallback.rs` (avant ses tests) :

```rust
//! La voix de repli quand le moteur voulu échoue : le premier modèle
//! audio.cpp installé, chargé seulement s'il doit parler, sinon la voix du
//! système.

use anyhow::Result;
use std::sync::{Arc, Mutex};

use super::{EngineSettings, TtsEngine, TtsSession};

type Loader = Box<dyn FnOnce() -> Result<Arc<dyn TtsEngine>> + Send>;

/// Un moteur chargé au premier besoin ; s'il échoue, `fallback` parle.
pub struct OnDemand {
    label: String,
    fallback: Arc<dyn TtsEngine>,
    load: Mutex<Option<Loader>>,
    engine: Mutex<Option<Arc<dyn TtsEngine>>>,
}

impl OnDemand {
    pub fn new<F>(label: String, fallback: Arc<dyn TtsEngine>, load: F) -> Self
    where
        F: FnOnce() -> Result<Arc<dyn TtsEngine>> + Send + 'static,
    {
        Self { label, fallback, load: Mutex::new(Some(Box::new(load))), engine: Mutex::new(None) }
    }

    fn resolve(&self) -> Arc<dyn TtsEngine> {
        let mut engine = self.engine.lock().unwrap();
        if let Some(engine) = engine.as_ref() {
            return Arc::clone(engine);
        }
        let loaded = self.load.lock().unwrap().take().map(|load| load());
        let chosen = match loaded {
            Some(Ok(e)) => e,
            _ => Arc::clone(&self.fallback),
        };
        *engine = Some(Arc::clone(&chosen));
        chosen
    }
}

impl TtsEngine for OnDemand {
    fn name(&self) -> String {
        match self.engine.lock().unwrap().as_ref() {
            Some(engine) => engine.name(),
            None => self.label.clone(),
        }
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        self.resolve().open_session()
    }

    fn max_sessions(&self) -> usize {
        1
    }
}

/// La voix de repli pour ces réglages.
pub fn fallback_engine(settings: &EngineSettings) -> Arc<dyn TtsEngine> {
    let system: Arc<dyn TtsEngine> = Arc::new(super::system::SystemEngine::new());
    #[cfg(feature = "audio-cpp")]
    {
        let installed = crate::models::available()
            .into_iter()
            .find(|p| p.runtime == crate::models::Runtime::AudioCpp && crate::models::is_installed(p));
        if let Some((package, path)) = installed.and_then(|p| crate::models::install_path(p).map(|path| (p, path))) {
            let audio_cpp = EngineSettings {
                model_path: path.to_string_lossy().into_owned(),
                family: package.family.to_string(),
                backend: settings.backend.clone(),
                threads: settings.threads,
                sessions: crate::models::sessions_for_family(package.family),
            };
            return Arc::new(OnDemand::new(package.display_name.to_string(), system, move || {
                super::try_audio_cpp(&audio_cpp)
            }));
        }
    }
    let _ = settings;
    system
}
```

Dans `src/tts/mod.rs` : `pub mod fallback;` après `pub mod lazy;`, puis
remplacer `build_engine` par :

```rust
/// Construit le moteur demandé, sans jamais bloquer l'appelant.
///
/// Les conditions qui se vérifient instantanément — feature absente, aucun
/// modèle configuré, fichier manquant — sont tranchées tout de suite, et donnent
/// lieu à un repli annoncé. Seul le chargement réel, qui prend quelques
/// secondes, part en arrière-plan : l'interface s'ouvre sans l'attendre.
pub fn build_engine(
    preference: EnginePreference,
    settings: &EngineSettings,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    build_engine_with(preference, settings, None)
}

/// `build_engine`, avec le calcul MLX de Kokoro quand l'app le fournit.
pub fn build_engine_with(
    preference: EnginePreference,
    settings: &EngineSettings,
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    if preference == EnginePreference::System {
        return (Arc::new(system::SystemEngine::new()), None);
    }
    if settings.family == "kokoro" {
        return build_kokoro(settings, mlx);
    }
    let fallback: Arc<dyn TtsEngine> = Arc::new(system::SystemEngine::new());
    if let Err(reason) = audio_cpp_precondition(settings) {
        let note = format!("audio.cpp indisponible ({reason}) — repli sur la voix système");
        return (fallback, Some(note));
    }

    let sessions = settings.sessions.max(1) as usize;
    let owned = settings.clone();
    let engine = lazy::LazyEngine::spawn("audio.cpp".to_string(), sessions, fallback, move || {
        try_audio_cpp(&owned)
    });
    (Arc::new(engine), None)
}

/// Kokoro : le calcul selon le fichier du paquet (`.onnx` : ORT ; sinon
/// MLX, fourni par l'app), espeak-ng cherché au chargement.
fn build_kokoro(
    settings: &EngineSettings,
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> (Arc<dyn TtsEngine>, Option<String>) {
    let fallback = fallback::fallback_engine(settings);
    let Some(dir) = crate::models::kokoro_dir(&settings.model_path) else {
        return (fallback, Some("Kokoro n'est pas installé — voir l'onglet Modèles".to_string()));
    };
    let backend = match kokoro_backend(settings, mlx) {
        Ok(backend) => backend,
        Err(reason) => return (fallback, Some(reason)),
    };
    let repli = Arc::clone(&fallback);
    let engine = lazy::LazyEngine::spawn("Kokoro".to_string(), 1, fallback, move || {
        let phonemizer = kokoro::phonemes::EspeakProcess::locate()?;
        let engine = kokoro::KokoroEngine::load(backend, Box::new(phonemizer), &dir, repli)?;
        Ok(Arc::new(engine) as Arc<dyn TtsEngine>)
    })
    .splitting_first_clause();
    (Arc::new(engine), None)
}

fn kokoro_backend(
    settings: &EngineSettings,
    mlx: Option<Arc<dyn kokoro::KokoroBackend>>,
) -> Result<Arc<dyn kokoro::KokoroBackend>, String> {
    if settings.model_path.ends_with(".onnx") {
        #[cfg(feature = "kokoro-onnx")]
        return Ok(Arc::new(kokoro::onnx::OrtBackend::new(settings.threads)));
        #[cfg(not(feature = "kokoro-onnx"))]
        return Err("Kokoro int8 : binaire compilé sans `kokoro-onnx`".to_string());
    }
    match mlx {
        Some(backend) if settings.backend != "cpu" => Ok(backend),
        _ => Err("Calcul Metal indisponible — Kokoro MLX ne tourne que dans l'app, sur la puce graphique".to_string()),
    }
}
```

`try_audio_cpp` doit être visible de `fallback.rs` : passer ses deux versions
(`#[cfg(feature = "audio-cpp")]` et `not`) en `pub(super) fn`, ou les laisser
privées : `fallback` est un sous-module de `tts`, il les voit déjà.

Run: `cargo test --lib tts:: reader`
Expected: PASS.

- [ ] **Étape 4 : la suite entière**

Run: `cargo test && cargo test -p sillon-ffi`
Expected: PASS.

- [ ] **Étape 5 : commit**

```bash
git add src/tts/kokoro/mod.rs src/tts/kokoro/backend.rs src/tts/fallback.rs src/tts/mod.rs src/tts/lazy.rs
git commit -m "$(cat <<'EOF'
feat(kokoro): moteur Kokoro (phonèmes, style, coupe à 510, contrôle de sortie, minutage), repli à la demande

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 10 : Kokoro au terminal par ONNX Runtime

La feature `kokoro-onnx` apporte `OrtBackend` : `ort` 2.0.0-rc.13 en
`load-dynamic`, une bibliothèque ONNX Runtime ≥ 1.31 cherchée à côté du binaire
(ou par `SILLON_ORT_LIB`), sa version vérifiée avant tout chargement (l'ORT
1.28 que `ort` télécharge par défaut est 4 fois plus lent en int8),
`min(engine.threads, 4)` fils, motif mémoire coupé. `package.sh` livre ORT
1.31 sous Linux. L'exemple `kokoro_serve` parle le protocole du banc, pour
mesurer le chemin Linux avec le pilote existant.

**Fichiers :**
- Modifier : `Cargo.toml` (dépendances de `kokoro-onnx`, exemple `kokoro_serve`)
- Créer : `src/tts/kokoro/onnx.rs`
- Modifier : `src/tts/kokoro/mod.rs` (`#[cfg(feature = "kokoro-onnx")] pub mod onnx;`)
- Créer : `examples/kokoro_serve.rs`
- Modifier : `scripts/package.sh`

**Interfaces :**
- Consomme : `KokoroBackend`, `KokoroRaw`, `KokoroEngine`, `EspeakProcess` (Tâches 7, 9).
- Produit :
  - `pub struct OrtBackend` ; `OrtBackend::new(threads: u32) -> Self` (fils = `threads.clamp(1, 4)`)
  - `pub fn library_path() -> PathBuf` ; `pub fn version_of(library: &Path) -> Result<String>` ; `pub fn check_version(found: &str) -> Result<()>` (≥ 1.31)
  - `pub const ONNX_FILE: &str = "model_quantized.onnx";`
  - exemple `kokoro_serve` : stdin `{"text": …, "lang": "fr", "out": "x.f32"}` → stdout `{"n": …, "ms": …, "dur": […]}`

- [ ] **Étape 1 : les dépendances**

Dans `Cargo.toml`, `[dependencies]` :

```toml
# Kokoro au terminal : ONNX Runtime chargé dynamiquement (≥ 1.31, livré à
# côté du binaire par package.sh), MIT/Apache.
ort = { version = "=2.0.0-rc.13", optional = true, default-features = false, features = ["load-dynamic", "ndarray", "std", "api-22"] }
libloading = { version = "0.8", optional = true }
```

La feature devient `kokoro-onnx = ["dep:ort", "dep:libloading"]`, et à la fin :

```toml
[[example]]
name = "kokoro_serve"
required-features = ["kokoro-onnx"]
```

Run: `cargo build --features kokoro-onnx`
Expected: compilation sans erreur (aucune bibliothèque ORT n'est nécessaire
pour compiler en `load-dynamic`).

- [ ] **Étape 2 : écrire les tests qui échouent**

Créer `src/tts/kokoro/onnx.rs` avec son module de tests, et déclarer le
module dans `src/tts/kokoro/mod.rs` après `pub mod voices;` :

```rust
#[cfg(feature = "kokoro-onnx")]
pub mod onnx;
```

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_recent_onnx_runtime_is_accepted() {
        assert!(check_version("1.31.0").is_ok());
        assert!(check_version("1.32.1").is_ok());
        assert!(check_version("2.0.0").is_ok());
        let err = check_version("1.28.0").unwrap_err().to_string();
        assert!(err.contains("1.28.0") && err.contains("4 fois plus lent"), "{err}");
        assert!(check_version("n'importe quoi").is_err());
    }

    #[test]
    fn at_most_four_threads() {
        assert_eq!(OrtBackend::new(8).threads, 4);
        assert_eq!(OrtBackend::new(2).threads, 2);
        assert_eq!(OrtBackend::new(0).threads, 1);
    }

    #[test]
    fn an_absent_library_is_named_in_the_error() {
        let err = version_of(Path::new("/nulle/part/libonnxruntime.so")).unwrap_err().to_string();
        assert!(err.contains("ONNX Runtime introuvable"), "{err}");
    }

    /// Avec ORT ≥ 1.31 et le paquet int8 :
    /// `SILLON_ORT_LIB=… SILLON_KOKORO_DIR=… cargo test --features kokoro-onnx --lib onnx -- --ignored`
    #[test]
    #[ignore]
    fn the_int8_model_speaks_with_durations() {
        let dir = std::path::PathBuf::from(std::env::var("SILLON_KOKORO_DIR").expect("SILLON_KOKORO_DIR"));
        let backend = OrtBackend::new(4);
        backend.load(&dir).unwrap();
        let raw = backend.infer(&[0, 44, 43, 54, 43, 4, 0], &[0.0; 256]).unwrap();
        crate::tts::kokoro::check(&raw, 7).unwrap();
        backend.release();
    }
}
```

Run: `cargo test --features kokoro-onnx --lib kokoro::onnx`
Expected: FAIL — `OrtBackend`, `check_version`, `version_of` introuvables.

- [ ] **Étape 3 : écrire le calcul ORT**

En tête de `src/tts/kokoro/onnx.rs` :

```rust
//! Kokoro par ONNX Runtime, au terminal (Linux, et `sillon` sous macOS).
//!
//! `ort` en `load-dynamic` : la bibliothèque est choisie ici, et sa version
//! vérifiée avant le moindre appel. Le modèle `model_quantized.onnx` (int8)
//! prend `input_ids`, `style` [1, 256] et `speed`, et rend `waveform` et
//! `durations`.

use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::{KokoroBackend, KokoroRaw};

pub const ONNX_FILE: &str = "model_quantized.onnx";
const MINIMUM: (u32, u32) = (1, 31);

#[cfg(target_os = "macos")]
const LIBRARY: &str = "libonnxruntime.dylib";
#[cfg(not(target_os = "macos"))]
const LIBRARY: &str = "libonnxruntime.so";

/// La bibliothèque ORT : `SILLON_ORT_LIB`, à côté du binaire, dans un `lib/`
/// voisin, sinon le nom nu (chemins de recherche du système).
pub fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SILLON_ORT_LIB") {
        return PathBuf::from(path);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        for candidate in [dir.join(LIBRARY), dir.join("lib").join(LIBRARY), dir.join("../lib").join(LIBRARY)] {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(LIBRARY)
}

#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "C" fn(u32) -> *const c_void,
    get_version_string: unsafe extern "C" fn() -> *const c_char,
}

/// La version d'une bibliothèque ORT, lue par `OrtGetApiBase`.
pub fn version_of(library: &Path) -> Result<String> {
    // SAFETY : on charge la bibliothèque et on appelle la seule fonction
    // d'entrée d'ORT, sans argument ; la chaîne rendue est statique.
    unsafe {
        let lib = libloading::Library::new(library)
            .map_err(|e| anyhow!("ONNX Runtime introuvable ({}) : {e}", library.display()))?;
        let base: libloading::Symbol<unsafe extern "C" fn() -> *const OrtApiBase> =
            lib.get(b"OrtGetApiBase").context("bibliothèque qui n'est pas ONNX Runtime")?;
        let base = base();
        anyhow::ensure!(!base.is_null(), "ONNX Runtime sans point d'entrée");
        let version = CStr::from_ptr(((*base).get_version_string)()).to_string_lossy().into_owned();
        Ok(version)
    }
}

/// Refuse un ORT plus ancien que 1.31, avec la version trouvée et la raison.
pub fn check_version(found: &str) -> Result<()> {
    let mut parts = found.trim().split('.').map(|p| p.parse::<u32>());
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        bail!("version d'ONNX Runtime illisible : {found}");
    };
    if (major, minor) < MINIMUM {
        bail!(
            "ONNX Runtime {found} refusé : il faut la {}.{} au moins (les versions antérieures sont 4 fois plus lentes en int8)",
            MINIMUM.0,
            MINIMUM.1
        );
    }
    Ok(())
}

/// Le calcul Kokoro sur processeur.
pub struct OrtBackend {
    pub(crate) threads: usize,
    session: Mutex<Option<Session>>,
}

impl OrtBackend {
    /// `threads` : `engine.threads`, borné à 4 (au-delà, rien à gagner et des
    /// pointes de charge).
    pub fn new(threads: u32) -> Self {
        Self { threads: threads.clamp(1, 4) as usize, session: Mutex::new(None) }
    }
}

impl KokoroBackend for OrtBackend {
    fn label(&self) -> &'static str {
        "int8"
    }

    fn load(&self, model_dir: &Path) -> Result<()> {
        let library = library_path();
        check_version(&version_of(&library)?)?;
        // `ort` lit ce chemin à sa première utilisation.
        std::env::set_var("ORT_DYLIB_PATH", &library);
        let session = Session::builder()?
            .with_intra_threads(self.threads)?
            .with_inter_threads(1)?
            .with_memory_pattern(false)?
            .commit_from_file(model_dir.join(ONNX_FILE))?;
        *self.session.lock().unwrap() = Some(session);
        Ok(())
    }

    fn warm_up(&self) -> Result<()> {
        self.infer(&[0, 43, 0], &[0.0; 256]).map(|_| ())
    }

    fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw> {
        let mut guard = self.session.lock().unwrap();
        let session = guard.as_mut().context("modèle Kokoro non chargé")?;
        let outputs = session.run(ort::inputs![
            "input_ids" => Tensor::from_array(([1usize, ids.len()], ids.to_vec()))?,
            "style" => Tensor::from_array(([1usize, style.len()], style.to_vec()))?,
            "speed" => Tensor::from_array(([1usize], vec![1.0f32]))?,
        ])?;
        let (_, waveform) = outputs["waveform"].try_extract_tensor::<f32>()?;
        let (_, durations) = outputs["durations"].try_extract_tensor::<f32>()?;
        Ok(KokoroRaw {
            samples: waveform.to_vec(),
            durations: durations.iter().map(|d| d.round().max(0.0) as u32).collect(),
        })
    }

    fn release(&self) {
        *self.session.lock().unwrap() = None;
    }
}
```

Run: `cargo test --features kokoro-onnx --lib kokoro::onnx && cargo test --features kokoro-onnx`
Expected: PASS (3 tests, 1 ignoré ; la suite entière avec la feature).
Les noms de l'API `ort` sont ceux du spike (`~/Library/Caches/sillon-bench/spike-kokoro/src/kokoro-ort-rs-dyn/src/main.rs`,
même version) ; si `with_memory_pattern` manque dans cette version, le chercher
dans `cargo doc -p ort --features kokoro-onnx` (`SessionBuilder`) et
l'adapter, sans retirer le réglage.

- [ ] **Étape 4 : le serveur du banc**

Créer `examples/kokoro_serve.rs` :

```rust
//! Kokoro int8 derrière le protocole du banc (`.superpowers/research/eval/`) :
//! une requête JSON par ligne, l'audio en f32 brut dans un fichier.
//!
//!   SILLON_KOKORO_DIR=<models/kokoro> SILLON_ORT_LIB=<libonnxruntime> \
//!     cargo run --release --features kokoro-onnx --example kokoro_serve
//!
//! stdin : {"text": "…", "lang": "fr", "out": "/tmp/x.f32"}
//! stdout : {"n": échantillons, "ms": durée de synthèse, "dur": [durées des mots]}

use std::io::{BufRead, Write};
use std::sync::Arc;

use sillon::tts::kokoro::onnx::OrtBackend;
use sillon::tts::kokoro::phonemes::EspeakProcess;
use sillon::tts::kokoro::KokoroEngine;
use sillon::tts::{silent::SilentEngine, TtsEngine, TtsParams};

fn main() -> anyhow::Result<()> {
    let dir = std::path::PathBuf::from(std::env::var("SILLON_KOKORO_DIR")?);
    let threads: u32 = std::env::var("THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(4);
    let engine = KokoroEngine::load(
        Arc::new(OrtBackend::new(threads)),
        Box::new(EspeakProcess::locate()?),
        &dir,
        Arc::new(SilentEngine),
    )?;
    let mut session = engine.open_session()?;
    println!("{}", serde_json::json!({"ready": true, "threads": threads}));
    std::io::stdout().flush()?;
    for line in std::io::stdin().lock().lines() {
        let request: serde_json::Value = serde_json::from_str(&line?)?;
        let text = request["text"].as_str().unwrap_or_default();
        let params = TtsParams { language: request["lang"].as_str().unwrap_or("fr").to_string(), ..TtsParams::default() };
        let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        let started = std::time::Instant::now();
        let out = session.synthesize_timed(text, &words, &params)?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let bytes: Vec<u8> = out.pcm.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(request["out"].as_str().unwrap_or("/dev/null"), bytes)?;
        let durations: Vec<f32> = out.words.unwrap_or_default().iter().map(|w| w.end - w.start).collect();
        println!("{}", serde_json::json!({"n": out.pcm.samples.len(), "ms": ms, "dur": durations}));
        std::io::stdout().flush()?;
    }
    Ok(())
}
```

Run: `cargo build --release --features kokoro-onnx --example kokoro_serve`
Expected: compilation sans erreur.

- [ ] **Étape 5 : ORT livré sous Linux**

Dans `scripts/package.sh`, après la jonction du shim (avant « Vérification hors
de l'arbre de compilation… ») :

```sh
# Kokoro au terminal : ONNX Runtime 1.31 à côté du binaire, et espeak-ng
# attendu sur le système (paquet `espeak-ng`).
case ",$FEATURES," in
*,kokoro-onnx,*)
    ORT_VERSION=1.31.0
    case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) ORT_ARCHIVE="onnxruntime-linux-x64-$ORT_VERSION" ;;
    Linux-aarch64) ORT_ARCHIVE="onnxruntime-linux-aarch64-$ORT_VERSION" ;;
    *) ORT_ARCHIVE="" ;;
    esac
    if [ -n "$ORT_ARCHIVE" ]; then
        mkdir -p target/ort
        if [ ! -d "target/ort/$ORT_ARCHIVE" ]; then
            curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/$ORT_ARCHIVE.tgz" \
                | tar -xz -C target/ort
        fi
        cp "target/ort/$ORT_ARCHIVE/lib/libonnxruntime.so.$ORT_VERSION" "$OUT/libonnxruntime.so"
        cp "target/ort/$ORT_ARCHIVE/LICENSE" "$OUT/LICENSE-onnxruntime"
        echo "Joint : ONNX Runtime $ORT_VERSION"
    else
        echo "ONNX Runtime non joint sur cette plateforme : SILLON_ORT_LIB=<libonnxruntime> au lancement." >&2
    fi
    ;;
esac
```

Run: `sh -n scripts/package.sh`
Expected: aucune erreur de syntaxe. (L'exécution complète est faite sous Linux,
hors de ce plan.)

- [ ] **Étape 6 : commit**

```bash
git add Cargo.toml Cargo.lock src/tts/kokoro/onnx.rs src/tts/kokoro/mod.rs examples/kokoro_serve.rs scripts/package.sh
git commit -m "$(cat <<'EOF'
feat(kokoro): calcul par ONNX Runtime ≥ 1.31 au terminal (feature kokoro-onnx), serveur du banc, ORT livré sous Linux

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 11 : le passage à Kokoro, une seule fois

Au premier lancement après la mise à jour, une configuration qui n'est pas
déjà sur Kokoro y passe : tout de suite si Kokoro est installé, à la fin de
son téléchargement s'il est seulement installable (l'app le lance). La
migration est retenue (`app.voice_migration`) dès qu'elle agit ; l'ancien
modèle reste installé ; un avis le dit. Au terminal, un Kokoro installé est
adopté de même ; le téléchargement reste réservé à l'app.

**Fichiers :**
- Modifier : `src/config.rs` (`app.voice_migration`)
- Créer : `src/reader/migration.rs`
- Modifier : `src/reader/mod.rs` (`notice`, `switch_when_downloaded`, `pub mod migration;`)
- Modifier : `src/reader/settings.rs` (`poll_download`, avis)
- Modifier : `src/reader/view.rs` (`Snapshot.notice`)
- Modifier : `src/launch.rs` (terminal)

**Interfaces :**
- Consomme : `available_with`, `is_installed`, `install_path`, `total_bytes` (Tâche 6) ; `align_voice_with_family` (Tâche 1).
- Produit :
  - `AppConfig.voice_migration: u32` (0 par défaut) ; `pub const KOKORO_MIGRATION: u32 = 1;`
  - `pub enum Step { Switch(&'static ModelPackage), Download(&'static ModelPackage), Record, Nothing }` ; `pub fn plan(config: &Config, available: &[&'static ModelPackage], installed: impl Fn(&ModelPackage) -> bool) -> Step`
  - `pub fn former_voice(config: &Config) -> String` (nom affiché de la voix actuelle)
  - `Reader::migrate_voice(&mut self, now: Instant) -> Option<String>` ; `Reader::notice(&self) -> Option<&str>` ; `Reader::dismiss_notice(&mut self)`
  - `Snapshot.notice: Option<String>`
  - `launch::migrate_terminal(config: &mut Config) -> Option<String>`

- [ ] **Étape 1 : écrire les tests qui échouent**

Créer `src/reader/migration.rs` avec le module de tests, et déclarer
`pub mod migration;` dans `src/reader/mod.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::models::{self, CATALOG};
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn by_id(id: &str) -> &'static ModelPackage {
        CATALOG.iter().find(|p| p.id == id).unwrap()
    }

    fn on_supertonic() -> Config {
        let mut c = Config::default();
        c.engine.family = "supertonic".into();
        c.engine.model_path = "/m/supertonic/supertonic-3-f16.gguf".into();
        c.voice.voice = "F2".into();
        c
    }

    #[test]
    fn an_installed_kokoro_is_taken_at_once() {
        let available = [by_id("kokoro_82m_mlx"), by_id("supertonic_3_f16")];
        let step = plan(&on_supertonic(), &available, |p| p.family == "kokoro");
        assert_eq!(step, Step::Switch(by_id("kokoro_82m_mlx")));
    }

    #[test]
    fn an_installable_kokoro_is_downloaded() {
        let available = [by_id("kokoro_82m_mlx")];
        assert_eq!(plan(&on_supertonic(), &available, |_| false), Step::Download(by_id("kokoro_82m_mlx")));
        // Une configuration neuve suit la même règle.
        assert_eq!(plan(&Config::default(), &available, |_| false), Step::Download(by_id("kokoro_82m_mlx")));
    }

    #[test]
    fn nothing_is_recorded_without_kokoro_in_this_build() {
        let available = [by_id("supertonic_3_f16")];
        assert_eq!(plan(&on_supertonic(), &available, |_| true), Step::Nothing);
    }

    #[test]
    fn a_config_already_on_kokoro_is_only_recorded() {
        let mut c = on_supertonic();
        c.engine.family = "kokoro".into();
        assert_eq!(plan(&c, &[by_id("kokoro_82m_mlx")], |_| true), Step::Record);
    }

    #[test]
    fn going_back_to_the_old_model_holds_after_a_relaunch() {
        let mut c = on_supertonic();
        c.app.voice_migration = KOKORO_MIGRATION;
        assert_eq!(plan(&c, &[by_id("kokoro_82m_mlx")], |_| true), Step::Nothing, "jamais deux fois");
    }

    #[test]
    fn the_former_voice_is_named_for_the_notice() {
        assert_eq!(former_voice(&Config::default()), "la voix du système");
        let mut c = Config::default();
        c.engine.family = "supertonic".into();
        c.engine.model_path = models::install_path(by_id("supertonic_3_f16")).unwrap().to_string_lossy().into_owned();
        assert_eq!(former_voice(&c), "Supertonic 3 · F16");
    }

    fn reader(config: Config) -> Reader {
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    #[test]
    fn a_reader_off_disk_migrates_nothing() {
        let mut r = reader(on_supertonic());
        r.set_persistent(false);
        r.set_mlx_compute(true);
        assert_eq!(r.migrate_voice(Instant::now()), None);
        assert_eq!(r.config().app.voice_migration, 0);
    }

    #[test]
    fn an_installed_kokoro_replaces_the_voice_and_says_so() {
        // Un paquet MLX complet, creux, dans le dossier isolé des tests.
        let mlx = by_id("kokoro_82m_mlx");
        let root = models::models_dir().unwrap();
        for (_, path, bytes) in models::planned_files_in(&root, mlx) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        }
        let mut r = reader(on_supertonic());
        r.set_mlx_compute(true);
        let notice = r.migrate_voice(Instant::now()).expect("un avis");
        assert!(notice.starts_with("Nouvelle voix : Kokoro remplace"), "{notice}");
        assert!(notice.contains("Réglages › Voix"), "{notice}");
        assert_eq!(r.config().engine.family, "kokoro");
        assert_eq!(r.config().voice.voice, "F2", "l'ancienne voix reste pour y revenir");
        assert_eq!(r.config().voice.per_language["fr"], "p-fenrir60");
        assert_eq!(r.config().app.voice_migration, KOKORO_MIGRATION);
        assert_eq!(r.notice(), Some(notice.as_str()));
        assert_eq!(r.migrate_voice(Instant::now()), None, "une seule fois");
        r.dismiss_notice();
        assert_eq!(r.notice(), None);
        models::remove(mlx).unwrap();
    }
}
```

(`planned_files_in` est `pub(crate)` : visible ici.)

Run: `cargo test --lib reader::migration`
Expected: FAIL — `plan`, `Step`, `KOKORO_MIGRATION`, `migrate_voice`, `voice_migration` introuvables.

- [ ] **Étape 2 : écrire la migration**

Dans `src/config.rs`, `AppConfig` :

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    /// Minutes pendant lesquelles la voix reste chargée après la dernière
    /// lecture. `-1` : toujours ; `0` : relâchée dès l'arrêt.
    pub keep_voice_minutes: i64,
    /// Migrations de voix déjà faites (1 : le passage à Kokoro).
    pub voice_migration: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { keep_voice_minutes: 10, voice_migration: 0 }
    }
}
```

En tête de `src/reader/migration.rs` :

```rust
//! Le passage à Kokoro : une seule fois, au premier lancement après la mise à
//! jour, pour une configuration neuve comme pour une configuration qui nomme
//! déjà un modèle. L'ancien modèle reste installé, et un avis le dit.

use std::time::Instant;

use crate::config::Config;
use crate::models::{self, ModelPackage};

use super::Reader;

/// La migration qui fait passer à Kokoro.
pub const KOKORO_MIGRATION: u32 = 1;

/// Ce que la migration fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Kokoro est installé : il devient le modèle actif.
    Switch(&'static ModelPackage),
    /// Kokoro est installable : il se télécharge, puis devient actif.
    Download(&'static ModelPackage),
    /// Déjà sur Kokoro : seulement retenir que c'est fait.
    Record,
    /// Rien : déjà faite, ou pas de Kokoro dans ce binaire.
    Nothing,
}

pub fn plan(config: &Config, available: &[&'static ModelPackage], installed: impl Fn(&ModelPackage) -> bool) -> Step {
    if config.app.voice_migration >= KOKORO_MIGRATION {
        return Step::Nothing;
    }
    let kokoro: Vec<&'static ModelPackage> = available.iter().copied().filter(|p| p.family == "kokoro").collect();
    let Some(first) = kokoro.first().copied() else {
        return Step::Nothing;
    };
    if config.engine.family == "kokoro" {
        return Step::Record;
    }
    match kokoro.iter().copied().find(|p| installed(p)) {
        Some(package) => Step::Switch(package),
        None => Step::Download(first),
    }
}

/// Le nom de la voix en place, pour l'avis.
pub fn former_voice(config: &Config) -> String {
    let path = config.engine.model_path.trim();
    if path.is_empty() {
        return "la voix du système".to_string();
    }
    models::CATALOG
        .iter()
        .find(|p| models::is_active_package(path, p))
        .map(|p| p.display_name.to_string())
        .unwrap_or_else(|| config.engine.family.clone())
}

fn switched(former: &str) -> String {
    format!("Nouvelle voix : Kokoro remplace {former}. L'ancien modèle reste installé : Réglages › Voix pour y revenir.")
}

impl Reader {
    /// Le passage à Kokoro, appelé une fois au lancement de l'app. Rend
    /// l'avis à montrer, s'il y en a un. Un lecteur hors disque ne bouge pas.
    pub fn migrate_voice(&mut self, now: Instant) -> Option<String> {
        if !self.persist {
            return None;
        }
        let config = self.current_config().clone();
        let available = models::available_with(self.mlx);
        let former = former_voice(&config);
        let notice = match plan(&config, &available, models::is_installed) {
            Step::Nothing => return None,
            Step::Record => None,
            Step::Switch(package) => {
                self.use_voice(package.id, now).ok()?;
                Some(switched(&former))
            }
            Step::Download(package) => {
                self.install_voice(package.id).ok()?;
                self.switch_when_downloaded = Some((package.id.to_string(), former.clone()));
                Some(format!(
                    "Nouvelle voix Kokoro en téléchargement ({}) : elle remplacera {former} à la fin.",
                    models::human_size(package.total_bytes())
                ))
            }
        };
        self.record_migration();
        self.notice = notice.clone();
        notice
    }

    fn record_migration(&mut self) {
        self.config.app.voice_migration = KOKORO_MIGRATION;
        if let Some(app) = self.session.as_mut() {
            app.config.app.voice_migration = KOKORO_MIGRATION;
        }
        let _ = self.config.save();
    }

    /// L'avis en attente (passage à Kokoro).
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }

    /// Un téléchargement lancé par la migration est fini : Kokoro devient
    /// la voix active, et l'avis le dit.
    pub(super) fn finish_migration(&mut self, downloaded: &str, now: Instant) {
        let Some((id, former)) = self.switch_when_downloaded.take() else { return };
        if id != downloaded {
            self.switch_when_downloaded = Some((id, former));
            return;
        }
        if self.use_voice(&id, now).is_ok() {
            self.notice = Some(switched(&former));
        }
    }
}
```

Dans `src/reader/mod.rs` : champs `notice: Option<String>,` et
`switch_when_downloaded: Option<(String, String)>,` dans `Reader`, à `None`
dans `new`.

Dans `src/reader/settings.rs`, `poll_download` :

```rust
    pub(super) fn poll_download(&mut self, now: Instant) {
        let done = self
            .download
            .as_ref()
            .is_some_and(|d| matches!(d.progress(), Progress::Done));
        if !done {
            return;
        }
        let id = self.download.take().map(|d| d.package.id.to_string());
        let Some(id) = id else { return };
        if self.switch_when_downloaded.as_ref().is_some_and(|(pending, _)| *pending == id) {
            self.finish_migration(&id, now);
            return;
        }
        let none_active = !self.voices().iter().any(|v| v.active);
        if none_active {
            let _ = self.use_voice(&id, now);
        }
    }
```

Dans `src/reader/view.rs` : `pub notice: Option<String>,` dans `Snapshot`
(après `document_language`), et `notice: reader.notice().map(str::to_string),`
dans les deux constructions.

Run: `cargo test --lib reader`
Expected: PASS.

- [ ] **Étape 3 : le terminal adopte un Kokoro installé**

Dans `src/launch.rs`, ajouter :

```rust
/// Le passage à Kokoro au terminal : un Kokoro installé est adopté, une
/// seule fois ; installable seulement, il est signalé sans rien retenir
/// (l'app le télécharge). Rend l'avis.
pub fn migrate_terminal(config: &mut Config) -> Option<String> {
    use crate::reader::migration::{former_voice, plan, Step, KOKORO_MIGRATION};
    let former = former_voice(config);
    match plan(config, &crate::models::available(), crate::models::is_installed) {
        Step::Switch(package) => {
            let path = crate::models::install_path(package)?;
            config.engine.model_path = path.to_string_lossy().into_owned();
            config.engine.family = package.family.to_string();
            if config.engine.engine == "auto" {
                config.engine.engine = "audio_cpp".to_string();
            }
            config.align_voice_with_family();
            config.app.voice_migration = KOKORO_MIGRATION;
            Some(format!("Nouvelle voix : Kokoro remplace {former}. L'ancien modèle reste installé (`s` pour y revenir)."))
        }
        Step::Record => {
            config.app.voice_migration = KOKORO_MIGRATION;
            None
        }
        Step::Download(_) => Some("Nouvelle voix Kokoro disponible : `s` pour l'installer.".to_string()),
        Step::Nothing => None,
    }
}
```

et dans `prepare`, juste avant `// Un modèle déjà installé devient le défaut, sans rien demander.` :

```rust
    // Le passage à Kokoro s'écrit sur la configuration du disque, jamais sur
    // celle que les options de la ligne de commande ont modifiée.
    if config_notice_is_none {
        let (mut on_disk, _) = Config::load();
        if let Some(notice) = migrate_terminal(&mut on_disk) {
            notices.push(notice);
            if on_disk.app.voice_migration != config.app.voice_migration {
                let _ = on_disk.save();
                config.engine.model_path = on_disk.engine.model_path.clone();
                config.engine.family = on_disk.engine.family.clone();
                config.engine.engine = on_disk.engine.engine.clone();
                config.voice.per_language = on_disk.voice.per_language.clone();
                config.app.voice_migration = on_disk.app.voice_migration;
            }
        } else if on_disk.app.voice_migration != config.app.voice_migration {
            let _ = on_disk.save();
            config.app.voice_migration = on_disk.app.voice_migration;
        }
    }
```

avec, en tête de `prepare`, juste après `let (mut config, config_notice) = Config::load();` :

```rust
    // Une configuration illisible n'est jamais réécrite.
    let config_notice_is_none = config_notice.is_none();
```

Dans `src/launch.rs`, module de tests s'il existe (sinon en créer un à la fin
du fichier) :

```rust
#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn the_terminal_never_downloads_and_records_nothing_then() {
        let mut config = Config::default();
        config.engine.family = "supertonic".into();
        let notice = migrate_terminal(&mut config);
        if crate::models::available().iter().any(|p| p.family == "kokoro") {
            assert!(notice.is_some_and(|n| n.contains("`s`")));
        }
        assert_eq!(config.app.voice_migration, 0, "rien de retenu sans adoption");
        assert_eq!(config.engine.family, "supertonic");
    }
}
```

Run: `cargo test && cargo test --features kokoro-onnx`
Expected: PASS.

- [ ] **Étape 4 : commit**

```bash
git add src/config.rs src/reader/migration.rs src/reader/mod.rs src/reader/settings.rs src/reader/view.rs src/launch.rs
git commit -m "$(cat <<'EOF'
feat(reader): passage unique à Kokoro, installé ou téléchargé, ancien modèle gardé, avis

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 12 : le pont UniFFI — calcul étranger, langue, voix par langue, avis

Le FFI gagne l'interface étrangère `KokoroCompute` (implémentée en Swift par
MLX à la Tâche 13), le constructeur `SillonReader::with_kokoro`, les voix par
langue, la langue du document, l'avis du passage à Kokoro, et les compteurs de
la mesure. `SillonReader::new()` ne change pas. Les bindings sont régénérés,
et le code Swift existant suit les nouveaux champs (sans toucher l'onglet
Voix). `sillon-ffi` n'active jamais `kokoro-onnx`.

**Fichiers :**
- Modifier : `ffi/Cargo.toml` (`anyhow`)
- Modifier : `ffi/src/lib.rs`
- Modifier : `scripts/build-ffi.sh` (garde : pas d'ORT dans l'app)
- Modifier : `macos/Sources/Settings/SettingsDefaults.swift`, `macos/Sources/Settings/SettingsDiff.swift`
- Modifier : `macos/Sources/Model/ReaderModel.swift` (appels nouveaux)
- Modifier : `macos/Tests/FakeSettingsBackend.swift`, `macos/Tests/Samples.swift`
- Créer : `macos/Tests/KokoroBridgeTests.swift`

**Interfaces :**
- Consomme : `KokoroBackend`, `KokoroRaw`, `check`, `build_engine_with` (Tâche 9) ; `Reader::set_mlx_compute` (Tâche 6) ; `set_document_language`, `preview_voice_in`, `DocumentLanguage`, `voices_by_language`, `voice_for_language` (Tâche 3) ; `migrate_voice`, `notice`, `dismiss_notice` (Tâche 11) ; `SynthesisStats.busy`, `App::stalls` (Tâches 4, 5).
- Produit (Rust / Swift) :
  - `#[uniffi::export(with_foreign)] pub trait KokoroCompute: Send + Sync { fn load(&self, model_dir: String) -> Result<(), SillonError>; fn warm_up(&self) -> Result<(), SillonError>; fn synthesize(&self, ids: Vec<i64>, style: Vec<f32>) -> Result<KokoroOutput, SillonError>; fn release(&self); }` → Swift `protocol KokoroCompute { func load(modelDir:) throws; func warmUp() throws; func synthesize(ids:style:) throws -> KokoroOutput; func release() }`
  - `KokoroOutput { samples: Vec<f32>, durations: Vec<u32> }`
  - `SillonReader::with_kokoro(compute: Arc<dyn KokoroCompute>) -> Arc<Self>` → Swift `SillonReader.withKokoro(compute:)`
  - `kokoro_bridge_check(compute, model_dir: String) -> Result<u64, SillonError>` → Swift `kokoroBridgeCheck(compute:modelDir:)`
  - `LanguageVoice { language, voice }` ; `SettingsView.voices_by_language: Vec<LanguageVoice>` ; `SettingsPatch.voice_for_language: Option<LanguageVoice>`
  - `VoiceChoice.language` ; `VoiceModel.voices_label`, `VoiceModel.licenses`
  - `DocumentLanguageView { detected: Option<String>, chosen: Option<String>, effective: String, minority_share: f32, mixed: bool }` ; `Snapshot.document_language`, `Snapshot.notice`
  - `SillonReader` : `set_document_language(language: Option<String>) -> Result<(), SillonError>`, `preview_voice_in(language: String) -> Result<(), SillonError>`, `migrate_voice() -> Option<String>`, `dismiss_notice()`, `measures() -> Measures { busy_seconds: f64, stalls: u32 }`
  - Swift `ReaderModel` : `setDocumentLanguage(_:) async throws`, `previewVoice(in:) async throws`, `migrateVoice() -> String?`, `dismissNotice()`

- [ ] **Étape 1 : écrire les tests Rust qui échouent**

Dans `ffi/src/lib.rs`, module `tests` :

```rust
    /// Un calcul « étranger » écrit en Rust : deux trames par jeton.
    struct FakeCompute {
        fail: bool,
    }

    impl KokoroCompute for FakeCompute {
        fn load(&self, _model_dir: String) -> Result<(), SillonError> {
            Ok(())
        }
        fn warm_up(&self) -> Result<(), SillonError> {
            Ok(())
        }
        fn synthesize(&self, ids: Vec<i64>, _style: Vec<f32>) -> Result<KokoroOutput, SillonError> {
            if self.fail {
                return Err(SillonError::Failed { message: "Calcul Metal indisponible".into() });
            }
            Ok(KokoroOutput { samples: vec![0.0; ids.len() * 2 * 600], durations: vec![2; ids.len()] })
        }
        fn release(&self) {}
    }

    #[test]
    fn a_foreign_compute_crosses_the_bridge() {
        assert_eq!(kokoro_bridge_check(Arc::new(FakeCompute { fail: false }), "/x".into()).unwrap(), 3 * 2 * 600);
        let err = kokoro_bridge_check(Arc::new(FakeCompute { fail: true }), "/x".into()).unwrap_err().to_string();
        assert!(err.contains("Calcul Metal indisponible"), "{err}");
    }

    #[test]
    fn the_language_of_the_document_reaches_the_snapshot() {
        let reader = SillonReader::silent();
        reader.open_text(
            "Monads let you chain computations that carry a context, without handling it by hand at every step.".into(),
            "test".into(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while reader.snapshot().document_language.is_none() {
            assert!(Instant::now() < deadline, "langue jamais publiée");
            std::thread::sleep(Duration::from_millis(5));
        }
        let lang = reader.snapshot().document_language.unwrap();
        assert_eq!(lang.effective, "en");
        assert!(!lang.mixed);
        reader.set_document_language(Some("fr".into())).unwrap();
        // L'instantané se republie au tour suivant du fil du lecteur.
        while reader.snapshot().document_language.and_then(|l| l.chosen).as_deref() != Some("fr") {
            assert!(Instant::now() < deadline, "choix jamais publié");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(reader.set_document_language(Some("de".into())).is_err());
    }

    #[test]
    fn a_voice_per_language_round_trips_and_the_silent_reader_never_migrates() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.voice_for_language = Some(LanguageVoice { language: "en".into(), voice: "af_sky".into() });
        let applied = reader.apply_patch(patch);
        assert_eq!(applied.voices_by_language, [LanguageVoice { language: "en".into(), voice: "af_sky".into() }]);
        assert_eq!(reader.migrate_voice(), None, "hors disque : rien ne bouge");
        assert_eq!(reader.measures().stalls, 0);
    }
```

Run: `cargo test -p sillon-ffi`
Expected: FAIL — `KokoroCompute`, `KokoroOutput`, `kokoro_bridge_check`, `LanguageVoice`, `document_language`… introuvables.

- [ ] **Étape 2 : l'interface étrangère et le constructeur**

Dans `ffi/Cargo.toml`, `[dependencies]` : `anyhow = "1.0"`.

Dans `ffi/src/lib.rs`, après `fn failed` :

```rust
/// Une erreur inattendue côté Swift (exception non prévue) devient un échec
/// ordinaire : UniFFI l'exige des erreurs d'une interface étrangère.
impl From<uniffi::UnexpectedUniFFICallbackError> for SillonError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        SillonError::Failed { message: e.reason }
    }
}

/// Le calcul de Kokoro, fourni par l'app (MLX). Le moteur Rust garde
/// phonétisation, découpage, voix, minutage, Sonic et lecteur ; ces appels
/// arrivent sur le fil de synthèse et bloquent jusqu'au résultat.
#[uniffi::export(with_foreign)]
pub trait KokoroCompute: Send + Sync {
    fn load(&self, model_dir: String) -> Result<(), SillonError>;
    fn warm_up(&self) -> Result<(), SillonError>;
    fn synthesize(&self, ids: Vec<i64>, style: Vec<f32>) -> Result<KokoroOutput, SillonError>;
    fn release(&self);
}

#[derive(uniffi::Record)]
pub struct KokoroOutput {
    pub samples: Vec<f32>,
    pub durations: Vec<u32>,
}

/// `KokoroCompute` vu par le moteur.
struct ForeignKokoro(Arc<dyn KokoroCompute>);

impl sillon::tts::kokoro::KokoroBackend for ForeignKokoro {
    fn label(&self) -> &'static str {
        "MLX"
    }
    fn load(&self, model_dir: &std::path::Path) -> anyhow::Result<()> {
        self.0.load(model_dir.to_string_lossy().into_owned()).map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    fn warm_up(&self) -> anyhow::Result<()> {
        self.0.warm_up().map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    fn infer(&self, ids: &[i64], style: &[f32]) -> anyhow::Result<sillon::tts::kokoro::KokoroRaw> {
        let out = self.0.synthesize(ids.to_vec(), style.to_vec()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(sillon::tts::kokoro::KokoroRaw { samples: out.samples, durations: out.durations })
    }
    fn release(&self) {
        self.0.release();
    }
}

/// Vérifie le pont vers un calcul Kokoro étranger : chargement, trois
/// jetons, libération, sortie contrôlée. Rend le nombre d'échantillons.
#[uniffi::export]
pub fn kokoro_bridge_check(compute: Arc<dyn KokoroCompute>, model_dir: String) -> Result<u64, SillonError> {
    use sillon::tts::kokoro::KokoroBackend;
    let backend = ForeignKokoro(compute);
    backend.load(std::path::Path::new(&model_dir)).map_err(failed)?;
    let raw = backend.infer(&[0, 43, 0], &[0.0; 256]).map_err(failed)?;
    backend.release();
    sillon::tts::kokoro::check(&raw, 3).map_err(failed)?;
    Ok(raw.samples.len() as u64)
}
```

Dans `#[uniffi::export] impl SillonReader`, après `new` :

```rust
    /// Le lecteur réel avec Kokoro en MLX : le calcul vient de l'app.
    #[uniffi::constructor]
    pub fn with_kokoro(compute: Arc<dyn KokoroCompute>) -> Arc<Self> {
        let backend: Arc<dyn sillon::tts::kokoro::KokoroBackend> = Arc::new(ForeignKokoro(compute));
        Arc::new(Self {
            handle: ReaderHandle::spawn(move || {
                let (config, _) = Config::load();
                let mlx = Arc::clone(&backend);
                let mut reader = Reader::new(
                    config,
                    Box::new(move |config: &Config| {
                        tts::build_engine_with(
                            config.engine_preference(),
                            &config.engine_settings(),
                            Some(Arc::clone(&mlx)),
                        )
                        .0
                    }),
                    Box::new(Player::new),
                    Instant::now(),
                );
                reader.set_mlx_compute(true);
                reader
            }),
        })
    }
```

- [ ] **Étape 3 : les nouveaux champs et appels**

Dans `ffi/src/lib.rs` :

1. Records :

```rust
#[derive(uniffi::Record, Clone, PartialEq)]
pub struct LanguageVoice {
    pub language: String,
    pub voice: String,
}

#[derive(uniffi::Record)]
pub struct DocumentLanguageView {
    pub detected: Option<String>,
    pub chosen: Option<String>,
    pub effective: String,
    pub minority_share: f32,
    /// Plus de 30 % des lettres dans l'autre langue.
    pub mixed: bool,
}

#[derive(uniffi::Record)]
pub struct Measures {
    pub busy_seconds: f64,
    pub stalls: u32,
}
```

2. `Snapshot` : après `preview: PreviewView,` ajouter
`pub document_language: Option<DocumentLanguageView>,` et `pub notice: Option<String>,` ;
dans `snapshot_of` :

```rust
        document_language: s.document_language.map(|l| DocumentLanguageView {
            mixed: l.mixed(),
            detected: l.detected,
            chosen: l.chosen,
            effective: l.effective,
            minority_share: l.minority_share,
        }),
        notice: s.notice,
```

3. `SettingsView` : `pub voices_by_language: Vec<LanguageVoice>,` en dernier ;
dans `settings_of` :
`voices_by_language: s.voices_by_language.into_iter().map(|(language, voice)| LanguageVoice { language, voice }).collect(),`.
`SettingsPatch` : `pub voice_for_language: Option<LanguageVoice>,` en dernier ;
dans `patch_from` : `voice_for_language: p.voice_for_language.map(|v| (v.language, v.voice)),`.

4. `VoiceChoice` : `pub language: String,` (rempli par `v.language` dans
`capabilities`). `VoiceModel` : `pub voices_label: String,` et
`pub licenses: String,` après `pace` (remplis par `v.voices_label`,
`v.licenses` dans `voices`).

5. Dans `impl SillonReader`, après `stop_preview` :

```rust
    /// La langue du document gardé (`None` : automatique).
    pub fn set_document_language(&self, language: Option<String>) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| {
                reader
                    .set_document_language(language.as_deref(), Instant::now())
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// La phrase d'exemple dans une langue : chaque voix dans la sienne.
    pub fn preview_voice_in(&self, language: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.preview_voice_in(&language, Instant::now()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// Le passage unique à Kokoro, au lancement de l'app ; l'avis à montrer.
    pub fn migrate_voice(&self) -> Option<String> {
        self.handle.ask(|reader| reader.migrate_voice(Instant::now())).flatten()
    }

    pub fn dismiss_notice(&self) {
        let _ = self.handle.ask(|reader| reader.dismiss_notice());
    }

    /// Temps passé à synthétiser et coupures du document ouvert : la mesure
    /// du filtre de charge.
    pub fn measures(&self) -> Measures {
        self.handle
            .ask(|reader| {
                reader.session().map(|app| Measures {
                    busy_seconds: app.synth.stats().busy.as_secs_f64(),
                    stalls: app.stalls() as u32,
                })
            })
            .flatten()
            .unwrap_or(Measures { busy_seconds: 0.0, stalls: 0 })
    }
```

Run: `cargo test -p sillon-ffi`
Expected: PASS.

- [ ] **Étape 4 : jamais d'ORT dans l'app**

Dans `scripts/build-ffi.sh`, après `FEATURES=${FEATURES-audio-cpp,ocr}` :

```bash
# L'app calcule Kokoro en MLX : ONNX Runtime n'y entre jamais.
case ",$FEATURES," in
*,kokoro-onnx,*)
    echo "Erreur : kokoro-onnx est réservé au terminal, pas à l'app" >&2
    exit 1
    ;;
esac
```

Run: `FEATURES=kokoro-onnx ./scripts/build-ffi.sh; echo "code $?"`
Expected: le message d'erreur, `code 1`.

- [ ] **Étape 5 : régénérer les bindings, faire suivre Swift**

Run: `./scripts/build-ffi.sh`
Expected: « Liaison prête dans macos/Generated et macos/Libraries ».

Dans `macos/Sources/Settings/SettingsDefaults.swift`, `SettingsPatch.empty` se
termine par `keepVoiceMinutes: nil, extraSet: [], extraRemove: [], voiceForLanguage: nil)`.

Dans `macos/Sources/Settings/SettingsDiff.swift`, avant `return p` :

```swift
        // Une voix par langue : la première langue changée part (un geste
        // n'en touche qu'une).
        let voicesBefore = Dictionary(old.voicesByLanguage.map { ($0.language, $0.voice) }, uniquingKeysWith: { $1 })
        if let changed = new.voicesByLanguage.first(where: { voicesBefore[$0.language] != $0.voice }) {
            p.voiceForLanguage = changed
        }
```

Dans `macos/Tests/FakeSettingsBackend.swift`, les `VoiceChoice(id:label:)`
reçoivent `language: ""` ; `updateSettings` de `FakeSettingsBackend` gagne :

```swift
        if let v = patch.voiceForLanguage {
            stored.voicesByLanguage.removeAll { $0.language == v.language }
            stored.voicesByLanguage.append(v)
        }
```

Dans `macos/Tests/Samples.swift`, `VoiceModel(…)` reçoit
`voicesLabel: "", licenses: ""` après `pace:`.

Dans `macos/Sources/Model/ReaderModel.swift`, après `stopPreview()` :

```swift
    /// La langue du document gardé (`nil` : automatique), hors du fil principal.
    func setDocumentLanguage(_ language: String?) async throws {
        let reader = reader
        try await Task.detached { try reader.setDocumentLanguage(language: language) }.value
        wake()
    }
    /// « Écouter » une voix dans sa langue.
    func previewVoice(in language: String) async throws {
        let reader = reader
        try await Task.detached { try reader.previewVoiceIn(language: language) }.value
        wake()
    }
    /// Le passage unique à Kokoro : appelé une fois au lancement.
    func migrateVoice() -> String? {
        reader.migrateVoice()
    }
    func dismissNotice() {
        reader.dismissNotice()
        wake()
    }
```

Créer `macos/Tests/KokoroBridgeTests.swift` :

```swift
import XCTest
@testable import Sillon

/// Un calcul Kokoro écrit en Swift, branché sur le pont : jamais MLX dans les
/// tests unitaires.
final class FakeKokoroCompute: KokoroCompute, @unchecked Sendable {
    private let lock = NSLock()
    private var counts = (load: 0, warmUp: 0, synthesize: 0, release: 0)
    var failLoad = false

    var loads: Int { lock.withLock { counts.load } }
    var warmUps: Int { lock.withLock { counts.warmUp } }
    var releases: Int { lock.withLock { counts.release } }

    func load(modelDir: String) throws {
        lock.withLock { counts.load += 1 }
        if failLoad { throw SillonError.Failed(message: "Calcul Metal indisponible") }
    }
    func warmUp() throws { lock.withLock { counts.warmUp += 1 } }
    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        lock.withLock { counts.synthesize += 1 }
        return KokoroOutput(samples: [Float](repeating: 0, count: ids.count * 2 * 600),
                            durations: [UInt32](repeating: 2, count: ids.count))
    }
    func release() { lock.withLock { counts.release += 1 } }
}

final class KokoroBridgeTests: XCTestCase {
    func testASwiftComputeCrossesTheBridge() throws {
        let compute = FakeKokoroCompute()
        XCTAssertEqual(try kokoroBridgeCheck(compute: compute, modelDir: "/x"), 3 * 2 * 600)
        XCTAssertEqual(compute.loads, 1)
        XCTAssertEqual(compute.releases, 1)
    }

    func testAFailureInSwiftComesBackAsItsMessage() {
        let compute = FakeKokoroCompute()
        compute.failLoad = true
        XCTAssertThrowsError(try kokoroBridgeCheck(compute: compute, modelDir: "/x")) { error in
            XCTAssertEqual(SettingsStore.message(of: error), "Calcul Metal indisponible")
        }
    }
}
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **` (les 4 fichiers non commités de l'onglet Voix
sont dans l'arbre de travail et compilent ; ils ne sont pas indexés ici).

- [ ] **Étape 6 : commit**

```bash
git add ffi/Cargo.toml ffi/src/lib.rs Cargo.lock scripts/build-ffi.sh macos/Sources/Settings/SettingsDefaults.swift macos/Sources/Settings/SettingsDiff.swift macos/Sources/Model/ReaderModel.swift macos/Tests/FakeSettingsBackend.swift macos/Tests/Samples.swift macos/Tests/KokoroBridgeTests.swift
git commit -m "$(cat <<'EOF'
feat(ffi): calcul Kokoro étranger (KokoroCompute), lecteur avec MLX, langue du document, voix par langue, avis et mesures

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 13 : Kokoro en MLX dans l'app, espeak-ng embarqué, préparation des noyaux Metal

Les fichiers Kokoro de mlx-audio-swift (MIT, commit `244458d`) sont repris dans
un framework `KokoroMLX` (mlx-swift 0.32.3 seul : imports `MLXLMCommon`,
`HuggingFace`, `MLXAudioCore` et le processeur de texte retirés, passage avant
public). `KokoroMLXCompute` implémente `KokoroCompute` : file série à soi,
plafond MLX 1 Go, cache réduit, `release()` qui libère poids et cache (compté :
la préparation et le lecteur partagent le modèle). L'app lance le lecteur avec
Kokoro, passe à Kokoro une fois (avis), et prépare les noyaux Metal une fois
par version, en arrière-plan. espeak-ng va dans `Contents/Helpers`, ses
données dans `Contents/Resources`, avec sa licence et l'adresse des sources.

**Fichiers :**
- Créer : `macos/Vendor/KokoroMLX/` (fichiers repris, `LICENSE`, `ORIGINE.md`)
- Créer : `macos/Sources/Voice/KokoroMLX.swift`, `macos/Sources/Voice/KokoroWarmUp.swift`
- Créer : `macos/Tests/KokoroWarmUpTests.swift`
- Modifier : `macos/project.yml`
- Modifier : `macos/Sources/AppDelegate.swift`
- Modifier : `scripts/build-macos.sh`
- Modifier : `NOTICE`

**Interfaces :**
- Consomme : `KokoroCompute`, `KokoroOutput`, `SillonReader.withKokoro(compute:)`, `ReaderModel.migrateVoice()` (Tâche 12) ; `FakeKokoroCompute` (`macos/Tests/KokoroBridgeTests.swift`, Tâche 12) ; `scripts/build-espeak.sh` (Tâche 7).
- Produit :
  - module Swift `KokoroMLX` : `public final class KokoroModel` (`static func load(directory: URL) throws -> KokoroModel`, `callAsFunction(inputIds:refS:speed:)`) et `public final class KokoroRunner` (`static func load(directory: URL, memoryLimit: Int, cacheLimit: Int) throws -> KokoroRunner`, `func run(ids: [Int32], style: [Float]) -> (samples: [Float], durations: [Int32])`, `static func clearCache()`) — seule porte de l'app vers MLX
  - `final class KokoroMLXCompute: KokoroCompute` ; `var calls: Int` (appels de synthèse, pour la mesure)
  - `@MainActor enum KokoroWarmUp { static var isRunning: Bool; static func modelDirectory(settings: SettingsView) -> String?; static func runIfNeeded(compute:modelDirectory:version:defaults:) -> Task<Void, Never>? }`
  - `AppDelegate.kokoroCompute: KokoroMLXCompute?` ; `AppDelegate.exportEspeakLocation(bundle:)`

- [ ] **Étape 1 : vérifier la chaîne Metal**

Run: `xcrun -sdk macosx metal -v`
Expected: une version du compilateur Metal. Sinon, **s'arrêter et demander à
l'utilisateur** d'installer le composant (`xcodebuild -downloadComponent MetalToolchain`,
quelques centaines de Mo) : mlx-swift compile ses noyaux `.metal` à la
construction, et sans lui ni l'app ni les tests unitaires ne se construisent.

- [ ] **Étape 2 : reprendre les fichiers Kokoro**

Run:
```bash
SRC=$(mktemp -d)/mlx-audio-swift
git clone --quiet https://github.com/Blaizzy/mlx-audio-swift "$SRC"
git -C "$SRC" checkout --quiet 244458d
M="$SRC/Sources/MLXAudioTTS/Models/StyleTTS2"
mkdir -p macos/Vendor/KokoroMLX/Blocks
cp "$M/Kokoro/KokoroConfig.swift" "$M/Kokoro/KokoroModules.swift" "$M/Kokoro/KokoroDecoder.swift" \
   "$M/SharedConfigs.swift" "$M/Albert.swift" macos/Vendor/KokoroMLX/
cp "$M"/Blocks/*.swift macos/Vendor/KokoroMLX/Blocks/
cp "$SRC/LICENSE" macos/Vendor/KokoroMLX/LICENSE
python3 - <<'EOF'
import pathlib, re
d = pathlib.Path("macos/Vendor/KokoroMLX")
c = d / "KokoroConfig.swift"; s = c.read_text()
s = s.replace("@preconcurrency import MLXLMCommon\n", "")
s = s.replace("\n    let quantization: BaseConfiguration.Quantization?\n", "\n")
s = s.replace('        case quantization\n        case quantizationAlt = "quantization_config"\n', "")
s = re.sub(r"\n        quantization =\n.*?forKey: \.quantizationAlt\)\n", "\n", s, flags=re.S)
assert "BaseConfiguration" not in s and "MLXLMCommon" not in s
c.write_text(s)
sc = d / "SharedConfigs.swift"; t = sc.read_text().replace("import MLXLMCommon\n", "")
assert "MLXLMCommon" not in t; sc.write_text(t)
dec = d / "KokoroDecoder.swift"; u = dec.read_text().replace("import MLXNN\n", "import MLXNN\nimport MLXFFT\n", 1)
dec.write_text(u)
for f in d.rglob("*.swift"):
    body = f.read_text()
    assert "HuggingFace" not in body and "MLXAudioCore" not in body, f
print("repris :", sorted(p.name for p in d.rglob("*.swift")))
EOF
```
Expected: la liste des fichiers repris (KokoroConfig, KokoroModules,
KokoroDecoder, SharedConfigs, Albert et les 8 blocs), sans erreur.

Créer `macos/Vendor/KokoroMLX/KokoroModel.swift` (le passage avant seul,
recopié de `KokoroModel.swift` au commit `244458d` ; ni génération, ni voix, ni
téléchargement, ni quantification) :

```swift
import Foundation
@preconcurrency import MLX
import MLXNN

/// Kokoro-82M (StyleTTS2), repris de mlx-audio-swift (MIT, Prince Canuma,
/// commit 244458d) : le passage avant seul. Phonétisation, voix, vitesse et
/// minutage sont côté Sillon ; la vitesse reste 1.
public final class KokoroModel: Module, @unchecked Sendable {
    public let config: KokoroConfig
    public let sampleRate: Int

    @ModuleInfo var bert: Albert
    @ModuleInfo(key: "bert_encoder") var bertEncoder: Linear
    @ModuleInfo var predictor: KokoroProsodyPredictor
    @ModuleInfo(key: "text_encoder") var textEncoder: KokoroTextEncoder
    @ModuleInfo var decoder: KokoroDecoder

    private init(config: KokoroConfig) {
        self.config = config
        self.sampleRate = config.sampleRate

        _bert = ModuleInfo(wrappedValue: Albert(config: config.plbert, vocabSize: config.nToken))
        _bertEncoder = ModuleInfo(
            wrappedValue: Linear(config.plbert.hiddenSize, config.hiddenDim),
            key: "bert_encoder"
        )
        _predictor = ModuleInfo(wrappedValue: KokoroProsodyPredictor(
            styleDim: config.styleDim, dHid: config.hiddenDim,
            nLayers: config.nLayer, maxDur: config.maxDur, dropout: 0.0
        ))
        _textEncoder = ModuleInfo(wrappedValue: KokoroTextEncoder(
            channels: config.hiddenDim, kernelSize: config.textEncoderKernelSize,
            depth: config.nLayer, nSymbols: config.nToken
        ), key: "text_encoder")
        _decoder = ModuleInfo(wrappedValue: KokoroDecoder(config: config))
    }

    // MARK: - Passage avant

    public func callAsFunction(
        inputIds: MLXArray, refS: MLXArray, speed: Float = 1.0
    ) -> (audio: MLXArray, predDur: MLXArray) {
        let seqLen = inputIds.shape[inputIds.ndim - 1]
        let inputLengths = MLXArray([Int32(seqLen)])
        var textMask = MLXArray(Array(0..<Int32(seqLen))).reshaped([1, -1])
        textMask = (textMask + 1) .> inputLengths.reshaped([-1, 1])

        let attMask = MLXArray(1) - textMask.asType(.int32)
        let (bertOut, _) = bert(inputIds, attentionMask: attMask)
        let dEn = bertEncoder(bertOut).transposed(0, 2, 1)

        let globalStyle = refS[0..., 128...]
        let acousticStyle = refS[0..., ..<128]

        let d = predictor.textEncoder(dEn, style: globalStyle, textLengths: inputLengths, mask: textMask)
        let (x, _) = predictor.lstm(d)
        let duration = predictor.durationProj(x)
        // Défensif : une durée NaN donnerait un entier absurde ; bornée.
        let maxFramesPerPhoneme = 100
        let durRaw = MLX.sigmoid(duration).sum(axis: -1) / speed
        let durSafe = nanToNum(durRaw, nan: 1.0)
        let predDur = MLX.clip(MLX.round(durSafe), min: 1, max: Float(maxFramesPerPhoneme))
            .asType(.int32)[0]

        let durArray: [Int32] = predDur.asArray(Int32.self)
        var indices = [MLXArray]()
        for (i, n) in durArray.enumerated() {
            let count = min(max(Int(n), 0), maxFramesPerPhoneme)
            if count > 0 {
                indices.append(MLX.repeated(MLXArray(Int32(i)), count: count))
            }
        }

        guard !indices.isEmpty else {
            let silence = MLXArray.zeros([1, 1])
            return (silence, predDur)
        }
        let allIndices = MLX.concatenated(indices, axis: 0)

        let predAlnTrg = MLXArray.zeros([inputIds.shape[1], allIndices.shape[0]])
        predAlnTrg[allIndices, MLXArray(Array(0..<Int32(allIndices.shape[0])))] = MLXArray(Float(1))
        let predAln = predAlnTrg.expandedDimensions(axis: 0)

        let en = MLX.matmul(d.transposed(0, 2, 1), predAln)
        let (f0Pred, nPred) = predictor.predict(en, globalStyle)

        let tEn = textEncoder(inputIds, inputLengths: inputLengths, mask: textMask)
        let asr = MLX.matmul(tEn, predAln)

        let audio = decoder(asr, f0Pred, nPred, acousticStyle)
        return (audio[0], predDur)
    }

    // MARK: - Chargement

    /// `config.json` et les poids `*.safetensors` du dossier (les voix, dans
    /// `voices/`, n'en font pas partie).
    public static func load(directory: URL) throws -> KokoroModel {
        let configData = try Data(contentsOf: directory.appendingPathComponent("config.json"))
        let config = try JSONDecoder().decode(KokoroConfig.self, from: configData)
        let model = KokoroModel(config: config)
        let weights = try loadWeights(modelDir: directory)
        let sanitized = model.sanitize(weights: weights)
        try model.update(parameters: ModuleParameters.unflattened(sanitized), verify: .noUnusedKeys)
        model.train(false)
        MLX.eval(model.parameters())
        return model
    }

    func sanitize(weights: [String: MLXArray]) -> [String: MLXArray] {
        let hasPackedQuantizedWeights = weights.keys.contains {
            $0.hasSuffix(".scales") || $0.hasSuffix(".biases")
        }
        let needsConvTranspose = !hasPackedQuantizedWeights

        var result = [String: MLXArray]()
        for (k, v) in weights {
            if k.contains("position_ids") { continue }

            var nk = k

            // LSTM : l'inverse d'abord, pour ne pas toucher aux clés avant.
            nk = nk.replacingOccurrences(of: ".weight_ih_l0_reverse", with: ".Wx_backward")
            nk = nk.replacingOccurrences(of: ".weight_hh_l0_reverse", with: ".Wh_backward")
            nk = nk.replacingOccurrences(of: ".bias_ih_l0_reverse", with: ".bias_ih_backward")
            nk = nk.replacingOccurrences(of: ".bias_hh_l0_reverse", with: ".bias_hh_backward")
            nk = nk.replacingOccurrences(of: ".weight_ih_l0", with: ".Wx_forward")
            nk = nk.replacingOccurrences(of: ".weight_hh_l0", with: ".Wh_forward")
            nk = nk.replacingOccurrences(of: ".bias_ih_l0", with: ".bias_ih_forward")
            nk = nk.replacingOccurrences(of: ".bias_hh_l0", with: ".bias_hh_forward")

            nk = nk.replacingOccurrences(of: ".gamma", with: ".weight")
            nk = nk.replacingOccurrences(of: ".beta", with: ".bias")
            nk = nk.replacingOccurrences(of: ".alpha1.", with: ".alpha1_")
            nk = nk.replacingOccurrences(of: ".alpha2.", with: ".alpha2_")

            var value = v
            if needsConvTranspose {
                if (nk.contains("F0_proj.weight") || nk.contains("N_proj.weight")) && v.ndim == 3 {
                    value = v.transposed(0, 2, 1)
                } else if nk.contains("noise_convs") && nk.hasSuffix(".weight") && v.ndim == 3 {
                    value = v.transposed(0, 2, 1)
                } else if nk.hasSuffix("weight_v") && v.ndim == 3 {
                    let (o, h, w) = (v.shape[0], v.shape[1], v.shape[2])
                    if !(o >= h && o >= w && h == w) {
                        value = v.transposed(0, 2, 1)
                    }
                }
            }

            result[nk] = value
        }
        return result
    }

    private static func loadWeights(modelDir: URL) throws -> [String: MLXArray] {
        let url = modelDir.appendingPathComponent("model.safetensors")
        if FileManager.default.fileExists(atPath: url.path) {
            return try MLX.loadArrays(url: url)
        }
        var allWeights = [String: MLXArray]()
        let files = try FileManager.default.contentsOfDirectory(at: modelDir, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "safetensors" && !$0.lastPathComponent.contains("voices") }
        for file in files {
            for (k, v) in try MLX.loadArrays(url: file) {
                allWeights[k] = v
            }
        }
        return allWeights
    }
}
```

Créer `macos/Vendor/KokoroMLX/KokoroRunner.swift` (ajout de Sillon, seule
porte d'entrée de l'app dans le framework) :

```swift
import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon dans ce framework : MLX ne se lie qu'ici, une seule
/// copie dans l'app. Ajouté par Sillon, hors des fichiers repris.
public final class KokoroRunner: @unchecked Sendable {
    private let model: KokoroModel

    /// Charge le modèle sous un plafond mémoire et un cache réduit.
    public static func load(directory: URL, memoryLimit: Int, cacheLimit: Int) throws -> KokoroRunner {
        GPU.set(memoryLimit: memoryLimit)
        GPU.set(cacheLimit: cacheLimit)
        return KokoroRunner(model: try KokoroModel.load(directory: directory))
    }

    private init(model: KokoroModel) {
        self.model = model
    }

    /// Un passage avant à vitesse 1 : l'audio à 24 kHz et une durée par jeton.
    public func run(ids: [Int32], style: [Float]) -> (samples: [Float], durations: [Int32]) {
        let input = MLXArray(ids).reshaped([1, -1])
        let refS = MLXArray(style, [1, style.count])
        let (audio, durations) = model(inputIds: input, refS: refS, speed: 1)
        eval(audio, durations)
        return (audio.reshaped([-1]).asType(.float32).asArray(Float.self),
                durations.reshaped([-1]).asType(.int32).asArray(Int32.self))
    }

    /// Vide le cache de MLX : la voix part au repos.
    public static func clearCache() {
        GPU.clearCache()
    }
}
```

Créer `macos/Vendor/KokoroMLX/ORIGINE.md` :

```markdown
# KokoroMLX

Fichiers Kokoro de [mlx-audio-swift](https://github.com/Blaizzy/mlx-audio-swift)
(licence MIT, Prince Canuma, voir `LICENSE`), commit `244458d` du 10 octobre 2026 :
`Kokoro/KokoroConfig.swift`, `KokoroModules.swift`, `KokoroDecoder.swift`,
`StyleTTS2/SharedConfigs.swift`, `Albert.swift`, `Blocks/*.swift`.

Changements : imports `MLXLMCommon`, `HuggingFace`, `MLXAudioCore` retirés ;
quantification retirée de `KokoroConfig` ; `import MLXFFT` ajouté au décodeur ;
`KokoroModel.swift` réduit au passage avant (`callAsFunction` public) et au
chargement d'un dossier. `KokoroRunner.swift` est un ajout de Sillon. Seule
dépendance : mlx-swift 0.32.3.
```

- [ ] **Étape 3 : le projet**

Dans `macos/project.yml` :

1. `packages` :

```yaml
packages:
  KeyboardShortcuts:
    url: https://github.com/sindresorhus/KeyboardShortcuts
    from: 2.2.0
  MLXSwift:
    url: https://github.com/ml-explore/mlx-swift
    exactVersion: 0.32.3
```

2. Une cible de plus, avant `Sillon:` :

```yaml
  KokoroMLX:
    type: framework
    platform: macOS
    sources: [Vendor/KokoroMLX]
    dependencies:
      - package: MLXSwift
        product: MLX
      - package: MLXSwift
        product: MLXNN
      - package: MLXSwift
        product: MLXFast
      - package: MLXSwift
        product: MLXFFT
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: fr.sillon.kokoromlx
        GENERATE_INFOPLIST_FILE: YES
        SWIFT_VERSION: "5.0"
        # Link.xcconfig lie le moteur Rust : rien à faire ici.
        OTHER_LDFLAGS: ""
```

3. Cible `Sillon`, `dependencies` : ajouter `- target: KokoroMLX` (embarqué).
**Pas** de `package: MLXSwift` ici : MLX se lie une seule fois, dans le
framework (deux copies feraient deux caches et deux plafonds mémoire) ; l'app
n'en voit que `KokoroRunner`. `postBuildScripts` : ajouter après le shim :

```yaml
      - name: Joindre espeak-ng (GPL-3.0, programme séparé)
        basedOnDependencyAnalysis: false
        script: |
          ESPEAK="$SRCROOT/../target/espeak"
          if [ -x "$ESPEAK/bin/espeak-ng" ]; then
            HELPERS="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/Helpers"
            RES="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH"
            mkdir -p "$HELPERS" "$RES/espeak-ng"
            cp "$ESPEAK/bin/espeak-ng" "$HELPERS/"
            rm -rf "$RES/espeak-ng-data"
            cp -R "$ESPEAK/share/espeak-ng-data" "$RES/"
            cp "$ESPEAK/COPYING" "$ESPEAK/SOURCE.txt" "$RES/espeak-ng/"
            codesign --force --sign - "$HELPERS/espeak-ng"
          else
            echo "warning: espeak-ng absent (./scripts/build-espeak.sh) : Kokoro se repliera"
          fi
```

- [ ] **Étape 4 : écrire les tests de la préparation**

Créer `macos/Tests/KokoroWarmUpTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class KokoroWarmUpTests: XCTestCase {
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.kokoro.preparation"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testTheMetalKernelsArePreparedOncePerVersion() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults)?.value
        XCTAssertEqual(compute.loads, 1)
        XCTAssertEqual(compute.warmUps, 1)
        XCTAssertEqual(compute.releases, 1, "le modèle est libéré tout de suite")
        XCTAssertFalse(KokoroWarmUp.isRunning)
        XCTAssertNil(KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults))
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "2", defaults: defaults)?.value
        XCTAssertEqual(compute.warmUps, 2, "nouvelle version : nouvelle préparation")
    }

    func testAFailedPreparationIsTriedAgainAtTheNextLaunch() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        compute.failLoad = true
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults)?.value
        XCTAssertNil(defaults.string(forKey: KokoroWarmUp.versionKey))
        XCTAssertNotNil(KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults))
    }

    func testOnlyTheMLXPackageIsPrepared() {
        var settings = SettingsView.defaults
        settings.family = "kokoro"
        settings.modelPath = "/m/kokoro/kokoro-v1_0.safetensors"
        XCTAssertEqual(KokoroWarmUp.modelDirectory(settings: settings), "/m/kokoro")
        settings.modelPath = "/m/kokoro/model_quantized.onnx"
        XCTAssertNil(KokoroWarmUp.modelDirectory(settings: settings))
        settings.family = "supertonic"
        settings.modelPath = "/m/supertonic/supertonic-3-f16.gguf"
        XCTAssertNil(KokoroWarmUp.modelDirectory(settings: settings))
    }

    func testEspeakIsLookedForInsideTheApp() {
        AppDelegate.exportEspeakLocation(bundle: .main)
        let helper = ProcessInfo.processInfo.environment["SILLON_ESPEAK"] ?? ""
        XCTAssertTrue(helper.hasSuffix("Contents/Helpers/espeak-ng"), helper)
        XCTAssertNotNil(ProcessInfo.processInfo.environment["SILLON_ESPEAK_DATA"])
    }
}
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/KokoroWarmUpTests`
Expected: échec de compilation — `KokoroWarmUp`, `exportEspeakLocation` introuvables.

- [ ] **Étape 5 : le calcul MLX et la préparation**

Créer `macos/Sources/Voice/KokoroMLX.swift` :

```swift
import Foundation
import KokoroMLX

/// Le calcul de Kokoro en MLX, appelé par le moteur Rust depuis son fil de
/// synthèse. Une file série à soi, jamais le fil principal. Le modèle est
/// partagé (préparation au lancement, lecteur) : il n'est libéré qu'au
/// dernier `release()`.
final class KokoroMLXCompute: KokoroCompute, @unchecked Sendable {
    private let queue = DispatchQueue(label: "fr.sillon.kokoro", qos: .userInitiated)
    private var runner: KokoroRunner?
    private var users = 0
    private var synthesized = 0

    /// Synthèses faites depuis le lancement (la mesure vérifie qu'il n'y en
    /// a aucune en pause).
    var calls: Int { queue.sync { synthesized } }

    func load(modelDir: String) throws {
        try queue.sync {
            users += 1
            if runner != nil { return }
            do {
                // Plafond mesuré au spike : 1 Go, cache réduit à 64 Mo.
                runner = try KokoroRunner.load(directory: URL(fileURLWithPath: modelDir),
                                               memoryLimit: 1 << 30, cacheLimit: 64 << 20)
            } catch {
                users -= 1
                throw SillonError.Failed(message: "Calcul Metal indisponible : \(error.localizedDescription)")
            }
        }
    }

    /// Une courte synthèse : « bala. » (b a l a .), style nul.
    func warmUp() throws {
        _ = try synthesize(ids: [0, 44, 43, 54, 43, 4, 0], style: [Float](repeating: 0, count: 256))
    }

    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        try queue.sync {
            guard let runner else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
            synthesized += 1
            let (samples, durations) = runner.run(ids: ids.map { Int32($0) }, style: style)
            return KokoroOutput(samples: samples, durations: durations.map { UInt32(max(0, $0)) })
        }
    }

    func release() {
        queue.sync {
            users = max(0, users - 1)
            guard users == 0, runner != nil else { return }
            runner = nil
            KokoroRunner.clearCache()
        }
    }
}
```

Créer `macos/Sources/Voice/KokoroWarmUp.swift` :

```swift
import Foundation

/// La première synthèse après une installation ou une mise à jour compile les
/// noyaux Metal (≈ 9 s), gardés ensuite dans le cache du système. L'app la
/// fait au lancement, à basse priorité, une fois par version, et libère le
/// modèle aussitôt.
@MainActor
enum KokoroWarmUp {
    static let versionKey = "kokoroPreparedVersion"
    /// Vrai pendant la préparation : l'onglet Voix dit « Voix en préparation
    /// (première fois)… ».
    private(set) static var isRunning = false

    /// Le dossier du paquet MLX actif ; aucun pour un autre modèle.
    static func modelDirectory(settings: SettingsView) -> String? {
        guard settings.family == "kokoro", settings.modelPath.hasSuffix(".safetensors") else { return nil }
        return URL(fileURLWithPath: settings.modelPath).deletingLastPathComponent().path
    }

    @discardableResult
    static func runIfNeeded(compute: KokoroCompute, modelDirectory: String?, version: String,
                            defaults: UserDefaults = .standard) -> Task<Void, Never>? {
        guard let directory = modelDirectory, !isRunning,
              defaults.string(forKey: versionKey) != version else { return nil }
        isRunning = true
        let work = Task.detached(priority: .background) { () -> Bool in
            do {
                try compute.load(modelDir: directory)
                defer { compute.release() }
                try compute.warmUp()
                return true
            } catch {
                return false
            }
        }
        return Task { @MainActor in
            let prepared = await work.value
            isRunning = false
            if prepared { defaults.set(version, forKey: versionKey) }
        }
    }
}
```

- [ ] **Étape 6 : l'app lance le lecteur avec Kokoro**

Dans `macos/Sources/AppDelegate.swift` :

1. Après `private(set) var reader: SillonReader!` :

```swift
    /// Le calcul MLX de Kokoro ; jamais avec le lecteur muet des tests.
    private(set) var kokoroCompute: KokoroMLXCompute?
```

2. Remplacer `reader = readerIsSilent ? SillonReader.silent() : SillonReader()` par :

```swift
        if readerIsSilent {
            reader = SillonReader.silent()
        } else {
            Self.exportEspeakLocation()
            let compute = KokoroMLXCompute()
            kokoroCompute = compute
            reader = SillonReader.withKokoro(compute: compute)
        }
```

3. Après `Onboarding.runIfNeeded()` (fin de `applicationDidFinishLaunching`) :

```swift
        startKokoro()
```

4. Ajouter :

```swift
    /// espeak-ng livré dans l'app : le moteur le trouve par ces variables,
    /// à poser avant la première voix.
    nonisolated static func exportEspeakLocation(bundle: Bundle = .main) {
        let helper = bundle.bundleURL.appendingPathComponent("Contents/Helpers/espeak-ng")
        setenv("SILLON_ESPEAK", helper.path, 1)
        if let resources = bundle.resourceURL {
            setenv("SILLON_ESPEAK_DATA", resources.path, 1)
        }
    }

    /// Le passage à Kokoro (une fois, avec un avis), puis la préparation des
    /// noyaux Metal (une fois par version).
    private func startKokoro() {
        if let notice = model.migrateVoice() {
            model.message = notice
            Announcement.post(notice)
        }
        guard let compute = kokoroCompute else { return }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "0"
        Task { @MainActor [model] in
            guard let model else { return }
            let settings = await model.loadSettings()
            KokoroWarmUp.runIfNeeded(compute: compute,
                                     modelDirectory: KokoroWarmUp.modelDirectory(settings: settings),
                                     version: version)
        }
    }
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **` (les nouveaux tests compris). Le framework
`KokoroMLX` et mlx-swift se compilent ; aucun test ne crée `KokoroMLXCompute`.

- [ ] **Étape 7 : construction de l'app, licences, taille**

Dans `scripts/build-macos.sh`, avant `./scripts/build-ffi.sh` :

```bash
# mlx-swift compile ses noyaux Metal : il faut le composant Metal Toolchain.
if ! xcrun -sdk macosx metal -v >/dev/null 2>&1; then
    echo "Erreur : Metal Toolchain absent — xcodebuild -downloadComponent MetalToolchain" >&2
    exit 1
fi
# espeak-ng (GPL-3.0) : programme séparé, joint à l'app par le projet.
[ -x target/espeak/bin/espeak-ng ] || ./scripts/build-espeak.sh
```

et à la fin, avant `echo "Prête : …"` :

```bash
APP_BUILT=macos/build/Release/Sillon.app
if [ ! -x "$APP_BUILT/Contents/Helpers/espeak-ng" ]; then
    echo "Erreur : espeak-ng manque dans l'app (Contents/Helpers)" >&2
    exit 1
fi
printf 'Bonjour.\n' | "$APP_BUILT/Contents/Helpers/espeak-ng" -q --ipa --tie -v fr-fr --path="$APP_BUILT/Contents/Resources"
echo "--- taille de l'app ---"
du -sh "$APP_BUILT"
find "$APP_BUILT" -name '*.metallib' -exec du -h {} +
```

Dans `NOTICE`, ajouter :

```text
La voix Kokoro-82M (hexgrad, poids et voix sous licence Apache 2.0) est
téléchargée à la demande depuis mlx-community/Kokoro-82M-bf16 (révision
a71e4d38b236d968966a2002c4c895dbd12b1c3c) ou
onnx-community/Kokoro-82M-v1.0-ONNX-timestamped (révision
dd4401a9add81ac692d20e240d22ec9dda82cc29). La voix p-fenrir60, livrée avec
Sillon (assets/voices), en est dérivée (Apache 2.0).

L'app macOS contient des fichiers de mlx-audio-swift (macos/Vendor/KokoroMLX,
licence MIT, Prince Canuma) et utilise mlx-swift (licence MIT, Apple).

L'app macOS contient espeak-ng 1.52.0 (Contents/Helpers/espeak-ng et ses
données), sous licence GNU GPL version 3. C'est un programme séparé, que
Sillon lance et auquel il parle par ses entrées et sorties : Sillon ne s'y
lie pas. Le texte de la licence est livré dans
Contents/Resources/espeak-ng/COPYING, et les sources exactes du binaire sont
celles de https://github.com/espeak-ng/espeak-ng/archive/refs/tags/1.52.0.tar.gz
(Contents/Resources/espeak-ng/SOURCE.txt), construites par
scripts/build-espeak.sh.

Au terminal, la feature kokoro-onnx charge ONNX Runtime (licence MIT,
Microsoft), livré à côté du binaire sous Linux par scripts/package.sh.
```

Run: `./scripts/build-macos.sh`
Expected: « Prête : macos/build/Release/Sillon.app », la ligne de phonèmes
d'espeak-ng embarqué, la taille de l'app et du metallib. **Noter ces deux
tailles dans le compte rendu de la tâche** (question ouverte de la spec : le
metallib mesuré au spike pesait 190 Mo).

- [ ] **Étape 8 : commit**

```bash
git add macos/Vendor/KokoroMLX macos/Sources/Voice/KokoroMLX.swift macos/Sources/Voice/KokoroWarmUp.swift macos/Tests/KokoroWarmUpTests.swift macos/project.yml macos/Sources/AppDelegate.swift scripts/build-macos.sh NOTICE
git commit -m "$(cat <<'EOF'
feat(macos): Kokoro en MLX (fichiers de mlx-audio-swift), espeak-ng embarqué, noyaux Metal préparés une fois par version

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---
### Tâche 14 : l'onglet Voix pour Kokoro (et la vitesse pour toutes les voix)

Première tâche qui touche l'onglet Voix : elle relit et commite d'abord les
quatre fichiers restés hors index au départ (la vitesse affichée pour toutes
les familles, puisque Sonic l'applique au son), puis ajoute ce que Kokoro
demande : « Voix française » et « Voix anglaise », chacune avec **Écouter**
dans sa langue ; « Langue de ce document » (Automatique avec la langue
détectée, Français, Anglais) ; la langue par défaut (Automatique, ou une
langue) ; la mention « Document mêlé » ; l'avis du passage à Kokoro ; « Voix
en préparation (première fois)… » pendant la préparation Metal. Hauteur,
énergie, intonation et échantillonnage restent masqués pour Kokoro (réglages
ignorés). L'onglet Modèles montre voix, licences et « Recommandé ».

**Fichiers :**
- Relire puis commiter (tels quels si la relecture est bonne) : `macos/Sources/Settings/SettingKey.swift`, `macos/Sources/Settings/VoiceTab.swift`, `macos/Tests/SettingsDiffTests.swift`, `macos/Tests/SettingsStoreTests.swift`
- Modifier : `macos/Sources/Settings/VoiceTab.swift`, `macos/Sources/Settings/SettingsStore.swift`, `macos/Sources/Settings/ModelsTab.swift`
- Modifier : `macos/Tests/VoiceTabTests.swift`, `macos/Tests/SettingsStoreTests.swift`, `macos/Tests/SettingsDiffTests.swift`, `macos/Tests/FakeSettingsBackend.swift`
- Créer : `macos/UITests/KokoroVoiceUITests.swift`

**Interfaces :**
- Consomme : `LanguageVoice`, `DocumentLanguageView`, `Snapshot.documentLanguage`, `Snapshot.notice`, `VoiceChoice.language`, `VoiceModel.voicesLabel/licenses`, `ReaderModel.setDocumentLanguage(_:)`, `previewVoice(in:)`, `dismissNotice()` (Tâche 12) ; `KokoroWarmUp.isRunning` (Tâche 13).
- Produit :
  - `SettingsBackend.previewVoice(in language: String) async throws`
  - `SettingsStore.setVoice(_ voice: String, language: String)` ; `SettingsStore.listen(language: String) async`
  - `VoiceTab.followsLanguage(_:) -> Bool`, `voices(_:language:) -> [VoiceChoice]`, `voice(for:in:_:) -> String`, `documentLanguageOptions(_:) -> [(title: String, value: String)]`, `mixedLine(_:) -> String?`, `voiceLine(_:preparing:) -> String?` ; `languageOptions` commence par « Automatique »
  - identifiants : `voix-francaise`, `voix-anglaise`, `ecouter-fr`, `ecouter-en`, `langue-document`, `document-mele`, `avis-voix`, `recommande`

- [ ] **Étape 1 : relire et commiter la vitesse pour toutes les voix**

Run: `git diff -- macos/Sources/Settings/SettingKey.swift macos/Sources/Settings/VoiceTab.swift macos/Tests/SettingsDiffTests.swift macos/Tests/SettingsStoreTests.swift`

Relire : `SettingKey` n'a plus `speed` (la vitesse n'est plus un réglage
qu'un modèle peut ignorer) ; `VoiceTab` montre le curseur de vitesse sans
`IgnorableNote` ni `onEditingChanged` (Immediate : appliquée au son tout de
suite) ; les tests retirent `.speed` et ajoutent
`testTheSpeedIsNotAModelSetting`. Rien d'autre ne doit y figurer.

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/SettingsDiffTests -only-testing:SillonTests/SettingsStoreTests -only-testing:SillonTests/VoiceTabTests`
Expected: `** TEST SUCCEEDED **`.

```bash
git add macos/Sources/Settings/SettingKey.swift macos/Sources/Settings/VoiceTab.swift macos/Tests/SettingsDiffTests.swift macos/Tests/SettingsStoreTests.swift
git commit -m "$(cat <<'EOF'
fix(macos): la vitesse se règle pour toutes les voix (Sonic l'applique au son), plus jamais marquée ignorée

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Étape 2 : écrire les tests qui échouent**

Dans `macos/Tests/FakeSettingsBackend.swift`, `enum Capabilities`, ajouter :

```swift
    static let kokoro = ModelCapabilities(
        family: "kokoro",
        voices: [VoiceChoice(id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr"),
                 VoiceChoice(id: "ff_siwis", label: "ff_siwis · française", language: "fr"),
                 VoiceChoice(id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en"),
                 VoiceChoice(id: "af_sky", label: "af_sky · américaine, femme", language: "en")],
        languages: ["fr", "en"],
        honoured: ["voice", "language"],
        requiresReference: false)
```

et, dans `FakeSettingsBackend`, après `previewVoice()` :

```swift
    func previewVoice(in language: String) async throws {
        log.append("écoute \(language)")
        if let failPreview { throw SillonError.Failed(message: failPreview) }
    }
```

(et dans `GatedSettingsBackend` : `func previewVoice(in language: String) async throws {}`).

Dans `macos/Tests/VoiceTabTests.swift`, remplacer
`testLanguagesAreThoseOfTheModelPlusTheOneInPlace` par :

```swift
    func testLanguagesStartWithAutomaticThenTheModelsOnesPlusTheOneInPlace() {
        let options = VoiceTab.languageOptions(["fr", "en"], current: "ja")
        XCTAssertEqual(options.map(\.value), ["auto", "fr", "en", "ja"])
        XCTAssertEqual(options[0].title, "Automatique")
        XCTAssertEqual(options[1].title, "Français")
        XCTAssertTrue(options[3].title.hasSuffix("(hors liste)"))
        XCTAssertEqual(VoiceTab.languageOptions(["fr"], current: "fr").count, 2)
    }
```

et ajouter :

```swift
    func testKokoroOffersOneVoicePerLanguage() {
        let caps = Capabilities.kokoro
        XCTAssertTrue(VoiceTab.followsLanguage(caps))
        XCTAssertFalse(VoiceTab.followsLanguage(Capabilities.supertonic))
        XCTAssertEqual(VoiceTab.voices(caps, language: "fr").map(\.id), ["p-fenrir60", "ff_siwis"])
        XCTAssertEqual(VoiceTab.voices(caps, language: "en").map(\.id), ["af_bella", "af_sky"])
        var values = SettingsView.defaults
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_bella", "rien de choisi : la première")
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "af_sky")]
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_sky")
    }

    func testTheDocumentLanguageSaysWhatWasDetected() {
        let detected = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.title),
                       ["Automatique (français)", "Français", "Anglais"])
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.value), ["", "fr", "en"])
        let unknown = DocumentLanguageView(detected: nil, chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(unknown).first?.title, "Automatique (non détectée)")
    }

    func testAMixedDocumentIsSaidWithItsMajority() {
        let mixed = DocumentLanguageView(detected: "fr", chosen: "en", effective: "en", minorityShare: 0.38, mixed: true)
        XCTAssertEqual(VoiceTab.mixedLine(mixed), "Document mêlé : 62 % en français")
        let plain = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0.1, mixed: false)
        XCTAssertNil(VoiceTab.mixedLine(plain))
    }

    func testTheFirstPreparationIsSaid() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: true), "Voix en préparation (première fois)…")
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: false), "Voix en chargement…")
    }
```

Dans `macos/Tests/SettingsStoreTests.swift`, ajouter :

```swift
    func testAVoicePerLanguageIsWrittenAlone() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.setVoice("af_sky", language: "en")
        await store.flush()
        XCTAssertEqual(backend.patches.last?.voiceForLanguage, LanguageVoice(language: "en", voice: "af_sky"))
        XCTAssertNil(backend.patches.last?.voice, "la voix des autres modèles ne bouge pas")
        XCTAssertEqual(store.values.voicesByLanguage, [LanguageVoice(language: "en", voice: "af_sky")])
    }

    func testListeningInALanguageWritesFirstThenSpeaksIt() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.setVoice("ff_siwis", language: "fr")
        await store.listen(language: "fr")
        XCTAssertEqual(backend.log, ["patch", "écoute fr"])
    }
```

Dans `macos/Tests/SettingsDiffTests.swift`, ajouter :

```swift
    func testAVoicePerLanguageBecomesItsOwnPatch() {
        let old = SettingsView.defaults
        var new = old
        new.voicesByLanguage = [LanguageVoice(language: "fr", voice: "ff_siwis")]
        let patch = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(patch.voiceForLanguage, LanguageVoice(language: "fr", voice: "ff_siwis"))
        XCTAssertNil(patch.voice)
        XCTAssertTrue(SettingsDiff.patch(from: new, to: new).isEmpty)
    }
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/VoiceTabTests -only-testing:SillonTests/SettingsStoreTests -only-testing:SillonTests/SettingsDiffTests`
Expected: échec de compilation — `followsLanguage`, `setVoice`, `listen(language:)`, `previewVoice(in:)` du protocole introuvables.

- [ ] **Étape 3 : le magasin**

Dans `macos/Sources/Settings/SettingsStore.swift`, le protocole :

```swift
@MainActor
protocol SettingsBackend: AnyObject {
    func loadSettings() async -> SettingsView
    func loadCapabilities() async -> ModelCapabilities
    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView
    func previewVoice() async throws
    /// La phrase d'exemple dans une langue (Kokoro : une voix par langue).
    func previewVoice(in language: String) async throws
    func stopPreview() async
}
```

et, après `listen()` :

```swift
    /// La voix d'une langue (Kokoro) : l'écran suit, l'écriture après la pause.
    func setVoice(_ voice: String, language: String) {
        var voices = values.voicesByLanguage.filter { $0.language != language }
        voices.append(LanguageVoice(language: language, voice: voice))
        set(\.voicesByLanguage, voices)
    }

    /// La phrase d'exemple dans `language`, avec ce qui vient d'être choisi.
    func listen(language: String) async {
        previewError.clear()
        await committer.flush()
        do { try await backend.previewVoice(in: language) } catch { previewError.fail(Self.message(of: error)) }
    }
```

- [ ] **Étape 4 : l'onglet Voix**

Dans `macos/Sources/Settings/VoiceTab.swift` :

1. Section « Modèle » : après `VoiceStateLine(state: model.snapshot.voice)` :

```swift
                if let notice = model.snapshot.notice {
                    HStack {
                        Text(notice).accessibilityIdentifier("avis-voix")
                        Button("Compris") { model.dismissNotice() }
                    }
                }
```

2. Section « Voix » : remplacer son contenu, de `voicePicker` jusqu'au bloc
`if store.isShown(.language) { … }` inclus, par :

```swift
                if Self.followsLanguage(store.capabilities) {
                    languageVoicePicker("fr", title: "Voix française", identifier: "voix-francaise")
                    languageVoicePicker("en", title: "Voix anglaise", identifier: "voix-anglaise")
                } else {
                    voicePicker
                }
                HStack {
                    if !Self.followsLanguage(store.capabilities) {
                        Button("Écouter") { Task { await store.listen() } }
                            .accessibilityIdentifier("ecouter")
                    }
                    Button("Arrêter") { Task { await store.stopListening() } }
                        .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                    if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                        Text(line).accessibilityIdentifier("etat-ecoute")
                    }
                }
                documentLanguage
                if store.isShown(.language) {
                    IgnorableNote(ignored: store.isIgnored(.language)) {
                        let title = Self.followsLanguage(store.capabilities) ? "Langue par défaut" : "Langue"
                        LabeledContent(title) {
                            PopUpPicker(label: title, identifier: "langue",
                                        options: Self.languageOptions(store.capabilities.languages,
                                                                      current: store.values.language),
                                        selection: store.binding(\.language))
                                .fixedSize()
                        }
                    }
                }
```

(les curseurs de vitesse et de volume, venus de l'étape 1, suivent sans changement).

3. Après `voicePicker`, ajouter :

```swift
    /// Une voix par langue (Kokoro), chacune avec son « Écouter ».
    @ViewBuilder private func languageVoicePicker(_ language: String, title: String, identifier: String) -> some View {
        LabeledContent(title) {
            HStack {
                PopUpPicker(label: title, identifier: identifier,
                            options: Self.voices(store.capabilities, language: language).map { (title: $0.label, value: $0.id) },
                            selection: Binding(get: { Self.voice(for: language, in: store.values, store.capabilities) },
                                               set: { store.setVoice($0, language: language) }))
                    .fixedSize()
                Button("Écouter") { Task { await store.listen(language: language) } }
                    .accessibilityLabel("Écouter la \(title.lowercased())")
                    .accessibilityIdentifier("ecouter-\(language)")
            }
        }
    }

    /// La langue du document ouvert, et la mention d'un document mêlé.
    @ViewBuilder private var documentLanguage: some View {
        if let language = model.snapshot.documentLanguage {
            LabeledContent("Langue de ce document") {
                PopUpPicker(label: "Langue de ce document", identifier: "langue-document",
                            options: Self.documentLanguageOptions(language),
                            selection: Binding(get: { language.chosen ?? "" }, set: { value in
                                Task {
                                    do {
                                        try await model.setDocumentLanguage(value.isEmpty ? nil : value)
                                        modelError.clear()
                                    } catch {
                                        modelError.fail(SettingsStore.message(of: error))
                                    }
                                }
                            }))
                    .fixedSize()
            }
            if let line = Self.mixedLine(language) {
                Text(line).accessibilityIdentifier("document-mele")
            }
        }
    }
```

4. Remplacer `languageOptions` et `voiceLine`, et ajouter les fonctions de
présentation, dans la partie `static` :

```swift
    /// « Automatique » d'abord, puis les langues du modèle et celle en place.
    static func languageOptions(_ codes: [String], current: String) -> [(title: String, value: String)] {
        let options = [(title: "Automatique", value: "auto")] + codes.map { (title: LanguageName.display($0), value: $0) }
        return Choices.including(options, current: current) { "\(LanguageName.display($0)) (hors liste)" }
    }

    /// Kokoro choisit sa voix par langue.
    static func followsLanguage(_ c: ModelCapabilities) -> Bool {
        c.family == "kokoro"
    }

    static func voices(_ c: ModelCapabilities, language: String) -> [VoiceChoice] {
        c.voices.filter { $0.language == language }
    }

    /// La voix en place pour une langue ; rien de choisi : la première.
    static func voice(for language: String, in values: SettingsView, _ c: ModelCapabilities) -> String {
        values.voicesByLanguage.first(where: { $0.language == language })?.voice
            ?? voices(c, language: language).first?.id ?? ""
    }

    static func documentLanguageOptions(_ d: DocumentLanguageView) -> [(title: String, value: String)] {
        let detected = d.detected.map { LanguageName.display($0).lowercased() } ?? "non détectée"
        return [(title: "Automatique (\(detected))", value: ""), (title: "Français", value: "fr"),
                (title: "Anglais", value: "en")]
    }

    /// « Document mêlé : 62 % en français » : la part de la langue
    /// majoritaire, au-delà de 30 % dans l'autre.
    static func mixedLine(_ d: DocumentLanguageView) -> String? {
        guard d.mixed else { return nil }
        let majority = Int(((1 - d.minorityShare) * 100).rounded())
        return "Document mêlé : \(majority) % en \(LanguageName.display(d.detected ?? d.effective).lowercased())"
    }

    /// L'état de la voix, une seule ligne : en chargement (ou en préparation
    /// des noyaux Metal, la première fois), ou la voix de repli avec la
    /// raison. Les voix listées sont alors celles du modèle configuré, pas
    /// celle qu'on entend.
    static func voiceLine(_ state: VoiceState, preparing: Bool = false) -> String? {
        switch state {
        case .loading: preparing ? "Voix en préparation (première fois)…" : "Voix en chargement…"
        case .fallback(let reason): "Voix du système en repli : \(reason)"
        default: nil
        }
    }
```

5. `VoiceStateLine` : `if let line = VoiceTab.voiceLine(state, preparing: KokoroWarmUp.isRunning) {`.

Dans `macos/Sources/Settings/ModelsTab.swift`, `row` : après `Text(m.summary)` :

```swift
                if m.recommended {
                    Text("Recommandé").fontWeight(.semibold).accessibilityIdentifier("recommande")
                }
                if !m.voicesLabel.isEmpty { Text(m.voicesLabel) }
```

et après `Text(m.pace)` :

```swift
                if !m.licenses.isEmpty { Text(m.licenses) }
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : le test d'interface (compilé seulement)**

Créer `macos/UITests/KokoroVoiceUITests.swift` :

```swift
import XCTest

/// La langue du document dans l'onglet Voix, et son audit. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/KokoroVoiceUITests`.
/// Le lecteur des tests est muet (pas de Kokoro) : la langue du document,
/// elle, vaut pour toutes les voix.
final class KokoroVoiceUITests: XCTestCase {
    func testTheDocumentLanguageIsShownAndChangeable() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.toolbars.buttons["Voix"].click()
        let picker = window.popUpButtons["langue-document"]
        XCTAssertTrue(picker.waitForExistence(timeout: 3), "langue du document absente")
        XCTAssertEqual(picker.value as? String, "Automatique (français)")
        picker.click()
        app.menuItems["Anglais"].click()
        XCTAssertTrue(picker.waitForValue("Anglais"))
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}

private extension XCUIElement {
    func waitForValue(_ expected: String, timeout: TimeInterval = 3) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if value as? String == expected { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        return false
    }
}
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build-for-testing`
Expected: `** TEST BUILD SUCCEEDED **`. Ne pas l'exécuter ici.

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/Settings/VoiceTab.swift macos/Sources/Settings/SettingsStore.swift macos/Sources/Settings/ModelsTab.swift macos/Tests/VoiceTabTests.swift macos/Tests/SettingsStoreTests.swift macos/Tests/SettingsDiffTests.swift macos/Tests/FakeSettingsBackend.swift macos/UITests/KokoroVoiceUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): onglet Voix Kokoro — une voix et un Écouter par langue, langue du document, document mêlé, avis du passage

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Étape 7 : exécuter les tests d'interface — seulement avec le feu vert de l'utilisateur**

Ne pas lancer sans accord. Demander d'abord : « Les tests d'interface prennent
la main sur le clavier et la souris pendant une à deux minutes. Je les
lance ? » Sans accord explicite, s'arrêter ici et le signaler.

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test-without-building -only-testing:SillonUITests/KokoroVoiceUITests -only-testing:SillonUITests/SettingsUITests`
Expected: `** TEST SUCCEEDED **`. Un échec d'audit se corrige dans `VoiceTab`
(libellé, valeur), dans un commit à part `fix(macos): …`.

---
### Tâche 15 : mesurer les critères de poids sur le M5, et écouter la coupe

Les critères d'acceptation de la spec se mesurent sur l'app construite (chemin
MLX), avec la méthode du banc : attente d'une machine calme, CPU de l'arbre de
processus, GPU `ioreg` moins le repos, empreinte physique. L'app joue un
scénario (`--measure-charge`) et annonce ses phases ; le pilote
`scripts/mesure-charge.py` (outils de `.superpowers/research/eval/light_run.py`)
échantillonne et rend les verdicts. **La mesure ne tourne que sur une machine
calme, avec l'accord de l'utilisateur** ; un seuil manqué se signale, il ne se
relâche pas.

**Fichiers :**
- Modifier : `macos/Sources/App/Measure.swift` (scénario de charge)
- Modifier : `macos/Sources/AppDelegate.swift` (`--measure-charge`)
- Créer : `scripts/mesure-charge.py`
- Modifier : `scripts/measure-macos.sh` (`--charge`)

**Interfaces :**
- Consomme : `SillonReader.measures()` (Tâche 12) ; `KokoroMLXCompute.calls` (Tâche 13) ; `light_run.wait_calm`, `TreeCPU`, `Sampler`, `phase_stats`, `cmd_duty` (`.superpowers/research/eval/light_run.py`, non versionné) ; l'exemple `kokoro_serve` (Tâche 10).
- Produit :
  - `Measure.charge(reader: SillonReader, compute: KokoroMLXCompute?) async` ; sortie : `PHASE <nom>`, `COMPTEUR busy_s=… stalls=… mlx_calls=…`, `MESURE <nom> <valeur> <unité>`
  - `scripts/mesure-charge.py app --app <Sillon.app> --out <json>` et `serve --serve "<commande>" --lang fr --out <json>`
  - `./scripts/measure-macos.sh --charge [--sans-construire]`

- [ ] **Étape 1 : le scénario de charge dans l'app**

Dans `macos/Sources/App/Measure.swift`, ajouter dans `enum Measure` :

```swift
    /// Un texte long : de quoi lire sept minutes sans atteindre la fin.
    static let longText = Array(repeating: """
    Le 14 juillet 1789, Camille Desmoulins harangua la foule devant le Palais-Royal, \
    et Paris bascula en quelques heures. Le rapport de la commission, long de 37 pages, \
    est arrivé un mardi ; personne n'a lu plus loin que le résumé. Pour corriger le bug, \
    lancez les tests, puis vérifiez que la configuration renvoie une erreur au lieu de planter. \
    Marguerite habitait depuis 41 ans la même maison étroite de la rue des Tanneurs, \
    et pendant tout ce temps, elle n'avait jamais fermé la porte du jardin à clé.
    """, count: 14).joined(separator: "\n\n")

    /// Le filtre de charge de la spec. Les phases s'annoncent sur la sortie
    /// standard (`PHASE nom`), avec les compteurs du moteur (`COMPTEUR …`) ;
    /// `scripts/mesure-charge.py` échantillonne CPU, GPU et mémoire.
    static func charge(reader: SillonReader, compute: KokoroMLXCompute?) async {
        do {
            try await mesurerCharge(reader: reader, compute: compute)
        } catch {
            // L'erreur a été écrite au point d'échec (`MESURE erreur …`).
        }
        phase("fin")
    }

    private static func phase(_ name: String) {
        print("PHASE \(name)")
        fflush(stdout)
    }

    private static func counters(_ reader: SillonReader, _ compute: KokoroMLXCompute?) {
        let m = reader.measures()
        print(String(format: "COMPTEUR busy_s=%.3f stalls=%d mlx_calls=%d", m.busySeconds, m.stalls, compute?.calls ?? -1))
        fflush(stdout)
    }

    private static func dormir(_ seconds: Int) async {
        try? await Task.sleep(for: .seconds(seconds))
    }

    private static func mesurerCharge(reader: SillonReader, compute: KokoroMLXCompute?) async throws {
        // Repos : l'app sans voix, puis le GPU de la machine au repos.
        reader.restVoice()
        phase("baseline")
        await dormir(20)
        print("MESURE empreinte_sans_voix \(residentMB()) Mo")
        fflush(stdout)

        // Voix chargée, texte long ouvert. La vitesse de l'utilisateur est
        // rendue avant la pause : la mesure ne doit rien laisser dans sa
        // configuration.
        let vitesse = reader.snapshot().speed
        reader.prepareVoice()
        reader.command(command: .setSpeed(speed: 1))
        let avant = reader.snapshot().revision
        reader.openText(markdown: longText, source: "charge")
        try await attendre(reader) { $0.revision != avant }
        if !reader.snapshot().playing { reader.command(command: .togglePlay) }
        try await attendreSon(reader)

        // Premier son à chaud après un saut loin : la phrase visée n'est pas
        // prête, elle part en tête et suite.
        for i in 1...5 {
            let cible = UInt32(20 * i)
            let t = ContinuousClock.now
            reader.command(command: .seekTo(sentence: cible))
            try await attendre(reader) { $0.sentence == cible && $0.audible }
            emit("saut_loin_premier_son", t.duration(to: .now))
        }
        reader.command(command: .seekTo(sentence: 0))
        try await attendreSon(reader)

        phase("advance")
        await dormir(10)

        phase("read_1x")
        counters(reader, compute)
        await dormir(120)
        counters(reader, compute)

        reader.command(command: .setSpeed(speed: 2))
        phase("read_2x")
        counters(reader, compute)
        await dormir(120)
        counters(reader, compute)
        reader.command(command: .setSpeed(speed: vitesse))

        // Pause : la fenêtre de synthèse en avance se remplit, puis plus rien.
        reader.command(command: .pause)
        phase("pause_settle")
        await dormir(5)
        phase("pause")
        counters(reader, compute)
        await dormir(20)
        counters(reader, compute)

        // Repos : modèle, cache MLX et espeak-ng libérés.
        phase("rest")
        reader.restVoice()
        try await attendre(reader) { $0.voice == .resting }
        await dormir(5)
        print("MESURE empreinte_repos \(residentMB()) Mo")
        fflush(stdout)
    }
```

(`ouvrirEtLire`, `attendre`, `attendreSon`, `emit` et `residentMB` existent
déjà dans ce fichier.)

Dans `macos/Sources/AppDelegate.swift`, juste avant le bloc
`if ProcessInfo.processInfo.arguments.contains("--measure"), !isUnitTesting {` :

```swift
        if ProcessInfo.processInfo.arguments.contains("--measure-charge"), !isUnitTesting {
            Task { @MainActor [reader = reader!, compute = kokoroCompute] in
                await Measure.charge(reader: reader, compute: compute)
                NSApp.terminate(nil)
            }
            return
        }
```

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **` (le scénario ne tourne pas sous les tests).

- [ ] **Étape 2 : le pilote**

Créer `scripts/mesure-charge.py` (exécutable) :

```python
#!/usr/bin/env python3
"""Filtre de charge de Sillon avec la méthode du banc : attente d'une machine
calme (load 1 min < 4), CPU de l'arbre de processus (proc_pid_rusage), GPU
`ioreg` moins le repos, empreinte physique. Les outils viennent de
.superpowers/research/eval/light_run.py (non versionné, présent sur la
machine de mesure). Rien n'est journalisé du texte lu.

  app   : Sillon.app --measure-charge (chemin MLX, critères de la spec)
  serve : l'exemple kokoro_serve (ORT, Linux) derrière light_run duty

Lancer avec l'interpréteur du banc (numpy, soundfile) :
  ~/Library/Caches/sillon-bench/light/venv-mlx/bin/python -I scripts/mesure-charge.py app \
      --app macos/build/Release/Sillon.app --out /tmp/sillon-charge.json
"""
import argparse, json, os, statistics, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", ".superpowers", "research", "eval"))
import light_run as LR  # noqa: E402

def cmd_app(a):
    waited, load1 = LR.wait_calm(a.calm, a.calm_max_s)
    print(json.dumps({"attente_calme_s": waited, "load1": load1}), flush=True)
    exe = os.path.join(a.app, "Contents", "MacOS", "Sillon")
    proc = subprocess.Popen([exe, "--measure-charge"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    smp = LR.Sampler(LR.TreeCPU(proc.pid))
    smp.start()
    counters, mesures = {}, []
    for line in proc.stdout:
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "PHASE":
            smp.phase = parts[1]
        elif parts[0] == "COMPTEUR":
            values = {k: float(v) for k, v in (p.split("=") for p in parts[1:])}
            values["t"] = time.time()
            counters.setdefault(smp.phase, []).append(values)
        elif parts[0] == "MESURE":
            mesures.append((parts[1], parts[2:]))
            print(line.strip(), flush=True)
    proc.wait()
    smp.stop = True

    base = LR.phase_stats(smp.rows, "baseline")
    phases = {}
    for name in ("read_1x", "read_2x", "pause", "rest"):
        st = LR.phase_stats(smp.rows, name)
        c = counters.get(name, [])
        if len(c) >= 2:
            wall = c[-1]["t"] - c[0]["t"]
            st["occupe"] = round((c[-1]["busy_s"] - c[0]["busy_s"]) / wall, 3) if wall > 0 else None
            st["coupures"] = int(c[-1]["stalls"] - c[0]["stalls"])
            st["appels_mlx"] = int(c[-1]["mlx_calls"] - c[0]["mlx_calls"])
        if st.get("gpu_sys_avg") is not None and base.get("gpu_sys_avg") is not None:
            st["gpu_sur_repos"] = round(st["gpu_sys_avg"] - base["gpu_sys_avg"], 1)
        phases[name] = st
    peak = max((r["fp_gb"] for r in smp.rows), default=None)
    sauts = [float(v[0]) for n, v in mesures if n == "saut_loin_premier_son"]
    sans_voix = next((float(v[0]) for n, v in mesures if n == "empreinte_sans_voix"), None)
    repos = next((float(v[0]) for n, v in mesures if n == "empreinte_repos"), None)
    verdicts = {
        "temps occupé à 1× ≤ 10 %": phases["read_1x"].get("occupe", 1.0) <= 0.10,
        "2× sans coupure": phases["read_2x"].get("coupures", 1) == 0,
        "empreinte ≤ 2 Go": peak is not None and peak <= 2.0,
        "premier son à chaud ≤ 0,3 s (médiane)": bool(sauts) and statistics.median(sauts) <= 300,
        "pause : aucun appel à MLX": phases["pause"].get("appels_mlx", 1) == 0,
        "pause : CPU nul (< 0,01 cœur)": (phases["pause"].get("cpu_cores_avg") or 1.0) < 0.01,
        "repos : empreinte de l'app sans voix ± 50 Mo": sans_voix is not None and repos is not None
        and abs(repos - sans_voix) <= 50,
    }
    result = {"baseline": base, "phases": phases, "empreinte_pic_go": peak, "sauts_ms": sauts,
              "empreinte_sans_voix_mo": sans_voix, "empreinte_repos_mo": repos, "verdicts": verdicts,
              "load1_depart": load1, "alerte_thermique": LR.therm_warn(), "echantillons": smp.rows}
    json.dump(result, open(a.out, "w"), indent=1, ensure_ascii=False)
    print(json.dumps({k: v for k, v in result.items() if k != "echantillons"}, ensure_ascii=False, indent=1))
    for name, ok in verdicts.items():
        print(f"{'OK ' if ok else 'NON'} {name}")
    return 0 if all(verdicts.values()) else 1

class SillonServe:
    """L'exemple kokoro_serve, vu comme un moteur de light_run."""
    def __init__(self, lang, cmd):
        import numpy as np
        self.np, self.sr, self.lang, self.desc = np, 24000, lang, "kokoro_serve (ORT int8)"
        self.p = subprocess.Popen(cmd, shell=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        json.loads(self.p.stdout.readline())
        self.tmp = tempfile.mktemp(suffix=".f32")
    def stream(self, text):
        self.p.stdin.write(json.dumps({"text": text, "lang": self.lang, "out": self.tmp}) + "\n")
        self.p.stdin.flush()
        json.loads(self.p.stdout.readline())
        yield self.np.fromfile(self.tmp, dtype=self.np.float32)
    def close(self):
        self.p.stdin.close()
        self.p.wait(10)

def cmd_serve(a):
    LR.load_backend = lambda args: (SillonServe(args.lang, a.serve), {})
    LR.cmd_duty(argparse.Namespace(backend="sillon_serve", opt=[], lang=a.lang, speeds="1,2", minutes=2,
                                   lookahead_s=10, idle_s=20, baseline_s=20, calm=a.calm,
                                   calm_max_s=a.calm_max_s, out=a.out))
    return 0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["app", "serve"])
    ap.add_argument("--app", default="macos/build/Release/Sillon.app")
    ap.add_argument("--serve", default="target/release/examples/kokoro_serve")
    ap.add_argument("--lang", default="fr")
    ap.add_argument("--calm", type=float, default=4.0)
    ap.add_argument("--calm-max-s", type=float, default=1800)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    sys.exit(cmd_app(a) if a.cmd == "app" else cmd_serve(a))

if __name__ == "__main__":
    main()
```

Dans `scripts/measure-macos.sh`, remplacer les lignes de
`if [ "${1:-}" != "--sans-construire" ]; then` à `fi` par :

```bash
CHARGE=0
CONSTRUIRE=1
for arg in "$@"; do
    case "$arg" in
    --charge) CHARGE=1 ;;
    --sans-construire) CONSTRUIRE=0 ;;
    esac
done
if [ "$CONSTRUIRE" = 1 ]; then
    ./scripts/build-macos.sh
fi
# Filtre de charge : le pilote du banc échantillonne l'app pendant son scénario.
if [ "$CHARGE" = 1 ]; then
    PY=${SILLON_BENCH_PY:-$HOME/Library/Caches/sillon-bench/light/venv-mlx/bin/python}
    exec "$PY" -I scripts/mesure-charge.py app --app "$APP" --out /tmp/sillon-charge.json
fi
```

Run: `chmod +x scripts/mesure-charge.py && python3 -m py_compile scripts/mesure-charge.py && bash -n scripts/measure-macos.sh`
Expected: aucune erreur.

- [ ] **Étape 3 : commit**

```bash
git add macos/Sources/App/Measure.swift macos/Sources/AppDelegate.swift scripts/mesure-charge.py scripts/measure-macos.sh
git commit -m "$(cat <<'EOF'
feat(mesure): filtre de charge de l'app (Kokoro MLX) avec la méthode du banc, verdicts des critères de la spec

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Étape 4 : mesurer — seulement avec l'accord de l'utilisateur, sur une machine calme**

Demander d'abord : « La mesure occupe la machine une dizaine de minutes et
fait parler l'app à voix haute (2 min à 1×, 2 min à 2×). Elle attend une
machine calme (load < 4, ni compilation ni modèle en cours). Je la lance ? »
Sans accord, s'arrêter et le signaler.

Préalables : l'app construite (`./scripts/build-macos.sh`) a été lancée une
fois normalement, a fait le passage à Kokoro, et Kokoro MLX est installé
(téléchargé **par l'app**). Puis :

Run:
```bash
source .superpowers/research/eval/calme.sh && calm kokoro-app
~/Library/Caches/sillon-bench/light/venv-mlx/bin/python -I .superpowers/research/eval/guard.py \
  --log .superpowers/research/eval/logs/guard.jsonl --tag kokoro-app -- \
  ./scripts/measure-macos.sh --charge --sans-construire
./scripts/measure-macos.sh --sans-construire
```
Expected: sept lignes `OK` (temps occupé à 1× ≤ 10 %, 2× sans coupure,
empreinte ≤ 2 Go, premier son ≤ 0,3 s, pause sans appel MLX ni CPU, repos
± 50 Mo), puis les médianes habituelles (`chaud_premier_son` ≤ 300 ms). Une
ligne `NON` : s'arrêter, garder `/tmp/sillon-charge.json`, et rapporter le
chiffre, la phase et les conditions (load, alerte thermique) sans toucher aux
seuils. Écrire le compte rendu dans
`.superpowers/research/2026-10-1x-mesure-kokoro-app.md` (non versionné) :
tableau des critères, valeurs mesurées et valeurs du spike (6,4 % ; 10,8 % ;
1,25–1,55 Go ; 0,11 s), taille de l'app et du metallib (Tâche 13).

Sous Linux (ORT, non bloquant), si une machine ou un conteneur ARM64 est
disponible et que l'utilisateur le veut : `cargo build --release --features
kokoro-onnx --example kokoro_serve`, puis
`python -I scripts/mesure-charge.py serve --serve "SILLON_KOKORO_DIR=… SILLON_ORT_LIB=… target/release/examples/kokoro_serve" --out /tmp/sillon-ort.json`
(spike : 9,7 % avec 4 fils, 1,78 Go à régler).

- [ ] **Étape 5 : écouter la coupe de la première phrase — avec l'utilisateur**

La coupe en tête et suite peut toucher l'intonation (spec, question ouverte).
Proposer à l'utilisateur, dans l'app : ouvrir le texte de mesure (copier
`Measure.longText`), laisser lire la 1re phrase en entier ; puis cliquer sur
une phrase lointaine qui commence par « Le 14 juillet 1789, » (non encore
synthétisée : elle part en deux morceaux) ; enfin y revenir après qu'elle a été
lue d'un trait (entière). Lui demander s'il entend une différence d'intonation
à la virgule. Si oui, le noter et lui demander de choisir : garder la coupe
(premier son plus rapide) ou la retirer (`splits_first_clause` à `false`).
Rapporter sa réponse ; ne rien changer sans elle.

---

## Couverture de la spec

| Exigence de la spec | Tâche |
|---|---|
| Voix `p-fenrir60` (recette ff_siwis / am_fenrir, livrée), `af_bella`, autres voix (ff_siwis, 28 anglaises, af_nicole chuchotée) | 1, 8 |
| Une voix par document, langue majoritaire en lettres, « Document mêlé » au-delà de 30 % | 1, 3, 14 |
| Détection `whatlang` (fr/en, ≥ 40 lettres, fiable, hors code), `voice.language = auto`, choix manuel par document retenu et suivi au renommage | 1, 3 |
| `[voice.per_language]`, autres familles sur `voice.voice` ; voix inconnue → défaut sans réécrire | 1 |
| Normalisation française pour un document français seulement, tous moteurs | 2 |
| Effets : voix de la langue → resynthèse ; autre langue → rien ; langue → remise en forme ; Kokoro/calcul/fils → rechargement ; vitesse immédiate | 3, (existant) |
| Contrat `synthesize_timed`, `splits_first_clause`, `Rendered.words`, minutage bout à bout | 4, 5 |
| Premier son en deux morceaux (≥ 3 mots, `, ; :`), seulement la phrase attendue, sans blanc | 5 |
| Surlignage par minutage, sinon poids ; temps de synthèse (1×) | 4 |
| `KokoroEngine` : vocabulaire 114 symboles, jeton 0 aux bords, ligne `len − 1`, coupe à 510, contrôle durées × 600 et NaN | 9 |
| `KokoroBackend` ; ORT `load-dynamic` ≥ 1.31 vérifié, `min(threads, 4)`, allocateur réglé | 9, 10 |
| espeak-ng en processus séparé, un par langue, sentinelle, traitement de misaki, une requête, ≤ 5 ms | 7 |
| Voix 510 × 256 safetensors ; `p-fenrir60` dans le binaire avec sa recette JSON | 8 |
| Alignement des mots (DP monotone, liaison au mot suivant, demi-espaces, bords, repli > 50 %) | 8 |
| `build_engine` : famille `kokoro`, MLX via l'app sinon ORT ; repli premier audio.cpp installé sinon système | 9 |
| Catalogue : `companions`, installé seulement complet, voix partagées, MLX seulement avec calcul, int8 seulement avec `kokoro-onnx`, fiche (langues, voix, rythme, licences) | 6, 14 |
| Kokoro voix par défaut, y compris configuration existante : passage unique, retenu, ancien modèle gardé, avis | 11, 13, 14 |
| FFI : `KokoroCompute` (interface étrangère), `with_kokoro`, `new()` inchangé, records (voix par langue, langue du document, fiche) | 12 |
| Swift : `Vendor/KokoroMLX` (MIT, 244458d, imports retirés, `callAsFunction` public, mlx-swift figé), `KokoroMLX.swift` (file série, plafond 1 Go, `release`) | 13 |
| `KokoroWarmUp` : au lancement, basse priorité, une fois par version ; « Voix en préparation (première fois)… » | 13, 14 |
| Construction : Metal Toolchain vérifié, espeak-ng dans `Contents/Helpers` avec données fr/en, licence et sources ; `SILLON_ESPEAK` ; recherche `SILLON_ESPEAK` → binaire → `PATH` | 7, 13 |
| Onglet Voix : voix française et anglaise avec Écouter, langue de ce document, langue par défaut, document mêlé, réglages ignorés masqués ; onglet Modèles : fiches Kokoro | 14 |
| Erreurs : Kokoro absent, espeak absent (Linux / app), espeak qui meurt (relance puis repli), MLX indisponible, ORT ancien, sortie invalide, > 510, langue non détectée, voix inconnue | 3, 7, 9, 10, 1 |
| En pause : aucun appel MLX ; au repos : modèle, cache et espeak-ng libérés | 9, 13, 15 |
| Critères d'acceptation (≤ 10 %, 2× sans coupure, ≤ 2 Go, ≤ 0,3 s, pause, repos) | 15 |
| Tests : faux moteurs, faux phonétiseur, protocole à sentinelle, langue, réglages, catalogue, alignement sur fixtures, parité et budget `#[ignore]`, recette `#[ignore]`, Swift (faux calcul, onglet, préparation) | 1–14 |
| Questions ouvertes : GPL (NOTICE, licence et sources livrées), taille de l'app (mesurée), coupe à l'écoute | 13, 15 |
| Hors périmètre respecté : Qwen3, autres langues de Kokoro, voix par bloc, ORT dans l'app, `mlx-rs`, Windows | — |

## Décisions prises là où la spec se tait

- **Valeurs figées pendant l'écriture du plan** : `onnx-community/Kokoro-82M-v1.0-ONNX-timestamped`
  révision `dd4401a9add81ac692d20e240d22ec9dda82cc29` ; tailles relevées sur
  le dépôt (327 115 152, 92 361 055, 2 351, 522 320 octets par voix) ; le
  `config.json` du dépôt MLX sert aux deux paquets (celui du dépôt ONNX n'a pas
  de vocabulaire).
- **Fiche Kokoro** : « Anglaise : 28 voix » (US et GB) ; la spec écrit
  « 29 voix », qui compte `ff_siwis` avec elles.
- **Choix du calcul par le fichier du paquet** : `.onnx` → ORT ; sinon MLX,
  qui exige le calcul de l'app et `engine.compute` différent de `cpu`. Dans
  l'app, `compute = cpu` avec Kokoro donne le repli et « Calcul Metal
  indisponible » (pas d'ORT dans l'app).
- **Coupe en tête et suite** : tête et suite sont synthétisées à partir des
  formes parlées des mots affichés, joints par des espaces (la phrase parlée
  entière n'est pas coupée au caractère près).
- **Langue au terminal** : un flux (`stdin`) prend la langue par défaut ; un
  fichier ou le presse-papiers sont détectés. Changer « Langue » à l'écran du
  terminal vaut à la prochaine ouverture (remise en forme).
- **Passage à Kokoro au terminal** : un Kokoro installé est adopté et retenu ;
  installable seulement, le terminal le signale (`s`) sans rien retenir.
- **Voix de l'autre langue** : l'effet est `Display` (rien n'est resynthétisé ni
  rechargé), la spec dit « aucun ».
- **MLX une seule fois dans l'app** : le framework `KokoroMLX` lie mlx-swift ;
  l'app n'y accède que par `KokoroRunner` (sinon deux copies de MLX, deux caches).
- **Préparation Metal** : la préparation et le lecteur partagent le même modèle
  (compteur d'utilisateurs) ; une préparation échouée est retentée au lancement
  suivant.
- **Mesure** : le premier son à chaud du critère est mesuré deux fois — ouverture
  d'un texte voix chaude (scénario existant) et saut lointain (tête et suite).
- **Metal Toolchain exigé** : la spec prévoyait d'embarquer le `mlx.metallib`
  figé faute de compilateur Metal ; mlx-swift compile ses `.metal` à la
  construction et échoue sans lui. Le plan exige donc le composant (Tâche 13,
  étape 1) et s'arrête pour le demander s'il manque.
- **Allocateur d'ORT** : motif mémoire coupé (`with_memory_pattern(false)`) ;
  l'arène reste celle par défaut, le réglage final se fixant à la mesure Linux
  (Tâche 15, non bloquante).
- **Parité anglaise** : misaki passe d'abord par un lexique anglais ; espeak-ng
  seul ne peut pas l'égaler. Le test borne la ressemblance (≥ 0,80) au lieu
  d'exiger l'égalité ; le français, lui, doit être identique.
