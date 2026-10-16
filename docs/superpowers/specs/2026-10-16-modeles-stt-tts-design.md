# Modèles de lecture et de dictée : télécharger, choisir, supprimer, mesurer

16 oct. 2026 · app macOS seulement (le terminal garde Kokoro int8 et sa page).

## Intention

L'utilisateur choisit lui-même le modèle qui lit (TTS) et celui qui écrit ce
qu'il dicte (STT), parmi plusieurs, sur la page des réglages. Il peut les
télécharger, **les supprimer** (essentiel : ils pèsent des gigaoctets), choisir
la voix du modèle de lecture, et voir d'un coup d'œil ce que vaut chaque
modèle (précision, vitesse, mémoire), chiffres publiés avant téléchargement,
chiffres mesurés sur son Mac ensuite.

Ce que l'utilisateur a demandé :
- modèles de dictée : Whisper, Parakeet, Voxtral Mini, et les modèles NVIDIA ;
- modèles de lecture : Kokoro, Qwen3-TTS, MagpieTTS, voix de macOS ;
- une **table** sur la page des réglages, deux sections (Lecture, Dictée) ;
- une **seule voix** pour la lecture, quelle que soit la langue du texte ;
- un indicateur de performance : publié avant, banc local après (« Mesurer »).

Hypothèses (à corriger si fausses) :
- la simplification du 15 oct. (plus d'onglet Modèles) est volontairement
  revue : le choix revient, mais sur la page unique, pas dans un onglet ;
- l'installation automatique de Kokoro au premier lancement reste : c'est le
  modèle par défaut, l'utilisateur en change ensuite s'il veut.

Revient sur une décision passée : « Une voix par langue » (p-fenrir60 en
français, af_bella en anglais) devient **une voix pour tout**. Avec Kokoro,
une voix anglaise lit le français avec un accent : la prononciation suit
toujours la langue du texte (espeak-ng), le timbre non. La voix par défaut
reste p-fenrir60.

## Ce qui est réalisable (recherche du 16 oct. 2026)

| Modèle | Rôle | Port Swift retenu | Poids | Français | Au catalogue |
|---|---|---|---|---|---|
| Kokoro 82M | lecture | déjà vendorisé (mlx-audio-swift) | 342 Mo | oui | oui |
| Qwen3-TTS 0.6B Base, 1.7B CustomVoice | lecture | mlx-audio-swift `Qwen3TTS` | à relever au plan (8 bit) | oui, sans locuteur natif | oui |
| MagpieTTS multilingual 357M | lecture | vendorisé depuis speech-swift (MLX) | ~411 Mo (8 bit) | oui | oui |
| Voix de macOS | lecture | `AVSpeechSynthesizer` | — | oui | oui (sans téléchargement) |
| Dictée de macOS | dictée | `SpeechAnalyzer` (déjà là) | — | oui | oui (sans téléchargement) |
| Parakeet TDT 0.6B v3 | dictée | mlx-audio-swift `Parakeet` | 2,5 Go | oui (25 langues) | oui |
| Whisper tiny, small, large-v3-turbo | dictée | mlx-audio-swift `Whisper` | 75 Mo → 1,6 Go | oui | oui |
| Canary 1b-v2 (NVIDIA) | dictée | mlx-audio-swift `Canary` | 1,1 Go (q8) | oui | oui, texte rendu en fin de dictée |
| Voxtral Mini 4B Realtime | dictée | mlx-audio-swift `VoxtralRealtime` | à relever au plan (4 bit) | oui | oui, remplace Voxtral Mini 3B |
| Voxtral Mini 3B | dictée | speech-swift seul | 3,8 Go | oui | **non** : le Realtime fait mieux pour dicter |
| Canary 180m-flash | dictée | speech-swift (CoreML) | 245 Mo | oui | **non** pour l'instant (autre pile) |
| Canary 1b-flash, canary-qwen-2.5b | dictée | aucun | — | — | **non** : pas de port Swift |

Identifiants Hugging Face exacts et révisions figées : relevés au plan de
chaque lot, comme pour Kokoro (`repo` + `revision` par commit).

## Architecture

### Approche retenue

- **mlx-audio-swift en dépendance** (paquet SPM) pour Whisper, Parakeet,
  Canary, Voxtral Realtime et Qwen3-TTS. MLX reste lié **une seule fois** :
  un framework `SpeechModels` (renommage de `KokoroMLX`, ou un framework
  voisin qui le porte) lie mlx-swift, mlx-swift-lm et mlx-audio-swift ; l'app
  ne lie que lui. Version : celle de mlx-swift compatible avec 0.32.3 et
  avec l'exigence `mlx-swift-lm ≥ 3.32.3` du paquet, vérifiée au lot 2.
- **MagpieTTS vendorisé** depuis speech-swift (fichiers MLX seuls, licence
  Apache 2.0 notée dans `ORIGINE.md`), comme Kokoro : speech-swift impose
  `mlx-swift-lm 3.31.4` exact, incompatible avec mlx-audio-swift.
- Kokoro garde son port vendorisé et corrigé (parité `KokoroParityTests`).

### Le catalogue : un seul, dans le cœur Rust

`src/models.rs` sait déjà télécharger (reprise, progression, annulation),
mesurer l'espace occupé, supprimer. On l'étend plutôt que d'en écrire un
second en Swift.

- `ModelPackage` gagne `kind: Kind` (`Speech` = lecture, `Dictation`) et de
  nouveaux `Runtime` : `Qwen3TtsMlx`, `MagpieMlx`, `ParakeetMlx`,
  `WhisperMlx`, `CanaryMlx`, `VoxtralRealtimeMlx`. Tous sont « app
  seulement » (`available_with(mlx: true)`) : le terminal ne les voit pas.
- Les dépôts à nombreux fichiers (Qwen3, Whisper…) sont décrits par leurs
  `companions`, comme Kokoro : liste figée au plan, pas de listing du dépôt
  à l'exécution (révision figée, tailles connues, rien d'inattendu).
