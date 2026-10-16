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
  Hugging Face) supprimé. Reste `fromDirectory(_:computeDType:)`, qui lit un
  dossier local (`config.json` et tous ses `*.safetensors`).

`STT/BaseConfiguration.swift` est un ajout de Sillon : le sous-ensemble de
`MLXLMCommon.BaseConfiguration` (mlx-swift-lm 3.32.3, MIT, Apple) que lisent
les modèles copiés, c'est-à-dire la quantification. Même nom, mêmes membres :
aucun site d'appel touché.

Pour vérifier : `diff -r` entre `Sources/MLXAudioSTT/Models/{Nemo,Parakeet}`
de l'archive `244458d` et `STT/{Nemo,Parakeet}` ne montre que les lignes
ci-dessus.

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
  milieu d'un caractère réparti sur deux jetons).
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
