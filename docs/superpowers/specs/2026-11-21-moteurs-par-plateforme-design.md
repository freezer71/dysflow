# Les moteurs de calcul par système

Conception du 2026-11-21. C'est le premier des trois projets du passage de
Dysflow à « tout en Rust, sur macOS, Windows et Linux » :

1. **les moteurs par système** (ce document) : la voix et la dictée calculées
   en Rust, sur le meilleur moteur de chaque système ;
2. l'interface en GPUI, qui remplace SwiftUI ;
3. l'intégration au système (lire la sélection, écrire dans l'app au premier
   plan, raccourcis globaux, micro, permissions), par système.

Les projets 2 et 3 auront leur propre conception.

## Pourquoi

Dysflow ne tourne en app que sur macOS. Le calcul de la voix et de la dictée
y est écrit en Swift sur MLX (`macos/Vendor/KokoroMLX`, ~23 000 lignes), donc
introuvable ailleurs. Le développeur, seul mainteneur, veut :

- que l'app marche sur macOS, Windows et Linux ;
- **ne maintenir que du Rust** (des bibliothèques C++ en dessous, comme MLX ou
  ONNX Runtime, sont acceptées) ;
- que chaque système fasse tourner le calcul sur **ce qu'il a de plus
  rapide**, derrière une couche d'abstraction : le reste de l'app ne sait pas
  quel moteur tourne.

## Objectifs

- Le calcul de Kokoro et de la dictée vit dans le cœur Rust (`src/`), plus
  dans Swift.
- Sur Mac : MLX, appelé depuis Rust par `mlx-rs`. Mêmes résultats et même
  vitesse que le Swift actuel (vérifiés par les fixtures de parité).
- Sur Windows et Linux : ONNX Runtime — Kokoro par `ort` (déjà là), la dictée
  par `sherpa-onnx`. Processeur seul en premier ; la carte graphique ensuite,
  quand une mesure le justifie.
- Le choix du moteur est automatique (système, puis matériel), avec un repli
  annoncé quand le moteur voulu échoue (le `Readiness::Fallback` d'aujourd'hui).
- La dictée garde un délai court et **borné**, quelle que soit la durée de la
  dictée.
- Tout se teste sans interface : le lecteur au terminal (`dysflow-cli`) et des
  bancs suffisent pour valider chaque moteur sur les trois systèmes.

## Hors sujet

- L'interface (projet 2) et l'intégration au système (projet 3). La dictée de
  repli par le système (aujourd'hui `SystemDictationEngine`, Apple Speech) est
  de l'intégration au système : projet 3.
- La carte graphique sous Windows et Linux (CUDA, DirectML, Windows ML,
  OpenVINO) : prévue par l'abstraction, pas livrée ici.
- De nouveaux modèles.

## À ne pas faire (échec même si « ça marche »)

- Une régression de qualité de voix sur Mac : la parité avec les fixtures
  existantes est la barrière.
- Une dictée dont le délai grandit avec sa durée (le défaut mesuré de la relecture glissante actuelle, voir « Mesures »).
- Deux copies d'ONNX Runtime de versions différentes dans un même binaire sans
  l'avoir vérifié sur les trois systèmes.
- Casser l'app Mac actuelle pendant la transition : chaque modèle passe de
  Swift à Rust seul, quand sa parité passe.

## Contraintes

- `ort` est épinglé `=2.0.0-rc.13`, ONNX Runtime ≥ 1.31 chargé dynamiquement
  (1.28 est 4 fois plus lent en int8 pour Kokoro).
- `mlx-rs` 0.34 compile MLX 0.32.2 depuis les sources par CMake (~4 min au
  premier build, réseau nécessaire). L'app Swift utilise mlx-swift 0.32.3.
- `sherpa-onnx` 2.0.2 embarque ONNX Runtime **1.28.2** en statique par défaut.
- MSRV actuelle 1.88 (cœur) et 1.91 (`ffi/`).
- Le Core ML d'ONNX Runtime a été écarté sur mesure pour Kokoro (spec
  `2026-10-11-voix-kokoro-design.md`).

## Approches considérées

