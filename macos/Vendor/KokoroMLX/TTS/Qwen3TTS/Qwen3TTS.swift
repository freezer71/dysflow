import Foundation
@preconcurrency import MLX
import MLXNN

/// Les erreurs de la copie de Qwen3-TTS (à la place de celles de
/// MLXAudioCore).
public enum Qwen3TTSError: Error, LocalizedError {
    case missingFile(String)
    case modelNotInitialized(String)
    case invalidInput(String)
    case unknownVoice(String)
    /// `shouldStop` a dit vrai en route : pas un échec.
    case stopped

    public var errorDescription: String? {
        switch self {
        case .missingFile(let f): return "Qwen3-TTS : fichier manquant \(f)"
        case .modelNotInitialized(let m): return "Qwen3-TTS : modèle incomplet (\(m))"
        case .invalidInput(let m): return "Qwen3-TTS : entrée invalide (\(m))"
        case .unknownVoice(let v): return "Qwen3-TTS : voix inconnue « \(v) »"
        case .stopped: return "Qwen3-TTS : synthèse arrêtée"
        }
    }
}

// MARK: - Qwen3TTS Model

public final class Qwen3TTSModel: Module, @unchecked Sendable {
    let config: Qwen3TTSModelConfig
    let talker: Qwen3TTSTalkerForConditionalGeneration
    var speechTokenizer: Qwen3TTSSpeechTokenizer?
    var tokenizer: Qwen2BPETokenizer?

    public var sampleRate: Int { config.sampleRate }

    init(config: Qwen3TTSModelConfig) {
        let talkerConfig = config.talkerConfig ?? {
            let json = "{}".data(using: .utf8)!
            return try! JSONDecoder().decode(Qwen3TTSTalkerConfig.self, from: json)
        }()
        self.config = config
        self.talker = Qwen3TTSTalkerForConditionalGeneration(config: talkerConfig)
    }

    // MARK: - Decode chunk helper

    /// Decode a chunk of codec codes to audio waveform.
    /// - Parameters:
    ///   - codes: Codec codes [1, time, numCodeGroups]
    ///   - chunkTokens: Tokens per decode chunk (controls decode granularity)
    /// - Returns: Decoded audio waveform (1D)
    private func decodeChunk(_ codes: MLXArray, chunkTokens: Int = 300) -> MLXArray {
        guard let speechTokenizer else { return MLXArray.zeros([1]) }

        var audioChunks = [MLXArray]()
        for chunk in speechTokenizer.streamingDecode(codes, chunkTokens: chunkTokens) {
            audioChunks.append(chunk)
        }
        var audio = concatenated(audioChunks, axis: -1)[0]

        let validLen = Int((codes[0..., 0..., 0] .> 0).sum().item(Int32.self))
            * speechTokenizer.decodeUpsampleRate
        if validLen > 0, validLen < audio.dim(0) {
            audio = audio[..<validLen]
        }

        eval(audio)
        return audio
    }

    // MARK: - VoiceDesign generation

