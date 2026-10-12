# La voix Kokoro-82M dans Sillon

Conception validée le 2026-10-11. Elle s'appuie sur trois mesures :
`.superpowers/research/eval/rapport.md` (§10 et §11),
`.superpowers/research/2026-10-11-spike-kokoro-integration.md` (le spike) et
`.superpowers/research/2026-10-09-vitesse-sonic-rapport.md` (la vitesse).

## Pourquoi

Les voix actuelles sont soit lourdes, soit plates. Qwen3-TTS 1.7B occupe la
machine environ 50 % du temps à 1×. MagpieTTS est plus lent que la parole.
Supertonic est rapide mais moins naturel. Kokoro-82M est **la seule voix légère
qui passe le filtre de charge** : 6,3 % du temps occupé à 1× sur le M5, 0,05 cœur,
1,25 à 1,55 Go, et 2× sans coupure. Il est aussi plus naturel que tous les autres
moteurs légers mesurés.

- **Français :** la voix maison `p-fenrir60` a un composite de 78,3, contre 73,0
  pour `ff_siwis` et 79,1 pour la référence Qwen3. Sa mélodie est dans la bande
  humaine (écart-type F0 3,5 demi-tons) et son accent est natif.
- **Anglais :** `af_bella` a un composite de 89,3, contre 92,1 pour Aiden (Qwen3).

## Ce qui existe

- **Moteurs** (`src/tts/`) : le contrat `TtsEngine` / `TtsSession` renvoie un
  `PcmBuffer` par phrase. `build_engine` choisit entre audio.cpp (Supertonic,
  MagpieTTS) et la voix du système, et `LazyEngine` charge en arrière-plan avec
  repli sur la voix du système. `Synthesizer` (`pool.rs`) synthétise en avance
  du curseur et rend des `Rendered { audio, speech }`.
- **Vitesse :** la synthèse se fait toujours à 1×. Sonic applique la vitesse au
  son pendant la lecture (`audio/tempo.rs`). Le chronomètre du lecteur compte en
  temps de synthèse (1×).
- **Surlignage :** il n'y a pas de minutage par mot. `Sentence::word_fraction`
  répartit la durée de la parole entre les mots selon leur nombre de lettres
  parlées (`Word::weight`).
- **Langue :** une seule valeur, `voice.language` (`fr` par défaut), pour tous
  les documents. Aucune détection. La normalisation (`src/text/` : dates,
  nombres, sigles) est écrite pour le français et s'applique à tout.
- **Catalogue** (`src/models.rs`) : un paquet est **un seul fichier** GGUF,
  filtré par familles compilées. Voix, langues et réglages honorés sont déclarés
  par famille.
- **Réglages** (`src/reader/settings.rs`) : `SettingsView`, `SettingsPatch`,
  `SettingsEffect` (de `Nothing` à `Reload`), `ModelCapabilities`.
- **FFI** (`ffi/src/lib.rs`, UniFFI 0.32) : `SillonReader::new()` construit le
  lecteur avec une fabrique de voix `Box<dyn Fn(&Config) -> Arc<dyn TtsEngine>>`.
  Le FFI n'a pas encore d'interface étrangère (trait implémenté côté Swift).
- **Repos de la voix :** `Reader::rest` abandonne le moteur après la durée
  « Garder la voix chargée » (`app.keep_voice_minutes`).
- **espeak-ng** sert déjà de voix du système sous Linux (`tts/system.rs`).

## Décisions

