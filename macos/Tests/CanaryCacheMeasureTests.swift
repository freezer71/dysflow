import Foundation
import XCTest
import MLX
@testable import Dysflow
@testable import KokoroMLX

/// Mesure (tâche 18) : pourquoi `CanaryDecoderCacheTests` échoue sur un M5.
/// Ignoré sans `DYSFLOW_CANARY_MESURE` (`TEST_RUNNER_DYSFLOW_CANARY_MESURE`
/// pour xcodebuild) : le dossier où écrire le relevé, un fichier texte par
/// exécution. Lancer deux fois, avec et sans `TEST_RUNNER_MLX_ENABLE_TF32=0`
/// (MLX lit `MLX_ENABLE_TF32` une seule fois par processus).
///
/// - le décodeur au hasard du test, sur le GPU puis sur le processeur :
///   écart du pas à pas et de la suite entière, chacun contre la suite
///   entière calculée sur le processeur (Float32 sans TF32) ;
/// - un seul produit matriciel, une ligne (M = 1) contre cinq (M = 5) ;
/// - avec `DYSFLOW_STT_MODELS` (dossier contenant `canary_1b_v2_q8/`) : le
///   texte de trois audios, décodés avec le cache (le chemin de l'app,
///   `CanarySpeech`) puis en repassant toute la suite à chaque jeton.
final class CanaryCacheMeasureTests: XCTestCase {
    private var lines: [String] = []

    private func log(_ line: String) {
        lines.append(line)
        print("[canary-mesure] \(line)")
    }

    func testMeasure() throws {
        let env = ProcessInfo.processInfo.environment
        guard let out = env["DYSFLOW_CANARY_MESURE"] else {
            throw XCTSkip("DYSFLOW_CANARY_MESURE absent")
        }
        let tf32 = env["MLX_ENABLE_TF32"] ?? "(défaut : 1)"
        let info = GPU.deviceInfo()
        log("MLX_ENABLE_TF32=\(tf32) GPU architecture=\(info.architecture)")
        log("macOS \(ProcessInfo.processInfo.operatingSystemVersionString)")

        matmul()
        synthetic(name: "testStepByStepMatchesTheFullPass", encoderLength: 20, count: 16, prompt: 5) { ($0 * 7 + 3) % 50 }
        synthetic(name: "testPositionsPastTheTableRepeatTheLastOne", encoderLength: 8, count: 1030, prompt: 1020) { ($0 * 11 + 1) % 50 }
        if let models = env["DYSFLOW_STT_MODELS"] {
            try realModel(URL(fileURLWithPath: models).appendingPathComponent("canary_1b_v2_q8"))
        } else {
            log("DYSFLOW_STT_MODELS absent : pas de mesure sur le vrai modèle")
        }

        let name = "canary-mesure-tf32-\(env["MLX_ENABLE_TF32"] ?? "defaut").txt"
        try (lines.joined(separator: "\n") + "\n").write(
            to: URL(fileURLWithPath: out).appendingPathComponent(name), atomically: true, encoding: .utf8)
    }

    // MARK: - Un produit matriciel

    /// x[M, 32] · w[32, 50] en Float32 : la ligne 0 selon M = 1 ou M = 5,
    /// sur le GPU et sur le processeur.
    private func matmul() {
        MLXRandom.seed(11)
        let x = MLXRandom.normal([5, 32])
        let w = MLXRandom.normal([32, 50])
        eval(x, w)
        let cpu1 = Device.withDefaultDevice(.cpu) { MLX.matmul(x[0..<1], w) }
        let cpu5 = Device.withDefaultDevice(.cpu) { MLX.matmul(x, w)[0..<1] }
        let gpu1 = Device.withDefaultDevice(.gpu) { MLX.matmul(x[0..<1], w) }
        let gpu5 = Device.withDefaultDevice(.gpu) { MLX.matmul(x, w)[0..<1] }
        let big = Device.withDefaultDevice(.cpu) { MLX.abs(cpu1).max() }
        eval(cpu1, cpu5, gpu1, gpu5, big)
        log("matmul Float32 [M,32]·[32,50], ligne 0, |y|max=\(big.item(Float.self))")
        log("  gpu M=1 contre cpu : \(gap(gpu1, cpu1))")
        log("  gpu M=5 contre cpu : \(gap(gpu5, cpu1))")
        log("  cpu M=5 contre cpu M=1 : \(gap(cpu5, cpu1))")
    }

