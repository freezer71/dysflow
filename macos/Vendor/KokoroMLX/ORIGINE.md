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
