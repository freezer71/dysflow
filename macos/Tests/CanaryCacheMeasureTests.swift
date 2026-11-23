import Foundation
import XCTest
import MLX
import MLXNN
@testable import Dysflow
@testable import KokoroMLX

/// Mesure (tâche 18) : pourquoi `CanaryDecoderCacheTests` échoue sur un M5.
/// Ignoré sans `DYSFLOW_CANARY_MESURE` (`TEST_RUNNER_DYSFLOW_CANARY_MESURE`
/// pour xcodebuild) : le dossier où écrire le relevé, un fichier texte par
/// exécution. Lancer deux fois, avec et sans `TEST_RUNNER_MLX_ENABLE_TF32=0`
/// (MLX lit `MLX_ENABLE_TF32` une seule fois par processus). Puis une fois
/// avec `TEST_RUNNER_DYSFLOW_CANARY_DECODEUR=cpu` : le décodeur seul, sur le
/// processeur, en bf16 tel que livré puis passé en Float32, sur la sortie de
/// l'encodeur que la passe GPU a laissée dans le dossier.
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
        let decoderOnCPU = env["DYSFLOW_CANARY_DECODEUR"] == "cpu"
        if decoderOnCPU {
            // Le processeur par défaut aussi pour le C++ de MLX (ses
            // opérateurs sans flux) : avec `withDefaultDevice` seul, un
            // calcul partait encore sur le GPU et s'y arrêtait sur un délai
            // dépassé. Avant toute ressource de MLX dans ce processus.
            Device.setDefault(device: Device(.cpu))
            log("MLX_ENABLE_TF32=\(tf32), appareil par défaut : processeur")
        } else {
            log("MLX_ENABLE_TF32=\(tf32) GPU architecture=\(GPU.deviceInfo().architecture)")
        }
        log("macOS \(ProcessInfo.processInfo.operatingSystemVersionString)")

        if !decoderOnCPU {
            matmul()
            synthetic(name: "testStepByStepMatchesTheFullPass", encoderLength: 20, count: 16, prompt: 5) { ($0 * 7 + 3) % 50 }
            synthetic(name: "testPositionsPastTheTableRepeatTheLastOne", encoderLength: 8, count: 1030, prompt: 1020) { ($0 * 11 + 1) % 50 }
        }
        if let models = env["DYSFLOW_STT_MODELS"] {
            try realModel(URL(fileURLWithPath: models).appendingPathComponent("canary_1b_v2_q8"),
                          out: URL(fileURLWithPath: out), decoderOnCPU: decoderOnCPU)
        } else {
            log("DYSFLOW_STT_MODELS absent : pas de mesure sur le vrai modèle")
        }

        let name = "canary-mesure-tf32-\(env["MLX_ENABLE_TF32"] ?? "defaut")\(decoderOnCPU ? "-decodeur-cpu" : "").txt"
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

    private func realModel(_ directory: URL, out: URL, decoderOnCPU: Bool) throws {
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try ParakeetFixtureTests.inputs(repo).filter {
            ["francais", "anglais", "douze_secondes"].contains($0.name)
        }
        func saved(_ name: String) throws -> (hidden: MLXArray, mask: MLXArray, tokens: [Int], prompt: Int) {
            let arrays = try loadArrays(url: out.appendingPathComponent("encodeur-\(name).safetensors"))
            return (try XCTUnwrap(arrays["hidden"]), try XCTUnwrap(arrays["mask"]),
                    try XCTUnwrap(arrays["tokens"]).asArray(Int32.self).map(Int.init),
                    try XCTUnwrap(arrays["prompt"]).item(Int.self))
        }
        if decoderOnCPU {
            // Le décodeur seul, sur le processeur, dans un processus où rien
            // ne passe par le GPU : la sortie de l'encodeur et la suite de
            // jetons viennent de la passe sur le GPU. Le processeur est lent
            // (plus de 30 min sans finir le premier audio en entier) : les
            // `DYSFLOW_CANARY_PAS` (6 par défaut) premiers pas seulement, sur
            // la suite du GPU imposée.
            let steps = Int(ProcessInfo.processInfo.environment["DYSFLOW_CANARY_PAS"] ?? "") ?? 6
            try Device.withDefaultDevice(.cpu) {
                let model = try CanaryModel.fromModelDirectory(directory)
                for widened in [false, true] {
                    if widened { widenDecoder(model) }
                    logDTypes(model)
                    for input in inputs {
                        let s = try saved(input.name)
                        let forced = forcedSteps(model, encoded: (s.hidden, s.mask), tokens: s.tokens, prompt: s.prompt, steps: steps)
                        log("audio \(input.name) — décodeur \(widened ? "passé en Float32" : "bf16 (tel que livré)") sur le processeur, \(steps) pas imposés : \(forced)")
                    }
                }
            }
            return
        }
        let model = try CanaryModel.fromModelDirectory(directory)
        let speech = CanarySpeech(model)
        logDTypes(model)
        var encodedByInput: [String: (MLXArray, MLXArray)] = [:]
        for input in inputs {
            let language = input.name == "anglais" ? "en" : "fr"
            let cached = try speech.transcribe(input.samples, language: language)
            let mel = model.preprocessAudio(MLXArray(input.samples)).asType(.float32)
            let encoded = model.encode(mel: mel)
            eval(encoded.hidden, encoded.mask)
            encodedByInput[input.name] = (encoded.hidden, encoded.mask)
            log("audio \(input.name) (\(input.samples.count) échantillons, \(language)) — encodeur \(encoded.hidden.dtype) \(encoded.hidden.shape)")
            log("  cache (app)  : \(cached)")
            let measured = try decodeBoth(model, encoded: (encoded.hidden, encoded.mask), language: language)
            logDecode("décodeur bf16 (tel que livré) sur le GPU", measured, app: cached)
            try save(arrays: ["hidden": encoded.hidden, "mask": encoded.mask,
                              "tokens": MLXArray((measured.prompt + measured.cachedTokens).map(Int32.init)),
                              "prompt": MLXArray(Int32(measured.prompt.count))],
                     url: out.appendingPathComponent("encodeur-\(input.name).safetensors"))
        }
        widenDecoder(model)
        logDTypes(model)
        for input in inputs {
            let language = input.name == "anglais" ? "en" : "fr"
            let encoded = try XCTUnwrap(encodedByInput[input.name])
            log("audio \(input.name)")
            logDecode("décodeur passé en Float32 sur le GPU", try decodeBoth(model, encoded: encoded, language: language), app: nil)
        }
    }

    /// Les paramètres bf16 du décodeur passés en Float32 (les poids 8 bits ne
    /// changent pas) : les activations suivent le type des échelles et des
    /// normes.
    private func widenDecoder(_ model: CanaryModel) {
        let widened = model.decoder.parameters().mapValues { $0.dtype == .bfloat16 ? $0.asType(.float32) : $0 }
        model.decoder.update(parameters: widened)
        eval(model.decoder.parameters())
    }

    /// Sur la suite `tokens` imposée (les `prompt` premiers : l'invite), à
    /// chacun des `steps` premiers pas : les logits du cache contre ceux de
    /// la suite entière, sur le même préfixe.
    private func forcedSteps(_ model: CanaryModel, encoded: (hidden: MLXArray, mask: MLXArray), tokens: [Int], prompt: Int, steps: Int) -> String {
        let decoder = model.decoder
        let state = decoder.makeState(encoderOutput: encoded.hidden, encoderMask: encoded.mask)
        var maxGap: Float = 0, maxLogit: Float = 0, argmaxDiffers = 0
        var dtypes = ""
        var pending = Array(tokens[..<prompt])
        let last = min(prompt + steps, tokens.count)
        for end in prompt...last {
            let ids = MLXArray(pending.map(Int32.init)).reshaped(1, pending.count)
            let cached = decoder(ids, state: state)
            let c = cached[0, cached.dim(1) - 1]
            let fullIds = MLXArray(tokens[..<end].map(Int32.init)).reshaped(1, end)
            let full = decoder(fullIds, encoderOutput: encoded.hidden, encoderMask: encoded.mask)
            let f = full[0, end - 1]
            eval(c, f)
            if dtypes.isEmpty {
                let x = decoder.embeddingLayerNorm(decoder.embedding(ids))
                eval(x)
                dtypes = "plongement normé \(x.dtype), logits \(c.dtype)"
            }
            maxGap = max(maxGap, MLX.abs(c - f).max().item(Float.self))
            maxLogit = max(maxLogit, MLX.abs(f).max().item(Float.self))
            if c.argMax().item(Int.self) != f.argMax().item(Int.self) { argmaxDiffers += 1 }
            if end < tokens.count { pending = [tokens[end]] }
        }
        return "\(dtypes) ; \(last - prompt + 1) pas ; écart des logits max \(maxGap), relatif \(maxGap / maxLogit), |logits|max \(maxLogit), argmax différent : \(argmaxDiffers)"
    }

    private func logDecode(_ title: String, _ measured: Both, app: String?) {
        log("  \(title) — dtypes : \(measured.dtypes)")
        log("    cache        : \(measured.cachedText)")
        log("    entière      : \(measured.fullText)")
        log("    jetons cache \(measured.cachedTokens.count), entière \(measured.fullTokens.count), premier jeton différent : \(measured.firstDiff.map(String.init) ?? "aucun")")
        log("    sur la suite du cache : écart des logits max \(measured.maxGap) (pas \(measured.maxGapStep)), relatif \(measured.maxGap / measured.maxLogit), |logits|max \(measured.maxLogit), marge top1-top2 min \(measured.minMargin), argmax différent : \(measured.argmaxDiffers)")
        log("    texte identique (cache, entière\(app == nil ? "" : ", app")) : \(measured.cachedText == measured.fullText && (app ?? measured.cachedText) == measured.cachedText)")
    }

    /// Les types des paramètres du décodeur, regroupés par suffixe
    /// (`weight`, `scales`, `biases`), et la forme de ses couches linéaires.
    private func logDTypes(_ model: CanaryModel) {
        var groups: [String: [String: Int]] = [:]
        for (key, value) in model.decoder.parameters().flattened() {
            let suffix = key.split(separator: ".").last.map(String.init) ?? key
            groups[suffix, default: [:]]["\(value.dtype)", default: 0] += 1
        }
        for suffix in groups.keys.sorted() {
            log("paramètres du décodeur « \(suffix) » : \(groups[suffix]!.sorted { $0.key < $1.key }.map { "\($0.key)×\($0.value)" }.joined(separator: ", "))")
        }
        var kinds: [String: Int] = [:]
        for (_, module) in model.decoder.leafModules().flattened() {
            var kind = "\(type(of: module))"
            if let q = module as? QuantizedLinear { kind += "(bits \(q.bits), groupe \(q.groupSize))" }
            kinds[kind, default: 0] += 1
        }
        log("couches du décodeur : \(kinds.sorted { $0.key < $1.key }.map { "\($0.key)×\($0.value)" }.joined(separator: ", "))")
    }

    private struct Both {
        var prompt: [Int] = []
        var dtypes = ""
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
    private func decodeBoth(_ model: CanaryModel, encoded: (hidden: MLXArray, mask: MLXArray), language: String) throws -> Both {
        var result = Both()
        let prompt = try XCTUnwrap(model.tokenizer).buildPromptTokens(
            config: model.config, sourceLanguage: language, targetLanguage: language)
        let eos = try XCTUnwrap(model.tokenizer).eosTokenId(config: model.config)
        result.prompt = prompt
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
            if step == 0 {
                let x = decoder.embeddingLayerNorm(decoder.embedding(ids))
                let h = decoder.finalNorm(x)
                eval(x, h)
                result.dtypes = "encodeur \(encoded.hidden.dtype), masque \(encoded.mask.dtype), plongement normé \(x.dtype), norme finale \(h.dtype), logits \(s.dtype)"
            }
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