    /// Sillon : une phrase entière, décodée d'un coup (`streamingDecode` par
    /// paquets de 300 trames) ; ni clonage, ni flux. `speakerEmbedding` : le
    /// x-vector figé d'une voix (Base). `shouldStop` est consulté toutes les
    /// 12 trames (≈ 1 s de son) et avant le décodage : s'il dit vrai,
    /// `Qwen3TTSError.stopped`.
    func generateVoiceDesign(
        text: String,
        instruct: String?,
        language: String,
        temperature: Float,
        topK: Int,
        topP: Float,
        repetitionPenalty: Float,
        minP: Float,
        maxTokens: Int,
        speakerEmbedding: MLXArray? = nil,
        shouldStop: (() -> Bool)? = nil
    ) throws -> MLXArray {
        guard speechTokenizer != nil, let tokenizer else {
            throw Qwen3TTSError.modelNotInitialized(
                "Qwen3TTS generateVoiceDesign requires both the speech tokenizer and the text tokenizer to be loaded."
            )
        }
        guard let talkerConfig = config.talkerConfig else {
            throw Qwen3TTSError.modelNotInitialized("talker_config")
        }

        // CustomVoice accepts `voice` as "speaker, instruction".
        let isCVModel = config.ttsModelType == "custom_voice"
        let customVoicePrompt = isCVModel ? Self.parseCustomVoicePrompt(instruct) : nil
        let speaker: String? = isCVModel ? customVoicePrompt?.speaker : nil
        let effectiveInstruct: String? = isCVModel ? customVoicePrompt?.instruction : instruct
        let (inputEmbedsInit, trailingTextHidden, ttsPadEmbed) = try prepareGenerationInputs(
            text: text,
            language: language,
            instruct: effectiveInstruct,
            speaker: speaker,
            speakerEmbedding: speakerEmbedding
        )

        // Cap max tokens based on text length
        let targetTokenCount = tokenizer.encode(text: text, allowSpecial: false).count
        let effectiveMaxTokens = min(maxTokens, max(75, targetTokenCount * 6))

        // Initialize cache
        let cache = talker.makeCache()
        var generatedCodes = [MLXArray]()
        var generatedCodebookTokens = [Int]()
        let eosTokenId = talkerConfig.codecEosTokenId

        // Suppress special tokens
        let suppressTokens = (talkerConfig.vocabSize - 1024 ..< talkerConfig.vocabSize)
            .filter { $0 != eosTokenId }

        var trailingIdx = 0
        var inputEmbeds = inputEmbedsInit
        let eosTokenArray = MLXArray([Int32(eosTokenId)]).reshaped(1, 1)
        let codeCache = talker.codePredictor.makeCache()

        for step in 0 ..< effectiveMaxTokens {
            // Sillon : à la place de `Task.checkCancellation()` (appel FFI
            // synchrone, pas de tâche à annuler).
            if step % 12 == 0, shouldStop?() == true { throw Qwen3TTSError.stopped }
            // Forward pass through talker
            let (logits, hidden) = talker(inputEmbeds, cache: cache)

            // Sample first codebook token
            let nextToken = sampleToken(
                logits,
                temperature: temperature,
                topP: topP,
                topK: topK,
                repetitionPenalty: repetitionPenalty,
                generatedTokens: generatedCodebookTokens,
                suppressTokens: suppressTokens,
                eosTokenId: eosTokenId,
                minP: minP
            )

            // Defer sync to the eval boundary with inputEmbeds.
            let isEOS = nextToken .== eosTokenArray

            // Generate remaining codebook tokens with code predictor
            var codeTokens = [nextToken]
            let codeHidden = hidden[0..., (-1)..., 0...]
            for layerCache in codeCache {
                _ = layerCache.trim(layerCache.offset)
            }

            for codeIdx in 0 ..< talkerConfig.numCodeGroups - 1 {
                let codeInput: MLXArray
                if codeIdx == 0 {
                    let code0Embed = talker.getInputEmbeddings()(nextToken)
                    codeInput = concatenated([codeHidden, code0Embed], axis: 1)
                } else {
                    codeInput = talker.codePredictor.codecEmbedding[codeIdx - 1](codeTokens.last!)
                }

                let (codeLogits, _, _) = talker.codePredictor(
                    codeInput, cache: codeCache, generationStep: codeIdx
                )

                let nextCode = sampleToken(
                    codeLogits,
                    temperature: temperature,
                    topP: topP,
                    topK: topK,
                    minP: minP
                )
                codeTokens.append(nextCode)
            }

            let allCodes = concatenated(codeTokens, axis: 1) // [1, num_code_groups]

            // Prepare next input
            let textEmbed: MLXArray
            if trailingIdx < trailingTextHidden.dim(1) {
                textEmbed = trailingTextHidden[0..., trailingIdx ..< (trailingIdx + 1), 0...]
                trailingIdx += 1
            } else {
                textEmbed = ttsPadEmbed
            }

            // Sum all code embeddings for next step
            var codecEmbed = talker.getInputEmbeddings()(nextToken)
            for (i, code) in codeTokens.dropFirst().enumerated() {
                codecEmbed = codecEmbed + talker.codePredictor.codecEmbedding[i](code)
            }

            inputEmbeds = textEmbed + codecEmbed
            eval(inputEmbeds, isEOS)

            let tokenId = Int(nextToken[0, 0].item(Int32.self))
            if isEOS.item(Bool.self) {
                break
            }
            generatedCodebookTokens.append(tokenId)
            generatedCodes.append(allCodes)
            // Sillon : plus de `Memory.clearCache()` tous les 50 pas : le
            // cache de MLX est plafonné (`MLXBudget` dans l'app), le vider
            // ne faisait que rendre puis réallouer les mêmes tampons.
        }

        if shouldStop?() == true { throw Qwen3TTSError.stopped }

        guard !generatedCodes.isEmpty else {
            return MLXArray.zeros([1])
        }

        // Full decode
        let codes = stacked(generatedCodes, axis: 1) // [1, seq_len, num_code_groups]
        let audio = decodeChunk(codes)
        eval(audio)
        return audio
    }