    // MARK: - Le décodeur au hasard du test

    private func makeDecoder() -> CanaryDecoder {
        MLXRandom.seed(7)
        let config = CanaryDecoderConfig(numLayers: 2, hiddenSize: 32, numAttentionHeads: 4, innerSize: 64)
        let decoder = CanaryDecoder(config: config, vocabSize: 50, hiddenSize: 32)
        decoder.train(false)
        return decoder
    }

    private func gap(_ a: MLXArray, _ b: MLXArray) -> Float {
        Device.withDefaultDevice(.cpu) { MLX.abs(a - b).max().item(Float.self) }
    }

    /// Mêmes poids, même encodeur, mêmes jetons que le test : à chaque pas,
    /// le pas à pas (cache) et la suite entière sur `device`, chacun contre
    /// la suite entière sur le processeur.
    private func synthetic(name: String, encoderLength: Int, count: Int, prompt: Int, token: (Int) -> Int) {
        let decoder = makeDecoder()
        let encoder = MLXRandom.normal([1, encoderLength, 32])
        let mask = MLXArray.ones([1, encoderLength])
        eval(encoder, mask)
        let tokens = (0..<count).map(token)

        func full(_ k: Int) -> MLXArray {
            let ids = MLXArray(tokens[..<k].map(Int32.init)).reshaped(1, k)
            let logits = decoder(ids, encoderOutput: encoder, encoderMask: mask)
            return logits[0, k - 1]
        }
        let checked = Array((prompt + 1)...count)
        var reference: [Int: MLXArray] = [:]
        Device.withDefaultDevice(.cpu) {
            for k in [prompt] + checked {
                let r = full(k)
                eval(r)
                reference[k] = r
            }
        }
        for device in [Device.gpu, Device.cpu] {
            Device.withDefaultDevice(device) {
                let state = decoder.makeState(encoderOutput: encoder, encoderMask: mask)
                let promptIds = MLXArray(tokens[..<prompt].map(Int32.init)).reshaped(1, prompt)
                let first = decoder(promptIds, state: state)
                let p = first[0, prompt - 1]
                let fp = full(prompt)
                eval(p, fp)
                log("\(name) sur \(device) — dtype \(p.dtype), |logits|max=\(gap(p, MLXArray(Float(0))))")
                log("  invite (\(prompt) jetons) : cache-entière \(gap(p, fp)), cache-cpu \(gap(p, reference[prompt]!)), entière-cpu \(gap(fp, reference[prompt]!))")
                var worst: (k: Int, gap: Float) = (0, 0)
                var firstOver: Int?
                var argmaxDiffers = 0
                var maxCacheCpu: Float = 0
                var maxFullCpu: Float = 0
                for k in checked {
                    let ids = MLXArray([Int32(tokens[k - 1])]).reshaped(1, 1)
                    let s = decoder(ids, state: state)[0, 0]
                    let f = full(k)
                    eval(s, f)
                    let g = gap(s, f)
                    if g > worst.gap { worst = (k, g) }
                    if g >= 1e-4, firstOver == nil { firstOver = k }
                    if s.argMax().item(Int.self) != f.argMax().item(Int.self) { argmaxDiffers += 1 }
                    maxCacheCpu = max(maxCacheCpu, gap(s, reference[k]!))
                    maxFullCpu = max(maxFullCpu, gap(f, reference[k]!))
                }
                log("  pas de 1 jeton (\(checked.count)) : cache-entière max \(worst.gap) (jeton \(worst.k)), premier ≥ 1e-4 : \(firstOver.map(String.init) ?? "aucun"), argmax différent : \(argmaxDiffers)")
                log("    cache-cpu max \(maxCacheCpu), entière-cpu max \(maxFullCpu)")
            }
        }
    }

    // MARK: - Le vrai modèle

