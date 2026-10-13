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
Sillon. Seule dépendance : mlx-swift 0.32.3.