    static func parseCustomVoicePrompt(_ voice: String?) -> (speaker: String, instruction: String?)? {
        guard let voice = voice?.trimmingCharacters(in: .whitespacesAndNewlines),
              !voice.isEmpty else {
            return nil
        }

        guard let commaIndex = voice.firstIndex(of: ",") else {
            return (speaker: voice, instruction: nil)
        }

        let speaker = voice[..<commaIndex].trimmingCharacters(in: .whitespacesAndNewlines)
        let instructionStart = voice.index(after: commaIndex)
        let instruction = voice[instructionStart...].trimmingCharacters(in: .whitespacesAndNewlines)

        guard !speaker.isEmpty else {
            return (speaker: voice, instruction: nil)
        }

        return (
            speaker: String(speaker),
            instruction: instruction.isEmpty ? nil : String(instruction)
        )
    }


    /// Sillon, correction du port : comme l'officiel (`modeling.py:2118-2122`),
    /// le dialecte d'un locuteur (Eric : sichuanais, Dylan : pékinois) ne
    /// remplace la langue que si elle est `chinese` ou `auto`. Le port le
    /// faisait toujours : Eric lisait le français avec l'id d'un dialecte.
    static func effectiveLanguage(requested: String, dialect: String?) -> String {
        guard let dialect, requested == "chinese" || requested == "auto" else { return requested }
        return dialect
    }