| | Mac | Windows / Linux | Pour | Contre |
|---|---|---|---|---|
| **A. Moteur natif par système** (retenue, bornée) | MLX par `mlx-rs` | ONNX Runtime (`ort`, `sherpa-onnx`) | Le plus rapide partout ; le port Swift → `mlx-rs` est une traduction à noyaux identiques (spike : 557 lignes Rust pour ~1 580 Swift, parité du premier coup) | Deux implémentations par modèle disponible sur les deux familles |
| B. ONNX Runtime partout | ORT + Core ML | ORT | Un seul graphe par modèle | Core ML d'ORT écarté sur mesure ; sur Mac, B revient au processeur (Kokoro 9,7 % occupé contre 6,4 %, 1,78 Go contre 0,68) |
| C. Rust pur (`candle`, `burn`) | Metal | CUDA / CPU | Pas de C++ | Ports Kokoro et Parakeet immatures et non mesurés ; le développeur accepte le C++ en dessous |

**Retenue : A, bornée.** « Deux implémentations » ne vaut que pour les modèles
qu'on veut sur les deux familles ; la liste Windows/Linux part réduite
(Kokoro, Parakeet v3, éventuellement Whisper small). Le décodage (TDT, boucle
autorégressive, mel) est à écrire en Rust dans toutes les approches ;
`sherpa-onnx` le fournit côté ONNX.

## Conception

### 1. Deux contrats de calcul dans le cœur

**Voix.** Les contrats existent et ne changent pas : `KokoroBackend`
(`src/tts/kokoro/backend.rs`) et `TextCompute` (`src/tts/text_compute.rs`).
Ce qui change, ce sont leurs implémentations :

- `kokoro::mlx::MlxBackend` (nouveau, `cfg(all(target_os = "macos",
  target_arch = "aarch64"))`, feature `mlx`) : le port du spike, en Rust.
- `kokoro::onnx::OrtBackend` : inchangé.
- L'implémentation Swift (`KokoroCompute`, UniFFI `with_foreign`) reste
  pendant la transition, puis disparaît.

Qwen3-TTS et Magpie passent en `TextCompute` Rust par `mlx-rs`, sur Mac
seulement, après la dictée (ordre plus bas).

**Dictée (nouveau module `src/stt/`).** Aujourd'hui toute la dictée est en
Swift (`DictationEngine`, `RollingTranscription`, `StreamingDictationEngine`,
`SpeechModelRunner`). Le cœur gagne :

- `trait Recognizer` : un modèle qui lit un morceau d'audio entier (16 kHz
  mono `f32`) et rend son texte (Parakeet, Whisper, Canary, Qwen3-ASR,
  Cohere). `load` / `warm_up` / `transcribe(samples, language, cancel)` /
  `release`, sur le modèle de `KokoroBackend`.
- `trait StreamingRecognizer` : un modèle qui lit en flux (Voxtral Realtime,
  Nemotron). `open_stream` → `feed(samples) -> texte nouveau` → `finish() ->
  texte définitif`.
- `Dictation` : la logique de dictée, sans micro ni interface. Elle reçoit
  l'audio par morceaux et émet des événements (provisoire, définitif) — ce que
  fait aujourd'hui le Swift, déplacé et testable en Rust.

### 2. La dictée découpe aux silences

