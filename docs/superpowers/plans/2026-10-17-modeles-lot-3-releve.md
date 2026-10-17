# Relevé lot 3 : vendoriser Qwen3-TTS et MagpieTTS dans `KokoroMLX`

Sources clonées dans `/Users/tharsan/.claude/jobs/cf439749/tmp/lot3/` :
- `mas/` = mlx-audio-swift, commit `244458d7b61e54ff135073ddc9ed76782bb20758` (10 oct. 2026), MIT.
  Son `Package.resolved` fixe mlx-swift **0.32.3** et mlx-swift-lm **3.32.3** (`3b339ad`).
- `ss/` = soniqo/speech-swift, HEAD `d74459f544a93f9f388393268694a13bbf8cd1f1` (17 oct. 2026), Apache-2.0.
  Dernier commit sur `Sources/MagpieTTS` : `28aef99510e438bcc5014df2582820d9fdba80e0` (11 juil. 2026). **C'est lui qu'il faut figer.**
- `mlm/` = mlx-swift-lm `3.32.3` ; `mlxs/` = mlx-swift `0.32.3` (vérifier les API).
- `hf/` = configs, en-têtes safetensors et cartes des dépôts HF ; `py/modeling.py` = Qwen3-TTS officiel (`QwenLM/Qwen3-TTS@022e286`).

Rien n'a été compilé ni exécuté. Le projet n'a pas été modifié.

Abréviations : `Q = mas/Sources/MLXAudioTTS/Models/Qwen3TTS/` et `M = ss/Sources/MagpieTTS/`.

**Déjà mesuré sur ce Mac** (M5, 16 Go, Python mlx-audio, `.superpowers/research/eval/rapport.md`) :
- Qwen3 1.7B CustomVoice 8 bits « allégé » : RTF soutenu **0,55–0,62** en français. La voix est occupée **64 %** du temps à 1×, et ne suit pas à 2×. Pic mémoire **3,5–3,7 Go**. 1er son 0,15–0,3 s (§5–6, l. 172-240).
- Qwen3 0.6B CustomVoice 8 bits : **pas plus rapide** en flux (RTF 0,45–0,48), WER EN 3,5–6,5 % (l. 133-134).
- Meilleur français mesuré : 1.7B **Base** avec un x-vector figé tiré de SIWIS (CC-BY-4.0). Accent +4,7, WER 0–1,1 % (l. 281-290, 428-433).
- MagpieTTS par audio.cpp (GGUF) : **1,4× plus lent que le temps réel** (commit `d40e7b2`). Aucune mesure du port MLX.

---

## 1. Qwen3-TTS dans mlx-audio-swift @244458d

### 1.a Fichiers (dossier `Q`, 4 398 lignes de Swift)