    func prepareGenerationInputs(
        text: String,
        language: String,
        instruct: String?,
        speaker: String? = nil,
        speakerEmbedding: MLXArray? = nil
    ) throws -> (MLXArray, MLXArray, MLXArray) {
        guard let tokenizer, let talkerConfig = config.talkerConfig else {
            throw Qwen3TTSError.modelNotInitialized("Tokenizer/config not loaded")
        }

        // Tokenize text with ChatML template
        // `<|im_start|>assistant\n\(text)<|im_end|>\n<|im_start|>assistant\n`.
        // Sillon : la phrase lue est encodée sans jetons spéciaux (un
        // `<|im_end|>` écrit dans un livre est du texte) ; seul le gabarit en
        // a. Mêmes ids que l'amont pour une phrase qui n'en contient pas.
        let tokenIds = tokenizer.encode(text: "<|im_start|>")
            + tokenizer.encode(text: "assistant\n\(text)", allowSpecial: false)
            + tokenizer.encode(text: "<|im_end|>\n<|im_start|>assistant\n")
        let inputIds = MLXArray(tokenIds.map { Int32($0) }).reshaped(1, -1)

        // Get text embeddings
        let textEmbed = talker.textProjection(talker.getTextEmbeddings()(inputIds))

        // TTS special tokens
        let ttsTokens = MLXArray(
            [Int32(config.ttsBosTokenId), Int32(config.ttsEosTokenId), Int32(config.ttsPadTokenId)]
        ).reshaped(1, 3)
        let ttsEmbeds = talker.textProjection(talker.getTextEmbeddings()(ttsTokens))
        let ttsBosEmbed = ttsEmbeds[0..., 0 ..< 1, 0...]
        let ttsEosEmbed = ttsEmbeds[0..., 1 ..< 2, 0...]
        let ttsPadEmbed = ttsEmbeds[0..., 2 ..< 3, 0...]

        // Speaker embedding (CustomVoice models only)
        var speakerEmbed: MLXArray? = nil
        var dialectName: String?
        if let speaker = speaker {
            guard let spkIdMap = talkerConfig.spkId,
                  let spkIdValue = spkIdMap[speaker.lowercased()] else {
                // Sillon : l'amont avertissait et parlait sans locuteur.
                throw Qwen3TTSError.unknownVoice(speaker)
            }
            let spkIdArray = MLXArray([Int32(spkIdValue.intValue)]).reshaped(1, 1)
            speakerEmbed = talker.getInputEmbeddings()(spkIdArray)
            if let dialectMap = talkerConfig.spkIsDialect,
               let dialectVal = dialectMap[speaker.lowercased()],
               dialectVal.isDialect,
               let name = dialectVal.dialectName,
               talkerConfig.codecLanguageId?[name] != nil {
                dialectName = name
            }
        }

        // Language ID (`auto` : aucun) — the speaker dialect only overrides
        // `chinese` / `auto` (Sillon, see `effectiveLanguage`).
        let languageId: Int? = talkerConfig.codecLanguageId?[
            Self.effectiveLanguage(requested: language.lowercased(), dialect: dialectName)
        ]

        // Build codec prefix
        let codecPrefill: [Int32] = if let langId = languageId {
            [
                Int32(talkerConfig.codecThinkId),
                Int32(talkerConfig.codecThinkBosId),
                Int32(langId),
                Int32(talkerConfig.codecThinkEosId)
            ]
        } else {
            [
                Int32(talkerConfig.codecNothinkId),
                Int32(talkerConfig.codecThinkBosId),
                Int32(talkerConfig.codecThinkEosId)
            ]
        }

        var codecEmbed = talker.getInputEmbeddings()(MLXArray(codecPrefill).reshaped(1, -1))
        // Sillon : mode x-vector de l'officiel (`x_vector_only`) — sans
        // locuteur nommé, le x-vector figé prend la place de l'embedding
        // `spk_id`.
        if speakerEmbed == nil, let speakerEmbedding {
            speakerEmbed = speakerEmbedding.asType(codecEmbed.dtype)
        }
        let codecEmbedSuffix = talker.getInputEmbeddings()(
            MLXArray([Int32(talkerConfig.codecPadId), Int32(talkerConfig.codecBosId)]).reshaped(1, 2)
        )
        if let spkEmbed = speakerEmbed {
            codecEmbed = concatenated([codecEmbed, spkEmbed.reshaped([1, 1, -1]), codecEmbedSuffix], axis: 1)
        } else {
            codecEmbed = concatenated([codecEmbed, codecEmbedSuffix], axis: 1)
        }

        // Instruct embedding (VoiceDesign only — not CustomVoice)
        var instructEmbed: MLXArray?
        if let instruct, !instruct.isEmpty {
            let instructText = "<|im_start|>user\n\(instruct)<|im_end|>\n"
            let instructIds = MLXArray(tokenizer.encode(text: instructText).map { Int32($0) }).reshaped(1, -1)
            instructEmbed = talker.textProjection(talker.getTextEmbeddings()(instructIds))
        }

        // Role embedding (first 3 tokens: <|im_start|>assistant\n)
        let roleEmbed = textEmbed[0..., ..<3, 0...]

        // Build pad/bos prefix
        let padCount = codecEmbed.dim(1) - 2
        let padEmbeds = broadcast(ttsPadEmbed, to: [1, padCount, ttsPadEmbed.dim(-1)])
        var combinedEmbed = concatenated([padEmbeds, ttsBosEmbed], axis: 1)
        combinedEmbed = combinedEmbed + codecEmbed[0..., ..<(-1), 0...]

        // Full input embedding
        var inputEmbeds: MLXArray = if let instructEmbed {
            concatenated([instructEmbed, roleEmbed, combinedEmbed], axis: 1)
        } else {
            concatenated([roleEmbed, combinedEmbed], axis: 1)
        }

        // Add first text token (index 3) + last codec embed
        let firstTextEmbed = textEmbed[0..., 3 ..< 4, 0...] + codecEmbed[0..., (-1)..., 0...]
        inputEmbeds = concatenated([inputEmbeds, firstTextEmbed], axis: 1)

        // Trailing text (tokens 4 to -5, plus EOS)
        let trailingTextHidden = concatenated(
            [textEmbed[0..., 4 ..< (textEmbed.dim(1) - 5), 0...], ttsEosEmbed],
            axis: 1
        )

        return (inputEmbeds, trailingTextHidden, ttsPadEmbed)
    }