- Les deux entrées « système » (voix de macOS, dictée de macOS) sont des
  lignes de la table sans paquet : toujours « installées », jamais
  supprimables.
- Un champ `published: Option<Score>` porte les chiffres publiés (voir
  « Performance »).

FFI (`ffi/src/lib.rs`) : `VoiceModel` devient `ModelRow` avec `kind`,
`disk_bytes` (espace réellement occupé), `published`, `measured` ;
`voices()` devient `models(kind)`. `install_voice`/`remove_model`/
`cancel_download` servent aux deux types. Un seul téléchargement à la fois
(comme aujourd'hui) : les autres boutons « Télécharger » sont grisés avec
« Un téléchargement est en cours ».

### Le choix : dans `config.toml`

- Lecture : `engine.model_path` (existant) désigne le paquet ; nouveau
  `voice.voice` unique (une voix, toutes langues). `voice.voices` par
  langue est lu une fois pour reprendre la voix française, puis ignoré.
- Dictée : nouveau `[dictation] model = "<id>"` (`"system"` par défaut).
  La langue de dictée (fr/en) reste un réglage à part ; Whisper, Parakeet,
  Canary et Voxtral la reçoivent comme indication.

### Lecture : moteurs nouveaux

Kokoro reçoit des phonèmes du cœur ; Qwen3 et Magpie prennent le **texte**.
Nouvelle interface étrangère, à côté de `KokoroCompute` :

```rust
#[uniffi::export(with_foreign)]
pub trait TextSpeechCompute: Send + Sync {
    fn load(&self, model_dir: String, runtime: String) -> Result<(), SillonError>;
    fn warm_up(&self) -> Result<(), SillonError>;
    fn synthesize(&self, text: String, language: String, voice: String,
                  cancel: Arc<SynthesisCancel>) -> Result<SpeechOutput, SillonError>;
    fn release(&self);
}
```

`SpeechOutput` : échantillons, fréquence. **Pas de temps par mot** : ces
modèles n'en donnent pas. `Synthesis.words` est alors `None` et le suivi
des mots se fait par estimation (répartition au prorata des syllabes de la
phrase). Conséquence pour la lecture bimodale : le surlignage du mot est
moins exact qu'avec Kokoro. La fiche du modèle le dit (« Suivi des mots
approché »).

Voix de macOS : `AVSpeechSynthesizer.write(_:toBufferCallback:)` côté
Swift, même interface ; les temps des mots viennent de
`willSpeakRangeOfSpeechString` quand macOS les donne.

### Dictée : moteurs nouveaux

**Révisé le 16 oct. 2026 (lot 2), décisions de l'utilisateur :**

- **Copie des fichiers utiles, pas de dépendance mlx-audio-swift.** Le
  paquet tirerait ~10 paquets (swift-syntax, mlx-swift-lm…), compilerait
  16 modèles et son propre téléchargement Hugging Face, et lierait la
  version de MLX à son calendrier. Parakeet, Canary, Voxtral Realtime et
  Whisper sont copiés depuis mlx-audio-swift (commit `244458d`, MIT) dans le
  framework MLX existant, comme Kokoro, sans `fromPretrained` ni import
  `HuggingFace`/`MLXLMCommon` ; MLX reste en 0.32.3, lié une fois. Whisper
  lit son vocabulaire dans `multilingual.tiktoken` par un petit décodeur à
  nous (pas de swift-transformers) si le relevé le confirme possible.
  (Remplace la puce « mlx-audio-swift en dépendance » de l'approche
  retenue pour la dictée ; Qwen3-TTS sera tranché au lot 3.)
