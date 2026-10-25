// Modifié par Dysflow (Apache-2.0 §4(b)) : chargement d'un dossier, arrêt `shouldStop`, bornes de longueur, une lecture GPU par trame — voir ORIGINE.md.
import Foundation
import MLX
import MLXNN

/// Magpie-TTS Multilingual 357M (NVIDIA) — MLX port for Apple Silicon.
///
/// Pipeline per synthesis call:
///   1. Per-language tokeniser → vocab IDs (T text tokens).
///   2. ``MagpieTextEncoder`` runs once over the prompt (6 layers).
///   3. ``MagpieDecoder.prefill(...)`` seeds the 110-frame baked speaker
///      context + BOS frame into the KV cache.
///   4. AR loop: at each step the LocalTransformer samples the 8 codebooks
///      sequentially, then the next decoder hidden is produced by
///      ``MagpieDecoder.step(...)``. EOS detection on the per-codebook
///      parallel head.
///   5. ``MagpieNanoCodec`` decodes the (T_frames, 8) code matrix to a 22.05
///      kHz waveform.
public final class MagpieTTS {

    // Sub-modules
    public let textEncoder: MagpieTextEncoder
    public let decoder: MagpieDecoder
    public let localTransformer: MagpieLocalTransformer
    public let nanoCodec: MagpieNanoCodec

    // Tokenisers per language (loaded lazily on first use).
    private let tokenizerDir: URL
    private var tokenizers: [MagpieLanguage: MagpieTokenizer] = [:]

    // Configs (kept around for downstream queries — sample rate, codebook count…)
    public let decoderConfig: MagpieDecoderConfig
    public let nanoCodecConfig: MagpieNanoCodecConfig

    public static let sampleRate = 22050
    public static let codecFramesPerSecond: Double = Double(22050) / Double(1024)  // 21.5

    public init(textEncoder: MagpieTextEncoder, decoder: MagpieDecoder,
                localTransformer: MagpieLocalTransformer, nanoCodec: MagpieNanoCodec,
                decoderConfig: MagpieDecoderConfig, nanoCodecConfig: MagpieNanoCodecConfig,
                tokenizerDir: URL) {
        self.textEncoder = textEncoder
        self.decoder = decoder
        self.localTransformer = localTransformer
        self.nanoCodec = nanoCodec
        self.decoderConfig = decoderConfig
        self.nanoCodecConfig = nanoCodecConfig
        self.tokenizerDir = tokenizerDir
    }

    // MARK: - Loading

    /// Lit `text_encoder/`, `decoder_prefill/`, `nanocodec_decoder/` et
    /// `tokenizer/` sous `directory` (le dossier du dépôt HF). `decoder_step/`
    /// n'est pas lu (mêmes poids que `decoder_prefill/`).
    public static func load(directory: URL) throws -> MagpieTTS {
        let textEncoderDir = directory.appendingPathComponent("text_encoder")
        let decoderPrefillDir = directory.appendingPathComponent("decoder_prefill")
        let nanocodecDir = directory.appendingPathComponent("nanocodec_decoder")
        let tokenizerDir = directory.appendingPathComponent("tokenizer")
        let teCfg = try decode(
            MagpieTextEncoderConfig.self,
            from: textEncoderDir.appendingPathComponent("config.json"))
        let decCfg = try decode(
            MagpieDecoderConfig.self,
            from: decoderPrefillDir.appendingPathComponent("config.json"))
        let codecCfg = try decode(
            MagpieNanoCodecConfig.self,
            from: nanocodecDir.appendingPathComponent("config.json"))

        let te = try MagpieWeightLoader.loadTextEncoder(
            bundleDir: textEncoderDir, config: teCfg)
        let (dec, lt) = try MagpieWeightLoader.loadDecoder(
            bundleDir: decoderPrefillDir, config: decCfg)
        let codec = try MagpieWeightLoader.loadNanoCodec(
            bundleDir: nanocodecDir, config: codecCfg)

        return MagpieTTS(textEncoder: te, decoder: dec,
                         localTransformer: lt, nanoCodec: codec,
                         decoderConfig: decCfg, nanoCodecConfig: codecCfg,
                         tokenizerDir: tokenizerDir)
    }