| Sujet | Décision |
|---|---|
| Voix française | `p-fenrir60` : timbre `ff_siwis` (colonnes 0–127), prosodie 0,4 `ff_siwis` + 0,6 `am_fenrir` (colonnes 128–255). Livrée avec Sillon. |
| Voix anglaise | `af_bella` |
| Autres voix | Choix possible : `ff_siwis` en français ; les 28 voix officielles US et GB en anglais (`af_nicole` marquée « chuchotée ») |
| macOS (app) | Kokoro en **MLX dans la couche Swift** (fichiers Kokoro de mlx-audio-swift, MIT, repris avec leur licence), appelé par le moteur Rust via une **interface étrangère UniFFI** |
| Linux, et `sillon` au terminal sous macOS | Rust + `ort`, modèle **int8**, ONNX Runtime **≥ 1.31 chargé dynamiquement**, de 2 à 4 fils |
| Phonétisation | espeak-ng (GPL-3.0) dans un **processus séparé**, en français comme en anglais |
| Langue | Détectée par document (français / anglais), avec un choix manuel par document |
| Vitesse | Inchangée : synthèse à 1×, Sonic à la lecture. Kokoro reçoit toujours `speed = 1` |
| Surlignage | Minutage par mot, tiré des durées de phonèmes de Kokoro |
| Voix par défaut | Kokoro (paquet recommandé), **y compris pour une configuration existante** : passage unique au premier lancement après la mise à jour (voir « Passage à Kokoro »). Supertonic, MagpieTTS et la voix du système restent installés, sélectionnables, et servent de repli |
| Hors sujet | Qwen3 |

Ces chemins ont été écartés sur mesure (spike) : le fournisseur Core ML d'ONNX,
les graphes fp16 et q8f16, l'ONNX Runtime 1.28 que `ort` télécharge par défaut
(4 fois plus lent en int8) et TTS.cpp (plante sur Metal).

## Critères d'acceptation

On les mesure sur le M5, avec la méthode du banc (voir Tests). Ils portent sur
le chemin MLX de l'app.

| Critère | Seuil | Mesuré au spike |
|---|---|---|
| Temps occupé à 1× | ≤ 10 % | 6,4 % |
| 2× | sans coupure | 10,8 % occupé, 0 coupure |
| Mémoire (empreinte physique) | ≤ 2 Go | 1,25–1,55 Go (plafond MLX 1 Go) |
| Premier son, voix chaude | ≤ 0,3 s | 0,11 s (1re proposition seule) |
| En pause | 0 % CPU, aucun appel à MLX | – |
| Après « Garder la voix chargée » | modèle, cache MLX et espeak-ng libérés ; l'empreinte revient à celle de l'app sans voix (± 50 Mo) | – |

Sous Linux (ORT, CPU), le seuil de 10 % reste limite : 9,7 % avec 4 fils (pointes
à 2,5 cœurs), 13 % avec 2 fils (0,25 cœur). On le note, sans en faire un critère
bloquant. Il faut aussi y régler la mémoire : 1,78 Go mesuré, contre 0,68 Go pour
la même bibliothèque appelée depuis Python.

## Comportement

### Voix et langue

- **Langue du document :** détectée à l'ouverture, sur le texte hors blocs de
  code, avec `whatlang` (MIT) restreint au français et à l'anglais.
  - Il faut au moins 40 lettres et un résultat jugé fiable (`is_reliable`).
    Sinon, la langue par défaut s'applique.
  - `voice.language` accepte une nouvelle valeur, `auto`, qui devient la valeur
    par défaut. Avec `fr` ou `en`, tous les documents prennent cette langue,
    sauf choix manuel.
- **Choix manuel par document :** dans l'onglet Voix, « Langue de ce document »
  propose Automatique (avec la langue détectée entre parenthèses), Français ou
  Anglais. Ce choix est retenu pour ce document, sous la même identité que sa
  position de lecture, et le suit quand il est renommé (`adopt`).
- **Documents mêlés :** **une langue par document**, celle qui a le plus de
  lettres. Les passages dans l'autre langue sont lus avec la voix du document.
  Si la langue minoritaire dépasse 30 % des lettres, l'onglet Voix l'indique
  (« Document mêlé : 62 % en français ») pour que l'utilisateur puisse changer
  de langue à la main. On ne change pas de voix d'un bloc à l'autre : sur des
  blocs courts, la détection hésite, et une voix qui change d'accent au fil du
  texte gêne le suivi des yeux.
- **Une voix par langue :** `[voice.per_language]` dans `config.toml`
  (`fr = "p-fenrir60"`, `en = "af_bella"`). Les autres familles gardent
  `voice.voice`.
- **La langue suit aussi la normalisation :** dates, nombres et sigles sont mis
  en mots en français pour un document français seulement. Un document anglais
  garde ses chiffres, et espeak-ng (`en-us`) les lit. Ce changement vaut pour
  tous les moteurs, Supertonic compris.

### Effets des réglages