    private func realModel(_ directory: URL) throws {
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try ParakeetFixtureTests.inputs(repo).filter {
            ["francais", "anglais", "douze_secondes"].contains($0.name)
        }
        let model = try CanaryModel.fromModelDirectory(directory)
        let speech = CanarySpeech(model)
        for input in inputs {
            let language = input.name == "anglais" ? "en" : "fr"
            let cached = try speech.transcribe(input.samples, language: language)
            let measured = try decodeBoth(model, input.samples, language: language)
            log("audio \(input.name) (\(input.samples.count) échantillons, \(language)) — dtype logits \(measured.dtype)")
            log("  cache (app)  : \(cached)")
            log("  cache (ici)  : \(measured.cachedText)")
            log("  entière      : \(measured.fullText)")
            log("  jetons cache \(measured.cachedTokens.count), entière \(measured.fullTokens.count), premier jeton différent : \(measured.firstDiff.map(String.init) ?? "aucun")")
            log("  sur la suite du cache : écart des logits max \(measured.maxGap) (pas \(measured.maxGapStep)), |logits|max \(measured.maxLogit), marge top1-top2 min \(measured.minMargin), argmax différent : \(measured.argmaxDiffers)")
            log("  texte identique : \(measured.cachedText == measured.fullText && cached == measured.cachedText)")
        }
    }

    private struct Both {
        var dtype: DType = .float32
        var cachedTokens: [Int] = []
        var fullTokens: [Int] = []
        var cachedText = ""
        var fullText = ""
        var firstDiff: Int?
        var maxGap: Float = 0
        var maxGapStep = 0
        var maxLogit: Float = 0
        var minMargin: Float = .infinity
        var argmaxDiffers = 0
    }

    /// Le décodage glouton de `CanaryModel.generate`, deux fois : avec le
    /// cache (et, à chaque pas, la suite entière sur le même préfixe), puis
    /// en repassant toute la suite à chaque jeton (l'avant-cache).
    private func decodeBoth(_ model: CanaryModel, _ samples: [Float], language: String) throws -> Both {
        var result = Both()
        let mel = model.preprocessAudio(MLXArray(samples)).asType(.float32)
        let encoded = model.encode(mel: mel)
        let prompt = try XCTUnwrap(model.tokenizer).buildPromptTokens(
            config: model.config, sourceLanguage: language, targetLanguage: language)
        let eos = try XCTUnwrap(model.tokenizer).eosTokenId(config: model.config)
        let decoder = model.decoder

        func full(_ tokens: [Int]) -> MLXArray {
            let ids = MLXArray(tokens.map(Int32.init)).reshaped(1, tokens.count)
            let logits = decoder(ids, encoderOutput: encoded.hidden, encoderMask: encoded.mask)
            return logits[0, tokens.count - 1]
        }

        let state = decoder.makeState(encoderOutput: encoded.hidden, encoderMask: encoded.mask)
        var tokens = prompt
        var pending = prompt
        for step in 0..<CanarySpeech.maxTokens {
            let ids = MLXArray(pending.map(Int32.init)).reshaped(1, pending.count)
            let logits = decoder(ids, state: state)
            let s = logits[0, logits.dim(1) - 1]
            let f = full(tokens)
            let sorted = MLX.sorted(f)
            eval(s, f, sorted)
            result.dtype = s.dtype
            let g = MLX.abs(s.asType(.float32) - f.asType(.float32)).max().item(Float.self)
            if g > result.maxGap { result.maxGap = g; result.maxGapStep = step }
            result.maxLogit = max(result.maxLogit, MLX.abs(f).max().item(Float.self))
            let n = sorted.dim(0)
            result.minMargin = min(result.minMargin, (sorted[n - 1] - sorted[n - 2]).item(Float.self))
            let next = s.argMax().item(Int.self)
            if next != f.argMax().item(Int.self) { result.argmaxDiffers += 1 }
            if next == eos { break }
            tokens.append(next)
            result.cachedTokens.append(next)
            pending = [next]
        }

        tokens = prompt
        for _ in 0..<CanarySpeech.maxTokens {
            let f = full(tokens)
            eval(f)
            let next = f.argMax().item(Int.self)
            if next == eos { break }
            tokens.append(next)
            result.fullTokens.append(next)
        }

        result.cachedText = CanarySpeech.clean(model.decode(tokens: result.cachedTokens))
        result.fullText = CanarySpeech.clean(model.decode(tokens: result.fullTokens))
        result.firstDiff = zip(result.cachedTokens, result.fullTokens).enumerated().first { $0.element.0 != $0.element.1 }?.offset
            ?? (result.cachedTokens.count == result.fullTokens.count ? nil : min(result.cachedTokens.count, result.fullTokens.count))
        return result
    }
}
