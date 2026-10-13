import Foundation
@preconcurrency import MLX
import MLXNN

/// Kokoro-82M (StyleTTS2), repris de mlx-audio-swift (MIT, Prince Canuma,
/// commit 244458d) : le passage avant seul. Phonétisation, voix, vitesse et
/// minutage sont côté Sillon ; la vitesse reste 1.
public final class KokoroModel: Module, @unchecked Sendable {
    public let config: KokoroConfig
    public let sampleRate: Int

    @ModuleInfo var bert: Albert
    @ModuleInfo(key: "bert_encoder") var bertEncoder: Linear
    @ModuleInfo var predictor: KokoroProsodyPredictor
    @ModuleInfo(key: "text_encoder") var textEncoder: KokoroTextEncoder
    @ModuleInfo var decoder: KokoroDecoder

    private init(config: KokoroConfig) {
        self.config = config
        self.sampleRate = config.sampleRate

        _bert = ModuleInfo(wrappedValue: Albert(config: config.plbert, vocabSize: config.nToken))
        _bertEncoder = ModuleInfo(
            wrappedValue: Linear(config.plbert.hiddenSize, config.hiddenDim),
            key: "bert_encoder"
        )
        _predictor = ModuleInfo(wrappedValue: KokoroProsodyPredictor(
            styleDim: config.styleDim, dHid: config.hiddenDim,
            nLayers: config.nLayer, maxDur: config.maxDur, dropout: 0.0
        ))
        _textEncoder = ModuleInfo(wrappedValue: KokoroTextEncoder(
            channels: config.hiddenDim, kernelSize: config.textEncoderKernelSize,
            depth: config.nLayer, nSymbols: config.nToken
        ), key: "text_encoder")
        _decoder = ModuleInfo(wrappedValue: KokoroDecoder(config: config))
    }

    // MARK: - Passage avant

    /// `shouldStop` (ajout de Sillon) : consulté quand les durées sont
    /// calculées, puis dans le décodeur après `encode`, après chaque bloc de
    /// `decode` et après chaque étage du générateur (chaque fois après une
    /// évaluation) ; s'il dit vrai, le passage s'arrête là et rend `nil`.
    /// `nil` : aucun point d'arrêt, aucune synchronisation de plus.
    public func callAsFunction(
        inputIds: MLXArray, refS: MLXArray, speed: Float = 1.0,
        shouldStop: (() -> Bool)? = nil
    ) -> (audio: MLXArray, predDur: MLXArray)? {
        let seqLen = inputIds.shape[inputIds.ndim - 1]
        let inputLengths = MLXArray([Int32(seqLen)])
        var textMask = MLXArray(Array(0..<Int32(seqLen))).reshaped([1, -1])
        textMask = (textMask + 1) .> inputLengths.reshaped([-1, 1])

        let attMask = MLXArray(1) - textMask.asType(.int32)
        let (bertOut, _) = bert(inputIds, attentionMask: attMask)
        let dEn = bertEncoder(bertOut).transposed(0, 2, 1)

        let globalStyle = refS[0..., 128...]
        let acousticStyle = refS[0..., ..<128]

        let d = predictor.textEncoder(dEn, style: globalStyle, textLengths: inputLengths, mask: textMask)
        let (x, _) = predictor.lstm(d)
        let duration = predictor.durationProj(x)
        // Défensif : une durée NaN donnerait un entier absurde ; bornée.
        let maxFramesPerPhoneme = 100
        let durRaw = MLX.sigmoid(duration).sum(axis: -1) / speed
        let durSafe = nanToNum(durRaw, nan: 1.0)
        let predDur = MLX.clip(MLX.round(durSafe), min: 1, max: Float(maxFramesPerPhoneme))
            .asType(.int32)[0]

        let durArray: [Int32] = predDur.asArray(Int32.self)
        // Sillon : point d'arrêt, après la synchronisation des durées.
        if shouldStop?() == true { return nil }
        var indices = [MLXArray]()
        for (i, n) in durArray.enumerated() {
            let count = min(max(Int(n), 0), maxFramesPerPhoneme)
            if count > 0 {
                indices.append(MLX.repeated(MLXArray(Int32(i)), count: count))
            }
        }

        guard !indices.isEmpty else {
            let silence = MLXArray.zeros([1, 1])
            return (silence, predDur)
        }
        let allIndices = MLX.concatenated(indices, axis: 0)

        let predAlnTrg = MLXArray.zeros([inputIds.shape[1], allIndices.shape[0]])
        predAlnTrg[allIndices, MLXArray(Array(0..<Int32(allIndices.shape[0])))] = MLXArray(Float(1))
        let predAln = predAlnTrg.expandedDimensions(axis: 0)

        let en = MLX.matmul(d.transposed(0, 2, 1), predAln)
        let (f0Pred, nPred) = predictor.predict(en, globalStyle)

        let tEn = textEncoder(inputIds, inputLengths: inputLengths, mask: textMask)
        let asr = MLX.matmul(tEn, predAln)

        guard let audio = decoder(asr, f0Pred, nPred, acousticStyle, shouldStop: shouldStop) else { return nil }
        return (audio[0], predDur)
    }