| Changement | Effet |
|---|---|
| Voix de la langue du document | Resynthèse depuis la phrase en cours |
| Voix de l'autre langue | Aucun (les paramètres de synthèse ne changent pas) |
| Langue de ce document, ou `voice.language` | Remise en forme (`Reparse`), parce que la normalisation change ; reprise à la même phrase |
| Modèle actif Kokoro, calcul (Metal / CPU), fils | Rechargement de la voix |
| Vitesse | Immédiat, par Sonic (inchangé) |

### Premier son

- Une phrase entière demande environ 0,46 s à chaud, et la 1re proposition seule
  0,11 s. Quand la phrase du curseur n'est pas prête (démarrage, saut,
  resynthèse), le moteur Kokoro la synthétise **en deux morceaux** : la tête,
  jusqu'à la première virgule, au premier point-virgule ou aux premiers
  deux-points (au moins 3 mots), puis la suite.
- La tête joue dès qu'elle est prête, et la suite s'enchaîne sans blanc. Les
  phrases suivantes restent entières : la coupe peut modifier légèrement
  l'intonation, donc on la limite à la phrase qui conditionne l'attente.

### Préparation des noyaux Metal

- Le premier chargement après une installation ou une mise à jour compile les
  pipelines Metal en environ 9 s. Ils restent ensuite dans le cache du système.
- L'app fait cette compilation **au lancement**, en arrière-plan et à basse
  priorité. Elle synthétise une proposition courte, puis libère le modèle tout
  de suite. Le numéro de version de l'app, retenu dans les préférences, évite de
  la refaire.
- Si l'utilisateur lance la lecture pendant cette préparation, le panneau
  affiche « Voix en préparation (première fois)… » (`Readiness::Loading`), et la
  lecture part dès que la voix est prête.

### Repos et pause

- **En pause :** aucun appel à MLX, aucune minuterie. Le processus espeak-ng
  attend sur son entrée sans consommer de CPU. La synthèse en avance s'arrête
  quand la fenêtre de préparation est pleine (comportement actuel du pool).
- **Au repos** (`Reader::rest`) : la libération du moteur Kokoro libère le
  modèle côté Swift (poids et cache MLX vidé) ou la session ORT, et arrête le
  processus espeak-ng.

### Onglet Modèles

| Paquet | Où il apparaît | Contenu | Taille | Licences |
|---|---|---|---|---|
| **Kokoro 82M · MLX** (recommandé) | app macOS seulement | `kokoro-v1_0.safetensors` et `config.json` (`mlx-community/Kokoro-82M-bf16`, révision `a71e4d38`) | 327 Mo | poids et voix Apache-2.0 ; code MLX MIT ; espeak-ng GPL-3.0 (processus séparé) |
| **Kokoro 82M · int8** | terminal (Linux, macOS) | `onnx/model_quantized.onnx` (`onnx-community/Kokoro-82M-v1.0-ONNX-timestamped`, 92 361 055 octets) et `config.json` | 92 Mo, plus ONNX Runtime (≈ 30 Mo, livré par `package.sh` sous Linux) | Apache-2.0 ; ORT MIT ; `ort` MIT/Apache |
| Voix (communes aux deux) | – | les 29 voix françaises et anglaises officielles (510 × 256, safetensors) | ≈ 15 Mo | Apache-2.0 |

- La fiche affiche les langues (« Français, anglais »), les voix (« Française :
  p-fenrir60, ff_siwis · Anglaise : 29 voix »), le rythme mesuré (« Quinze fois
  plus vite que la parole sur M5 : lecture fluide ») et les licences.
- **`p-fenrir60` est livrée avec Sillon** (522 Ko, avec sa recette JSON et la
  mention Apache-2.0). Elle n'est pas téléchargée.
- **Voix par défaut :** Kokoro, pour une configuration neuve comme pour une
  configuration qui nomme déjà un modèle (voir « Passage à Kokoro »).
  L'onglet Modèles marque Kokoro « Recommandé ».

### Passage à Kokoro (une seule fois)

Décision du 2026-10-11 : Kokoro **remplace** la voix par défaut, y compris
dans une configuration existante. La configuration de l'utilisateur nomme
aujourd'hui un modèle ; elle passe donc à Kokoro.