    // MARK: - Token sampling

    func sampleToken(
        _ logits: MLXArray,
        temperature: Float = 0.9,
        topP: Float = 1.0,
        topK: Int = 50,
        repetitionPenalty: Float = 1.0,
        generatedTokens: [Int]? = nil,
        suppressTokens: [Int]? = nil,
        eosTokenId: Int? = nil,
        minP: Float = 0.0
    ) -> MLXArray {
        var logitsSlice = logits[0..., (-1)..., 0...].squeezed(axis: 1) // [batch, vocab_size]

        // Suppress tokens by setting to -inf
        if let suppress = suppressTokens, !suppress.isEmpty {
            let suppressArr = MLXArray(suppress.map { Int32($0) }).reshaped(1, -1)
            let negInf = MLXArray.full([1, suppress.count], values: MLXArray(-Float.infinity), dtype: logitsSlice.dtype)
            logitsSlice = putAlong(logitsSlice, suppressArr, values: negInf, axis: -1)
        }

        // Repetition penalty
        if let tokens = generatedTokens, !tokens.isEmpty, repetitionPenalty != 1.0 {
            let unique = Array(Set(tokens)).filter { $0 < logitsSlice.dim(-1) }
            if !unique.isEmpty {
                let tokenIds = MLXArray(unique.map { Int32($0) }).reshaped(1, -1)
                let selected = takeAlong(logitsSlice, tokenIds, axis: -1)
                let penalized = which(
                    selected .< 0,
                    selected * repetitionPenalty,
                    selected / repetitionPenalty
                )
                logitsSlice = putAlong(logitsSlice, tokenIds, values: penalized, axis: -1)
            }
        }

        // Greedy if temperature 0
        if temperature <= 0 {
            return argMax(logitsSlice, axis: -1, keepDims: true)
        }

        // Preserve EOS logit so top-k/top-p/min-p do not permanently suppress it.
        let eosLogit: MLXArray? = if let eosTokenId, eosTokenId >= 0, eosTokenId < logitsSlice.dim(-1) {
            logitsSlice[0..., eosTokenId ..< (eosTokenId + 1)]
        } else {
            nil
        }

        // Apply top-k filtering (match mlx_lm.apply_top_k ordering and masking semantics)
        var filteredLogits = logitsSlice
        let vocabSize = logitsSlice.dim(-1)
        if topK > 0, topK < vocabSize {
            let kth = min(topK - 1, max(vocabSize - 1, 0))
            if kth >= 0 {
                let maskIdx = argPartition(-logitsSlice, kth: kth, axis: -1)[0..., topK...]
                let negInf = MLXArray.full(maskIdx.shape, values: MLXArray(-Float.infinity), dtype: logitsSlice.dtype)
                filteredLogits = putAlong(filteredLogits, maskIdx, values: negInf, axis: -1)
            }
        }

        // Apply top-p (nucleus) sampling
        if topP > 0, topP < 1.0 {
            let probs = softmax(filteredLogits, axis: -1)

            // Sort in ASCENDING order (like Python)
            let sortedIndices = argSort(filteredLogits, axis: -1)
            let sortedProbs = takeAlong(probs, sortedIndices, axis: -1)

            // Cumulative probabilities
            let cumProbs = cumsum(sortedProbs, axis: -1)

            // Rearrange cumulative probs back to original order
            // Create inverse index mapping using putAlong
            let vocabSize = sortedIndices.dim(-1)
            let arangeIndices = MLXArray(0 ..< vocabSize).reshaped(1, -1).asType(Int32.self)
            let zeros = MLXArray.zeros(sortedIndices.shape, type: Int32.self)
            let inverseIndices = putAlong(zeros, sortedIndices, values: arangeIndices, axis: -1)
            let cumProbsOrigOrder = takeAlong(cumProbs, inverseIndices, axis: -1)

            // Mask tokens where cumulative prob > (1 - top_p)
            // Keep tokens that are in the top_p nucleus
            let threshold = 1.0 - topP
            let mask = cumProbsOrigOrder .> threshold
            let negInf = MLXArray.full(filteredLogits.shape, values: MLXArray(-Float.infinity), dtype: filteredLogits.dtype)
            filteredLogits = which(mask, filteredLogits, negInf)
        }

        // Apply min-p sampling behavior (default kept at 0.0 for now)
        if minP > 0.0 {
            let scaledMinP = Float(log(Double(minP)))
            // Indices sorted in descending order (like Python `argsort(-logits)`)
            let sortedIndices = argSort(-filteredLogits, axis: -1)
            let sortedLogits = takeAlong(filteredLogits, sortedIndices, axis: -1)
            let topLogits = sortedLogits[0..., 0 ..< 1]
            let scaledMinPArray = MLXArray.full(
                topLogits.shape,
                values: MLXArray(scaledMinP),
                dtype: sortedLogits.dtype
            ) + topLogits
            let removeMask = sortedLogits .< scaledMinPArray
            let negInf = MLXArray.full(sortedLogits.shape, values: MLXArray(-Float.infinity), dtype: sortedLogits.dtype)
            let filteredSortedLogits = which(removeMask, negInf, sortedLogits)

            let invArange = MLXArray(0 ..< vocabSize).reshaped(1, -1).asType(Int32.self)
            let inverseIndices = putAlong(MLXArray.zeros(sortedIndices.shape, type: Int32.self), sortedIndices, values: invArange, axis: -1)
            filteredLogits = takeAlong(filteredSortedLogits, inverseIndices, axis: -1)
        }

        if let eosLogit, let eosTokenId {
            let eosIdx = MLXArray([Int32(eosTokenId)]).reshaped(1, 1)
            filteredLogits = putAlong(filteredLogits, eosIdx, values: eosLogit, axis: -1)
        }

        // Sample with temperature
        let token = categorical(filteredLogits / temperature)
        return token.reshaped(1, 1)
    }


