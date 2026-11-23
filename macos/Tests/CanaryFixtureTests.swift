import XCTest
import MLX
@testable import Dysflow
@testable import KokoroMLX

/// P1 du port de Canary en Rust (procédure de port, tâche 19) : le Canary
/// Swift actuel (`CanaryModel.fromModelDirectory`, puis `generate` comme
/// `CanarySpeech` le lance dans l'app), sur les entrées de la tâche, écrit la
/// référence que le port Rust doit rendre,
/// `tests/fixtures/mlx/canary_1b_v2_q8.json`.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient `canary_1b_v2_q8/`) et
/// `DYSFLOW_FIXTURE_COMMIT` (le commit d'origine, écrit dans la fixture).
/// Pour xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh CanaryFixtureTests
///
/// Les entrées : les cinq de Parakeet (`ParakeetFixtureTests.inputs`). La
/// langue, source et cible, est imposée : `en` pour l'anglais, `fr` pour le
/// reste. Pour chaque entrée : la forme et les moyennes du mel (Float32, tel
/// que `generate` le donne à l'encodeur) et de la sortie de l'encodeur, la
/// longueur encodée, l'invite et les jetons gardés (`traceEmitter`), puis le
/// texte que l'app garde (`CanarySpeech.clean`, 1 024 jetons au plus).
/// Décodage glouton (température 0) : aucun tirage, aucune graine.
final class CanaryFixtureTests: XCTestCase {
    /// La révision épinglée du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let model = (id: "canary_1b_v2_q8", revision: "0b6b32ee10f30c89e3ead7249bb636445e3019ee")

    struct Case: Codable {
        let input: String
        let language: String
        let mel: ParakeetFixtureTests.Stats
        let encoder: ParakeetFixtureTests.Stats
        let encoded_length: Int
        let prompt: [Int]
        /// Les jetons gardés (sans `<|endoftext|>`), dans l'ordre.
        let tokens: [Int]
        /// Le texte que l'app garde : `CanarySpeech.clean` de celui de
        /// `generate` (`<|…|>` retirés, sans espaces autour).
        let text: String
    }

    struct Fixture: Codable {
        let model: String
        let revision: String
        let commit: String
        let generator: String
        let encoder_dtype: String
        let quantization: String
        let max_tokens: Int
        let seed: String?
        let inputs: [ParakeetFixtureTests.Input]
        let cases: [Case]
    }

    final class Trace {
        var prompt: [Int] = []
        var tokens: [Int] = []
        var calls = 0
    }

    func testWritesTheCanaryFixture() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try ParakeetFixtureTests.inputs(repo)

        let (id, revision) = Self.model
        let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
        let configText = try String(contentsOf: dir.appendingPathComponent("config.json"), encoding: .utf8)
        let quantization = try ParakeetFixtureTests.quantization(configText)
        try autoreleasepool {
            let model = try CanaryModel.fromModelDirectory(dir)
            var cases: [Case] = []
            var encoderDType = ""
            for input in inputs {
                let language = input.name == "anglais" ? "en" : "fr"
                let (c, dtype) = try Self.run(model, input.name, language, input.samples)
                cases.append(c)
                encoderDType = dtype
            }
            let fixture = Fixture(
                model: id, revision: revision, commit: commit,
                generator: "macos/Tests/CanaryFixtureTests.swift",
                encoder_dtype: encoderDType, quantization: quantization,
                max_tokens: CanarySpeech.maxTokens, seed: nil,
                inputs: ParakeetFixtureTests.described(inputs), cases: cases)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            var data = try encoder.encode(fixture)
            data.append(0x0A)
            try data.write(to: repo.appendingPathComponent("tests/fixtures/mlx/\(id).json"))
        }
        Memory.clearCache()
    }

    /// Une entrée : le mel et l'encodeur comme `generate` les enchaîne, puis
    /// l'invite, les jetons et le texte par `generate`, avec les réglages de
    /// `CanarySpeech.transcribe` (langue source et cible, 1 024 jetons au
    /// plus), puis `CanarySpeech.clean`.
    static func run(_ model: CanaryModel, _ name: String, _ language: String,
                    _ samples: [Float]) throws -> (Case, String) {
        let mel = model.preprocessAudio(MLXArray(samples)).asType(.float32)
        eval(mel)
        let encoded = model.encode(mel: mel)
        eval(encoded.hidden, encoded.lengths)

        let trace = Trace()
        model.traceEmitter = { prompt, tokens in
            trace.prompt = prompt
            trace.tokens = tokens
            trace.calls += 1
        }
        let parameters = model.defaultGenerationParameters.with(language: language, maxTokens: CanarySpeech.maxTokens)
        let text = CanarySpeech.clean(model.generate(samples: samples, generationParameters: parameters).text)
        model.traceEmitter = nil
        XCTAssertEqual(trace.calls, 1, "\(name) : une trace")

        let c = Case(
            input: name, language: language,
            mel: ParakeetFixtureTests.stats(mel), encoder: ParakeetFixtureTests.stats(encoded.hidden),
            encoded_length: Int(encoded.lengths[0].item(Int32.self)),
            prompt: trace.prompt, tokens: trace.tokens, text: text)
        return (c, "\(encoded.hidden.dtype)")
    }
}