- **Quand :** au premier lancement après la mise à jour, si Kokoro est
  installé ou installable dans ce binaire (paquet MLX dans l'app, paquet int8
  avec `kokoro-onnx`).
  - **Installé :** le modèle actif passe à Kokoro tout de suite
    (`engine.model_path`, `engine.family = "kokoro"`, voix `p-fenrir60` et
    `af_bella` dans `[voice.per_language]`).
  - **Installable :** Kokoro est **proposé**, rien ne se télécharge sans
    accord (décision du 2026-10-12). « Passer à Kokoro » lance le
    téléchargement (annulable dans l'onglet Modèles) ; le modèle actif passe
    à Kokoro à la fin du téléchargement. « Plus tard » ferme la proposition ;
    Kokoro reste installable depuis l'onglet Modèles.
- **Une seule fois :** la migration est retenue dans `config.toml`
  (`app.voice_migration`) quand elle agit : passage fait (tout de suite, ou à
  la fin du téléchargement accepté), ou proposition refusée. Elle ne se refait
  jamais, même si l'utilisateur revient ensuite à l'ancien modèle. Une
  proposition laissée sans réponse, ou un téléchargement accepté puis échoué
  ou annulé, ne retient rien : la proposition revient aussitôt, et au
  lancement suivant.
  Sans Kokoro possible dans ce binaire, rien n'est retenu : une version qui
  l'apporte fera la migration.
- **Rien n'est perdu :** l'ancien modèle reste installé et sélectionnable
  (« Modèle actif » dans l'onglet Voix, « Utiliser » dans l'onglet Modèles).
  Ses réglages propres (voix, échantillonnage) restent dans `config.toml`.
- **Un court avis le dit :** « Nouvelle voix : Kokoro remplace Supertonic 3 ·
  F16. L'ancien modèle reste installé : Réglages › Voix pour y revenir. »
  La proposition : « Nouvelle voix disponible (326,4 Mo) — Passer à
  Kokoro ? » (taille écrite comme dans l'onglet Modèles), avec les boutons
  « Passer à Kokoro » et « Plus tard ». L'app affiche avis et proposition
  dans le panneau et l'onglet Voix, et les annonce à VoiceOver.
- **Au terminal :** un Kokoro installé est adopté de la même façon, avec le
  même avis dans la barre d'état. Installable seulement, le terminal le
  signale (`s` pour installer), sans rien télécharger ni retenir.

## Architecture

### Rust

- **Contrat de synthèse** (`src/tts/mod.rs`) :
  - `TtsSession::synthesize_timed(text, words, params) -> Synthesis { pcm,
    words: Option<Vec<WordSpan>> }`. `words` contient les formes parlées des mots
    affichés (`Word::spoken`, sinon `Word::text`). L'implémentation par défaut
    appelle `synthesize` et ne rend pas de minutage : audio.cpp et la voix du
    système ne changent pas.
  - `TtsEngine::splits_first_clause()` vaut faux par défaut, vrai pour Kokoro.
- **Pool** (`pool.rs`) :
  - `Job` porte les mots ; `Rendered` gagne `words: Option<Arc<[WordSpan]>>`.
  - La coupe en tête et suite s'applique à la phrase du curseur quand elle n'est
    pas prête.
  - Le lecteur reçoit deux segments de la même phrase. Le second commence à la
    durée du premier, et les minutages sont mis bout à bout.
- **Surlignage** (`doc/mod.rs`, `app/mod.rs`) :
  - Avec un minutage, `word_fraction` et `word_start` le suivent. Sans minutage,
    la répartition par poids actuelle reste.
  - Les temps sont en temps de synthèse (1×), l'unité du chronomètre du lecteur :
    il n'y a rien à diviser par la vitesse.
- **Module `src/tts/kokoro/`** :
  - `mod.rs` : `KokoroEngine<B: KokoroBackend>`, une session (le calcul est en
    série). Le moteur met les phonèmes en identifiants (vocabulaire de
    `config.json`, 114 symboles) et entoure la suite du jeton 0 au début et à la
    fin. Il prend la ligne de style `len(phonèmes) − 1`. Au-delà de 510 phonèmes,
    il coupe à la proposition.
  - `backend.rs` : le trait `KokoroBackend` (`infer(ids, style) -> (échantillons
    24 kHz, durées par jeton)`, `warm_up()`, `release()`). Le moteur vérifie que
    le nombre d'échantillons vaut la somme des durées × 600. Sinon, ou si la
    sortie contient un NaN, la phrase échoue.
  - `onnx.rs` (feature `kokoro-onnx`) : `ort` en `load-dynamic`. La version
    d'ORT est vérifiée (≥ 1.31) au chargement. Le nombre de fils internes vaut
    `min(engine.threads, 4)`. L'allocateur est réglé : arène réduite,
    `memory_pattern` coupé, réglage final fixé par mesure.
  - `phonemes.rs` : le trait `Phonemizer`, avec un `EspeakProcess` persistant
    (`espeak-ng -q --ipa -v fr-fr` ou `en-us`, un processus par langue lancé à
    la demande). Le protocole est ligne à ligne, avec une phrase sentinelle pour
    reconnaître la fin d'une réponse. Le traitement de misaki
    (`EspeakG2P` : ponctuation gardée, accents toniques, table de
    remplacements) est porté pour obtenir les mêmes phonèmes que la référence.
    Les mots seuls, pour l'alignement, partent dans la même requête que la
    phrase : un seul aller-retour. Budget : ≤ 5 ms par phrase.
  - `voices.rs` : lit les tenseurs 510 × 256 (`safetensors`, Apache-2.0).
    `p-fenrir60` est incluse dans le binaire (`assets/voices/p-fenrir60.safetensors`,
    avec `p-fenrir60.json`).
  - `align.rs` : le minutage par mot (voir plus bas).
- **Construction** (`build_engine`) :
  - La famille `kokoro` choisit le moteur Kokoro. Avec `engine.compute = metal`,
    le moteur passe par le calcul fourni par l'app ; sinon, ou sans app, il passe
    par ORT.
  - Repli en cas d'échec : le premier modèle audio.cpp installé, sinon la voix du
    système (`LazyEngine` reçoit ce repli au lieu de la seule voix du système).
- **Langue** (`src/text/language.rs`) : `detect(texte) -> Option<(Langue, part)>`.
  `parse_document` reçoit la langue et n'applique la normalisation française
  qu'au français. `TtsParams.language` vient de la langue du document, et
  `TtsParams.voice` de `voice.per_language`.
- **Catalogue** (`models.rs`) :
  - `ModelPackage` gagne `companions: &[Companion]` (dépôt, révision, fichier, destination, taille),
    des fichiers téléchargés dans le dossier de la famille. Un paquet est installé quand tous ses fichiers ont la taille
    attendue. Les voix vont dans `models/kokoro/voices/`, partagé par les deux
    paquets.
  - Famille `kokoro` : langues `fr`, `en` ; voix par langue (`VoiceOption`
    gagne `language`) ; `honoured_options = ["voice", "language"]`, ce qui masque
    hauteur, énergie, intonation et échantillonnage ; une session.
  - `available()` tient compte de ce qui existe à l'exécution : le paquet MLX
    seulement si l'app a fourni le calcul MLX, le paquet int8 seulement avec
    `kokoro-onnx`.
- **Réglages** (`reader/settings.rs`, `config.rs`) :
  - `SettingsView` et `SettingsPatch` gagnent `voices_by_language` et
    `voice_for_language: Option<(langue, voix)>`.
  - Nouvelle action `set_document_language(Option<langue>)`.
  - L'instantané gagne `document_language { détectée, choisie, part }`.
  - `ModelCapabilities.voices` porte la langue de chaque voix.
  - `SettingsEffect::between` traite un changement de langue comme `Reparse`.
  - Passage à Kokoro : `Reader::migrate_voice() -> Option<String>` (l'avis,
    ou le texte de la proposition), appelé une fois au lancement de l'app ;
    `Reader::accept_offer()` (téléchargement, passage à la fin) et
    `Reader::dismiss_offer()` (refus retenu) ; l'instantané gagne `notice` et
    `offer` ; `app.voice_migration` (entier, 0 par défaut) retient que le
    passage a eu lieu ou a été refusé.

### Minutage des mots (`align.rs`)

1. Kokoro rend une durée par jeton d'entrée, en trames de 600 échantillons
   (25 ms à 24 kHz). La longueur de l'audio vaut exactement la somme des durées
   × 600 (vérifié sur les 8 phrases du spike).
2. La phrase est phonétisée **entière** (`Sentence::text`, qui garde les
   liaisons et le contexte des nombres), et chaque mot affiché **seul**.
3. En français, espeak fusionne des mots (liaisons, nombres, `parse_config`) :
   c'était le cas pour 4 phrases sur 16 au spike. Une programmation dynamique
   (distance d'édition, alignement monotone) rattache chaque phonème de la
   phrase à un mot affiché. Une consonne de liaison va au mot qui la suit.
4. Le temps d'un mot est la somme des durées de ses phonèmes, plus la moitié
   des espaces et ponctuations voisines (convention de `join_timestamps`). Les
   jetons de bord vont au premier et au dernier mot.
5. Un mot affiché fait de ponctuation seule (« ? », « : » de la typographie
   française) n'a aucun phonème : il prend les ponctuations de la phrase
   situées entre ses voisins (partagées dans l'ordre s'il y en a plusieurs).
6. Une phrase de plus de 510 phonèmes, synthétisée en plusieurs morceaux, est
   alignée morceau par morceau : chaque morceau reçoit les mots dont le compte
   de phonèmes rejoint le sien (un mot qui finit sur la ponctuation de la
   coupe est préféré), et les temps sont mis bout à bout. La table
   d'alignement est plafonnée à 1 000 000 de cases.
7. Si l'alignement est trop mauvais (coût > 50 % de la longueur), qu'un mot
   n'a aucun phonème ou que la table dépasserait le plafond, la phrase garde
   la répartition par poids. L'événement est journalisé (la raison chiffrée,
   jamais le texte).

Aligner sur les mots affichés, et non sur les mots de `Sentence::text`, règle le
cas des formes parlées : « 1789 » est un mot à l'écran et quatre mots dans la
voix.

### FFI

```rust
#[uniffi::export(with_foreign)]
pub trait KokoroCompute: Send + Sync {
    fn load(&self, model_dir: String) -> Result<(), SillonError>;
    fn warm_up(&self) -> Result<(), SillonError>;
    fn synthesize(&self, ids: Vec<i64>, style: Vec<f32>) -> Result<KokoroOutput, SillonError>;
    fn release(&self);
}
#[derive(uniffi::Record)]
pub struct KokoroOutput { pub samples: Vec<f32>, pub durations: Vec<u32> }
```

- Nouveau constructeur `SillonReader::with_kokoro(compute)`. Sa fabrique de voix
  enveloppe `compute` dans un `KokoroBackend`.
- `SillonReader::new()` reste tel quel : sans calcul MLX, le chemin est ORT ou
  le repli.
- Le Rust garde phonétisation, découpage, voix, minutage, Sonic et lecteur.
  Swift ne fait que le calcul. Les appels arrivent sur le fil de synthèse du
  pool et bloquent jusqu'au résultat. Un PCM de phrase pèse environ 0,5 Mo, et
  sa copie est négligeable.
- Ajouts aux records : voix par langue, langue du document, fiche Kokoro du
  catalogue. Les bindings sont régénérés par `scripts/build-ffi.sh`.

### Swift

- **`macos/Vendor/KokoroMLX/`** : les fichiers Kokoro de mlx-audio-swift
  (commit `244458d`) : `KokoroConfig`, `KokoroModel`, `KokoroModules`,
  `KokoroDecoder`, et les blocs StyleTTS2 dont ils dépendent (`Blocks/`,
  `SharedConfigs.swift`). On y joint `LICENSE` (MIT, Prince Canuma) et une note
  d'origine.
  - Retraits : les imports `MLXLMCommon`, `HuggingFace`, `MLXAudioCore`, et
    `KokoroMultilingualProcessor` (la phonétisation est en Rust).
  - Correctif du spike : `callAsFunction(inputIds:refS:speed:)` devient public.
  - Seule dépendance : `mlx-swift` (MLX, MLXNN), à version figée dans
    `project.yml`.
- **`macos/Sources/Voice/KokoroMLX.swift`** : implémente `KokoroCompute`.
  - Le calcul passe par une file série à soi, jamais par le fil principal.
  - Plafond MLX à 1 Go (`GPU.set(memoryLimit:)`, cache réduit).
  - `release()` libère les poids et vide le cache MLX.
- **`KokoroWarmUp`** : la préparation au lancement, une fois par version de
  l'app (suite de préférences isolée en test).
- **Construction de l'app :**
  - `build-macos.sh` vérifie la présence du Metal Toolchain
    (`xcodebuild -downloadComponent MetalToolchain`). Sans lui, il embarque le
    `mlx.metallib` de la version de MLX figée.
  - Le metallib mesuré au spike pèse 190 Mo : à vérifier sur l'app construite.
- **espeak-ng dans l'app :**
  - `scripts/build-espeak.sh` construit un binaire à partir d'une version
    figée. Il va dans `Contents/Helpers/espeak-ng`, avec ses données réduites à
    `fr` et `en` (`Contents/Resources/espeak-ng-data/`), sa licence et l'adresse
    exacte des sources.
  - L'app passe le chemin au moteur par `SILLON_ESPEAK`.
  - Au terminal, le moteur cherche dans cet ordre : `SILLON_ESPEAK`, à côté du
    binaire, puis `PATH`.
- **Onglet Voix** (famille `kokoro`) :
  - « Voix française » et « Voix anglaise », chacune avec **Écouter** (phrase
    d'exemple dans sa langue).
  - « Langue de ce document » (quand un document est ouvert) et « Langue par
    défaut » (Automatique, Français, Anglais).
  - La mention « Document mêlé » quand elle s'applique.
  - Hauteur, énergie, intonation et échantillonnage restent masqués (réglages
    ignorés).
- **Onglet Modèles :** les fiches Kokoro, avec licences et voix ; aucun nouveau
  contrôle.

## Erreurs

| Situation | Réaction |
|---|---|
| Modèle Kokoro absent ou incomplet | Repli (audio.cpp installé, sinon système). Message « Kokoro n'est pas installé » avec lien vers l'onglet Modèles |
| espeak-ng introuvable | Kokoro ne se charge pas : repli. Linux : « espeak-ng introuvable — installez le paquet `espeak-ng` ». App : « Installation de Sillon incomplète (espeak-ng) » |
| espeak-ng s'arrête en cours de lecture | Relancé une fois. S'il échoue encore, la phrase est sautée (comportement du pool) et la voix passe en repli, avec message |
| MLX indisponible (pas de GPU Metal, metallib illisible) | Sans ORT dans l'app : repli, message « Calcul Metal indisponible » |
| Compilation Metal (premier lancement) | « Voix en préparation (première fois)… » ; la lecture part ensuite seule |
| ONNX Runtime absent ou plus ancien que 1.31 | Refus de charger, avec la version trouvée et la raison (« 4 fois plus lent en int8 ») ; repli |
| Sortie invalide (NaN, longueur ≠ durées × 600) | Échec de la phrase, journalisé |
| Phrase de plus de 510 phonèmes | Coupée à la proposition, puis recollée (minutages compris) |
| Langue non détectée | Langue par défaut ; l'onglet Voix l'indique (« non détectée ») |
| Voix de `config.toml` inconnue | `p-fenrir60` ou `af_bella`, sans réécrire la valeur (règle des valeurs non listées) |

## Tests

- **Rust, unitaires, avec de faux moteurs :**
  - Un `FakeKokoroBackend` rend des durées connues et des échantillons
    silencieux de la bonne longueur. On vérifie : contrôle durées × 600 ; ligne
    de style `len − 1` ; jetons de bord ; coupe au-delà de 510 ; coupe en tête et
    suite pour la seule phrase du curseur ; minutage bout à bout ; `release()`
    appelé au repos ; NaN refusé.
  - Un `FakePhonemizer` remplace espeak-ng ; le protocole à sentinelle est testé
    contre un faux processus (script) qui répond en plusieurs lignes.
  - Langue : paragraphes français et anglais, texte court (langue par défaut),
    document mêlé (langue majoritaire, part), blocs de code ignorés.
  - Réglages : aller-retour de `voice_for_language` ; changement de langue →
    `Reparse` ; voix de l'autre langue → aucune resynthèse ; capacités de
    `kokoro` (voix par langue, réglages masqués).
  - Passage à Kokoro : configuration existante → Kokoro installé adopté ;
    Kokoro installable seulement proposé (ni téléchargement ni retenue),
    accepté → téléchargé puis adopté à la fin, retenu alors ; échec ou
    annulation → rien d'adopté ni de retenu ; refus retenu ; retenu une fois
    (un second lancement ne refait rien, un retour à l'ancien modèle tient
    après relance) ; rien de retenu sans Kokoro possible ; ancien modèle
    toujours installé ; lecteur hors disque : rien ne bouge.
  - Catalogue : paquet multi-fichiers installé seulement complet ; voix
    partagées ; MLX absent du catalogue sans calcul fourni.
- **Alignement, sur des phrases françaises :**
  - Les 16 phrases du spike, avec leurs phonèmes espeak enregistrés en fixtures
    JSON (le test tourne sans espeak). Elles comprennent les 4 cas où les groupes
    ne correspondent pas aux mots : nombres développés, `parse_config`,
    liaisons « est arrivé », « les épaules », « chez elle », « on en ».
  - On vérifie : mots dans l'ordre, chacun ≥ 1 trame, somme égale à la durée,
    liaison rattachée au mot suivant, repli sur les poids quand l'alignement est
    mauvais.
- **Avec espeak-ng installé** (`#[ignore]`, lancés à la main) :
  - parité des phonèmes Rust avec `ref/phonemes.json` (misaki) sur le jeu v1
    français et anglais ;
  - budget ≤ 5 ms par phrase.
  - Avec les voix téléchargées : `p-fenrir60` livrée égale la recette recalculée
    à partir de `ff_siwis` et `am_fenrir` (écart ≤ 1e-6).
- **Swift :**
  - un faux `KokoroCompute` branché sur `SillonReader.withKokoro` ;
  - onglet Voix : deux sélecteurs, « Automatique (français) », mention
    « Document mêlé », Écouter dans la bonne langue ;
  - préparation Metal faite une fois par version ;
  - `KokoroMLX` n'est jamais chargé dans les tests unitaires.
- **Mesure** (méthode du banc, `spike_run.py duty` / `light_run.py duty`) :
  - attente d'un load < 4, 20 s de GPU au repos, avance de 10 s, 2 min à 1×,
    2 min à 2× ;
  - CPU de l'arbre de processus, GPU `ioreg` moins le repos, empreinte physique.
  - macOS : `Measure.swift` (`--measure`) gagne ce scénario de charge, ainsi que
    le premier son à chaud, la pause (CPU nul sur 20 s) et le repos (empreinte
    après `restVoice`). `scripts/measure-macos.sh` rend les lignes `MESURE` des
    critères d'acceptation.
  - Linux : un exemple `kokoro_serve` parle le protocole stdin/stdout du banc,
    pour réutiliser le pilote tel quel.

## Questions ouvertes

- **GPL d'espeak-ng si Sillon est un jour distribué.** Le processus séparé fait
  de l'ensemble un agrégat, ce qui isole la licence. Il faudra quand même livrer
  le texte de la GPL et offrir les sources exactes du binaire embarqué. Un
  phonétiseur permissif reste possible (lexique, règles de liaison, G2P
  neuronal), mais il coûte au moins 1,6 point de WER tant que les liaisons ne
  sont pas écrites.
- **Taille de l'app :** jusqu'à 190 Mo de metallib, à mesurer.
- **Intonation à la coupure de la première phrase :** à vérifier à l'écoute.

## Hors périmètre

- Qwen3-TTS.
- Les autres langues de Kokoro (espagnol, italien, portugais, hindi, japonais,
  chinois).
- Un changement de voix d'un bloc à l'autre dans un document mêlé.
- Une voix d'homme française : aucune ne garde l'accent à ce jour (§11).
- Remplacer espeak-ng par un phonétiseur permissif.
- `mlx-rs` (MLX depuis Rust), non mesuré. Core ML, l'ANE, CUDA sous Linux.
- ORT dans l'app macOS : le repli de l'app est audio.cpp ou la voix du système.
- Une synthèse en flux à l'intérieur d'une phrase, au-delà de la coupe en tête
  et suite.
- Windows.
