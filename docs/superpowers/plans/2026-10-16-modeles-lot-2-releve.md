# Relevé lot 2 : vendoriser Parakeet TDT v3, Canary 1b-v2, Voxtral Realtime et Whisper dans `KokoroMLX`

Source : clone `mlx-audio-swift` au commit `244458d` (`R = …/research-mlx-audio/repo`).
Son `Package.resolved` fixe mlx-swift **0.32.3** et mlx-swift-lm **3.32.3**, les mêmes versions que Sillon.
Rien n'a été compilé ni exécuté. Les fichiers HF téléchargés sont dans `…/cf439749/tmp/hf/`.

---

## 1. Fichiers à copier, et ce qu'il faut remplacer

### 1.a Liste exacte (chemins relatifs à `R/Sources/`)

**Socle commun : 1 156 lignes**

| Fichier | Lignes | Pour |
|---|---|---|
| `MLXAudioSTT/Generation.swift` | 80 | `STTGenerateParameters`, protocole `STTGenerationModel` |
| `MLXAudioSTT/Models/GLMASR/STTOutput.swift` | 152 | `STTGeneration`, `STTGenerationInfo`, `STTError`, `STTOutput` |
| `MLXAudioCore/DSP.swift` | 282 | `hanningWindow`, `hammingWindow`, `MelScale`, `PadMode`, `melFilters`, `stft` (Parakeet, Canary via Parakeet, Whisper, Voxtral) |
| `MLXAudioCore/SentencePieceTokenizer.swift` | 642 | Canary seulement. Autonome : `TokenizerError` et `SentencePieceModelError` y sont définis |

À ne **pas** prendre : `ModelUtils.swift` (HF), `AudioUtils.swift` (AVFoundation), `GenerationTypes.swift` (inutilisé), `Streaming/*`, `MLXAudioSTT.swift` (fabrique HF).

**Nemo, commun à Parakeet et Canary : 858 lignes.** Tous ces fichiers sont requis, via les alias de `ParakeetNemoAliases.swift`.
- `MLXAudioSTT/Models/Nemo/NemoAlignment.swift` 301
- `NemoAttention.swift` 192
- `NemoDecodingLogic.swift` 110
- `NemoRNNTConfig.swift` 92
- `NemoRNNTLayers.swift` 163

**Parakeet : 2 200 lignes.** Tout le dossier `MLXAudioSTT/Models/Parakeet/` :
- `ParakeetAudio.swift` 125
- `ParakeetConfig.swift` 500
- `ParakeetConformer.swift` 286
- `ParakeetCTCLayers.swift` 27
- `ParakeetModel.swift` 1 206
- `ParakeetNemoAliases.swift` 33
- `ParakeetTokenizer.swift` 23

**Canary : 1 137 lignes, plus Parakeet et Nemo en entier.** Canary dépend de Parakeet :
- `ParakeetAudio` est appelé en `CanaryModel.swift:456` ;
- `ParakeetConformer` en `:332` ;
- `ParakeetConformerConfig` en `CanaryConfig.swift:177` ;
- `ParakeetPreprocessConfig` en `CanaryConfig.swift:72`.

Fichiers propres à Canary :
- `MLXAudioSTT/Models/Canary/CanaryConfig.swift` 373
- `CanaryModel.swift` 764

**Whisper : 1 526 lignes.** Dossier `MLXAudioSTT/Models/Whisper/` :
- `WhisperAudio.swift` 120
- `WhisperConfig.swift` 194
- `WhisperLayers.swift` 342
- `WhisperModel.swift` 678
- `WhisperTokenizer.swift` 192 : à **réécrire**, voir §4

**Voxtral Realtime : 2 674 lignes.** Dossier `MLXAudioSTT/Models/VoxtralRealtime/` :
- `VoxtralRealtime.swift` 657
- `VoxtralRealtimeAudio.swift` 199
- `VoxtralRealtimeConfig.swift` 284
- `VoxtralRealtimeDecoder.swift` 428
- `VoxtralRealtimeEncoder.swift` 502
- `VoxtralRealtimeStreamSession.swift` 419
- `VoxtralRealtimeTokenizer.swift` 101 : tekken.json lu à la main, aucune dépendance
- `VoxtralRealtimeTranscriptText.swift` 84

**Total ≈ 9 550 lignes.** Les quatre modèles et le socle ne dépendent d'aucun autre fichier de `MLXAudioSTT`. Vérifié par croisement des symboles définis hors de cet ensemble : aucune référence.

**Pas de collision de noms avec `Vendor/KokoroMLX`.** Les fonctions globales de Kokoro sont `kokoroHanning`, `kokoroStft`, `stopsHere`, `interpolate1d`, `weightNorm`. Aucune extension non privée n'est en conflit.

### 1.b Modules mlx-swift

Les produits de mlx-swift 0.32.3 (`Package.swift:330-336`) sont `MLX`, `MLXRandom`, `MLXNN`, `MLXOptimizers`, `MLXFFT`, `MLXLinalg`, `MLXFast`.

En 0.32.3, `MLXFast`, `MLXFFT` et `MLXRandom` sont **aussi des espaces de noms dans `MLX`** :
- `public enum MLXFast` en `Source/MLX/MLXFast.swift:5` ;
- `enum MLXFFT` en `Source/MLX/FFT.swift:32` ;
- `enum MLXRandom` en `Source/MLX/Random.swift:87` ;
- les fonctions libres `categorical` en `Random.swift:1289` et `asStrided` en `Ops.swift:367`.

Les fichiers à copier n'importent que `MLX`, `MLXNN` et `MLXFast`. Les seuls `import MLXFast` sont `CanaryModel.swift:5` et `WhisperLayers.swift:3`. Ce produit est déjà lié au framework : **aucun nouveau produit à ajouter** dans `project.yml`.

`MLX.loadArrays(url:)` est en `IO.swift:125`, `QuantizationMode` (Codable) en `Ops.swift:1100`, `Memory.peakMemory` et `Memory.clearCache()` sont dans `MLX`.

### 1.c Symboles externes à remplacer

