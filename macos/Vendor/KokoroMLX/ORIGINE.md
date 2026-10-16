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