Le régime actuel (relire tout l'audio depuis le début) a un coût qui grandit
avec la durée : mesuré à 7,7 s de délai final sur processeur et 6 s de retard
provisoire même en MLX, après 40 s de dictée.

Nouveau régime pour les `Recognizer` :

- un détecteur de silence découpe la parole en segments : Silero VAD fourni
  par `sherpa-onnx` sous Windows/Linux ; sur Mac, **sans ONNX Runtime**
  (l'app n'en embarque toujours pas) — Silero porté en `mlx-rs` ou un
  détecteur en Rust pur, au choix du constructeur sur mesure *(appel de
  l'assistant)* ;
- **dans le segment en cours, la relecture glissante continue**, au rythme de
  `RollingTranscription` (max(0,4 s, 3 × la dernière lecture)) : le texte se
  forme mot à mot tant que la machine suit, et le coût reste borné par la
  longueur d'un segment ;
- à chaque silence, le segment est lu une dernière fois et son texte devient
  définitif ; un segment est coupé de force au-delà d'une durée maximale
  (au choix du constructeur, 10 à 15 s).

Les `StreamingRecognizer` gardent leur régime (chaque bout d'audio lu une
fois).

**Qualité.** Chaque segment garde une marge d'audio de part et d'autre de la
coupure. La parité de dictée se juge à deux niveaux *(appel de
l'assistant)* : le modèle seul (`Recognizer` en `mlx-rs` contre Swift, même
texte sur un audio fixé), et la chaîne `Dictation` (taux d'erreur de mots au
plus 1 point au-dessus de la relecture complète actuelle, sur au moins trois
audios de 40 s ou plus ; la mesure du spike, 0,8 % contre 1,5 %, tient en un
mot sur 133 et reste à confirmer).

*Décision de l'utilisateur :* sur une machine lente, voir le texte arriver
phrase par phrase, environ une demi-seconde après chaque pause, est
acceptable.

### 3. Le choix du moteur

Un module `src/compute/` décide une fois, au démarrage :

```text
enum Accelerator { Mlx, OrtCpu /* plus tard : OrtCuda, OrtDirectMl, … */ }
fn available() -> Vec<Accelerator>   // par système et matériel, du meilleur au moins bon
```

- macOS Apple Silicon : `[Mlx, OrtCpu]` ; Windows, Linux : `[OrtCpu]`.
- Chaque paquet du catalogue (`src/models.rs`) déclare, au lieu d'un seul
  `Runtime`, **ses moteurs** (`&[Runtime]`) : par exemple Kokoro →
  `[KokoroMlx, KokoroOnnx]`, Parakeet v3 → `[ParakeetMlx, ParakeetSherpa]`.
  Les poids diffèrent par moteur (safetensors MLX, ONNX int8) : un paquet par
  moteur, regroupés par famille, comme les variantes de précision
  d'aujourd'hui.
- Le catalogue montré à l'utilisateur ne garde, par famille, que le paquet du
  **meilleur moteur disponible** sur sa machine : sur Mac, les paquets MLX ;
  ailleurs, les paquets ONNX *(appel de l'assistant)*.
- Le repli ne télécharge jamais rien : un moteur qui ne charge pas cède au
  paquet suivant **déjà installé** de la même famille, sinon à la voix du
  système (voix) ou annonce l'échec (dictée), avec le message de repli
  d'aujourd'hui *(appel de l'assistant)*.
- **Réglages existants** : les identifiants de paquet enregistrés
  (`parakeet_tdt_v3`, le paquet Kokoro MLX…) restent ceux des paquets MLX, et
  leurs dossiers téléchargés restent valables : aucune migration, aucun
  nouveau téléchargement sur un Mac déjà installé. Le choix enregistré
  désigne un paquet ; sur un autre système, un identifiant MLX est lu comme
  sa famille *(appel de l'assistant)*.
- Un réglage caché permet de forcer un moteur, pour les bancs et le
  diagnostic.

### 4. Une seule copie d'ONNX Runtime

Kokoro (`ort`, ≥ 1.31, dynamique) et la dictée (`sherpa-onnx`) doivent partager
**la même** bibliothèque ONNX Runtime ≥ 1.31, livrée à côté du binaire comme
aujourd'hui. Piste : `sherpa-onnx` en mode `shared`, construit contre cette
copie. **Premier lot du plan** : le vérifier sur macOS, Linux (x86_64, ARM64)
et Windows. Si c'est impossible, repli : la dictée tourne dans un processus
séparé (comme espeak-ng aujourd'hui) avec son propre ONNX Runtime.

### 5. Le pont avec l'app Mac pendant la transition

**Les copies de MLX dans l'app.** L'app lie aujourd'hui MLX par mlx-swift (`Cmlx` de
mlx-swift 0.32.3, MLX 0.32.2 mesuré au lot 0) ; `mlx-rs` compile sa propre copie (0.32.2), en statique, dans
`libdysflow_ffi.a`. Deux copies dans un binaire risquent des symboles en
double et deux allocateurs Metal. **Lot 0** : le vérifier. Dans l'ordre de
préférence *(appel de l'assistant)* : (a) `mlx-sys` construit sur les
sources `Cmlx` de mlx-swift, versions alignées ; (b) si c'est impossible,
la transition se fait d'un bloc côté app : tous les modèles sont portés et
validés par les bancs et `dysflow-cli`, puis l'app passe en une fois et
mlx-swift part. Résultat du lot 0 : un troisième cas, deux copies côte à côte
(voir « Décisions »).

L'app Swift continue d'appeler le cœur par UniFFI. Pour chaque modèle porté,
le cœur cesse de demander le calcul à Swift et le fait lui-même ; le code
Swift du modèle est supprimé dans le même lot. La dictée Swift
(`MLXDictationEngine`, `StreamingDictationEngine`) devient une fine couche
qui passe l'audio du micro à `Dictation` et relaie ses événements, jusqu'au
projet 3.

## Windows

Rien n'existe encore pour Windows : ni CI, ni paquet, et le cœur dépend
d'outils Unix (`libc`, `update.rs` qui lance `sh`, `dysflow-espeak` construit
par `build-espeak.sh`, sans lequel Kokoro n'a pas de phonèmes). **Décision de
l'utilisateur** : Windows fait partie de ce projet. Le lot W le prépare :

- un runner Windows dans la CI (compilation et tests du cœur) ;
- `dysflow-cli` qui se construit et lit sous Windows : `dysflow-espeak` pour
  Windows, ONNX Runtime livré à côté du binaire, les chemins Unix remplacés ;
  la mise à jour automatique peut rester absente sous Windows dans ce projet.

**Vérification sur matériel.** L'utilisateur a un PC Windows et un PC Linux.
La CI valide la compilation et les tests ; les bancs (vitesse, délai de
dictée, mémoire) se lancent sur ces deux machines à la fin des lots 1, 4 et 8,
et leurs relevés sont joints au lot.

## Ordre des lots

0. Les copies de MLX dans l'app (voir 5) ; décide si la suite passe
   modèle par modèle ou d'un bloc.
W. Windows : CI et `dysflow-cli` (voir « Windows »).
1. Une seule copie d'ONNX Runtime pour `ort` et `sherpa-onnx`, vérifiée sur
   les trois systèmes (risque principal ; bloque Windows/Linux).
2. `src/compute/` et le catalogue à plusieurs moteurs.
3. Kokoro en `mlx-rs` dans le cœur ; l'app l'utilise ; `KokoroCompute` Swift
   supprimé. Feature `mlx`, build CMake mis en cache dans la CI macOS.
4. `src/stt/` : `Recognizer`, `Dictation` (découpe aux silences), Parakeet v3
   par `sherpa-onnx` ; une sous-commande de dictée de fichier dans
   `dysflow-cli` pour tester sous Linux et Windows.
5. Parakeet v3 en `mlx-rs` ; l'app Mac dicte par `Dictation` du cœur.
6. Les autres modèles de dictée Mac en `mlx-rs`, un lot chacun, chacun
   avec sa fixture de parité : Whisper, Canary, Nemotron et Voxtral
   (`StreamingRecognizer`), Qwen3-ASR, Cohere, Phonon.
7. Qwen3-TTS et Magpie en `mlx-rs` ; `TextSpeechCompute` Swift supprimé.
8. Whisper small par `sherpa-onnx` sous Windows/Linux si la mesure le garde.

Après le lot 7, `macos/Vendor/KokoroMLX` est vide et supprimé.

## Tests et mesures

- **Parité** par modèle porté en `mlx-rs` : fixture produite par la référence
  Python/Swift (comme `kokoro-parite.json`), mêmes durées, écart moyen
  < 0,01 (le seuil de `KokoroParityTests`). Pour la dictée : même texte que
  le Swift sur un audio fixé.
- **Bancs** (dans `examples/`, hors binaire livré) : premier appel, médiane à
  chaud, facteur temps réel, mémoire résidente et pic MLX ; pour la dictée,
  délai du texte définitif après la fin de la parole et pire retard du
  provisoire, sur 40 s de parole au moins.
- **Seuils** : voix — ceux de la spec Kokoro (≤ 10 % occupé à 1×, 2× sans
  coupure) ; dictée — texte définitif ≤ 1 s après la fin d'un segment sur le
  M5 en processeur seul, et délai **indépendant** de la durée de la dictée.
- Les mesures se font machine au calme (charge < 2, pas de swap), et le
  relevé l'indique.

## Problèmes connus, à traiter à part

- **Mémoire de Kokoro sur les phrases longues** : pic MLX ~1,44 Go sur
  162 jetons, en Rust comme en Swift (la spec Kokoro annonce 0,68 Go, mesuré
  sur des phrases plus courtes). Touche l'app actuelle.

## Laissé au constructeur

- Les noms exacts des types et des modules, et le découpage en fichiers.
- La durée maximale d'un segment et les réglages du détecteur de silence.
- La forme du réglage caché qui force un moteur.
- La mise en cache du build de MLX dans la CI.

## Mesures (spikes du 2026-11-21, M5, 16 Go)

Code jetable, hors dépôt.

**Kokoro, `mlx-rs` 0.34 contre Swift (mlx-swift 0.32.3), mêmes plafonds.**

| | Rust `mlx-rs` | Swift MLX |
|---|---|---|
| Durées (34 jetons, graine 7) | identiques à la fixture | identiques |
| Écart moyen à la fixture | 0,00090 | 0,00127 |
| Médiane à chaud, phrase courte (2,15 s d'audio) | 0,140–0,184 s | 0,151–0,157 s |
| Médiane à chaud, phrase longue (6,65 s) | 0,43–0,47 s | 0,49–0,54 s |
| Pic MLX, phrase longue | 1,43–1,44 Go | 1,44 Go |
| Premier lancement après compilation (une mesure) | 4,5 s | 12,5 s |

**Parakeet TDT 0.6B v3 int8, `sherpa-onnx` 2.0.2, processeur seul**
(audio de 42,6 s ; machine chargée pendant la mesure, meilleur des passes) :

| | 4 fils | MLX (GPU) |
|---|---|---|
| Lecture hors ligne de 42,6 s | 5,4 s | 0,45 s |
| Relecture glissante actuelle : délai final / pire retard provisoire | 7,7 s / 14 s | 0,9 s / 6 s |
| Découpe aux silences : délai par segment (1 à 5 s) | 0,12–0,45 s | non mesuré |
| Mémoire résidente max | 1,1–1,6 Go | 1,0 Go |
| Taux d'erreur de mots | 0,8 % (1,5 % en segments) | 0,8 % |

Whisper small int8 en segments, 4 fils : 0,5–1,2 s par segment, 2,3 %
d'erreur de mots.

## Décisions

**Lot 0 (2026-11-21) : deux copies de MLX côte à côte, cohabitation
vérifiée pour un test de fumée ; la transition se fait modèle par modèle,
sous réserve du test de charge du lot 3.**

- **Résultat** : l'édition des liens passe sans `duplicate symbol`, sans
  rien changer à `mlx-sys`. `MLXCoexistenceTests` (MLX Swift, puis
  `mlxRustSmoke()` par UniFFI, puis de nouveau MLX Swift : 6 chaque fois)
  et `KokoroParityTests` (vrai modèle, écart < 0,01) passent dans le même
  processus. Ce n'est ni (a) ni (b), mais un troisième cas : **deux
  copies côte à côte**, dans deux images distinctes. (a) donnerait une
  seule copie (`mlx-sys` construit sur les sources `Cmlx`) ; il n'a pas
  été tenté, l'édition des liens passant sans lui. (b) n'est pas
  nécessaire.
- **Portée de la vérification** : la cohabitation est vérifiée pour un
  test de fumée, pas au-delà. La copie Rust n'a sommé que trois flottants ;
  `KokoroParityTests` n'exerce que mlx-swift ; les deux allocateurs Metal
  n'ont pas été mis sous charge par deux vrais modèles ; seul le build
  Debug a été testé, et `nm` lu en Debug seulement.
- **Pourquoi l'édition des liens passe** : les deux copies vivent dans deux images
  distinctes. MLX de mlx-swift est dans `KokoroMLX.framework` (669 symboles
  `_mlx_*` exportés), MLX de `mlx-rs` dans l'exécutable de l'app
  (`Dysflow.debug.dylib` en Debug : 602 symboles `_mlx_*`). L'espace de
  noms à deux niveaux de macOS lie chaque référence à son image. Les
  versions sont déjà les mêmes : `Cmlx` de mlx-swift 0.32.3 et `mlx-sys`
  0.7.0 (`mlx-rs` 0.34.0) embarquent tous deux MLX 0.32.2.
- **Symboles en conflit** : aucun à l'édition des liens.
- **Conséquence pour l'ordre des lots** : modèle par modèle (lots 3 à 7).
  mlx-swift reste lié tant qu'un modèle Swift subsiste.
- **La copie Rust n'est liée que dans le build de test jusqu'au lot 3**
  (revue finale des fondations) : `mlx` est une feature de `ffi/`,
  désactivée par défaut, que seul le build de test de la CI active
  (`FEATURES=ocr,mlx ./scripts/build-ffi.sh`, puis `MLXCoexistenceTests`) ;
  `mlx_rust_smoke` n'existe qu'avec elle. `build-macos.sh` (donc
  `release-macos.sh`) la refuse et vérifie qu'aucun symbole `_mlx_` n'est
  dans `libdysflow_ffi.a` : un Mac installé ne reçoit rien de MLX en Rust.
  Avant de la livrer, le lot 3 devra traiter : les sources que `mlx-sys`
  télécharge au build sans empreinte (MLX, fmt, nlohmann/json,
  metal-cpp) ; l'avis de licence de gguflib (MIT), absent de l'app ; la
  taille de l'app Release mesurée avec et sans (`nm … | grep -c ' _mlx_'`,
  `libdysflow_ffi.a` 165 Mo avec, 122 Mo sans).
- **À régler au lot 3, avant qu'un modèle Rust serve dans l'app livrée** :
  - *Un test de charge.* Deux vrais modèles, un par copie (par exemple
    Kokoro en `mlx-rs` et un modèle de dictée en mlx-swift), qui calculent
    dans le même processus, en Release, avec le plafond mémoire de l'app
    posé : résultats justes des deux côtés, et pic mémoire relevé pour
    chaque copie. Si ce test échoue, revenir à (a) ou à (b).
  - *Les noyaux Metal de la copie Rust.* `mlx-sys` écrit `mlx.metallib`
    dans `~/.mlx/lib/<clé>/` et inscrit ce chemin absolu dans le binaire.
    La copie Rust, compilée sans `SWIFTPM_BUNDLE`, ne lit pas le
    `default.metallib` de mlx-swift ; elle cherche `mlx.metallib` à côté de
    son image (`Contents/MacOS/`), puis ce chemin de la machine de build.
    Vérifié : sans `~/.mlx`, `smoke()` panique (« Failed to load the default
    metallib »). L'app livrée doit joindre `mlx.metallib` dans
    `Contents/MacOS/` (ou fixer le chemin au lancement).
  - *Deux allocateurs Metal.* Chaque copie a son cache et son plafond : le
    plafond posé par l'app (mlx-swift) ne borne pas la copie Rust. Le
    budget mémoire doit couvrir les deux pendant la transition.

**Lot 1 (2026-11-21) : une seule copie d'ONNX Runtime 1.31.0, partagée
par Kokoro (`ort`) et sherpa-onnx, sur macOS, Linux x86_64, Linux ARM64 et
Windows x86_64. Le repli (dictée dans un processus séparé) n'est pas
nécessaire.**

- **Résultat par système** (porte `examples/ort_single_copy.rs`, lancée
  par `scripts/ort-gate.sh` en CI depuis un dossier qui a la forme du
  paquet) : sur les quatre, Kokoro int8 dit « Bonjour. », Silero VAD (par
  sherpa-onnx) y trouve un segment, l'ORT de Kokoro et celui de sherpa-onnx
  sont tous deux 1.31.0, et une seule image du processus exporte
  `OrtGetApiBase`. Vérifié aussi sur le M5, et sous Linux ARM64 dans un
  conteneur sur le M5. Reste à vérifier sur les PC
  Windows et Linux de l'utilisateur (archive `ort-gate` de la CI).
- **La porte voit le problème** : avec le sherpa-onnx statique par défaut,
  elle échoue (sur le M5) : « Kokoro 1.31.0, sherpa-onnx 1.28.2 » et deux
  copies chargées (le binaire, qui embarque la 1.28.2, et la bibliothèque
  1.31.0). Une seconde copie 1.31.0, chargée sous un autre nom par
  `DYSFLOW_ORT_LIB`, est vue aussi (M5 et Linux ARM64, conteneur).
- **Comment sherpa-onnx est lié** :
  - `sherpa-onnx` =2.0.2 en `shared` (feature `stt-sherpa`). La
    bibliothèque C (`libsherpa-onnx-c-api.so`/`.dylib`,
    `sherpa-onnx-c-api.dll`) est prise telle que publiée par k2-fsa
    (archive `shared` de la v2.0.2, empreinte épinglée,
    `scripts/sherpa-onnx.sh`) ; sherpa-onnx n'est pas recompilé.
  - À côté du binaire, elle charge par son nom (`libonnxruntime.so`,
    `libonnxruntime.dylib`, `onnxruntime.dll`) l'ONNX Runtime 1.31.0 du
    paquet NuGet que livre déjà `package.sh` : le même fichier que Kokoro.
    L'ORT 1.28.2 de l'archive n'est jamais copié. Elle ne prend à ORT que
    `OrtGetApiBase` (et l'API par son numéro, qu'ORT garde d'une version à
    l'autre) ; `SherpaOnnxGetOnnxruntimeVersionStr` interroge ce même
    `OrtGetApiBase`.
  - Sous Linux, la bibliothèque C demande `OrtGetApiBase@VERS_1.28.2` ; ORT
    ne définit que sa propre version : la requête est renommée en
    `VERS_1.31.0` (`scripts/elf-version-ref.py` : 3 caractères de même
    longueur dans `.dynstr` et le `vna_hash`, 5 octets en tout, rien
    d'autre). À refaire à chaque
    montée d'ORT ou de sherpa-onnx.
  - Le script de build de `sherpa-onnx-sys` est remplacé par deux
    `--config` (clé `links`) : le binaire n'est lié qu'à la bibliothèque C,
    pas à ONNX Runtime (sous Linux ARM64, il y inscrivait
    `libonnxruntime.so.1`, que le paquet n'a pas), et rien n'est téléchargé
    au build. `build.rs` ajoute le chemin de recherche à côté du binaire
    (`$ORIGIN`, `@loader_path`). Sans ces `--config` (Clippy, MSRV), le
    script de build télécharge son archive, avec ORT 1.28.2, que la porte
    refuse.
  - Sous Linux, l'éditeur de liens de GNU résout aussi les dépendances de la
    bibliothèque C : l'ORT 1.31.0 est posé à côté d'elle au build, sans que
    le binaire en dépende (seul `libsherpa-onnx-c-api.so` dans ses `NEEDED`).
  - L'app macOS n'en a rien : `ffi/` n'active pas `stt-sherpa`, et la garde
    de `build-ffi.sh` (aucun `ort` dans le graphe) la refuse.
- **Taille ajoutée au paquet** : la bibliothèque C seule, ORT étant déjà
  livré. Linux x86_64 5,8 Mo (2,3 Mo compressés), Linux ARM64 5,2 Mo
  (2,1 Mo), macOS 4,9 Mo (2,0 Mo), Windows 4,8 Mo (2,0 Mo, runtime C
  statique : aucune DLL Visual C++ de plus). Le binaire `dysflow` ne
  grandit que des liaisons Rust qu'il appellera (lot 4).
- **À régler au lot 4** : un binaire lié à sherpa-onnx ne démarre plus sans
  sa bibliothèque C ni ONNX Runtime à côté de lui (aujourd'hui, sans ORT,
  `dysflow` démarre et la voix du système lit) ; joindre la bibliothèque C
  et la licence Apache-2.0 de sherpa-onnx au paquet ; les tests du binaire
  avec `stt-sherpa` ne trouvent pas la bibliothèque depuis `target/*/deps/`.