**`import MLXAudioCore`** : à supprimer (le code vit dans le même module).
- `ParakeetAudio.swift:3`, `ParakeetModel.swift:4`, `CanaryModel.swift:4`, `WhisperAudio.swift:3`, `WhisperModel.swift:5`, `VoxtralRealtime.swift:4`, `VoxtralRealtimeAudio.swift:3`, `VoxtralRealtimeStreamSession.swift:3`.
- **Piège à `WhisperAudio.swift:23`** : `MLXAudioCore.melFilters(` est qualifié par le module, parce que la méthode statique `melFilters(nMels:)` (`:19`) masque la fonction globale. Le remplacer par `KokoroMLX.melFilters(`, ou renommer la méthode statique.
- `VoxtralRealtimeAudio.swift:11` et `ParakeetAudio.swift:35` appellent `melFilters(` sans qualification, et aucun membre homonyme ne la masque : rien à changer.

**`import MLXLMCommon`** : `ParakeetModel.swift:5`, `CanaryConfig.swift:2`, `VoxtralRealtime.swift:6`. Le seul symbole utilisé est `BaseConfiguration` :

| Fichier:ligne | Usage |
|---|---|
| `ParakeetModel.swift:1167` | `let perLayerQuantization: BaseConfiguration.PerLayerQuantization?` |
| `ParakeetModel.swift:1173` | `try? BaseConfiguration(from: decoder)` (échoue toujours : pas de `model_type` dans les configs NeMo, repli à `:1178-1196`) |
| `ParakeetModel.swift:1190-1191` | `BaseConfiguration.PerLayerQuantization(quantization: BaseConfiguration.Quantization(groupSize:bits:), perLayerQuantization: [:])` |
| `ParakeetModel.swift:1029` | `perLayerQuant.quantization(layer: path)?.asTuple` |
| `CanaryConfig.swift:267-268, 310-311` | types des champs `quantization` / `perLayerQuantization` |
| `CanaryConfig.swift:331` | `try? BaseConfiguration(from: decoder)` |
| `CanaryConfig.swift:353-354` | `decodeIfPresent(BaseConfiguration.Quantization.self, forKey: .quantization / .quantizationConfig)` |
| `CanaryConfig.swift:370-371` | `BaseConfiguration.Quantization(groupSize:bits:)` |
| `CanaryModel.swift:523-526` | `.quantization(layer:)`, `.asTuple` |
| `VoxtralRealtime.swift:651, 654` | `BaseConfiguration.PerLayerQuantization?`, `try? BaseConfiguration(from: decoder)` |
| `VoxtralRealtime.swift:527` | `.quantization(layer:)?.asTuple` |

**`quantize(model:…)`** se trouve dans **MLXNN 0.32.3** (`Source/MLXNN/Quantized.swift`) :
- `:114`, `quantize(model:filter:(String, Module) -> (groupSize: Int, bits: Int, mode: QuantizationMode)?, apply:)` : utilisé en `ParakeetModel.swift:1027`, `CanaryModel.swift:518` et `VoxtralRealtime.swift:522` ;
- `:65`, `quantize(model:groupSize:bits:mode:filter:(String, Module) -> Bool, apply:)` : utilisé en `WhisperModel.swift:503`.

Rien à remplacer. Ces appels se résolvent sur MLXNN, pas sur MLXLMCommon.

**Remplacement minimal proposé** : un fichier `QuantizationConfig.swift` qui recopie le sous-ensemble utile de `MLXLMCommon/BaseConfiguration.swift` (mlx-swift-lm 3.32.3, 227 lignes, MIT Apple), en gardant le même nom, ce qui évite de toucher les sites d'appel :

```swift
import Foundation
import MLX

/// Sous-ensemble de MLXLMCommon.BaseConfiguration (mlx-swift-lm 3.32.3, MIT) :
/// seule la quantification est lue.
struct BaseConfiguration: Decodable, Sendable {
    let modelType: String
    struct Quantization: Codable, Sendable, Equatable {
        init(groupSize: Int, bits: Int) { self.groupSize = groupSize; self.bits = bits }
        let groupSize: Int
        let bits: Int
        private var _mode: QuantizationMode? = nil
        var mode: QuantizationMode { _mode ?? .affine }
        var asTuple: (Int, Int, QuantizationMode) { (groupSize, bits, mode) }
        enum CodingKeys: String, CodingKey { case groupSize = "group_size", bits, _mode = "mode" }
    }
    enum QuantizationOption: Sendable { case skip; case quantize(Quantization) }
    struct PerLayerQuantization: Sendable {
        var quantization: Quantization? = nil
        var perLayerQuantization: [String: QuantizationOption]
        init(quantization: Quantization? = nil, perLayerQuantization: [String: QuantizationOption]) {
            self.quantization = quantization; self.perLayerQuantization = perLayerQuantization
        }
        func quantization(layer: String) -> Quantization? {
            switch perLayerQuantization[layer] {
            case .skip: return nil
            case .quantize(let q): return q
            case nil: return quantization
            }
        }
    }
    private struct Container: Decodable {
        var perLayer: PerLayerQuantization
        struct Key: CodingKey {
            let stringValue: String; let intValue: Int? = nil
            init(stringValue: String) { self.stringValue = stringValue }
            init?(intValue: Int) { return nil }
        }
        init(from decoder: Decoder) throws {
            let q = try Quantization(from: decoder)
            var map: [String: QuantizationOption] = [:]
            let c = try decoder.container(keyedBy: Key.self)
            for key in c.allKeys {
                switch key.stringValue {
                case "group_size", "bits", "mode", "quant_method", "linear_class", "quantization_mode": continue
                default:
                    if let f = try? c.decode(Bool.self, forKey: key) { if !f { map[key.stringValue] = .skip } }
                    else { map[key.stringValue] = .quantize(try c.decode(Quantization.self, forKey: key)) }
                }
            }
            perLayer = PerLayerQuantization(quantization: q, perLayerQuantization: map)
        }
    }
    private let container: Container?
    var perLayerQuantization: PerLayerQuantization? { container?.perLayer }
    enum CodingKeys: String, CodingKey { case modelType = "model_type", container = "quantization" }
}
```

