# KokoroMLX

Fichiers Kokoro de [mlx-audio-swift](https://github.com/Blaizzy/mlx-audio-swift)
(licence MIT, Prince Canuma, voir `LICENSE`), commit `244458d` du 10 octobre 2026 :
`Kokoro/KokoroConfig.swift`, `KokoroModules.swift`, `KokoroDecoder.swift`,
`StyleTTS2/SharedConfigs.swift`, `Albert.swift`, `Blocks/*.swift`.

Changements : imports `MLXLMCommon`, `HuggingFace`, `MLXAudioCore` retirés ;
quantification retirée de `KokoroConfig` ; `import MLXFFT` ajouté au décodeur ;
`KokoroModel.swift` réduit au passage avant (`callAsFunction` public) et au
chargement d'un dossier ; `callAsFunction` prend en plus `shouldStop`
(optionnel, `nil` par défaut) et rend donc un optionnel. Points d'arrêt :
juste après `predDur.asArray` (synchronisation déjà là), puis dans
`KokoroDecoder.swift` après `encode`, après chaque bloc de `decode` et après
chaque étage d'upsampling de `KokoroGenerator` (fonction `stopsHere` : MLX
étant paresseux, l'intermédiaire est évalué puis `shouldStop` consulté).
S'il dit vrai, le passage rend `nil`. Sans `shouldStop`, ni évaluation ni
test : le chemin d'origine. `KokoroRunner.swift` est un ajout de
Sillon.

Corrections reprises de mlx-audio Python 0.5.8 (la chaîne du banc
d'écoute) : le bloc upsample de `AdainResBlock1d` ôte la première trame de
la convolution transposée sans marge, au lieu de remplacer la dernière par
un zéro (qui faussait la normalisation du bloc, dans le décodeur et dans le
prédicteur de hauteur) ; les phases initiales des harmoniques de
`SineGenerator` sont uniformes sur [0, 1), non normales. Parité vérifiée
par `KokoroParityTests` (référence : `scripts/kokoro-parite.py`).

Seule dépendance : mlx-swift 0.32.3.

Le détail exact de ces changements : `patches/kokoro-mlx.patch`, à la racine
du dépôt (`diff -ruN`, de l'amont vers ce dossier). Pour le refaire :
télécharger `https://github.com/Blaizzy/mlx-audio-swift/archive/244458d.tar.gz`,
ranger dans un dossier `a/` les fichiers de
`Sources/MLXAudioTTS/Models/StyleTTS2` comme ici (`Kokoro/*.swift` sauf
`KokoroMultilingualProcessor.swift` à plat, `SharedConfigs.swift`,
`Albert.swift`, `Blocks/`), copier les `.swift` de ce dossier dans `b/`, puis
`diff -ruN a b`. `KokoroModules.swift`, `Albert.swift` et six des huit
`Blocks/` sont identiques à l'amont.

## STT : socle, Nemo et Parakeet

Même dépôt, même commit `244458d`, copiés dans `STT/` (un dossier à plat par
modèle) :
- `STT/Generation.swift` ← `Sources/MLXAudioSTT/Generation.swift` ;
- `STT/STTOutput.swift` ← `Sources/MLXAudioSTT/Models/GLMASR/STTOutput.swift` ;
- `STT/DSP.swift` ← `Sources/MLXAudioCore/DSP.swift` ;
- `STT/Nemo/*.swift` ← `Sources/MLXAudioSTT/Models/Nemo/` (5 fichiers) ;
- `STT/Parakeet/*.swift` ← `Sources/MLXAudioSTT/Models/Parakeet/` (7
  fichiers, sans le `README.md`).

Tous identiques à l'amont, sauf :
- `Parakeet/ParakeetAudio.swift` : `import MLXAudioCore` retiré (le code est
  dans ce module) ;
- `Parakeet/ParakeetModel.swift` : `import MLXAudioCore`, `import MLXLMCommon`
  et `import HuggingFace` retirés ; `fromPretrained` (téléchargement depuis
  Hugging Face) supprimé ; `init` interne au lieu de `private` (pour
  Phonon-2, plus bas). Reste `fromDirectory(_:computeDType:)`, qui lit un
  dossier local (`config.json` et tous ses `*.safetensors`).

`STT/BaseConfiguration.swift` est un ajout de Sillon : le sous-ensemble de
`MLXLMCommon.BaseConfiguration` (mlx-swift-lm 3.32.3, MIT, ml-explore — licence jointe : third_party/licences/mlx-swift-lm-LICENSE) que lisent
les modèles copiés, c'est-à-dire la quantification. Même nom, mêmes membres :
aucun site d'appel touché.

Pour vérifier : `diff -r` entre `Sources/MLXAudioSTT/Models/{Nemo,Parakeet}`
de l'archive `244458d` et `STT/{Nemo,Parakeet}` ne montre que les lignes
ci-dessus.

## STT : Phonon-2 (ajout de Sillon)

`STT/Parakeet/PhononLoader.swift` est écrit par Sillon : il charge Phonon-2
(Fermion Research, https://huggingface.co/FermionResearch/Phonon-2, poids
sous CC-BY-4.0, dérivés de Parakeet TDT 0.6B v3 de NVIDIA) dans le graphe
Parakeet ci-dessus. L'archive téléchargée (`phonon-2.bps.tar.zst`) est lue
par `/usr/bin/tar`, sans rien décompresser sur le disque ; le conteneur
`model.fermion` est décodé d'après `fermion_container.py` (Apache-2.0) du
même dépôt, les noms de transformers ramenés à ceux du graphe comme le fait
`map_weight_name` de mlx-audio (`parakeet/redux.py`). Les couches à cinq
valeurs de l'encodeur deviennent deux plans 2 bits affines (exacts) sommés,
les convolutions de noyau 1 des matrices denses.

Seul changement dans les fichiers copiés : l'`init` de `ParakeetModel`
passe de `private` à interne, pour que ce chargeur construise le modèle.

## STT : Canary 1b-v2

Même dépôt, même commit `244458d` :
- `STT/SentencePieceTokenizer.swift` ← `Sources/MLXAudioCore/SentencePieceTokenizer.swift`
  (identique) ;
- `STT/Canary/*.swift` ← `Sources/MLXAudioSTT/Models/Canary/` (2 fichiers).

Changements :
- `Canary/CanaryConfig.swift` : `import MLXLMCommon` retiré ;
- `Canary/CanaryModel.swift` : `import HuggingFace` et `import MLXAudioCore`
  retirés ; `fromPretrained` supprimé ; `fromModelDirectory(_:)` rendu
  synchrone (`throws` au lieu de `async throws` : il n'attendait rien).
  Décodage pas à pas : l'amont repassait toute la suite dans le décodeur à
  chaque jeton ; `generate` passe l'invite une fois puis un jeton à la fois,
  avec un `KVCacheSimple` (`TTS/KVCache.swift`) par bloc pour
  l'auto-attention, et les clés, valeurs et masque de l'attention croisée
  calculés une fois par lecture (`CanaryDecoder.makeState`,
  `callAsFunction(_:state:)`, `CanaryAttention.keysValues`/`attend`). Mêmes
  opérations, dont la dernière position répétée au-delà de la table de
  1 024 ; seul l'ordre des additions diffère (logits égaux à 1e-4 près en
  Float32 : `CanaryDecoderCacheTests`). Le décodeur de l'amont
  (`callAsFunction(_:encoderOutput:encoderMask:startPosition:)`) reste.
- `STT/BaseConfiguration.swift` : `BaseConfiguration`, `Quantization` et
  `PerLayerQuantization` passent `public` (comme dans MLXLMCommon), parce que
  `CanaryConfig`, public, a des champs de ces types. Leurs membres restent
  internes.

Le vocabulaire est lu dans `config.json` (clé `tokenizer.model_base64`, un
modèle SentencePiece), ce que fait déjà le chargeur amont : le dépôt
`Mediform/canary-1b-v2-mlx-q8` se charge tel quel, sans fichier compagnon.

`defaultGenerationParameters` limite la sortie à 200 jetons, trop peu pour une
longue dictée : Sillon appellera `generate` avec `maxTokens: 1024` (la table
de positions du décodeur fait 1 024 ; au-delà, la dernière position est
répétée). `language` (`"fr"`, `"en"`…) sert de langue source et cible : pas de
détection automatique.

## STT : Voxtral Realtime

Même dépôt, même commit `244458d` : `STT/VoxtralRealtime/*.swift` ←
`Sources/MLXAudioSTT/Models/VoxtralRealtime/` (8 fichiers, sans le
`README.md`).

Tous identiques à l'amont, sauf :
- `VoxtralRealtimeAudio.swift`, `VoxtralRealtimeStreamSession.swift` :
  `import MLXAudioCore` retiré ;
- `VoxtralRealtime.swift` : `import MLXAudioCore`, `import MLXAudioVAD`,
  `import MLXLMCommon` et `import HuggingFace` retirés ; la surcharge
  `generate(audio:generationParameters:vad:)` (découpage par détection de
  voix, Silero) supprimée, elle seule employait `MLXAudioVAD` ;
  `fromPretrained` supprimé. Reste `fromDirectory(_:)` (`config.json`,
  `tekken.json` et tous les `*.safetensors` du dossier). La quantification
  passe par la `BaseConfiguration` locale.

La session de flux (`makeStreamSession`, `step`, `finish`, `Delta`) est
gardée telle quelle. Une session ou un modèle à la fois ; après avoir
libéré une session, `Memory.clearCache()`.

## STT : Whisper

Même dépôt, même commit `244458d` : `STT/Whisper/{WhisperAudio,WhisperConfig,
WhisperLayers,WhisperModel}.swift` ← `Sources/MLXAudioSTT/Models/Whisper/`
(sans le `README.md`). `WhisperConfig.swift` et `WhisperLayers.swift` sont
identiques à l'amont.

Changements :
- `WhisperAudio.swift` : `import MLXAudioCore` retiré ; `MLXAudioCore.melFilters(`
  devient `KokoroMLX.melFilters(` (la méthode statique `melFilters(nMels:)`
  masque la fonction globale de `STT/DSP.swift`, d'où la qualification) ;
- `WhisperModel.swift` : `import HuggingFace` et `import MLXAudioCore`
  retirés ; `fromPretrained` et `downloadTokenizerAssets` (téléchargement du
  vocabulaire depuis openai/whisper-*) supprimés ; `fromDirectory(_:)` rendu
  synchrone, sans `cache`. Il exige `config.json` et `multilingual.tiktoken`
  (sinon `STTError.modelNotInitialized`), refuse un `n_vocab` autre que 51865
  ou 51866 (`STTError.invalidInput` : les modèles `.en` ont une autre table),
  lit le vocabulaire avant les poids, et, sans `generation_config.json` (cas
  des dépôts mlx-community), prend les ids spéciaux et `suppress_tokens` de
  `WhisperSpecialTokens`. `init(config:generationConfig:)` passe interne : seul
  `fromDirectory` fabrique un modèle, toujours avec son vocabulaire (le
  `fatalError` de `transcribeChunk` devient inatteignable). En flux, le texte
  partiel est décodé avec `dropIncompleteTail` (pas de U+FFFD transitoire au
  milieu d'un caractère réparti sur deux jetons). Dans `transcribeChunk`, les
  masques des jetons interdits (`suppressLogits`, `suppressFromIndex`)
  sont construits une fois avant la boucle (`suppressMask`,
  `suppressFromIndexMask`) au lieu d'à chaque jeton, et ajoutés dans le
  même ordre : mêmes logits au bit près.
- `WhisperTokenizer.swift` : réécrit, sans swift-transformers. Même API pour
  `WhisperModel` (`buildPromptTokens`, `resolveLanguage`, `decode(tokens:)`) ;
  la lecture de `tokenizer_config.json` / `added_tokens_decoder` est retirée.

Ajout de Sillon : `STT/Whisper/TiktokenDecoder.swift`, avec
- `TiktokenDecoder` : lit `multilingual.tiktoken` (« base64 rang », rangs
  contigus ; la ligne `= 50256` est un jeton vide) et ne fait que ids → texte
  (octets concaténés puis UTF-8). Tout id hors de la table est un jeton
  spécial : omis avec `skipSpecial`, écrit `<|id|>` sinon ;
- `WhisperSpecialTokens` : les ids spéciaux de Whisper multilingue déduits de
  `n_vocab` (99 langues pour 51865, 100 pour 51866 avec `yue`), vérifiés contre
  les `generation_config.json` d'openai/whisper-tiny et
  openai/whisper-large-v3-turbo. Pour turbo, tout ce qui suit les langues
  (tâche, `notimestamps`, horodatages) est décalé de +1.

Le décodeur HF pouvait retirer l'espace avant la ponctuation
(`clean_up_tokenization_spaces`) ; tiktoken, la référence d'OpenAI, ne nettoie
rien, et nous non plus. Toujours passer `language` (`"fr"`, `"en"`) : aucune
détection de langue n'est codée.

## TTS : Qwen3-TTS

Même dépôt, même commit `244458d` : `TTS/Qwen3TTS/{Qwen3TTS,Qwen3TTSTalker,
Qwen3TTSCodePredictor,Qwen3TTSConfig,Qwen3TTSSpeechTokenizer}.swift` ←
`Sources/MLXAudioTTS/Models/Qwen3TTS/`. `Qwen3TTSSpeakerEncoder.swift` n'est
pas copié : le x-vector de la voix française est précalculé
(`assets/voices/qwen3-06b-siwis.safetensors`, Tâche 3), et le clonage est
hors périmètre. `TTS/Qwen3TTS/EuclideanCodebook.swift` ←
`Sources/MLXAudioCodecs/Mimi/Quantization.swift`, lignes 1-54 (imports et la
seule classe `EuclideanCodebook`), identique ; rien d'autre de Mimi (son
encodeur tire `MLXLMCommon` et `MLXAudioCore`, et `VectorQuantization`,
`SplitResidualVectorQuantizer` y existent sous les mêmes noms que dans
`Qwen3TTSSpeechTokenizer.swift` : un seul module ici).

`TTS/KVCache.swift` dérive de mlx-swift-lm 3.32.3
(`Libraries/MLXLMCommon/KVCache.swift`, MIT, ml-explore — licence jointe :
third_party/licences/mlx-swift-lm-LICENSE) : un protocole `KVCache` réduit à
ce que lit Qwen3-TTS (`offset`, `update(keys:values:)`, `trim(_:)`), et
`KVCacheSimple` recopié des lignes 408-497 et 552-554 (`debugDescription`),
même allocation par pas de 256, sans `BaseKVCache`, sans l'état sérialisable
(`innerState`, `state`, `isTrimmable`) ni la conversion en cache quantifié ;
`offset` y est déclaré (il venait de `BaseKVCache`).

Changements, fichier par fichier :
- `Qwen3TTSTalker.swift`, `Qwen3TTSCodePredictor.swift` : `import MLXLMCommon`
  retiré (le `KVCache` local suffit) ; rien d'autre.
- `Qwen3TTSConfig.swift` : `import MLXLMCommon` retiré ; `rope_scaling` décodé
  en une petite structure `Qwen3RopeScaling` (seul `mrope_section` est lu,
  par `Qwen3TTSTalker.swift:272`) au lieu de `[String: StringOrNumber]`, dans
  la config du talker et celle du prédicteur de codes ; mêmes valeurs dans
  les deux dépôts (`[24, 20, 20]`, `null` pour le prédicteur).
  `BaseConfiguration.Quantization` / `.PerLayerQuantization` : ceux de
  `STT/BaseConfiguration.swift`.
- `Qwen3TTSSpeechTokenizer.swift` : `import MLXLMCommon` et
  `import MLXAudioCodecs` retirés ; l'encodeur du codec
  (`Qwen3TTSSpeechTokenizerEncoder`, ≈ 95 lignes) supprimé, avec la
  propriété `encoderModel` et `encode(_:)` du wrapper ; `hasEncoder` rend
  `false`. Dans `sanitize`, l'appel à
  `Qwen3TTSSpeakerEncoder.stripSpeakerEncoderPrefix(from:)` est remplacé par
  le même test en ligne (une composante `speaker_encoder` suivie d'au moins
  une autre). `-> sending` et `nonisolated(unsafe)` de `streamingStep`
  compilent en Swift 5 : gardés.
- `Qwen3TTS.swift`, taillé (1 373 → ≈ 600 lignes) :
  - imports `HuggingFace`, `MLXAudioCore`, `MLXLMCommon`, `Tokenizers`
    retirés ; conformité `SpeechGenerationModel`,
    `defaultGenerationParameters`, `generate(…)`, `generateStream(…)`,
    `makeGenerationStream`, `resolveVoiceDesignGenerationSettings`,
    `requireGenerationComponents`, `AudioGenerationInfo`, `SendingBox` et
    tout `GenerateParameters` supprimés ; `fromPretrained` supprimé ;
  - le clonage supprimé : `prepareReferenceConditioning`,
    `prepareICLGenerationInputs`, `Qwen3TTSReferenceConditioning`,
    `extractSpeakerEmbedding`, `speakerEncoder`, le cache de référence et
    son verrou ;
  - tous les `print` supprimés ;
  - `AudioGenerationError` remplacé par `Qwen3TTSError` (public :
    `missingFile`, `modelNotInitialized`, `invalidInput`, `unknownVoice`,
    `stopped`) ;
  - `var tokenizer: Qwen2BPETokenizer?` (notre encodeur, Tâche 4) au lieu de
    swift-transformers. Pré-découpe : chiffres un à un (`\p{N}`, comme
    `transformers.Qwen2Tokenizer`), pas le `\p{N}{1,3}` du `tokenizer.json`
    que générait l'amont ;
  - la phrase lue est encodée avec `allowSpecial: false` : un
    `<|im_end|>`, `<|endoftext|>` ou `<think>` écrit dans un livre est lu
    comme du texte, jamais pris pour un jeton de contrôle ; le gabarit
    ChatML garde ses jetons. Le gabarit est encodé en trois morceaux
    (`<|im_start|>`, `assistant\n` + phrase, `<|im_end|>\n<|im_start|>assistant\n`),
    qui donnent les mêmes ids que l'amont pour une phrase sans ces chaînes ;
  - `fromModelDirectory(_:)` rendu synchrone (`throws`). Il ne lit que
    `model.safetensors` (l'amont : tous les `*.safetensors` du dossier, un
    fichier égaré s'y mêlait), vérifie **avant** de lire les poids que
    `speech_tokenizer` est un dossier, sinon
    `Qwen3TTSError.missingFile("speech_tokenizer/")`. **Supprimé :** la
    branche qui effaçait le dossier du modèle (`try? fm.removeItem(at:
    modelDir)`) et la génération de `tokenizer.json` dans le dossier
    (`generateTokenizerJson`). Le chargeur n'écrit, ne modifie ni n'efface
    rien : le catalogue Rust est seul maître des fichiers ;
  - après le chargement du talker, l'embedding texte (non quantifié dans les
    dépôts, 622 Mo en BF16 pour le 1.7B) passe en 8 bits
    (`quantize(… groupSize: 64, bits: 8, mode: .affine)` filtré sur
    `text_embedding`), comme « allégé » au banc d'écoute ;
    `QuantizedEmbedding` hérite d'`Embedding` en MLXNN 0.32.3, donc
    `getTextEmbeddings()` reste valable ;
  - pic du chargement : les dictionnaires de poids du talker (`allWeights`,
    `talkerWeights`, `talkerPairs`, dont les tenseurs `speaker_encoder.*`)
    vivent dans un bloc `do { … }` refermé **avant** la quantification de
    l'embedding texte (la copie BF16 n'est plus tenue à côté de la copie
    8 bits), et `Memory.clearCache()` est appelé à la fin de
    `fromModelDirectory` (tampons BF16, `speaker_encoder.*` et encodeur du
    codec rendus) ;
  - `loadSpeechTokenizer` : lit `speech_tokenizer/config.json` (exigé : plus
    de repli sur des valeurs par défaut) et `speech_tokenizer/model.safetensors`
    seul ; les poids de l'encodeur (`encoder.*` bruts, `encoder_model.*`
    nettoyés, 225 tenseurs, 225 Mo en F32) sont écartés avant
    `update(verify: .all)` ;
  - `prepareGenerationInputs` : paramètre `speakerEmbedding: MLXArray?` (mode
    x-vector de l'officiel, `x_vector_only`) : sans locuteur nommé, le
    x-vector prend la place de l'embedding `spk_id`. Un locuteur CustomVoice
    inconnu lève `Qwen3TTSError.unknownVoice` (l'amont avertissait et parlait
    sans locuteur). Rend `throws` au lieu de `fatalError` ;
  - **correction du dialecte** : `effectiveLanguage(requested:dialect:)` ;
    le dialecte d'Eric (sichuanais) ou de Dylan (pékinois) ne remplace la
    langue que si elle est `chinese` ou `auto`, comme l'officiel
    (`modeling.py:2118-2122`). L'amont le faisait toujours : Eric lisait le
    français avec l'id de langue d'un dialecte chinois. Test :
    `Qwen3TTSRunnerTests.testADialectSpeakerKeepsTheRequestedLanguage` ;
  - `generateVoiceDesign` : sans `conditioning`, `refAudio`, `refText`,
    `streamingInterval`, `onToken`, `onInfo`, `onAudioChunk` ni le décodage
    en flux ; la phrase entière est décodée par `streamingDecode` par
    paquets de 300 trames (`decodeChunk`, inchangé). Ajouts :
    `speakerEmbedding` (passé à `prepareGenerationInputs`) et
    `shouldStop: (() -> Bool)?`. Les trois `try Task.checkCancellation()`
    deviennent : dans la boucle, toutes les 12 trames (≈ 1 s de son),
    `shouldStop` ; après la boucle, une fois avant le décodage. S'il dit
    vrai : `Qwen3TTSError.stopped`. Le `Memory.clearCache()` tous les 50
    pas est retiré (le cache de MLX est plafonné par `MLXBudget` dans
    l'app : le vider rendait puis réallouait les mêmes tampons) ;
  - `Qwen3TTSSpeechTokenizer.swift`, `streamingDecode` : même raison, plus
    de `Memory.clearCache()` après chaque paquet.

Ajouts de Sillon, hors des fichiers repris : `TTS/Qwen3TTS/Qwen2BPETokenizer.swift`
(Tâche 4 ; ici, `encode(text:allowSpecial:)`), `TTS/StableSeed.swift` (une
graine par phrase, FNV-1a 64 bits sur l'UTF-8 NFC, stable d'un lancement à
l'autre) et `TTS/Qwen3TTSRunner.swift` (la porte : vérifie les fichiers avant
toute lecture, charge, choisit la voix — `siwis` pour Base, avec le x-vector
livré dans le framework ; les 9 locuteurs de `spk_id` pour CustomVoice —,
sème `MLXRandom` avec `StableSeed`, synthétise avec les réglages de
`generation_config.json` : température 0,9, top-k 50, top-p 1, pénalité
1,05). Une synthèse à la fois, sous `MLXGate` dans l'app.

Le détail exact : `patches/qwen3tts-mlx.patch` (`diff -ruN`, de l'amont vers
`TTS/Qwen3TTS/`). Pour le refaire : télécharger
`https://github.com/Blaizzy/mlx-audio-swift/archive/244458d.tar.gz`, copier
dans `a/` les cinq fichiers ci-dessus de `Sources/MLXAudioTTS/Models/Qwen3TTS`
et, sous le nom `EuclideanCodebook.swift`, les lignes 1-54 de
`Sources/MLXAudioCodecs/Mimi/Quantization.swift` ; copier les `.swift` de
`TTS/Qwen3TTS/` dans `b/`, puis `diff -ruN a b`. `EuclideanCodebook.swift`
est identique ; `Qwen2BPETokenizer.swift` y apparaît en entier (ajout).

## TTS : MagpieTTS

Fichiers de [speech-swift](https://github.com/soniqo/speech-swift) (licence
Apache-2.0, soniqo, jointe : `third_party/licences/speech-swift-LICENSE` ;
pas de `NOTICE` à la racine du dépôt), dossier `Sources/MagpieTTS`, au
commit `28aef99510e438bcc5014df2582820d9fdba80e0` (11 juillet 2026, le
dernier qui touche ce dossier ; HEAD du clone : `d74459f`, même contenu).
Copiés dans `TTS/Magpie/` : `Configuration.swift`, `Transformer.swift`,
`TextEncoder.swift`, `Decoder.swift`, `LocalTransformer.swift`,
`NanoCodec.swift`, `Sampling.swift`, `WeightLoader.swift`, `SubVocab.swift`,
`ByT5Encoder.swift`, `Tokenizer.swift`, `MagpieTTS.swift`, `EnglishG2P.swift`,
et `Resources/cmudict_ipa_en.txt` (3,1 Mo, inchangé, livré dans le
framework). Non copiés : `Downloader.swift`, `ChineseG2P.swift`,
`JapaneseG2P.swift`, les dictionnaires espagnol, allemand, pinyin et
`heteronyms_en.txt`.

Le dictionnaire anglais vient de NVIDIA NeMo, dérivé de CMUdict 0.07
(Carnegie Mellon University, licence de type BSD), avec des entrées de
NeMo et de WikipediaHomographData (Apache 2.0) : son en-tête de licence
(les lignes `;;;` du début) est joint, sans le préfixe, dans
`third_party/licences/cmudict-LICENSE`.

Seule dépendance : mlx-swift 0.32.3 (`MLX`, `MLXNN`) ; rien d'ajouté aux
`dependencies` de `KokoroMLX`. Aucun nom public ne collide avec le reste du
framework : tout est préfixé `Magpie…`, sauf les trois fonctions passées en
`internal` ci-dessous.

Changements, fichier par fichier :
- `Sampling.swift` : `import MLXRandom` retiré (`MLXRandom` est un espace de
  noms de `MLX` en 0.32.3) ; `topKFilter`, `forbidIds`, `sampleTopK`
  passent de `public` à `internal` (fonctions globales aux noms génériques,
  à ne pas exporter du framework).
- `Configuration.swift` : `MagpieTTSVariant` (dépôt HF et nombre de bits,
  pour le téléchargeur) supprimé ; `MagpieTTSError` gagne `case stopped`
  (« Magpie : synthèse arrêtée »).
- `MagpieTTS.swift` : `import MLXRandom` et `import AudioCommon` retirés ;
  `fromPretrained` (téléchargeur) supprimé ; `load(from paths:
  MagpieTTSDownloader.Paths)` devient `load(directory: URL)`, qui lit
  `text_encoder/`, `decoder_prefill/`, `nanocodec_decoder/` et `tokenizer/`
  sous le dossier (le catalogue Rust télécharge ces fichiers) ;
  `synthesizeStream` et `runStreaming` supprimés (le flux rejoue le codec sur
  tout le tampon à chaque morceau : RTF 0,93 au lieu de 0,24) ; le bloc
  `MAGPIE_DEBUG` (un `print` des 5 premières trames) supprimé ;
  `synthesize(…)` et `sampleCodeSequence(…)` prennent
  `shouldStop: (() -> Bool)? = nil`, consulté toutes les 20 trames (≈ 1 s de
  son) dans la boucle, et une fois avant le codec ; s'il dit vrai :
  `MagpieTTSError.stopped`. Deux barrières dans `sampleCodeSequence` : plus
  de jetons que le `max_len` de l'encodeur de texte (2 048) lève
  `MagpieTTSError.textEncodingFailed` (« phrase trop longue pour Magpie »)
  au lieu de lire la table des positions hors limites (comportement
  indéfini dans MLX) ; les pas sont bornés à `max_len − baked_T − 1` (même
  raison côté décodeur), quel que soit `params.maxSteps`.
- `Tokenizer.swift` : `import NaturalLanguage` (inutilisé) retiré ; le
  français passe par `MagpieRunner.prepare` (NFC) avant `lowercased()` et
  byT5 : un « é » décomposé (PDF) aurait donné deux octets inattendus ;
  espagnol, allemand (dictionnaires non copiés), chinois et japonais (G2P
  non copiés) tombent sur `charLookup(text)`, comme le hindi ;
  `lookupGroups` et `lookupFlatTokens`, qui ne servaient qu'au chinois et au
  japonais, supprimés. Sillon ne lit Magpie qu'en français et en anglais.
  Deux commentaires corrigés : celui du japonais (plus de translittération)
  et celui de l'anglais (le code fait NFKC, pas le NFD de NeMo).
- `EnglishG2P.swift` : `Bundle.module` (SPM seulement) →
  `Bundle(for: MagpieRunner.self)` (le framework) ; `spanish`, `german`
  supprimés ; `MagpieDictG2P` déclaré `@unchecked Sendable` (le dictionnaire
  est écrit une fois sous `NSLock`, puis seulement lu) pour ses `static let`.

Ajout de Sillon, hors des fichiers repris : `TTS/MagpieRunner.swift` (la
porte : voix par id du catalogue, `sofia` si vide ; `en…` → anglais, sinon
français ; NFC ; graine `StableSeed` ; borne de trames `maxSteps` = 4 par
octet de texte + 50, sans dépasser `max_len − baked_T − 1`, au lieu des 500
trames ≈ 23 s de l'amont, qui tronquaient une phrase longue ;
échantillonnage d'origine, température 0,6, top-k 80 ; `nil` si arrêtée).
Une synthèse à la fois, sous `MLXGate` dans l'app (cache des vocabulaires et
graine de MLX partagés).

**Garantie de longueur** (`MagpieRunner`) : une phrase de plus de
`maxChunkBytes` octets (471 pour ce paquet : `(max_len − baked_T − 1 − 50)
/ 4`) est découpée par `chunks` après une ponctuation forte, sinon faible,
sinon à une espace (en dernier recours entre deux caractères, jamais dans
un octet UTF-8), et les morceaux sont synthétisés puis mis bout à bout,
`shouldStop` consulté entre deux. Ainsi la borne de trames d'un morceau
(4 × octets + 50) reste sous le contexte du décodeur : aucun morceau n'est
tronqué par le plafond, et l'encodeur de texte ne voit jamais plus de
2 048 jetons. Tests : `MagpieRunnerTests` (texte français de 3 000 octets).

`MagpieRunner.load` lit d'abord le dictionnaire anglais
(`MagpieDictG2P.english.ensureLoaded()`) : absent, `missingFile` au
chargement, au lieu d'un anglais lu lettre à lettre en silence
(`phonemize` rend le texte brut si le dictionnaire manque).

**Mémoire :** les poids 8 bits sont déquantifiés en FP32 au chargement
(`WeightLoader.swift`), ≈ 1,6 Go en RAM pour 357 M de paramètres (doc de
speech-swift). Dans `loadAndDequantize`, le dictionnaire lu est vidé dès
les triplets formés, et chaque triplet lâché sitôt sa version FP32 faite :
le pic n'additionne plus tout le fichier 8 bits à tout le FP32. Précision
inchangée.

**Une lecture GPU par trame** (`MagpieTTS.swift`) : dans
`sampleLocalTransformer`, les jetons des huit livres de codes restent des
`MLXArray` (le jeton tiré sert directement à l'embedding du livre suivant)
et sont lus ensemble à la fin (`concatenated(…).asArray`), au lieu d'un
`eval` + `asArray` par livre ; l'`eval(hLast)` après chaque pas du décodeur
est retiré (il est calculé avec la trame suivante). Mêmes opérations, même
ordre des tirages : mêmes codes.

Chaque fichier modifié porte en tête la mention exigée par l'Apache-2.0
(§4(b)) : « Modifié par Sillon … — voir ORIGINE.md ».

Le détail exact : `patches/magpie-mlx.patch` (`diff -ruN`, de l'amont vers
`TTS/Magpie/`). Pour le refaire : télécharger
`https://github.com/soniqo/speech-swift/archive/28aef99510e438bcc5014df2582820d9fdba80e0.tar.gz`,
copier dans `a/` les treize `.swift` ci-dessus de `Sources/MagpieTTS`, copier
ceux de `TTS/Magpie/` dans `b/`, puis `diff -ruN a b`. Le dictionnaire est
identique (`cmp`).