    // MARK: - Loading

    /// Sillon : synchrone ; lit `config.json`, `model.safetensors` (seul),
    /// `vocab.json`, `merges.txt`, `tokenizer_config.json` et
    /// `speech_tokenizer/{config.json,model.safetensors}`. N'écrit, ne
    /// modifie ni n'efface rien dans `modelDir`.
    public static func fromModelDirectory(_ modelDir: URL) throws -> Qwen3TTSModel {
        // Load main config
        let configData = try Data(contentsOf: modelDir.appendingPathComponent("config.json"))
        let config = try JSONDecoder().decode(Qwen3TTSModelConfig.self, from: configData)

        // Sillon : le codec doit être un dossier, vérifié avant de lire les
        // poids. L'amont effaçait le dossier du modèle s'il ne l'était pas.
        let speechTokenizerPath = modelDir.appendingPathComponent("speech_tokenizer")
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: speechTokenizerPath.path, isDirectory: &isDir),
              isDir.boolValue else {
            throw Qwen3TTSError.missingFile("speech_tokenizer/")
        }

        let model = Qwen3TTSModel(config: config)

        // Sillon : les dictionnaires de poids intermédiaires (le fichier
        // entier, `speaker_encoder.*` compris, et sa version nettoyée) vivent
        // dans ce bloc seulement : relâchés avant la quantification de
        // l'embedding texte, ils ne gardent pas la copie BF16 en vie à côté
        // de la copie 8 bits.
        do {
            // Load talker weights
            let allWeights = try MLX.loadArrays(url: modelDir.appendingPathComponent("model.safetensors"))

            // Sanitize and load talker weights
            let talkerWeights = Qwen3TTSTalkerForConditionalGeneration.sanitize(weights: allWeights)
            let talkerPairs = talkerWeights.map { ($0.key, $0.value) }

            // Quantized checkpoints store packed weights and companion .scales tensors.
            // Convert talker Linear layers before loading those tensors.
            if config.quantization != nil || config.perLayerQuantization != nil {
                quantize(model: model.talker) { path, _ in
                    guard talkerWeights["\(path).scales"] != nil else {
                        return nil
                    }

                    if let perLayerQuant = config.perLayerQuantization,
                       let layerQuant = perLayerQuant.quantization(layer: path) {
                        return layerQuant.asTuple
                    }

                    return config.quantization?.asTuple
                }
            }

            try model.talker.update(parameters: ModuleParameters.unflattened(talkerPairs), verify: .all)
            eval(model.talker.parameters())
        }