    private static func decode<T: Decodable>(_ type: T.Type, from url: URL) throws -> T {
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw MagpieTTSError.missingFile(url.lastPathComponent)
        }
        let data = try Data(contentsOf: url)
        return try JSONDecoder().decode(T.self, from: data)
    }

    // MARK: - Tokeniser access

    public func tokenizer(for language: MagpieLanguage) throws -> MagpieTokenizer {
        if let cached = tokenizers[language] { return cached }
        // Japanese has no shipped JSON; reuse the EN vocab + a katakana
        // transliteration fall-back inside the tokenizer.
        let fileLang = (language == .japanese) ? MagpieLanguage.english : language
        let url = tokenizerDir.appendingPathComponent("\(fileLang.rawValue).json")
        let tok = try MagpieTokenizer.load(from: url, language: language)
        tokenizers[language] = tok
        return tok
    }

    // MARK: - Synthesis (batch)

    /// Synthesize a complete utterance in one pass. Returns 22.05 kHz PCM
    /// (Float32, ±1.0).
    public func synthesize(text: String,
                           speaker: MagpieSpeaker = .sofia,
                           language: MagpieLanguage = .english,
                           prephonemized: Bool = false,
                           params: MagpieTTSParams = MagpieTTSParams(),
                           shouldStop: (() -> Bool)? = nil) throws -> [Float] {
        if let seed = params.seed { MLXRandom.seed(seed) }
        let tok = try tokenizer(for: language)
        let ids = tok.tokenize(text, prephonemized: prephonemized)
        if ids.isEmpty {
            throw MagpieTTSError.textEncodingFailed("empty token sequence after tokenisation")
        }
        let codes = try sampleCodeSequence(
            textIds: ids, speaker: speaker, params: params, shouldStop: shouldStop)
        if shouldStop?() == true { throw MagpieTTSError.stopped }
        if codes.isEmpty { return [] }
        let codeMatrix = MLXArray(codes.flatMap { $0 }, [codes.count, MagpieNumCodebooks])
            .reshaped([1, codes.count, MagpieNumCodebooks])
        let audio = nanoCodec(codeMatrix)
        eval(audio)
        return audio[0].asArray(Float.self)
    }

    // MARK: - Core AR loop

    /// Run the AR loop and return the sampled `T_frames × 8` code matrix as a
    /// flat list of frames. `shouldStop` est consulté toutes les 20 trames
    /// (≈ 1 s de son) ; s'il dit vrai : `MagpieTTSError.stopped`.
    private func sampleCodeSequence(textIds: [Int],
                                     speaker: MagpieSpeaker,
                                     params: MagpieTTSParams,
                                     shouldStop: (() -> Bool)? = nil) throws -> [[Int32]] {
        // Dysflow : au-delà de `max_len`, les plongements de position seraient
        // lus hors table (comportement indéfini dans MLX). `MagpieRunner`
        // découpe le texte avant ; ce test reste la dernière barrière.
        guard textIds.count <= textEncoder.config.maxLen else {
            throw MagpieTTSError.textEncodingFailed(
                "phrase trop longue pour Magpie : \(textIds.count) jetons, \(textEncoder.config.maxLen) au plus")
        }
        // Même raison côté décodeur : dernière position = baked_T + pas − 1.
        let maxSteps = min(params.maxSteps, decoderConfig.maxLen - decoderConfig.bakedT - 1)
        let textIdsArr = MLXArray(textIds.map(Int32.init), [textIds.count])
            .reshaped([1, textIds.count])
        let memMask = MLXArray.ones(textIdsArr.shape, dtype: .float32)
        let encOut = textEncoder(textIdsArr, mask: memMask)
        eval(encOut)

        var (hLast, caches) = decoder.prefill(
            speakerIdx: speaker.rawValue,
            encoderOutput: encOut,
            encoderMask: memMask)
        eval(hLast)

        var frames: [[Int32]] = []
        var position = decoderConfig.bakedT
        for step in 0..<max(0, maxSteps) {
            if step % 20 == 0, shouldStop?() == true { throw MagpieTTSError.stopped }
            let codes = sampleLocalTransformer(
                hLast: hLast,
                forbidEos: (step < params.minFrames),
                temperature: params.temperature,
                topK: params.topK)
            if step >= params.minFrames, codes.contains(MagpieAudioEosId) {
                break
            }
            frames.append(codes)
            // Advance decoder by one frame.
            let codesArr = MLXArray(codes, [MagpieNumCodebooks])
                .reshaped([1, 1, MagpieNumCodebooks])
            let audioEmb = decoder.embedAudioFrame(codesArr)
            let (_, h, _) = decoder.step(
                audioEmb: audioEmb,
                encoderOutput: encOut,
                encoderMask: memMask,
                caches: caches,
                position: position)
            hLast = h
            position += 1
            // Dysflow : pas d'`eval(hLast)` ici ; le pas du décodeur est
            // calculé avec la trame suivante, à son unique lecture.
        }
        return frames
    }

    // MARK: - LocalTransformer per-frame sampling

    /// Sample the 8 codebooks for one frame autoregressively.
    /// Public so the CoreML backend (which runs the big models on
    /// CoreML/ANE but borrows the MLX-loaded LocalTransformer for
    /// per-frame sampling) can drive sampling from a CoreML-produced
    /// `h_last` without re-implementing the LT in pure Swift.
    public func sampleLocalTransformer(hLast: MLXArray,
                                        forbidEos: Bool,
                                        temperature: Float,
                                        topK: Int) -> [Int32] {
        let cache = MagpieKVCache()
        var x = localTransformer.inProjection(hLast)
        // Dysflow : les jetons restent sur le GPU d'un livre de codes au
        // suivant (mêmes opérations, même tirage), lus une seule fois par
        // trame au lieu de huit.
        var tokens = [MLXArray]()
        tokens.reserveCapacity(MagpieNumCodebooks)
        for k in 0..<MagpieNumCodebooks {
            let h = localTransformer(x, position: k, cache: cache)
            // The last position's hidden produces the codebook-k logits.
            let tail = h[0..., (h.dim(1) - 1)..<h.dim(1), 0...]
            var logits = localTransformer.outProjections[k](tail[0])  // (1, V)
            var forbid: [Int] = [Int(MagpieAudioBosId)]
            if forbidEos { forbid.append(Int(MagpieAudioEosId)) }
            logits = forbidIds(logits, ids: forbid)
            let tok = sampleTopK(logits, temperature: temperature, k: topK)
            tokens.append(tok)
            if k < MagpieNumCodebooks - 1 {
                let idArr = tok.reshaped([1, 1])  // int32
                let emb = decoder.audioEmbeddings[k](idArr)  // (1, 1, d_dec)
                x = localTransformer.inProjection(emb)
            }
        }
        return concatenated(tokens).asArray(Int32.self)
    }
}