Ce code reproduit le comportement amont dans les trois cas réels :
- **Voxtral** : `model_type` est présent, `quantization: {group_size 64, bits 4, mode affine}`.
- **Parakeet** : `model_type` est absent, donc repli à `ParakeetModel.swift:1178`.
- **Canary Mediform** : `model_type` absent, `quantization: {bits: 8}` sans `group_size`, donc repli `CanaryFlatQuantization` (groupe 64). Groupe 64 vérifié sur l'en-tête safetensors : poids `[4096, 256]` U32, `scales` `[4096, 16]`, donc 1 024 entrées et des groupes de 64.

Variante plus petite : se passer de `BaseConfiguration` et ne garder qu'une `struct Quantization` plate, puisqu'aucun des dépôts visés n'a de quantification par couche. Le modèle Voxtral en a besoin dans le principe, pas avec la config mlx-community actuelle. Je recommande la copie fidèle ci-dessus : ~70 lignes, aucun site d'appel touché.

**`import HuggingFace`** : `ParakeetModel.swift:6`, `CanaryModel.swift:2`, `WhisperModel.swift:2`, `VoxtralRealtime.swift:7`. Les fonctions de téléchargement sont à **supprimer entières** :

| Fichier | Lignes à retirer | Contenu |
|---|---|---|
| `ParakeetModel.swift` | 1057-1080 | `fromPretrained` (`HubCache`, `Repo.ID`, `ModelUtils.resolveOrDownloadModel`) |
| `CanaryModel.swift` | 488-508 | `fromPretrained` |
| `WhisperModel.swift` | 566-639 et 641-663 | `downloadTokenizerAssets` (`HubClient`), `fromPretrained` ; paramètre `cache: HubCache = .default` en `:484` et appel en `:543-546` |
| `VoxtralRealtime.swift` | 540-559 | `fromPretrained` |

**`import Tokenizers` (swift-transformers)** : seulement dans `WhisperTokenizer.swift:2`, utilisé en `:5`, `:26` et `:134`. Voir §4.

**`import MLXAudioVAD`** : seulement `VoxtralRealtime.swift:5`, utilisé en `:129-201`. Voir §5.

**MLXAudioCodecs** : aucun usage dans ces fichiers.

---

## 2. Signatures exactes

### Chargement depuis un dossier local