| Fichier | Lignes | À prendre ? |
|---|---|---|
| `Qwen3TTS.swift` | 1 373 | oui, **taillé** (voir §1.c) : il en reste ≈ 750 |
| `Qwen3TTSTalker.swift` | 368 | oui, tel quel sauf l'import |
| `Qwen3TTSCodePredictor.swift` | 243 | oui, tel quel sauf l'import |
| `Qwen3TTSConfig.swift` | 595 | oui, imports et `StringOrNumber` (voir plus bas) |
| `Qwen3TTSSpeechTokenizer.swift` | 1 464 | oui, **sans l'encodeur** (`:794-887`, ≈ 95 lignes), qui ne sert qu'au clonage |
| `Qwen3TTSSpeakerEncoder.swift` | 355 | **non** pour CustomVoice. Optionnel pour Base (x-vector calculé sur l'appareil) ; inutile si le x-vector est précalculé |

Il faut aussi `EuclideanCodebook`, pris de `mas/Sources/MLXAudioCodecs/Mimi/Quantization.swift:5-54` (≈ 50 lignes, MLX/MLXNN seuls). Il est utilisé par le décodeur en `Qwen3TTSSpeechTokenizer.swift:11,22`.

**Ne pas copier le reste de Mimi :**
- l'encodeur demande `SeanetEncoder`, `ProjectedTransformer`, `ConvDownsample1d` et `MLXAudioCodecs.SplitResidualVectorQuantizer` (`:797-866`) ;
- `Transformer.swift` de Mimi importe `MLXLMCommon` et `MLXAudioCore` ;
- **conflits de noms** : `VectorQuantization` et `SplitResidualVectorQuantizer` existent dans Mimi *et* dans `Qwen3TTSSpeechTokenizer.swift:9,93`. En amont, ils vivent dans deux modules ; ici, ce serait un seul.

**Total à copier ≈ 3 300 lignes**, plus ≈ 350 lignes à écrire (§1.c, §1.d) et ≈ 150 lignes de porte Sillon.

**Aucune collision** avec `macos/Vendor/KokoroMLX` (Kokoro et les STT du lot 2). Vérifié en croisant les noms de types définis.

**Swift 5 :** `sending` et `nonisolated(unsafe)` apparaissent seulement dans les parties retirées (flux), plus `Qwen3TTSSpeechTokenizer.swift:973` (`-> sending`) et `:1013`. `nonisolated(unsafe)` est déjà présent dans le dossier vendorisé (`STT/Whisper/WhisperAudio.swift:16`).

### 1.b Modules mlx-swift

Les fichiers n'importent que `MLX`, `MLXNN` et `Foundation`. **Rien à ajouter** dans `project.yml`.

`MLXFast.scaledDotProductAttention` (`Talker.swift`) est un espace de noms de `MLX` en 0.32.3 (relevé du lot 2). `MultiHeadAttention.createAdditiveCausalMask` (`Talker.swift:299`) vient de MLXNN. `compile(shapeless:)` (`Talker.swift:33-37`) vient de MLX.

### 1.c Symboles externes à remplacer

**`import MLXLMCommon`** (`Talker.swift:3`, `CodePredictor.swift:3`, `Config.swift:2`, `SpeechTokenizer.swift:4`, `Qwen3TTS.swift:5`) :

| Symbole | Où | Remplacement |
|---|---|---|
| `KVCache` (protocole), `KVCacheSimple` | `Talker.swift:160,243,280,284,309-310,348,355` ; `CodePredictor.swift:45,121,156,161,188-189,227-229,240` ; `SpeechTokenizer.swift:381,443,471,477,500-501` | **écrire `KVCache.swift`, ≈ 70 lignes** : protocole `{ var offset: Int { get }; func update(keys:values:) -> (MLXArray, MLXArray); @discardableResult func trim(_:) -> Int }` et `KVCacheSimple` recopié de `mlm/Libraries/MLXLMCommon/KVCache.swift:408-497` (allocation par pas de 256, `trim` = recul de l'offset). Seuls `offset`, `update` et `trim` servent (`Qwen3TTS.swift:438-440`). Licence MIT, à joindre comme au lot 2 |
| `BaseConfiguration.Quantization` / `.PerLayerQuantization` | `Config.swift:537-538, 569-576`, `Qwen3TTS.swift:1160-1172` | **déjà là** : `STT/BaseConfiguration.swift` du lot 2, mêmes noms |
| `StringOrNumber` | `Config.swift:18,58,213,279` (`ropeScaling`) | `ropeScaling` n'est **lu nulle part** ailleurs. Le plus simple : décoder en `[String: AnyCodableIgnored]` ou supprimer les deux champs. Sinon, recopier l'enum (`mlm/…/JSONDecodingTypes.swift:62`, ≈ 40 lignes) |
| `GenerateParameters` | `Qwen3TTS.swift:30-37, 59, 88, 107, 141, 167, 181, 653-666` | retirer : ces lignes sont l'API publique `SpeechGenerationModel` et le flux. La porte Sillon appelle directement `generateVoiceDesign(...)` (`:308`) avec ses réglages : température 0,9, top-k 50, top-p 1, pénalité 1,05 (`generation_config.json`) |

**`import MLXAudioCore`** (`Qwen3TTS.swift:4`) :
- `AudioGenerationError` (`:327, 646, 649, 718, 723, 760, 1219`) : remplacer par une petite enum locale ;
- `AudioGenerationInfo` (`:523-531`) et `AudioGeneration` (flux) : retirer ;
- `SendingBox` (`:114`) : retirer avec le flux ;
- `computeMelSpectrogram` (`:870`, x-vector Base seulement) : **déjà présent**, `STT/DSP.swift:236`.

**`import MLXAudioCodecs`** (`SpeechTokenizer.swift:3`) :
- `EuclideanCodebook` : à copier, voir §1.a ;
- le reste ne sert qu'à l'encodeur, retiré.

Retirer l'encodeur, concrètement :
1. dans `Qwen3TTSSpeechTokenizer.init` (`:1044-1055`), ne pas créer `encoderModel` ;
2. dans `loadSpeechTokenizer` (`Qwen3TTS.swift:1269-1273`), **filtrer les clés `encoder_model.*`** avant `update(verify: .all)`, car `.all` inclut `.noUnusedKeys` (`mlxs/Source/MLXNN/Module.swift:392,554`).

Les poids de l'encodeur pèsent 225 Mo en F32 (en-tête lu : 225 tenseurs encoder, 271 decoder). On gagne ces 225 Mo de RAM, pas le disque : un seul fichier.

**`import HuggingFace`** (`Qwen3TTS.swift:2`) : supprimer `fromPretrained` (`:1124-1136`).

**`import Tokenizers` (swift-transformers)** (`Qwen3TTS.swift:7`) :
- type de `var tokenizer` (`:16`) ;
- seul appel : **`tokenizer.encode(text:) -> [Int]`**, en `:381, 734, 766, 897, 970` ;
- chargement `AutoTokenizer.from(modelFolder:)` en `:1204`.

`generateTokenizerJson` (`:1178-1200`, `:1285-1372`) **écrit `tokenizer.json` dans le dossier du modèle** au premier chargement : effet de bord à retirer (le dossier est géré par le catalogue Rust). Voir §1.d.

**À nettoyer :** les `print` de débogage (`:275, 278, 924, 926, 934, 961, 1193, 1237, 1243, 1277`).

**À remplacer :** `try Task.checkCancellation()` (`:415, 514, 532`) par un `shouldStop: (() -> Bool)?` consulté toutes les ~12 trames (≈ 1 s de son). Il y a déjà une synchronisation par trame (`eval` en `:483`, `item` en `:485-487`). Les appels FFI sont synchrones : il n'y a pas de `Task` à annuler. Même contrat que `KokoroRunner.run(shouldStop:)`.

### 1.d Encodeur BPE à nous (remplace swift-transformers)

**Besoin :** encoder du texte en ids Qwen2, en `ByteLevel` BPE.
- Fichiers : `vocab.json` (2 776 833 o), `merges.txt` (1 671 839 o) et `tokenizer_config.json` (33 jetons ajoutés, 151643–151675, mêmes ids dans les deux dépôts).
- Pré-découpage : la regex de `Qwen3TTS.swift:1336`. `NSRegularExpression` (ICU) accepte `\p{L}`, `\p{N}`, `(?i:…)` et `(?!\S)`.

**Étapes :**
1. normaliser en **NFC** (`precomposedStringWithCanonicalMapping`). Le `tokenizer.json` officiel de Qwen2 a un normaliseur NFC ; celui généré par le port met `null`. Ce point compte pour un texte en NFD (PDF) ;
2. découper sur les jetons ajoutés littéraux. Seuls `<|im_start|>` (151644) et `<|im_end|>` (151645) apparaissent dans les gabarits (`:733, 765, 896, 969`) ;
3. appliquer la regex ;
4. projeter chaque octet par la table GPT-2 `bytes_to_unicode` (256 entrées) ;
5. fusionner les paires par rang (dictionnaire `"a b" → rang` ; cache par morceau) ;
6. chercher l'id dans `vocab.json`.

**Sortie attendue :** `<|im_start|>assistant\n` doit donner **exactement 3 jetons**, et `<|im_end|>\n<|im_start|>assistant\n` exactement 5. Le code coupe aux indices 3 et `count-5` (`:975, 991, 995-996`).

**Complexité honnête :**
- ≈ 180–220 lignes ;
- chargement ≈ 0,2–0,4 s : parse JSON de 151 643 entrées et de 151 387 fusions. À mettre en cache dans l'objet ;
- **test de parité indispensable** : 20 à 30 phrases FR et EN (accents, apostrophes typographiques, chiffres, « … », guillemets « »). Les ids de référence viennent de `transformers.AutoTokenizer` (Python), figés en fixture.

Le risque est faible (algorithme connu, le lot 2 a fait de même pour Whisper/tiktoken). Les pièges sont les espaces devant la ponctuation et `\p{N}{1,3}`.

### 1.e Chargement depuis un dossier local

`Qwen3TTSModel.fromModelDirectory(_ modelDir: URL) async throws` (`Qwen3TTS.swift:1138-1245`). À rendre synchrone (`throws`) : il n'attend que `AutoTokenizer`.

Ce qu'il lit :
- `config.json` ;
- **tous** les `*.safetensors` du dossier (`:1148-1152`) : attention aux fichiers égarés ;
- `vocab.json`, `merges.txt`, `tokenizer_config.json` ;
- `speech_tokenizer/config.json` et `speech_tokenizer/*.safetensors` (`:1210-1213`, `:1247-1278`).

Un `speech_tokenizer` qui n'est pas un dossier **efface le dossier du modèle** (`:1214-1221`, `try? fm.removeItem(at: modelDir)`) : à retirer absolument (le catalogue Rust est seul maître des fichiers).

`model.safetensors.index.json`, `preprocessor_config.json`, `generation_config.json` et `speech_tokenizer/{configuration,preprocessor_config}.json` ne sont **pas lus**.

### 1.f Génération

`generateVoiceDesign(text:instruct:language:refAudio:refText:temperature:topK:topP:repetitionPenalty:minP:maxTokens:streamingInterval:onToken:onInfo:onAudioChunk:) throws -> MLXArray` (`:308-571`).

**CustomVoice :**
- la voix passe par `instruct` sous la forme `"locuteur"` ou `"locuteur, consigne"` (`:363-367`, `parseCustomVoicePrompt` `:573`) ;
- le locuteur est cherché en minuscules dans `talker_config.spk_id` (`:920-921`) ;
- un nom inconnu **n'échoue pas** : il imprime un avertissement (`:926`) et parle sans locuteur. La porte doit valider le nom.

**Langue :** `"french"` / `"english"` (clé de `codec_language_id`, `:913-915`), ou `"auto"`.

**Bogue du port, à corriger :** la langue de dialecte d'Eric (sichuan) ou de Dylan (beijing) **remplace toujours** la langue demandée (`:928-936`). L'officiel ne le fait que si la langue est `chinese` ou `auto` (`py/modeling.py:2118-2122`). Sans correction, Eric et Dylan liraient le français avec l'id de langue d'un dialecte chinois.

**Longueur :**
- `maxTokens` est borné à `max(75, 6 × jetons de texte)` trames (`:381-382`), soit 0,48 s par jeton BPE ;
- cette borne coupe une phrase qui « s'emballe » (déjà retenu dans `gen_mlx.py`).

**Sortie :**
- `MLXArray` 1-D, float, **24 000 Hz** (`config.sample_rate`, `speech_tokenizer/config.json` : `output_sample_rate 24000`, `decode_upsample_rate 1920`) ;
- **12,5 trames/s**, 16 livres de codes par trame (1 du talker + 15 du prédicteur de codes, `:442-463`) ;
- le décodage passe par `streamingDecode` par paquets de 300 trames (`:216-233`).

**Flux :**
- `generateVoiceDesign(..., streamingInterval:, onAudioChunk:)` décode tous les `streamingInterval × 12,5` trames, par `decoder.streamingStep` (`:494-507`, convolutions causales avec état) ;
- l'API publique `generateStream` (`:82-135`) enveloppe le tout dans une `AsyncThrowingStream`. À ne pas reprendre : le contrat Sillon est une phrase = un PCM ;
- `onAudioChunk` reste disponible si un jour `TextSpeechCompute` rend des morceaux.

**Aléa :**
- l'échantillonnage passe par `categorical` (RNG global de MLX, `:1118`), donc chaque synthèse diffère ;
- proposer `MLXRandom.seed(hash(phrase))` sous `MLXGate` : une phrase resynthétisée (saut, cache vidé) sonne pareil.

**Fils :**
- « Model instances still require serialized use » (`mas/Sources/MLXAudioTTS/Generation.swift:11`) ;
- l'état du décodeur de flux est partagé (`resetStreamingState`, `:405-412`) et le cache de référence est sous verrou (`:25-26`) ;
- **une synthèse à la fois**, sous `MLXGate`.

**Mémoire :** `Memory.clearCache()` est appelé toutes les 50 trames (`:509-511`) et après chaque paquet décodé (`SpeechTokenizer.swift:1094`).

**Vitesse :**
- mlx-audio-swift ne publie **aucun chiffre** ;
- speech-swift (un autre port, même architecture), sur M2 Max : RTF **0,85** pour 1.7B 8 bits et **0,76** pour 0.6B 8 bits (`ss/docs/benchmarks/tts-roundtrip.md:9-16`) ;
- sur ce Mac (Python) : 0,55–0,62 (1.7B) et 0,45–0,48 (0.6B) ;
- en Swift, il faut **remesurer**. La boucle fait 16 passages séquentiels par trame et une synchronisation `item()` par trame.

### 1.g Optimisations déjà validées en Python (« allégé », `gen_mlx.py:20-37`)

- encodeur du codec retiré : −225 Mo ;
- embedding texte (151 936 × 2048, BF16, **622 Mo**, non quantifié dans les dépôts : en-tête lu) quantifié en 8 bits au chargement. En Swift : `quantize(model: talker.model, filter: { path, m in path == "text_embedding" && m is Embedding … })` sur MLXNN 0.32.3. Il faut que `getTextEmbeddings()` reste un `Embedding` ; `QuantizedEmbedding` en hérite-t-il en 0.32.3 ? **À vérifier** ;
- décodeur du codec en float16 en option (457 → 229 Mo).

---

## 2. Dépôts Hugging Face (Qwen3)

Licence **Apache-2.0** partout (poids Qwen et conversions mlx-community). Pas de dépôt à accès restreint.

### 2.a `mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit` @ `50f45ef0047cde7e84c2ef04326acb8ada2436a7`

| Fichier | Octets |
|---|---|
| `config.json` | 5 522 |
| `generation_config.json` | 245 |
| `merges.txt` | 1 671 839 |
| `model.safetensors` | 1 304 461 214 |
| `model.safetensors.index.json` | 77 731 |
| `preprocessor_config.json` | 127 |
| `speech_tokenizer/config.json` | 2 336 |
| `speech_tokenizer/configuration.json` | 76 |
| `speech_tokenizer/model.safetensors` | 682 293 092 |
| `speech_tokenizer/preprocessor_config.json` | 234 |
| `tokenizer_config.json` | 7 344 |
| `vocab.json` | 2 776 833 |
| (`README.md` 1 026, `.gitattributes` 1 519) | |
| **Utiles (lus par le loader)** | **1 991 218 180** |

Contenu et configuration :
- `tts_model_type: base` ;
- `spk_id: {}` : **aucun locuteur** ;
- talker : hidden 1024, 28 couches ;
- 76 tenseurs `speaker_encoder.*` dans `model.safetensors` (x-vector, `enc_dim 1024`) ;
- quantification `{group_size 64, bits 8, mode affine}`.

### 2.b `mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` @ `41d3337e8b7f2843a75841595fc14e4b9a7a4b96`

Mêmes fichiers. `model.safetensors` = **2 393 308 931**, `config.json` = 6 058, `model.safetensors.index.json` = 71 786. **Utiles : 3 080 066 433 octets.**

- `tts_model_type: custom_voice` ; talker : hidden 2048, 28 couches ; pas d'encodeur de locuteur.
- **9 locuteurs** (`talker_config.spk_id`, id de codec) : `serena` 3066, `vivian` 3065, `uncle_fu` 3010, `ryan` 3061, `aiden` 2861, `ono_anna` 2873, `sohee` 2864, `eric` 2875, `dylan` 2878.
  - Langues natives (carte `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` @ `0c0e3051…`, l. 152-164) : Vivian, Serena, Uncle_Fu = chinois ; Dylan = pékinois ; Eric = sichuanais ; Ryan, Aiden = anglais ; Ono_Anna = japonais ; Sohee = coréen.
  - **Aucun locuteur français natif.**
- Langues (`codec_language_id`) : chinese, english, german, italian, portuguese, spanish, japanese, korean, **french (2061)**, russian, plus beijing_dialect et sichuan_dialect.

### 2.c Variante à envisager : `mlx-community/Qwen3-TTS-12Hz-0.6B-CustomVoice-8bit` @ `049ef77fe8816b536193c0c25f9a214d17921282`

- `model.safetensors` 1 286 743 170 ; utiles ≈ 1 973 500 672 octets ;
- mêmes 9 locuteurs (sans consigne de style : réservée au 1.7B) ;
- existe aussi en 4, 5 et 6 bits et en bf16.

### 2.d Français : ce que disent la carte et les mesures

**Carte officielle (WER %, plus bas = mieux) :**
- français multilingue, clone 12 Hz : 2,931 (0.6B) / 2,858 (1.7B) ;
- locuteur cible : 3,781 (0.6B CV) / 3,605 (1.7B CV).
- La carte conseille la langue native du locuteur (l. 152).

**Mesures locales :**
- Vivian FR : WER médian 3,3 %, accent « un peu bizarre » ;
- Aiden FR : plus stable en timbre, plus d'erreurs ;
- Ryan FR : 7–26 % (à exclure en FR) ;
- 6 bits : français cassé.

### 2.e Le problème du « 0.6B Base, une voix fixe » de la spec

Sans audio de référence, `prepareGenerationInputs` (`:885-1001`) construit le préfixe **sans locuteur** : ni `spk_id` (vide pour Base), ni x-vector. Rien ne fixe le timbre, qui dépend de l'échantillonnage. On attend donc **une voix différente à chaque phrase** : la structure le montre, à confirmer à l'écoute.

Deux sorties :
- **A (recommandée, simple)** : remplacer 0.6B Base par **0.6B CustomVoice** (§2.c). Mêmes poids, 9 voix nommées.
- **B (meilleur français mesuré)** : Base avec **x-vector figé**. C'est le mode `x_vector_only` de l'officiel (`py/modeling.py:2103-2104, 2166-2171`) : l'embedding de locuteur prend la place de l'embedding `spk_id`, sans ICL ni encodeur de codec.
  - Côté Swift : un paramètre `speakerEmbedding: MLXArray?` dans `prepareGenerationInputs`, utilisé en `:959-960`. Environ 10 lignes.
  - Le vecteur (1 024 floats pour 0.6B, 2 048 pour 1.7B) est **précalculé** une fois en Python mlx-audio sur l'extrait SIWIS `siwisx-0011` (CC-BY-4.0, crédit Idiap). Il est livré dans l'app (≈ 4–8 Ko).
  - Ce n'est pas du clonage par l'utilisateur. Mais la spec range le clonage hors périmètre : **décision à prendre**.

---

## 3. MagpieTTS dans speech-swift

### 3.a Fichiers MLX (dossier `M`, 2 699 lignes au total)

Le module `MagpieTTS` **ne dépend pas de mlx-swift-lm**. Sa cible (`ss/Package.swift:633-645`) dépend de `AudioCommon`, `MLXCommon` (déclaré, jamais importé), `MLX`, `MLXNN`, `MLXFast` et `MLXRandom`. L'`exact: "3.31.4"` de mlx-swift-lm (`Package.swift:243`) vise d'autres modules du dépôt : **pas d'obstacle**.

| Fichier | Lignes | Imports | À prendre ? |
|---|---|---|---|
| `Configuration.swift` | 261 | Foundation | oui |
| `Transformer.swift` | 316 | MLX, MLXNN | oui. Son propre cache KV (`MagpieSACache`, `MagpieXACache`, `MagpieKVCache`, `:8-28`) |
| `TextEncoder.swift` | 48 | MLX, MLXNN | oui |
| `Decoder.swift` | 155 | MLX, MLXNN | oui |
| `LocalTransformer.swift` | 53 | MLX, MLXNN | oui |
| `NanoCodec.swift` | 295 | MLX, MLXNN | oui |
| `Sampling.swift` | 43 | MLX, MLXRandom | oui |
| `WeightLoader.swift` | 223 | MLX, MLXNN | oui |
| `SubVocab.swift` | 63 | Foundation | oui |
| `ByT5Encoder.swift` | 38 | Foundation | oui (français) |
| `Tokenizer.swift` | 289 | Foundation, NaturalLanguage (import inutilisé) | oui, **taillé** : retirer les cas zh, ja, es, de, hi, ou copier leurs G2P |
| `MagpieTTS.swift` | 344 | MLX, MLXNN, MLXRandom, **AudioCommon** | oui, **taillé** |
| `EnglishG2P.swift` | 106 | Foundation | si anglais : plus `Resources/cmudict_ipa_en.txt` (3,1 Mo) |
| `Downloader.swift` | 56 | AudioCommon | **non** |
| `ChineseG2P.swift`, `JapaneseG2P.swift` | 409 | NaturalLanguage, CoreFoundation | non |

**Total FR + EN ≈ 2 280 lignes copiées**, ≈ 150 modifiées, ≈ 150 de porte. Pour le français seul, sans `EnglishG2P` ni dictionnaire : ≈ 2 170.

**Symboles externes :**
- `AudioChunk` (de `AudioCommon`) n'est utilisé que par le flux (`MagpieTTS.swift:138-300`, `synthesizeStream` et `runStreaming`) : retirer. Le flux « rejoue le codec sur tout le tampon à chaque morceau », RTF 0,93 au lieu de 0,24 (`ss/docs/inference/magpie-tts.md`, « Performance ») : **à ne pas reprendre** ;
- `MagpieTTSDownloader.Paths` (`MagpieTTS.swift:63`, `load(from:)`) : remplacer par `load(directory: URL)` ;
- `fromPretrained` (`:54-61`) : supprimer ;
- `Bundle.module` (`EnglishG2P.swift:42`) n'existe qu'en SPM : passer par `Bundle(for: <classe du framework>)`, ou lire le dictionnaire dans le dossier du modèle ;
- `ProcessInfo…MAGPIE_DEBUG` (`:197`) : retirer.

**API de mlx-swift 0.32.3, vérifiées :**
- `dequantized(_:scales:biases:groupSize:bits:)` (`mlxs/Source/MLX/Ops.swift:1149`) ;
- `conv1d` (`:614`), `convTransposed1d(...outputPadding:groups:)` (`:835`) ;
- `sorted(_:axis:)` (`:2885`), `` `where` `` (`:3369`), `take(_:axis:)` ;
- `MLXRandom.uniform(low:high:_:)` (`Random.swift:206`), `MLXRandom.seed(_:)` (`:95`).

Il faut **ajouter `MLXRandom`** aux dépendances de `KokoroMLX` (`project.yml:42-50`), ou remplacer `import MLXRandom` par `MLX`, où `MLXRandom` est aussi un espace de noms (relevé du lot 2). La seconde option ne touche pas `project.yml`.

**Collisions :** aucune. Tout est préfixé `Magpie…`, sauf trois fonctions globales publiques : `topKFilter`, `forbidIds`, `sampleTopK` (`Sampling.swift:8,21,35`). Absentes du dossier vendorisé aujourd'hui ; les passer en `internal`.

### 3.b Texte, G2P et français

**Français : octets byT5, pas de phonèmes.**
- `MagpieByT5Encoder.encode` : chaque octet UTF-8 devient `octet + 3 + 633` (`ByT5Encoder.swift`, `SubVocab.swift`) ;
- précédé d'un `lowercased()` (`Tokenizer.swift`, cas `.french`), puis EOS = 2361 ;
- **pas de G2P, pas de dictionnaire** : `tokenizer/fr.json` suffit (`type byt5_byte`, `vocab_size 2360`, pas de `pad_with_space`).

**À ajouter : NFC.** Un « é » décomposé donnerait deux octets inattendus.

**Ce qui n'est pas porté : la normalisation de texte NeMo** (`apply_TN`), qui gère nombres et abréviations. Le constructeur de documents de Sillon envoie déjà la **forme parlée** : « 2027 » devient « deux mille vingt-sept » (`Sentence.text`, `src/doc/mod.rs:165-175`). Reste un risque sur les sigles et symboles : à écouter.

**Anglais :**
- G2P par dictionnaire IPA (`MagpieDictG2P`, 125 k entrées, 3,1 Mo, tiré du `.nemo`) ;
- hors dictionnaire : graphèmes en minuscules (`EnglishG2P.swift:95-98`), de qualité médiocre ;
- variante : les phonèmes d'espeak que Sillon a déjà pour Kokoro, passés en `prephonemized`. Mais les conventions IPA de NeMo et d'espeak diffèrent : **à ne pas tenter au lot 3**.

### 3.c Codec et architecture

- Codec **NanoCodec** : décodeur seul (`NanoCodec.swift`, FSQ inverse + HiFi-GAN causal), **22 050 Hz**, 1 024 échantillons par trame, soit **21,5 trames/s**, 8 livres × 2 024 codes.
- Encodeur de texte : 6 couches, d=768. Décodeur : 12 couches avec attention croisée (1 tête). LocalTransformer : 1 couche, d=256.
- 110 trames de contexte de locuteur « cuites » (`baked_T`).
- `max_len` 2 048 positions (décodeur et encodeur de texte).

**Défaut amont :** `maxSteps` vaut **500 trames ≈ 23 s** (`Configuration.swift`, `MagpieTTSParams`). Une phrase longue serait **tronquée**. Il faut une borne proportionnelle au texte, par exemple `min(2048 − 111, 4 × octets + 50)`, à calibrer.

**Chargement :**
- les poids 8 bits sont **déquantifiés en FP32** (`WeightLoader.swift:79-147`) : RAM ≈ 1,6 Go en INT8 (doc speech-swift) ;
- `decoder_step/` **n'est pas lu** par MLX : `load` ne lit que `text_encoder`, `decoder_prefill` et `nanocodec_decoder` (`MagpieTTS.swift:63-86`).

**Vitesse publiée (M4 Pro, INT4 glouton, phrase entière) :** RTF 0,23–0,32 (`ss/docs/inference/magpie-tts.md`). Aucune mesure en 8 bits, ni sur M5 avec ce port. **À mesurer.** audio.cpp était à 1,4× le temps réel ; le port MLX devrait faire bien mieux.

**Fils :**
- le cache `tokenizers` est muté paresseusement (`MagpieTTS.swift:97-107`) ;
- `MLXRandom.seed` est global (`:117`) ;
- **pas sûr entre fils** : sous `MLXGate`, une synthèse à la fois.

**Échantillonnage :**
- défaut température 0,6, top-k 80 (glouton possible, `temperature 0`) ;
- `minFrames` 4 ;
- arrêt par `shouldStop` à ajouter dans la boucle (`:191-214`), toutes les ~20 trames.

### 3.d Dépôt HF : `aufklarer/Magpie-TTS-Multilingual-357M-MLX-8bit` @ `260839c0a2c46993735d72082c3fba177710d477`

| Fichier | Octets | Utile |
|---|---|---|
| `text_encoder/config.json` | 2 461 | oui |
| `text_encoder/model.safetensors` | 108 912 472 | oui |
| `decoder_prefill/config.json` | 9 650 | oui |
| `decoder_prefill/model.safetensors` | 129 439 762 | oui |
| `decoder_step/config.json` | 9 650 | non |
| `decoder_step/model.safetensors` | 129 439 762 | **non** (mêmes poids que prefill) |
| `nanocodec_decoder/config.json` | 466 | oui |
| `nanocodec_decoder/model.safetensors` | 63 165 546 | oui |
| `tokenizer/fr.json` | 29 555 | oui |
| `tokenizer/en.json` | 29 699 | si anglais |
| `tokenizer/{de,es,hi,it,vi,zh}.json` | ≈ 29,6 k chacun | non |
| `tokenizer/manifest.json`, `manifest.json`, `config.json` | 869, 572, 32 | non |
| **Utiles FR + EN** | **301 589 611** (≈ 302 Mo, au lieu de 431 Mo pour tout) | |

- Variante 4 bits : `aufklarer/…-MLX-4bit` @ `13563ab7a97bc578e126dfb379cd621030a32df4` (258 Mo en tout). speech-swift a **retiré l'INT4 des TTS** (`4eb3085`, « decommission int4 ») : prendre le 8 bits.
- Les 5 voix, dans l'ordre du paquet (`decoder_prefill/config.json`, `baked_speaker_names`) : **0 Sofia, 1 Aria, 2 Jason, 3 Leo, 4 John Van Stan**. Elles sont choisies par `MagpieSpeaker(rawValue:)` ou `init?(named:)` (`Configuration.swift`). Toutes anglaises : la carte NVIDIA dit « 5 English speaker voices », dont 4 propriétaires et 1 publique (LibriVox), l. 79, 86.
- **Point d'attention, version :** le dépôt NVIDIA `main` (@ `446bff55e1fe3d3aa29c771574e7d2b0fa322990`) est désormais **v2607** (21 juil. 2026) : 12 langues, empilement de trames ×2, et un autre ordre de locuteurs (Aria 0, Jason 1, John 2, Leo 3, Sofia 4). Le port MLX est **l'ancienne version, à 9 langues**. Les poids MLX sont figés par révision, donc rien ne casse. En revanche, les chiffres de la carte NVIDIA actuelle (français CER 2,70 / 1,54) ne s'appliquent pas tels quels. La carte MLX annonce un CER français de 0 % sur une phrase.

### 3.e Licence NVIDIA Open Model License

La carte MLX renvoie à la version du **14 juin 2024** (PDF lu). La carte NVIDIA renvoie à la version web du **24 oct. 2025**.

Ce que disent les deux :
- usage commercial permis ;
- licence perpétuelle, mondiale, gratuite, « revocable » ;
- les sorties appartiennent à l'utilisateur ;
- **résiliation si l'on attaque NVIDIA** en contrefaçon (2.1) ;
- NVIDIA peut mettre l'accord à jour, et il faut alors s'y conformer ou cesser (2.1) ;
- **redistribution** (3.1) : joindre une copie de l'accord, plus un fichier « Notice » contenant « Licensed by NVIDIA Corporation under the NVIDIA Open Model License ».

**Version 2025 en plus :**
- clause « **Guardrail** » : contourner une limitation technique ou de sécurité du modèle met fin aux droits ;
- NVIDIA y précise que le clonage a été retiré « for security reasons » : Sillon n'en fait pas.

**Pour Sillon :**
- l'app **télécharge à l'exécution** depuis HF : elle ne redistribue pas les poids ;
- c'est l'utilisateur qui « utilise » le modèle : afficher la licence et un lien dans la fiche (champ `licenses` du catalogue) ;
- le fichier Notice n'est obligatoire que si Sillon **livre** les poids (ce n'est pas le cas) ;
- par prudence, ajouter la mention dans les crédits ;
- pas d'usage de la marque NVIDIA au-delà de l'origine du modèle (4).

**Code :** speech-swift est sous Apache-2.0. Joindre `LICENSE`, et un `NOTICE` s'il y en a un (aucun à la racine).

---

## 4. Suivi des mots

**Aucun des deux modèles ne rend de durées.**
- Qwen3 consomme un jeton de texte par trame (`Qwen3TTS.swift:468-474`). C'est un rythme d'entrée, pas un alignement : le texte avance plus vite que la parole.
- Magpie calcule ses probabilités d'attention croisée **explicitement** (`Transformer.swift:199-237`, `MLX.softmax(scores)` en `:35` du bloc), pas par un SDPA fusionné.

**L'estimation existe déjà dans Sillon.**
- `TtsSession::synthesize_timed` rend `words: None` par défaut (`src/tts/mod.rs:152-157`).
- Le lecteur répartit alors la durée selon le poids de chaque mot : lettres alphanumériques de la **forme parlée**, plus une pause selon la ponctuation (`Word::weight`, `src/doc/mod.rs:157-161`).
- Ce poids est utilisé par `word_fraction` (`:198-200`), `word_fraction_range` (`:209-227`) et `word_start`, et appelé par `App::current_word_fraction` (`src/app/mod.rs:345-371`) et `start_offset` (`:170-175`).
- **Il n'y a rien à brancher pour avoir un surlignage.** La spec parlait de syllabes ; les lettres sont déjà en place et testées.

**Ce qu'il faut ajouter :**
1. **Couper le silence de tête et de queue** dans la porte Swift (seuil ≈ −50 dBFS, garder 30 ms). `Rendered.speech` vaut la durée entière du PCM (`src/tts/pool.rs:743-758`) : un blanc de début décalerait tout le surlignage. Kokoro le fait par ses durées.
2. **Mesurer avant de choisir lettres ou syllabes**, avec un banc Rust. Sur un corpus FR et EN, comparer l'estimation par lettres (et une variante par groupes de voyelles) aux **vraies** durées de Kokoro (`synthesize_timed`), en erreur absolue moyenne de début de mot. On garde la meilleure ; c'est peu coûteux.
3. **Option Magpie (à prototyper, pas promise)** :
   - prendre l'argmax de l'attention croisée à chaque trame : position d'octet du texte, moyenne sur les couches choisies ;
   - pour le français en byT5, un octet correspond à un caractère, donc la correspondance avec les mots est triviale ;
   - on obtient un début par mot (trame × 1024 / 22 050 s) ;
   - NeMo entraîne Magpie avec des a priori d'alignement (carte l. 79), donc l'attention devrait être monotone. Mais quelles couches ? Inconnu, à tester ;
   - si `spans.len() == words.len()`, `Synthesis.words = Some(...)`. Sinon, repli sur l'estimation.

---

## 5. Forme de l'intégration

### 5.a Rust (`ffi/src/lib.rs`, à côté de `KokoroCompute` `:62-72`)

```rust
#[derive(uniffi::Record)]
pub struct SpeechOutput {
    pub samples: Vec<f32>,
    pub sample_rate: u32,            // 24 000 (Qwen3) ou 22 050 (Magpie)
    /// Début de chaque mot de `words`, en s, quand le moteur sait le dire
    /// (Magpie par attention croisée, plus tard). `None` : estimation par lettres.
    pub word_starts: Option<Vec<f32>>,
}

#[uniffi::export(with_foreign)]
pub trait TextSpeechCompute: Send + Sync {
    fn load(&self, model_dir: String, runtime: String) -> Result<(), SillonError>;
    fn warm_up(&self) -> Result<(), SillonError>;
    fn synthesize(&self, text: String, language: String, voice: String,
                  words: Vec<String>, cancel: Arc<SynthesisCancel>)
                  -> Result<SpeechOutput, SillonError>;
    fn release(&self);
}
```

**Écarts avec la spec :**
- `words` (formes parlées, déjà passées à `synthesize_timed`) et `word_starts` optionnel : ils laissent la porte ouverte au §4.3 sans casser l'interface. La version stricte de la spec reste possible.

**Côté cœur :**
- `src/tts/text_compute.rs` : un `TextComputeEngine` (`TtsEngine`) dont la session redéfinit `synthesize_cancellable` (rend `Err(Cancelled)` sur `SillonError::Cancelled`) ;
- **une seule session** (comme Kokoro et Magpie audio.cpp, `d40e7b2`) ;
- la fréquence de 22 050 ou 24 000 Hz passe par `PcmBuffer.sample_rate`, que la lecture rééchantillonne déjà (`src/audio/tempo.rs:82-155`) ;
- comptage des parts comme `SharedCompute` (`src/tts/kokoro/backend.rs:78-88`), généralisé.

**Côté catalogue :** `Runtime::Qwen3TtsMlx` et `Runtime::MagpieMlx` dans `src/models.rs:36-49`, `kind: Speech`, app seulement. Les fichiers sont décrits en `companions` avec `dest` pour les sous-dossiers (`speech_tokenizer/…`, `text_encoder/…`).

**Voix :** le nom du locuteur tel quel :
- Qwen : en minuscules (`vivian`, `aiden`…) ; **exclure `eric` et `dylan` en français** tant que le bogue §1.f n'est pas corrigé ;
- Magpie : `sofia`…`john_van_stan`.

**Langue :** `TtsParams.language` (`fr`/`en`) devient `french`/`english` pour Qwen et `.french`/`.english` pour Magpie.

### 5.b Swift

`TextSpeechMLXCompute` (dans `macos/Sources/Voice/`) joue pour `TextSpeechCompute` le rôle que `KokoroMLXCompute` (`macos/Sources/Voice/KokoroMLX.swift`) joue pour Kokoro. Il repose sur deux portes ajoutées dans le framework : `Qwen3TTSRunner` et `MagpieRunner` (pendants de `KokoroRunner.swift`). L'app ne lie toujours pas MLX.

- `load`, `synthesize` et `release` passent **tous sous `MLXGate.shared`** (`macos/Sources/Voice/MLXGate.swift`) : un calcul MLX à la fois, partagé avec Kokoro et la dictée.
- Kokoro et un moteur texte ne sont **jamais chargés ensemble** (un seul modèle de lecture, spec « Mémoire »). Changer de moteur fait `release()` sur l'ancien, puis `Memory.clearCache()`.
- `warm_up` lance une phrase courte (« Bonjour. », ≈ 12 trames Qwen ou 20 trames Magpie) pour compiler `compile(shapeless:)` (`Talker.swift:33`) et les noyaux Metal.

### 5.c Plafonds mémoire : un vrai trou à combler

**Situation actuelle :**
- `Memory.memoryLimit` et `cacheLimit` sont **globaux au processus** ;
- Kokoro pose 1 Go et 64 Mo (`KokoroMLX.swift`, `KokoroMLXCompute.memoryLimit/cacheLimit`) ;
- la dictée pose poids + 2 Go et 256 Mo à chaque chargement et à chaque lecture (`MLXDictationEngine.swift:147-186`) ;
- **libérée, la dictée remet les plafonds de Kokoro en dur** (`SpeechModels.release()`, `:180-183`).

Avec Qwen 1.7B chargé (≈ 2,9 Go en RAM, encodeur retiré, embedding texte non quantifié), la première dictée terminée imposerait 1 Go à Qwen.

**Proposition :**
- un petit registre `MLXBudget` dans l'app : chaque modèle chargé (lecture, dictée) y inscrit son besoin (poids + marge, cache) ;
- les plafonds valent **la somme des modèles chargés** ;
- ils sont reposés sous `MLXGate` avant chaque calcul ;
- `SpeechModels.release()` et `KokoroMLXCompute` passent par lui au lieu des constantes ;
- même famille d'appels que `DictationMLX.setLimits` (`macos/Vendor/KokoroMLX/DictationMLX.swift:10-13`).

**Ordres de grandeur :**
- Qwen 1.7B CV : ≈ 3,5–3,7 Go de pic mesuré (Python, allégé) ;
- Qwen 0.6B CV : ≈ 2,5 Go ;
- Magpie 8 bits : ≈ 1,6 Go (FP32 après déquantification) ;
- avec Voxtral (3,1 Go) chargé pour dicter, Qwen 1.7B + Voxtral ≈ 7 Go **sur un Mac de 16 Go**.

---

## 6. Risques, décision et effort

### Qwen3-TTS : **GO pour 1.7B CustomVoice ; 0.6B Base à remplacer par 0.6B CustomVoice (ou Base + x-vector figé)**

**Risques, du plus au moins grave :**
1. **Charge.** RTF ≈ 0,55–0,62 en français sur ce M5 : 64 % du temps occupé à 1×, **ne suit pas à 2×**. La raison du rejet du 11 oct. (« trop lourd ») n'a pas disparu. Le port Swift fait une synchronisation `item()` par trame et 15 passages séquentiels du prédicteur. Il ne sera pas plus rapide que Python, et **il faut le mesurer**. La fiche doit annoncer une cadence honnête (champ `pace`).
2. **0.6B Base sans voix** (§2.e) : décision produit.
3. **BPE à nous** : ≈ 200 lignes. Risque faible, à condition d'un test de parité d'ids.
4. **Français non natif** : aucun locuteur français. Vivian ou Aiden passent, Ryan est à exclure. Le dialecte forcé d'Eric et Dylan est un bogue du port, à corriger (`Qwen3TTS.swift:928-936`).
5. **Mémoire** : 2,5–3,7 Go, et le trou des plafonds du §5.c.
6. **Effets de bord du loader** : écriture de `tokenizer.json` et **suppression du dossier du modèle** (`:1214-1221`). À retirer.

**Effort :**
- ≈ 3 300 lignes copiées, dont ≈ 600 retirées ou modifiées (`Qwen3TTS.swift` surtout) ;
- ≈ 350 lignes écrites : `KVCache` 70, BPE 200, erreur, `EuclideanCodebook` copié 50, porte 150 ;
- un patch `patches/qwen3tts-mlx.patch` comme pour Kokoro ;
- environ 1,5 fois le travail d'un modèle du lot 2.

### MagpieTTS : **GO, pour le français d'abord ; l'anglais avec le dictionnaire de 3 Mo**

**Risques :**
1. **Qualité en français non entendue avec ce port.** La carte MLX donne un CER de 0 % sur une phrase, et speech-swift fait un aller-retour par ASR. Il faut écouter, et passer au banc d'écoute existant.
2. **Troncature à 23 s** (`maxSteps` 500) : borne à calculer selon le texte.
3. **Voix toutes anglaises** : l'accent en français est à écouter.
4. **Version du modèle** : le port MLX est l'ancienne version à 9 langues, pas la v2607.
5. **RAM en FP32** : ≈ 1,6 Go pour un modèle de 357 M.
6. **Licence NVIDIA** : révocable, avec une clause « guardrail » (version 2025) et des mises à jour imposées. Elle est acceptable en téléchargement par l'utilisateur, à afficher dans la fiche.

**Effort :**
- ≈ 2 200 lignes copiées, ≈ 150 modifiées : retirer le flux, `AudioCommon`, le téléchargeur et les G2P inutiles ; ajouter `shouldStop`, NFC et la borne de longueur ;
- porte ≈ 150 lignes ;
- plus simple que Qwen : rien de mlx-swift-lm, pas de BPE, le français en octets.

### Commun (une fois)

| Travail | Lignes | Effort |
|---|---|---|
| `TextSpeechCompute` (FFI), moteur Rust à une session, catalogue (2 `Runtime`, `companions` à sous-dossiers) | ≈ 400 Rust | |
| `TextSpeechMLXCompute` Swift | ≈ 250 | |
| `MLXBudget` | ≈ 60 | |
| coupe des silences | ≈ 30 | |
| banc lettres/syllabes contre durées Kokoro | ≈ 120 Rust | |
| parité (longueur, absence de NaN, ids BPE) | | sur la CI seulement |

---

## Annexe : commandes de vérification utilisées

```sh
# Arbres et révisions HF
curl -s https://huggingface.co/api/models/<repo>               # sha, licence
curl -s https://huggingface.co/api/models/<repo>/tree/main?recursive=true

# En-têtes safetensors (sans télécharger)
curl -sL -r 0-7 <url> → n
curl -sL -r 8-$((n+7)) <url>
```

Fichiers relevés : `hf/*.tree.json`, `hf/*/config.json`, `hf/st_header.json`, `hf/m_header.json`, `hf/magpie8/**`, `hf/noml.pdf`, `py/modeling.py`.