    // MARK: - Chargement

    /// `config.json` et les poids `*.safetensors` du dossier (les voix, dans
    /// `voices/`, n'en font pas partie).
    public static func load(directory: URL) throws -> KokoroModel {
        let configData = try Data(contentsOf: directory.appendingPathComponent("config.json"))
        let config = try JSONDecoder().decode(KokoroConfig.self, from: configData)
        let model = KokoroModel(config: config)
        let weights = try loadWeights(modelDir: directory)
        let sanitized = model.sanitize(weights: weights)
        try model.update(parameters: ModuleParameters.unflattened(sanitized), verify: .noUnusedKeys)
        model.train(false)
        MLX.eval(model.parameters())
        return model
    }

    func sanitize(weights: [String: MLXArray]) -> [String: MLXArray] {
        let hasPackedQuantizedWeights = weights.keys.contains {
            $0.hasSuffix(".scales") || $0.hasSuffix(".biases")
        }
        let needsConvTranspose = !hasPackedQuantizedWeights

        var result = [String: MLXArray]()
        for (k, v) in weights {
            if k.contains("position_ids") { continue }

            var nk = k

            // LSTM : l'inverse d'abord, pour ne pas toucher aux clés avant.
            nk = nk.replacingOccurrences(of: ".weight_ih_l0_reverse", with: ".Wx_backward")
            nk = nk.replacingOccurrences(of: ".weight_hh_l0_reverse", with: ".Wh_backward")
            nk = nk.replacingOccurrences(of: ".bias_ih_l0_reverse", with: ".bias_ih_backward")
            nk = nk.replacingOccurrences(of: ".bias_hh_l0_reverse", with: ".bias_hh_backward")
            nk = nk.replacingOccurrences(of: ".weight_ih_l0", with: ".Wx_forward")
            nk = nk.replacingOccurrences(of: ".weight_hh_l0", with: ".Wh_forward")
            nk = nk.replacingOccurrences(of: ".bias_ih_l0", with: ".bias_ih_forward")
            nk = nk.replacingOccurrences(of: ".bias_hh_l0", with: ".bias_hh_forward")

            nk = nk.replacingOccurrences(of: ".gamma", with: ".weight")
            nk = nk.replacingOccurrences(of: ".beta", with: ".bias")
            nk = nk.replacingOccurrences(of: ".alpha1.", with: ".alpha1_")
            nk = nk.replacingOccurrences(of: ".alpha2.", with: ".alpha2_")

            var value = v
            if needsConvTranspose {
                if (nk.contains("F0_proj.weight") || nk.contains("N_proj.weight")) && v.ndim == 3 {
                    value = v.transposed(0, 2, 1)
                } else if nk.contains("noise_convs") && nk.hasSuffix(".weight") && v.ndim == 3 {
                    value = v.transposed(0, 2, 1)
                } else if nk.hasSuffix("weight_v") && v.ndim == 3 {
                    let (o, h, w) = (v.shape[0], v.shape[1], v.shape[2])
                    if !(o >= h && o >= w && h == w) {
                        value = v.transposed(0, 2, 1)
                    }
                }
            }

            result[nk] = value
        }
        return result
    }

    private static func loadWeights(modelDir: URL) throws -> [String: MLXArray] {
        let url = modelDir.appendingPathComponent("model.safetensors")
        if FileManager.default.fileExists(atPath: url.path) {
            return try MLX.loadArrays(url: url)
        }
        var allWeights = [String: MLXArray]()
        let files = try FileManager.default.contentsOfDirectory(at: modelDir, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "safetensors" && !$0.lastPathComponent.contains("voices") }
        for file in files {
            for (k, v) in try MLX.loadArrays(url: file) {
                allWeights[k] = v
            }
        }
        return allWeights
    }
}