        // Sillon : l'embedding texte, non quantifié dans les dépôts (622 Mo
        // en BF16), passe en 8 bits au chargement, comme au banc d'écoute.
        quantize(model: model.talker, groupSize: 64, bits: 8, mode: .affine, filter: { path, module in
            path.hasSuffix("text_embedding") && !(module is QuantizedEmbedding)
        })
        eval(model.talker.parameters())

        // Sillon : notre encodeur BPE (pas de swift-transformers, pas de
        // `tokenizer.json` écrit dans le dossier).
        model.tokenizer = try Qwen2BPETokenizer(directory: modelDir)

        try loadSpeechTokenizer(model: model, path: speechTokenizerPath)
        // Sillon : le cache de MLX vidé en fin de chargement (tampons de la
        // copie BF16, des tenseurs `speaker_encoder.*` et de l'encodeur du
        // codec écartés) : le pic du chargement ne reste pas tenu.
        Memory.clearCache()
        return model
    }

    private static func loadSpeechTokenizer(model: Qwen3TTSModel, path: URL) throws {
        let configData = try Data(contentsOf: path.appendingPathComponent("config.json"))
        let tokenizerConfig = try JSONDecoder().decode(Qwen3TTSTokenizerConfig.self, from: configData)

        let speechTokenizer = Qwen3TTSSpeechTokenizer(config: tokenizerConfig)

        // Load weights — Sillon : `model.safetensors` seul, sans l'encodeur
        // (`encoder.*` brut, `encoder_model.*` une fois nettoyé), qui ne sert
        // qu'au clonage : `verify: .all` refuserait ces clés sans module.
        let tokenizerWeights = try MLX.loadArrays(url: path.appendingPathComponent("model.safetensors"))
            .filter { !$0.key.hasPrefix("encoder.") }

        let sanitized = Qwen3TTSSpeechTokenizer.sanitize(weights: tokenizerWeights)
            .filter { !$0.key.hasPrefix("encoder_model.") }
        let pairs = sanitized.map { ($0.key, $0.value) }
        try speechTokenizer.update(parameters: ModuleParameters.unflattened(pairs), verify: .all)
        eval(speechTokenizer.parameters())

        model.speechTokenizer = speechTokenizer
    }
}