- **Relecture glissante** pour Parakeet, Whisper et Canary, qui ne
  transcrivent qu'un enregistrement terminé : toutes les ~1,5 s, le modèle
  relit tout ce qui a été dit depuis le début, le texte provisoire se met à
  jour (`.text(_, isFinal: false)`) ; à la fin, une dernière lecture donne
  le texte définitif. Une relecture ne démarre pas tant que la précédente
  court. Voxtral Realtime, lui, transcrit au fil de l'eau (session
  `step`/`finish`, délai 480 ms).
- **Variantes légères** : Parakeet TDT 0.6B v3 (2,5 Go, seule version
  mlx-community), Whisper tiny, small et large-v3-turbo en 4 bits (23 Mo,
  140 Mo, 464 Mo), Canary 1b-v2 q8 (1,1 Go), Voxtral Mini 4B Realtime
  4 bits (3,1 Go).

Un **aiguilleur** (`RoutingDictationEngine`, conforme à `DictationEngine`)
est donné au contrôleur de dictée : à chaque `start`, il passe au moteur du
modèle choisi (`dictationModel` dans `UserDefaults`, `system` par défaut).
Micro par `AVAudioSinkNode` (comme `SystemDictationEngine`), 16 kHz mono.
Modèle choisi absent (supprimé, fichiers manquants) ou qui ne se charge
pas : dictée de macOS, et la section Dictée le dit.

### Mémoire

Un seul modèle de lecture et un seul modèle de dictée chargés à la fois.
Le modèle de dictée se charge à la première dictée et se libère après
`keep_voice_minutes` sans usage (même règle que la voix).

## La table (page des réglages)

Deux sections, **Lecture** et **Dictée**, chacune une table SwiftUI
(`Table`, colonnes triables désactivées) :

| Colonne | Contenu |
|---|---|
| (choix) | bouton radio : le modèle utilisé ; actif seulement si installé |
| Modèle | nom, puis une ligne grise : langues · précision des poids |
| Taille | sur le disque si installé, sinon à télécharger |
| Précision | `94 %` ; un « ~ » et une info-bulle « publié » si non mesuré |
| Vitesse | `15×` le temps réel |
| Mémoire | pic mesuré, ou « — » |
| État / action | `Télécharger` · barre + `Annuler` · `Supprimer` · `Mesurer` |

Sous la table Lecture : **Voix** (menu des voix du modèle choisi) et
`▶ Écouter`. Kokoro propose ses 29 voix plus p-fenrir60 ; Qwen3 1.7B
CustomVoice ses 9 locuteurs ; Qwen3 0.6B Base une voix fixe (le clonage
n'est pas dans ce projet) ; Magpie ses 5 voix ; macOS les voix installées,
Premium et Améliorées en tête, avec un lien « Ajouter des voix… » qui ouvre
les Réglages Système (l'app ne peut pas les télécharger).

Voix choisie absente du nouveau modèle (changement de modèle) : la voix
par défaut de ce modèle, sans message.

### Supprimer

- `Supprimer` sur tout modèle installé, y compris celui qui est utilisé.
- Confirmation toujours : « Supprimer Parakeet v3 ? 2,5 Go seront libérés.
  Il faudra le retélécharger pour l'utiliser. » ; si c'est le modèle
  utilisé, ajouter « La dictée passera à celle de macOS. » (ou « La lecture
  passera à la voix de macOS »).
- Un modèle en cours d'usage (lecture qui parle, dictée ouverte) est
  d'abord arrêté puis libéré (`release`), puis ses fichiers effacés.
- Fichiers partagés entre paquets d'une famille (`dest` commun) : effacés
  seulement quand plus aucun paquet installé ne les nomme (règle actuelle
  de `remove_in`).
- Après suppression, la ligne affiche à nouveau la taille à télécharger,
  et les chiffres mesurés sont oubliés.
- Le pied de chaque section : « 4,3 Go utilisés par les modèles · 120 Go
  libres ».

### Accessibilité

Chaque ligne est un élément VoiceOver : « Parakeet v3, utilisé, installé,
2,5 Go, précision 94 % mesurée, 40 fois le temps réel ». Les actions sont
des boutons nommés. Progression annoncée tous les 25 %. Identifiants
d'accessibilité stables pour les tests UI (`modele-<id>`,
`supprimer-<id>`…).

## Performance

### Chiffres publiés

Dans le catalogue, relevés à la main au plan du lot 4, avec leur source :
- dictée : taux d'erreur de mots (WER) publié en français (FLEURS fr ou
  équivalent), affiché en précision = 100 − WER ;