```swift
// ParakeetModel.swift:936 `public extension ParakeetModel {` → public
static func fromDirectory(
    _ modelDir: URL,
    computeDType: DType = .bfloat16
) throws -> ParakeetModel

// CanaryModel.swift:510
public static func fromModelDirectory(_ modelDir: URL) async throws -> CanaryModel
//  (async sans aucun await interne vrai : peut devenir synchrone)

// WhisperModel.swift:482
public static func fromDirectory(
    _ modelDirectory: URL,
    cache: HubCache = .default
) async throws -> WhisperModel
//  → après retrait de HF : `public static func fromDirectory(_ modelDirectory: URL) throws -> WhisperModel`

// VoxtralRealtime.swift:489 `public extension VoxtralRealtimeModel {` → public
static func fromDirectory(_ modelDir: URL) throws -> VoxtralRealtimeModel
```

### Génération (protocole `STTGenerationModel`, `Generation.swift:51-63`)

```swift
public protocol STTGenerationModel: AnyObject {
    var defaultGenerationParameters: STTGenerateParameters { get }
    func generate(audio: MLXArray, generationParameters: STTGenerateParameters) -> STTOutput
    func generateStream(audio: MLXArray, generationParameters: STTGenerateParameters)
        -> AsyncThrowingStream<STTGeneration, Error>
}
// + extension : generate(audio:generationParameters: STTGenerateParameters? = nil) (nil → defaults)
```

Chaque modèle l'implémente :

| Modèle | Fichier:ligne |
|---|---|
| Parakeet | `public func generate(audio: MLXArray, generationParameters: STTGenerateParameters) -> STTOutput` (`ParakeetModel.swift:117`) et `generateStream` (`:200`). En plus, `public func generateBatch(audios: [MLXArray], generationParameters: STTGenerateParameters = STTGenerateParameters()) throws -> [STTOutput]` (`:168`) |
| Canary | `CanaryModel.swift:385` (`generate`), `:434` (`generateStream`) |
| Whisper | `WhisperModel.swift:36` (`generate`), `:92` (`generateStream`) |
| Voxtral | `VoxtralRealtime.swift:57` (`generate`), `:203` (`generateStream`) ; `:126` est la surcharge VAD, à retirer |

### Voxtral : session de flux (`VoxtralRealtimeStreamSession.swift`)

```swift
public final class VoxtralRealtimeStreamSession {
    public struct Delta {                 // :71 — pas Sendable déclaré, pas d'init public
        public let text: String
        public let tokenIds: [Int]
    }
    public init(model: VoxtralRealtimeModel, temperature: Float = 0.0,
                maxTokens: Int = 4096, transcriptionDelayMs: Int? = nil)    // :111
    public var text: String { get }        // :128 transcription complète
    public var tokens: [Int] { get }       // :130
    public var isFinished: Bool { get }    // :132 (EOS ou maxTokens)
    @discardableResult public func step(_ samples: [Float]) -> Delta      // :143
    @discardableResult public func step(_ samples: MLXArray) -> Delta     // :151 (moyenne si >1D, puis asArray)
    @discardableResult public func finish() -> Delta                      // :159 une seule fois, après le dernier step
}
public extension VoxtralRealtimeModel {
    func makeStreamSession(temperature: Float = 0.0, maxTokens: Int = 4096,
                           transcriptionDelayMs: Int? = nil) -> VoxtralRealtimeStreamSession   // :363
    func transcribeStreaming(audio: MLXArray,
                             generationParameters: STTGenerateParameters = STTGenerateParameters(),
                             chunkMs: Int = 480, onDelta: ((String) -> Void)? = nil) -> STTOutput  // :380
}
```

### `STTGenerateParameters` (`Generation.swift:3-49`, struct `Sendable`, tous les champs `let`)

| Champ | Type | Défaut |
|---|---|---|
| `maxTokens` | Int | 8192 |
| `temperature` | Float | 0.0 |
| `topP` | Float | 0.95 |
| `topK` | Int | 0 |
| `verbose` | Bool | false |
| `language` | String? | nil |
| `chunkDuration` | Float | 1200.0 |
| `minChunkDuration` | Float | 1.0 |
| `repetitionPenalty` | Float | 1.0 |
| `repetitionContextSize` | Int | 32 |
| `kvBits` | Int? | nil |
| `kvGroupSize` | Int | 64 |
| `quantizedKVStart` | Int | 0 |

Valeurs par modèle (`defaultGenerationParameters`) :
- **Parakeet** (`:58`) : `language: "en"`, le reste identique.
- **Canary** (`:361`) : `maxTokens: 200`, `topP: 1`, `language: "en"`.
- **Whisper** (`:21`) : `maxTokens: maxTargetPositions-16` (= 432), `topP 1`, `language nil`, `chunkDuration 30`, `minChunkDuration 0.1`.
- **Voxtral** (`:35`) : `maxTokens 4096`, `topP 1`, `language "en"`.

### `STTGeneration` (`STTOutput.swift:13`)

```swift
public enum STTGeneration: Sendable { case token(String); case info(STTGenerationInfo); case result(STTOutput) }
```

### `STTOutput` (`STTOutput.swift:80`, `@unchecked Sendable`)

- `text: String` : le champ texte ;
- `segments: [[String: Any]]?` ;
- `language: String?` ;
- `promptTokens`, `generationTokens`, `totalTokens` : `Int` ;
- `promptTps`, `generationTps`, `totalTime`, `peakMemoryUsage` : `Double`.

### Format audio

Partout un `MLXArray` 1D en Float32, à **16 kHz** et en mono :
- un tableau à plus d'une dimension est moyenné sur le dernier axe (`ParakeetModel.swift:817-819`, `CanaryModel.swift:455`, `WhisperModel.swift:41`, `VoxtralRealtime.swift:61`) ;
- **aucun ré-échantillonnage** : `resampleAudio` / `loadAudioArray` restent dans `AudioUtils`, non copié. À faire en amont, par AVAudioConverter.

Cas particulier de Canary, `preprocessAudio` (`:448`) : un tableau 3D, ou 2D dont la dernière dimension vaut `features`, est pris comme mel déjà calculé.

### Bloquant ? Thread-safety ?

**`generate` est synchrone et bloque le fil d'appel** : boucles de décodage avec `eval` / `item`.

**`generateStream` n'est pas réellement asynchrone.** Tout le travail est écrit dans la *closure de construction* d'`AsyncThrowingStream { continuation in … }`, que Swift exécute immédiatement, sur le fil appelant, dans l'`init`. Le flux est donc entièrement calculé avant d'être rendu (`ParakeetModel.swift:204`, `CanaryModel.swift:438`, `WhisperModel.swift:96`, `VoxtralRealtime.swift:207`). Pour du vrai flux : la session Voxtral, ou le `onDelta` de `transcribeStreaming`.

**Aucune garantie de concurrence.** Les classes `Module` ne sont pas `Sendable` et ont un état mutable :
- Parakeet : `compiledEncoderFeaturesByShape` (`:56`), `lazy var compiledTDTStep` (`:75`), `tdtDecoderImplementation` modifié et restauré dans `generateBatch` (`:176-187`) ;
- Voxtral : `adaScaleDelay` (`:33`), `bytesCache` du tokenizer (`VoxtralRealtimeTokenizer.swift:40`) ;
- Whisper : `tokenizer` est `var` ; le cache des filtres mel est protégé par `NSLock` (`WhisperAudio.swift:16-17`, `nonisolated(unsafe)`) ;
- `Memory.clearCache()` est global.

→ Sérialiser : un modèle par file série, ou un acteur, comme `KokoroRunner` (`@unchecked Sendable`). Une seule inférence MLX à la fois dans l'app.

`WhisperModel.transcribeChunk` fait un `fatalError` si le tokenizer est absent (`:191-193`).

---

## 3. Canary

### Ce que lit le loader

`fromModelDirectory` (`CanaryModel.swift:510-533`) lit :
- `config.json` ;
- tous les `*.safetensors` du dossier, triés (`:689-704`, erreur s'il n'y en a aucun) ;
- puis le tokenizer, via `CanaryTokenizer.fromModelDirectory` (`:23-57`).

Ordre de recherche du tokenizer :
1. `tokens.txt` (`:24`, facultatif ; format « jeton id » par ligne, `:132-163`) ;
2. puis le premier trouvé parmi : `tokenizer.model` (SentencePiece protobuf, `:28`), `tokenizer.json` (format HF unigram `model.vocab` + `model.unk_id`, `SentencePieceTokenizer.swift:410-440`), ou **`config.tokenizer.model_base64`** (`:34`, clé décodée en `CanaryConfig.swift:347-351`).

Sans aucun tokenizer, `tokenizer` vaut `nil` : la sortie est `"<id><id>…"` (`:475`), et les ids du prompt viennent des valeurs par défaut de la config (0, 1, 2, 3, fausses pour v2).

### `Mediform/canary-1b-v2-mlx-q8` @ `0b6b32ee10f30c89e3ead7249bb636445e3019ee`

Fichier téléchargé : `tmp/hf/canary-mediform/config.json`, 673 039 octets.

**Le vocabulaire est bien dans la config**, clé **`tokenizer.model_base64`** : 671 740 caractères base64, soit 503 803 octets de modèle SentencePiece protobuf et **16 384 pièces**.

**Le loader Swift peut l'utiliser tel quel** (branche `:34`, `SentencePieceTokenizer(sentencePieceModelData:)`). Ids lus dans le protobuf :

| Jeton | Id |
|---|---|
| `<unk>` | 0 |
| `<\|endoftext\|>` | 3 |
| `<\|startoftranscript\|>` | 4 |
| `<\|pnc\|>` | 5 |
| `<\|nopnc\|>` | 6 |
| `<\|startofcontext\|>` | 7 |
| `<\|noitn\|>` | 9 |
| `<\|notimestamp\|>` | 11 |
| `<\|nodiarize\|>` | 13 |
| `<\|emo:undefined\|>` | 16 |
| `<\|en\|>` | 64 |
| `<\|fr\|>` | 71 |

La config n'a ni `startofcontext_id`, ni `emo_undefined_id`, ni `endoftext_id`. Les valeurs par défaut (0, 1, 2, 3) seraient fausses, mais `tokenId(_:fallback:)` (`:100-108`) les remplace par la recherche dans le SentencePiece : correct.

Autres points sur cette config :
- `transf_decoder` est reconnu (`CanaryConfig.swift:335`) ;
- `vocab_size` et `enc_output_dim` sont absents, et les valeurs par défaut (16 384 et 1 024) sont justes ;
- `quantization: {"bits": 8}` sans `group_size` : repli à 64, ce qui est correct.

Fichiers du dépôt :

| Fichier | Taille (octets) |
|---|---|
| `config.json` | 673 039 |
| `model.safetensors` | 1 136 436 574 (sha256 lfs `f637b904…5ba5`) |
| `README.md` | 78 |
| `.gitattributes` | 1 519 |

Les clés des poids sont au format natif MLX : `encoder.layers.*`, `transf_decoder.layers.*`, `head.classifier.*`, donc branche `sanitizeMLXNative` (`:535`). En tout 2 217 tenseurs, `scales` et `biases` présents.

→ **Aucun fichier compagnon nécessaire.** C'est d'ailleurs le dépôt cité par le README amont (`README.md:152`).

### `TechHara/canary-1b-v2-mlx-q8` @ `b2eb58949610f2f6292fbfd791925f1187ab80c1` : à éviter

| Fichier | Taille (octets) |
|---|---|
| `config.json` | 1 174 |
| `model.safetensors` | 1 138 546 184 |
| `tokens.json` | 170 013 (`{"vocab_size":16384,"tokens":[…]}`) |

Le loader ne lit pas `tokens.json`. Il attend `tokens.txt`, `tokenizer.model`, `tokenizer.json` ou `model_base64`. Le tokenizer serait donc `nil`. De plus, sa config annonce `emo_undefined_id: 2`, faux (c'est 16).

### `nvidia/canary-1b-v2` (main)

Contient `tokenizer.json` (2 189 980 octets, `tokenizer_config.json` 328, `config.json` 1 591, `model.safetensors` 3 916 173 816, `.nemo` 6,36 Go). Inutile avec Mediform. Je n'ai pas vérifié que ce `tokenizer.json` a la forme unigram `model.vocab` / `model.unk_id` qu'attend `SentencePieceTokenizer.swift:410-428`.

### Remarques sur Canary

- **Pas de découpage en blocs** dans `generate` (`:385-432`) : tout l'audio passe en un seul passage.
- `maxTokens` vaut 200 par défaut (`:363`). La table de positions du décodeur fait 1 024 au plus (`CanaryFixedPositionalEncoding`, `:169`), et l'encodeur a `pos_emb_max_len 5000` (après sous-échantillonnage ×8, soit environ 400 s). → Il faudra découper les longs audios côté Sillon, et mesurer.
- `language` sert à la fois de source et de cible (`:391-395`) : pas de détection automatique, il faut donner `"fr"` ou `"en"`.
- Les jetons spéciaux de ce SentencePiece sont de type 4 (USER_DEFINED). `SentencePieceTokenizer.decode` ne filtre que `control` et `unused` (`SentencePieceTokenizer.swift:570`) : un `<|…|>` généré en cours de route apparaîtrait dans le texte. Seul l'EOS est coupé (`:412`).

---

## 4. Whisper sans swift-transformers

### Usages du tokenizer

Usages de `inner` (swift-transformers), il n'y en a que deux :
- `WhisperTokenizer.swift:26` : `AutoTokenizer.from(modelFolder:)` ;
- `WhisperTokenizer.swift:134` : `inner.decode(tokens: filtered, skipSpecialTokens: true)`.

**Aucun** `encode`, aucun `convertTokenToId`.

Le reste de `WhisperTokenizer` lit `tokenizer_config.json` (`added_tokens_decoder`, `:137-155`) et `generation_config.json` pour les ids spéciaux. Usages depuis `WhisperModel.swift` :
- `:191` : `tokenizer` (fatal si `nil`) ;
- `:198` : `buildPromptTokens(language:task:)` ;
- `:217` et `:239` : `endOfTextId` ;
- `:236` : `timestampBeginId` ;
- `:243` et `:271` : `decode(tokens:)` ;
- `:274-276` : `isMultilingual`, `languageToId` ;
- `:555` : `init(modelDirectory:baseConfig:generationConfig:tokenizerDirectory:) async`.

Dans `transcribeChunk`, `generationConfig?.beginSuppressTokens ?? [endOfTextId]` (`:217`) et `generationConfig?.suppressTokens ?? []` (`:218`).

### Défaut amont sans fichiers de tokenizer

Si on supprimait simplement le téléchargement, sans `tokenizer_config.json` ni `generation_config.json` (cas des dépôts mlx-community) :
- `languageToId` serait vide, donc `isMultilingual = false` (`:78-82`) ;
- le prompt se réduirait à `[sot, notimestamps]`, **sans langue ni `<|transcribe|>`** ;
- l'id de repli `noTimestampsId = sot + (vocab > 51864 ? 105 : 104)` (`:51`) donne 50363 pour turbo, **faux** : c'est `<|nospeech|>`, et le bon id est 50364 ;
- aucune suppression de jetons.

La table doit donc être fournie en dur.

### Dépôts mlx-community (vérifiés)

| Dépôt @ rev | Fichiers |
|---|---|
| `whisper-tiny-4bit` @ `064afea9…01ff` | config 335, model.safetensors 22 029 431, multilingual.tiktoken 816 730 |
| `whisper-small-4bit` @ `1ac90abb…3c5b` | config 339, model.safetensors 139 111 057, multilingual.tiktoken 816 730 |
| `whisper-large-v3-turbo-4bit` @ `0f058d38…93a8` | config 341, model.safetensors 463 462 982, multilingual.tiktoken 816 730 |

Points communs aux trois :
- **pas** de `tokenizer.json`, ni de `tokenizer_config.json`, ni de `generation_config.json` ;
- `multilingual.tiktoken` identique (sha1 `132270c73c6631a8cd9cf17fcf269d6e204d46c0`, 50 257 lignes, rangs 0 à 50256 contigus) ;
- dernière ligne : `= 50256`. Python décode `"="` en `b''`, mais **`Data(base64Encoded: "=")` rend `nil` en Swift** (vérifié) : il faut traiter ce cas comme une chaîne vide.

Configs (format mlx-whisper, lu par `WhisperConfig.swift:96-131`) :
- tiny : `n_mels 80`, `n_vocab 51865`, `n_text_ctx 448`, quantization `{group_size 64, bits 4}` ;
- turbo : `n_mels 128`, `n_vocab 51866`, `n_audio_layer 32`, `n_text_layer 4`, même quantization.

### Ids spéciaux

Vérifiés sur `openai/whisper-tiny` et `openai/whisper-large-v3-turbo`, `generation_config.json` (`lang_to_id` contigus). Règle : `L = n_vocab − 51765 − 1`, soit 99 pour 51865 et 100 pour 51866.

| Jeton | 51865 (tiny, small) | 51866 (large-v3-turbo) |
|---|---|---|
| `<\|endoftext\|>` | 50257 | 50257 |
| `<\|startoftranscript\|>` | 50258 | 50258 |
| langues | 50259…50357 (99) | 50259…50358 (100, + `yue` = 50358) |
| `<\|en\|>` | 50259 | 50259 |
| `<\|fr\|>` | 50265 | 50265 |
| `<\|translate\|>` | 50358 | **50359** |
| `<\|transcribe\|>` | 50359 | **50360** |
| `<\|startoflm\|>` | 50360 | **50361** |
| `<\|startofprev\|>` | 50361 | **50362** |
| `<\|nospeech\|>` | 50362 | **50363** |
| `<\|notimestamps\|>` | 50363 | **50364** |
| premier horodatage `<\|0.00\|>` | 50364 | **50365** |

Seuls les jetons placés après la liste des langues sont décalés de +1 pour turbo. `en`, `fr`, `sot` et `eot` ne bougent pas.

`suppress_tokens` officiel : une liste de base identique pour les deux, plus `[sot, translate, transcribe, startoflm, startofprev, nospeech]`. Vérifié : seule différence tiny / turbo, `50358` contre `50363`. `begin_suppress_tokens` vaut `[220, 50257]`.

### Code proposé (complet)

**`TiktokenDecoder.swift`** (nouveau) :

```swift
import Foundation

/// Décodeur tiktoken minimal pour Whisper : lit `multilingual.tiktoken`
/// (une ligne « jeton_base64 rang », rangs 0…50256 contigus) et ne fait que
/// ids → texte, seul usage de WhisperModel. Pas d'encodage.
final class TiktokenDecoder {
    enum Failure: Error, CustomStringConvertible {
        case malformedLine(Int)
        case nonContiguousRank(line: Int, rank: Int)
        var description: String {
            switch self {
            case .malformedLine(let n): return "multilingual.tiktoken : ligne \(n) illisible"
            case .nonContiguousRank(let n, let r): return "multilingual.tiktoken : rang \(r) inattendu ligne \(n)"
            }
        }
    }

    /// Octets du jeton, indexés par rang.
    private let bytesByRank: [[UInt8]]

    var count: Int { bytesByRank.count }

    init(url: URL) throws {
        let text = try String(contentsOf: url, encoding: .utf8)
        var table: [[UInt8]] = []
        table.reserveCapacity(50_257)
        var lineNumber = 0
        for line in text.split(whereSeparator: \.isNewline) {
            lineNumber += 1
            let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
            guard parts.count == 2, let rank = Int(parts[1]) else {
                throw Failure.malformedLine(lineNumber)
            }
            guard rank == table.count else {
                throw Failure.nonContiguousRank(line: lineNumber, rank: rank)
            }
            // Rang 50256 : « = », chaîne vide ; Data(base64Encoded:) rend nil.
            table.append([UInt8](Data(base64Encoded: String(parts[0])) ?? Data()))
        }
        bytesByRank = table
    }

    /// Concatène les octets puis décode en UTF-8 (U+FFFD sur séquence invalide,
    /// comme `errors="replace"` de tiktoken). Les ids hors table sont ignorés :
    /// l'appelant filtre déjà les jetons spéciaux.
    /// `dropIncompleteTail` coupe une séquence UTF-8 inachevée en fin (utile pour
    /// le flux : sinon un caractère à cheval sur deux jetons produit un U+FFFD
    /// transitoire et le diff par préfixe de WhisperModel réémet tout le texte).
    func decode(_ ids: [Int], dropIncompleteTail: Bool = false) -> String {
        var out: [UInt8] = []
        out.reserveCapacity(ids.count * 4)
        for id in ids where id >= 0 && id < bytesByRank.count {
            out.append(contentsOf: bytesByRank[id])
        }
        if dropIncompleteTail {
            out.removeLast(out.count - Self.completeUTF8Length(out))
        }
        return String(decoding: out, as: UTF8.self)
    }

    /// Longueur du plus long préfixe qui ne finit pas au milieu d'un caractère.
    private static func completeUTF8Length(_ b: [UInt8]) -> Int {
        var i = b.count - 1
        var continuation = 0
        while i >= 0, continuation < 3, b[i] & 0xC0 == 0x80 { i -= 1; continuation += 1 }
        guard i >= 0 else { return b.count }
        let lead = b[i]
        let needed: Int
        if lead < 0x80 { needed = 1 }
        else if lead & 0xE0 == 0xC0 { needed = 2 }
        else if lead & 0xF0 == 0xE0 { needed = 3 }
        else if lead & 0xF8 == 0xF0 { needed = 4 }
        else { return b.count }               // octet invalide : laisser U+FFFD
        return b.count - i < needed ? i : b.count
    }
}

/// Ids spéciaux de Whisper multilingue (tokenizer.py d'OpenAI), déduits de n_vocab.
/// Vérifiés contre generation_config.json d'openai/whisper-tiny (51865) et
/// openai/whisper-large-v3-turbo (51866).
struct WhisperSpecialTokens {
    /// Ordre officiel ; 99 premières pour n_vocab 51865, 100 (avec "yue") pour 51866.
    static let languageCodes = [
        "en","zh","de","es","ru","ko","fr","ja","pt","tr","pl","ca","nl","ar","sv","it","id","hi","fi","vi",
        "he","uk","el","ms","cs","ro","da","hu","ta","no","th","ur","hr","bg","lt","la","mi","ml","cy","sk",
        "te","fa","lv","bn","sr","az","sl","kn","et","mk","br","eu","is","hy","ne","mn","bs","kk","sq","sw",
        "gl","mr","pa","si","km","sn","yo","so","af","oc","ka","be","tg","sd","gu","am","yi","lo","uz","fo",
        "ht","ps","tk","nn","mt","sa","lb","my","bo","tl","mg","as","tt","haw","ln","ha","ba","jw","su","yue",
    ]

    /// Partie non spéciale de `suppress_tokens` (identique tiny / large-v3-turbo).
    static let suppressBase = [
        1, 2, 7, 8, 9, 10, 14, 25, 26, 27, 28, 29, 31, 58, 59, 60, 61, 62, 63, 90, 91, 92, 93, 359, 503, 522,
        542, 873, 893, 902, 918, 922, 931, 1350, 1853, 1982, 2460, 2627, 3246, 3253, 3268, 3536, 3846, 3961,
        4183, 4667, 6585, 6647, 7273, 9061, 9383, 10428, 10929, 11938, 12033, 12331, 12562, 13793, 14157,
        14635, 15265, 15618, 16553, 16604, 18362, 18956, 20075, 21675, 22520, 26130, 26161, 26435, 28279,
        29464, 31650, 32302, 32470, 36865, 42863, 47425, 49870, 50254,
    ]

    let endOfText = 50257
    let startOfTranscript = 50258
    let languageToId: [String: Int]
    let translate: Int
    let transcribe: Int
    let startOfLM: Int
    let startOfPrev: Int
    let noSpeech: Int
    let noTimestamps: Int
    let timestampBegin: Int

    enum Failure: Error { case unsupportedVocabSize(Int) }

    init(vocabSize: Int) throws {
        let languageCount = vocabSize - 51765 - 1          // 99 ou 100
        guard languageCount == 99 || languageCount == 100 else {
            throw Failure.unsupportedVocabSize(vocabSize)  // modèles .en (51864) non gérés
        }
        var map: [String: Int] = [:]
        for (i, code) in Self.languageCodes.prefix(languageCount).enumerated() {
            map[code] = 50259 + i
        }
        languageToId = map
        translate = 50259 + languageCount
        transcribe = translate + 1
        startOfLM = translate + 2
        startOfPrev = translate + 3
        noSpeech = translate + 4
        noTimestamps = translate + 5
        timestampBegin = translate + 6
    }

    var suppressTokens: [Int] {
        Self.suppressBase + [startOfTranscript, translate, transcribe, startOfLM, startOfPrev, noSpeech]
    }
    var beginSuppressTokens: [Int] { [220, endOfText] }

    /// Pour remplir `WhisperModel.generationConfig` quand le dossier n'a pas de
    /// generation_config.json (cas mlx-community).
    var generationConfig: WhisperGenerationConfig {
        var g = WhisperGenerationConfig()
        g.isMultilingual = true
        g.decoderStartTokenId = startOfTranscript
        g.eosTokenId = endOfText
        g.noTimestampsTokenId = noTimestamps
        g.prevSotTokenId = startOfPrev
        g.suppressTokens = suppressTokens
        g.beginSuppressTokens = beginSuppressTokens
        g.langToId = languageToId.reduce(into: [:]) { $0["<|\($1.key)|>"] = $1.value }
        g.taskToId = ["transcribe": transcribe, "translate": translate]
        return g
    }
}
```

`WhisperGenerationConfig` (`WhisperConfig.swift:155-185`) n'a que des `var` optionnels et aucun `init` déclaré. Swift synthétise donc `init()` et l'init membre à membre (internes), et `WhisperGenerationConfig()` compile dans le même module.

**`WhisperTokenizer.swift` réécrit** : même API pour `WhisperModel`, sans swift-transformers.

```swift
import Foundation

public final class WhisperTokenizer {
    let inner: TiktokenDecoder
    let isMultilingual: Bool

    let startOfTranscriptId: Int
    let endOfTextId: Int
    let noTimestampsId: Int
    let prevSotId: Int?
    let transcribeId: Int?
    let translateId: Int?
    let timestampBeginId: Int

    let languageToId: [String: Int]
    let specialTokenIds: Set<Int>

    init(tiktokenURL: URL, special: WhisperSpecialTokens) throws {
        inner = try TiktokenDecoder(url: tiktokenURL)
        isMultilingual = true
        startOfTranscriptId = special.startOfTranscript
        endOfTextId = special.endOfText
        noTimestampsId = special.noTimestamps
        prevSotId = special.startOfPrev
        transcribeId = special.transcribe
        translateId = special.translate
        timestampBeginId = special.timestampBegin
        languageToId = special.languageToId
        specialTokenIds = Set(special.endOfText..<special.timestampBegin)
    }

    // buildPromptTokens(language:task:) et resolveLanguage(_:) : copie inchangée
    // des lignes 98-127 d'origine.

    public func decode(tokens: [Int], dropIncompleteTail: Bool = false) -> String {
        let filtered = tokens.filter { id in
            id >= 0 && id < timestampBeginId && !specialTokenIds.contains(id)
        }
        return inner.decode(filtered, dropIncompleteTail: dropIncompleteTail)
    }

    // normalizeLanguageKey(_:) et fullNameToCode : copie inchangée (172-191).
    // loadAddedTokensDecoder / tokenId(in:name:) / extractLanguageCode : supprimés.
}
```

**Changements dans `WhisperModel.fromDirectory` (`:482-564`)** :
1. Signature : `public static func fromDirectory(_ modelDirectory: URL) throws -> WhisperModel` (plus de `HubCache`, plus d'`async`).
2. Après `:496`, construire `let special = try WhisperSpecialTokens(vocabSize: config.vocabSize)` et `generationConfig = generationConfig ?? special.generationConfig`, avant `WhisperModel(config:generationConfig:)` en `:498`.
3. Remplacer `:536-560` par `model.tokenizer = try WhisperTokenizer(tiktokenURL: modelDirectory.appendingPathComponent("multilingual.tiktoken"), special: special)`.
4. Dans le flux (`:243`), appeler `tokenizer.decode(tokens: generated, dropIncompleteTail: true)`.

Différence possible avec l'amont, à vérifier par l'écoute : le décodeur HF peut appliquer `clean_up_tokenization_spaces`. Je ne l'ai pas vérifié dans swift-transformers. Si c'est le cas, il retire l'espace avant `? ! . ,`, ce qui est incorrect en français. tiktoken (la référence d'OpenAI) ne nettoie rien.

À noter :
- avec `language: nil`, `buildPromptTokens` n'ajoute pas de jeton de langue (`:101-103`), et aucune détection de langue n'est codée. **Toujours passer `"fr"` ou `"en"`** ;
- les audios de plus de 30 s sont coupés en fenêtres fixes de 30 s, **sans recouvrement** (`:165-182`), avec le risque de couper un mot à la jointure.

---

## 5. Voxtral

### Retrait du VAD : propre

Lignes à supprimer dans `VoxtralRealtime.swift` :
- `:5` (`import MLXAudioVAD`) ;
- `:126-201` : la surcharge `generate(audio:generationParameters:vad: (model: SileroVAD, config: SpeechSegmentConfig)?)`. Elle seule utilise `SileroVAD`, `SpeechSegmentConfig` et `segmentSpeech`, tous définis dans `MLXAudioVAD/SpeechSegmenter.swift:6, :162`.

Aucun autre fichier copié n'y fait référence. Dans l'amont, seuls `CohereTranscribe` et les outils CLI l'utilisent, et ils ne sont pas copiés.

À retirer aussi :
- `:6-7` (`MLXLMCommon`, `HuggingFace`) ;
- `:540-559` (`fromPretrained`).

`:650-657` (`VoxtralQuantizationConfig`) se garde avec la `BaseConfiguration` locale du §1.c.

### Fichiers lus par `fromDirectory` (`:490-538`)

- `config.json` ;
- `tekken.json` (obligatoire, `VoxtralRealtimeTokenizer.swift:50-60`) ;
- tous les `*.safetensors`.

`mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit` (main) contient `config.json` 1 513, `model.safetensors` 3 133 798 126, `model.safetensors.index.json` 118 632 (ignoré) et `tekken.json` 14 910 348.

Sa config a `model_type: voxtral_realtime` et `quantization {64, 4, affine}`. Elle n'a **pas** de `transcription_delay_ms`, d'où la valeur par défaut de **480 ms** (`VoxtralRealtimeConfig.swift:276`). `n_left_pad_tokens` vaut 32 par défaut, `bos` 1, `eos` 2, `streaming_pad` 32.

### Flux de session

Unités : 16 kHz ; un jeton = 1 280 échantillons = 80 ms (`frameRate 12.5`, `VoxtralRealtime.swift:9-15`, et `:178` de la session).

**`step` accepte n'importe quelle taille de bloc** : les échantillons sont mis en tampon (`pendingSamples`, `:146`). Aucune taille n'est imposée.
- Le commentaire d'en-tête cite des blocs micro de 80 ms (`:8`) ;
- `transcribeStreaming` utilise `chunkMs = 480` par défaut (`:383`).
- Chaque `step` qui fait avancer le décodage paie `eval` et un prefill.

Les blocs de 80 ms donnent la latence la plus basse. Des blocs de 480 ms coûtent moins d'appels. Mesure à faire.

**Délai par défaut : `transcriptionDelayMs` = `nil`, donc `config.transcriptionDelayMs` = 480 ms**, soit `numDelayTokens(480)` = 6 jetons.
- Longueur du prompt : `1 + 32 + 6 = 39` (`:184`).
- Garde : 1 jeton gelé en fin (`frozenGuardTokens`, `:82`).
- Le prefill n'arrive qu'une fois `emitLimit ≥ 39`, soit environ 8 jetons d'audio réel, soit **environ 640 ms avant le premier texte** (déduit de `:223-224`, `:296`). Ensuite le texte suit l'audio avec à peu près 480 + 80 ms de retard.
- `finish()` ajoute les zéros de fin hors ligne (`:200-208`). Après lui, plus aucun `step` n'est pris.

`language` n'est **pas utilisé** par Voxtral : il est seulement recopié dans `STTOutput.language` (`:115`). Le modèle est multilingue sans invite.

La session et le modèle ne sont pas thread-safe : un seul appelant à la fois. Après avoir libéré la session, faire `Memory.clearCache()` (commentaire `:249-254`).

---

## 6. Parakeet

### Paramètre `language` : sans effet sur la transcription

Il n'est que recopié dans `STTOutput.language` (`ParakeetModel.swift:164, 195, 260, 274`). La seule autre occurrence est la valeur par défaut `"en"` (`:65`). Aucune occurrence dans les autres fichiers Parakeet ou Nemo.

Le décodage TDT n'a pas d'invite de langue : **v3, multilingue (25 langues européennes), reconnaît la langue tout seul**. Laisser `language` à `nil`, ou en tenir compte uniquement pour l'affichage.

### Fichiers lus par `fromDirectory` (`:949-1055`)

- **`config.json` seul.** Les jetons `-Infinity`, `Infinity` et `NaN` y sont remplacés par `null` (`:937-947`). Le vocabulaire vient de `joint.vocabulary` (8 192 entrées pour v3) via `ParakeetConfigParser`. Le type (TDT, TDT-CTC, RNNT, CTC) est déduit par `ParakeetVariantResolver`.
- **Tous les `*.safetensors` du dossier** (`:1016-1022`), fusionnés.

Ni `tokenizer.model`, ni `vocab.txt`, ni `tokenizer.vocab` ne sont lus.

`mlx-community/parakeet-tdt-0.6b-v3` (main) contient `config.json` 244 093, `model.safetensors` 2 508 288 736 (non quantifié, pas de clé `quantization`), plus `tokenizer.model` 360 916, `tokenizer.vocab` 101 024 et `vocab.txt` 46 772, inutiles au loader. Config : `target …EncDecRNNTBPEModel`, `decoding.model_type tdt`, `durations [0,1,2,3,4]`, `max_symbols 10`, `features 128`.

### À noter

- `computeDType` vaut `.bfloat16` par défaut, et tous les poids flottants sont convertis après chargement (`:1037-1050`).
- Découpage : `chunkDuration` 1 200 s avec 2 s de recouvrement dans `generate` (`:125-157`). `generateStream` force des blocs de 5 s avec 1 s de recouvrement si `chunkDuration ≥ 1199` (`:211`).
- Les loaders chargent **tous** les `.safetensors` du dossier : un dossier par modèle, sans fichier étranger.
