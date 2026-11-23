import XCTest
import MLX
import MLXNN
@testable import KokoroMLX

/// Les étages internes de Whisper, écrits pour que le port Rust relance
/// chacun sur l'entrée exacte du Swift et les compare élément par élément
/// (`stt::mlx::whisper::stages`, tâche 17 ; la méthode R54 de la tâche 14).
/// Pas une fixture : un dossier hors du dépôt.
///
/// Ignoré sans `DYSFLOW_STT_MODELS` (le dossier qui contient les trois
/// paquets de Whisper) et `DYSFLOW_STAGES_OUT` (le dossier où écrire
/// `<modèle>-etages.safetensors`), chacun préfixé de `TEST_RUNNER_` pour
/// xcodebuild :
///
///   TEST_RUNNER_DYSFLOW_STT_MODELS=… TEST_RUNNER_DYSFLOW_STAGES_OUT=… \
///   ./scripts/test-macos.sh WhisperStagesTests
///
/// Entrée : le français de la fixture, en `fr`. Étages : la fenêtre de Hann,
/// les caractéristiques, chaque convolution de l'encodeur, les positions,
/// chaque sous-étage du bloc 0, la sortie de l'encodeur ; puis le décodeur
/// sur l'invite (sur la sortie de l'encodeur du Swift), ses logits, et un pas
/// de plus sur le jeton le plus probable.
final class WhisperStagesTests: XCTestCase {
    func testWritesTheWhisperStages() throws {
        let env = ProcessInfo.processInfo.environment
        guard let root = env["DYSFLOW_STT_MODELS"], let out = env["DYSFLOW_STAGES_OUT"] else {
            throw XCTSkip("DYSFLOW_STT_MODELS ou DYSFLOW_STAGES_OUT absent : rien à écrire")
        }
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fr = try ParakeetFixtureTests.pcm16(repo.appendingPathComponent("macos/Tests/Fixtures/dictee-charge.wav"))
        for (id, _) in WhisperFixtureTests.models {
            try autoreleasepool {
                let model = try WhisperModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent(id))
                try Self.write(model, fr, to: URL(fileURLWithPath: out).appendingPathComponent("\(id)-etages.safetensors"))
            }
            Memory.clearCache()
        }
    }

    static func write(_ model: WhisperModel, _ samples: [Float], to url: URL) throws {
        var d: [String: MLXArray] = [:]
        let nFft = WhisperAudioConfig.nFft
        // La fenêtre de `WhisperAudio.logMelSpectrogram`, même expression.
        let nIdx = MLXArray(0..<nFft).asType(.float32)
        d["window"] = 0.5 * (1.0 - cos((2.0 * Float.pi * nIdx) / Float(nFft)))
        let features = WhisperAudio.encoderFeatures(audio: MLXArray(samples), nMels: model.config.numMelBins)
        d["features"] = features

        let enc = model.model.encoder
        let c1 = gelu(enc.conv1(features)); d["e_conv1"] = c1
        let c2 = gelu(enc.conv2(c1)); d["e_conv2"] = c2
        let pos = c2 + enc.embedPositions.weight[0..<c2.shape[1]]; d["e_pos"] = pos
        let l = enc.layers[0]
        let ln1 = l.selfAttnLayerNorm(pos); d["l0_ln1"] = ln1
        let (attn, _, _) = l.selfAttn(ln1, keyValueInput: ln1); d["l0_attn"] = attn
        let res1 = pos + attn; d["l0_res1"] = res1
        let ln2 = l.finalLayerNorm(res1); d["l0_ln2"] = ln2
        let fc1 = l.fc1(ln2); d["l0_fc1"] = fc1
        let g = gelu(fc1); d["l0_gelu"] = g
        let fc2 = l.fc2(g); d["l0_fc2"] = fc2
        d["l0"] = res1 + fc2
        d["l0_direct"] = l(pos)
        let encoded = enc(features); d["encoder"] = encoded
        eval(Array(d.values))

        let special = WhisperSpecialTokens(vocabularySize: model.config.vocabSize)
        let prompt = [special.startOfTranscript, special.french, special.transcribe, special.noTimestamps]
        let promptArray = MLXArray(prompt.map(Int32.init)).expandedDimensions(axis: 0)
        d["d_prompt"] = promptArray
        let dec = model.model.decoder
        var caches = (0..<model.config.decoderLayers).map { _ in WhisperLayerCache() }
        let hidden = dec(tokens: promptArray, startPosition: 0, encoderHidden: encoded, caches: &caches)
        d["d_hidden"] = hidden
        let logits = dec.projectToVocab(hidden[0, -1])
        d["d_logits"] = logits
        eval(logits)
        let token = logits.argMax(axis: -1).item(Int.self)
        d["d_step_token"] = MLXArray([Int32(token)])
        let step = dec(tokens: MLXArray([Int32(token)]).expandedDimensions(axis: 0),
                       startPosition: prompt.count, encoderHidden: encoded, caches: &caches)
        d["d_step_hidden"] = step
        d["d_step_logits"] = dec.projectToVocab(step[0, -1])
        eval(Array(d.values))
        try save(arrays: d, url: url)
    }
}