- lecture : rien de fiable sur la précision ; vitesse et mémoire laissées
  vides (« — ») jusqu'à la mesure.

Affichés avec « ~ » et l'info-bulle « Chiffre publié (source), pas encore
mesuré sur ce Mac ».

### Banc local (« Mesurer »)

Enregistrements fournis avec l'app : `assets/bench/fr.wav` + `fr.txt`,
`en.wav` + `en.txt`, ~20 s chacun, enregistrés pour le projet (libres de
droits). Texte de référence normalisé (minuscules, ponctuation retirée,
nombres en lettres) avant comparaison.

- **Dictée** : transcrit les deux enregistrements ; précision = 100 − WER
  (distance d'édition sur les mots) dans la langue de dictée réglée ;
  vitesse = durée audio / durée de calcul ; mémoire = pic de l'empreinte
  du processus pendant la mesure moins l'empreinte avant.
- **Lecture** : synthétise le texte de référence avec la voix choisie ;
  vitesse, mémoire comme ci-dessus ; précision = **intelligibilité par
  aller-retour** : le son est transcrit par le meilleur modèle de dictée
  installé (dictée de macOS à défaut), puis comparé au texte. La colonne
  le dit dans son info-bulle (« mots retrouvés par la dictée »).
- Durée : 10 à 40 s ; une barre et `Annuler`. Une seule mesure à la fois,
  jamais pendant un téléchargement ni pendant la lecture.
- Résultats rangés dans `~/Library/Application Support/sillon/bench.json`,
  par id de paquet, avec la date et la version de l'app ; oubliés à la
  suppression du modèle.

Le banc ne lance **jamais** seul : uniquement sur `Mesurer`.

## Erreurs

- Échec de téléchargement : la ligne garde la barre en rouge, le message
  du cœur, et `Réessayer`.
- Place insuffisante : vérifiée avant de commencer (`free_space`), refus
  avec « Il manque 1,2 Go sur le disque ».
- Échec de chargement d'un modèle choisi : repli sur le modèle système de
  son type, la ligne dit « Ne se charge pas : … » et la section aussi.
- Échec de mesure : la cellule garde l'ancien chiffre, une info-bulle dit
  pourquoi.

## Découpage en lots

1. **Catalogue, table, suppression** : `Kind` et lignes système dans le
   catalogue ; `models(kind)` au FFI ; voix unique (reprise de la voix
   française) ; table Lecture (Kokoro + voix de macOS) et Dictée (macOS) ;
   Télécharger, Annuler, Supprimer avec confirmation, Utiliser ; menu
   Voix (29 voix de Kokoro, voix de macOS). Voix de macOS comme moteur de
   lecture choisi. `[dictation] model`.
2. **Moteurs de dictée** : mlx-audio-swift lié une fois ; Parakeet v3,
   Whisper (tiny, small, large-v3-turbo), Canary 1b-v2, Voxtral Realtime ;
   `MLXDictationEngine`.
3. **Moteurs de lecture** : `TextSpeechCompute` ; Qwen3-TTS 0.6B Base et
   1.7B CustomVoice ; MagpieTTS vendorisé ; suivi des mots estimé.
4. **Performance** : chiffres publiés ; enregistrements de référence ;
   banc local ; colonnes Précision, Vitesse, Mémoire.

Chaque lot se fusionne seul. Un plan par lot, écrit au début du lot.

## Tests

- Rust : catalogue (`kind`, filtres app/terminal, lignes système non
  supprimables), suppression des fichiers partagés, migration voix par
  langue → voix unique, `[dictation] model` absent ou inconnu → `system`.
- Swift unitaires : modèle de la table (états d'une ligne, boutons actifs,
  texte de confirmation), choix du moteur de dictée et repli, WER (cas
  connus), lecture/écriture de `bench.json`.
- Parité : un test par nouveau moteur sur un court extrait (texte attendu
  pour la dictée, longueur et absence de NaN pour la lecture).
- UI : télécharger (paquet factice local), supprimer avec confirmation,
  changer de voix. **Sur la CI seulement** (jamais `xcodebuild test` en
  local : l'app s'ouvre et bloque le Mac).

## Hors périmètre

Terminal ; clonage de voix (Qwen3 Base) ; VoiceDesign ; Voxtral Mini 3B ;
Canary 180m-flash, 1b-flash, canary-qwen ; traduction pendant la dictée ;
téléchargement des voix de macOS depuis l'app (impossible).
