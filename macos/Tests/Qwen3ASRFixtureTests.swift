import XCTest
import MLX
@testable import Dysflow
@testable import KokoroMLX

/// P1 du port de Qwen3-ASR en Rust (procédure de port, tâche 22) : le
/// Qwen3-ASR Swift actuel (`Qwen3ASRModel.fromModelDirectory`, puis
/// `generate` comme `Qwen3ASRSpeech` le lance dans l'app), sur les entrées de
/// la tâche, écrit la référence que le port Rust doit rendre,
/// `tests/fixtures/mlx/<modèle>.json`, pour `qwen3_asr_06b_8bit` et
/// `qwen3_asr_17b_4bit`. Un essai par modèle : un seul modèle en mémoire.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient les deux paquets) et
/// `DYSFLOW_FIXTURE_COMMIT` (le commit d'origine, écrit dans la fixture).
/// Pour xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh Qwen3ASRFixtureTests/testWritesTheSmallModelFixture
///
/// Les entrées : celles de Whisper (`WhisperFixtureTests.inputs`), les cinq
/// de Parakeet plus `trente_cinq_secondes`, dont les quatre fenêtres pleines
/// de l'encodeur passent en un lot. La langue est imposée, jamais devinée
/// (`Qwen3ASRSpeech`) : `en` pour l'anglais, `fr` pour le reste ; l'invite
/// finit par « language French<asr_text> » ou « language English<asr_text> ».
///
/// Pour chaque entrée : la forme et les moyennes du mel (`[1, bandes,
/// trames]`, tel que `generate` le donne à l'encodeur, après la marge à 1 s)
/// et de la sortie de l'encodeur, le nombre de jetons audio, l'invite et les
/// jetons gardés (`traceEmitter`), puis le texte que l'app garde. À part :
/// les invites de 13 jetons audio en français, en anglais et sans langue
/// (`buildPrompt`, celle de `Qwen3ASRSmokeTests`). Décodage glouton
/// (température 0) : aucun tirage, aucune graine.
final class Qwen3ASRFixtureTests: XCTestCase {
    /// Les révisions épinglées du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let small = (id: "qwen3_asr_06b_8bit", revision: "89e96d92ba34aca20b3e29fb10cc284097d1219f")
    static let large = (id: "qwen3_asr_17b_4bit", revision: "78a389c776a5483b2d0d4ea5494e11012e0d6159")

    struct Case: Codable {
        let input: String
        let language: String
        let mel: ParakeetFixtureTests.Stats
        let encoder: ParakeetFixtureTests.Stats
        /// Les `<|audio_pad|>` de l'invite (`preprocessAudio`).
        let audio_tokens: Int
        let prompt: [Int]
        /// Les jetons gardés (sans `<|im_end|>` ni `<|endoftext|>`).
        let tokens: [Int]
        /// Le texte que l'app garde : celui de `generate`, sans espaces autour.
        let text: String
    }

    struct Prompts: Codable {
        let fr: [Int]
        let en: [Int]
        let none: [Int]
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
        /// `buildPrompt(numAudioTokens: 13, language:)`.
        let prompts: Prompts
        let inputs: [ParakeetFixtureTests.Input]
        let cases: [Case]
    }

    final class Trace {
        var prompt: [Int] = []
        var tokens: [Int] = []
        var calls = 0
    }

    func testWritesTheSmallModelFixture() throws {
        try write(Self.small)
    }

    func testWritesTheLargeModelFixture() throws {
        try write(Self.large)
    }

    private func write(_ model: (id: String, revision: String)) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try WhisperFixtureTests.inputs(repo)

        let (id, revision) = model
        let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
        let configText = try String(contentsOf: dir.appendingPathComponent("config.json"), encoding: .utf8)
        let quantization = try ParakeetFixtureTests.quantization(configText)
        try autoreleasepool {
            let model = try Qwen3ASRModel.fromModelDirectory(dir)
            let prompts = Prompts(
                fr: Self.ids(model.buildPrompt(numAudioTokens: 13, language: "fr")),
                en: Self.ids(model.buildPrompt(numAudioTokens: 13, language: "en")),
                none: Self.ids(model.buildPrompt(numAudioTokens: 13)))
            var cases: [Case] = []
            var encoderDType = ""
            for input in inputs {
                let language = WhisperFixtureTests.language(input.name)
                let (c, dtype) = try Self.run(model, input.name, language, input.samples)
                cases.append(c)
                encoderDType = dtype
            }
            let fixture = Fixture(
                model: id, revision: revision, commit: commit,
                generator: "macos/Tests/Qwen3ASRFixtureTests.swift",
                encoder_dtype: encoderDType, quantization: quantization,
                max_tokens: model.defaultGenerationParameters.maxTokens, seed: nil,
                prompts: prompts, inputs: ParakeetFixtureTests.described(inputs), cases: cases)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            var data = try encoder.encode(fixture)
            data.append(0x0A)
            try data.write(to: repo.appendingPathComponent("tests/fixtures/mlx/\(id).json"))
        }
        Memory.clearCache()
    }

    static func ids(_ prompt: MLXArray) -> [Int] {
        prompt.asArray(Int32.self).map { Int($0) }
    }

    /// Une entrée : la tranche que `generate` lit (une seule ici, marge à
    /// 1 s comprise), son mel et la sortie de l'encodeur, puis l'invite, les
    /// jetons et le texte par `generate`, avec les réglages de
    /// `Qwen3ASRSpeech.transcribe`.
    static func run(_ model: Qwen3ASRModel, _ name: String, _ language: String,
                    _ samples: [Float]) throws -> (Case, String) {
        let parameters = model.defaultGenerationParameters.with(language: language)
        let chunks = splitAudioIntoChunks(
            MLXArray(samples), sampleRate: model.sampleRate,
            chunkDuration: parameters.chunkDuration, minChunkDuration: parameters.minChunkDuration)
        XCTAssertEqual(chunks.count, 1, "\(name) : une tranche")
        let (features, mask, audioTokens) = model.preprocessAudio(chunks[0].0)
        eval(features)
        let encoded = model.getAudioFeatures(features, featureAttentionMask: mask)
        eval(encoded)

        let trace = Trace()
        model.traceEmitter = { prompt, tokens in
            trace.prompt = prompt
            trace.tokens = tokens
            trace.calls += 1
        }
        let text = model.generate(audio: MLXArray(samples), generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
        model.traceEmitter = nil
        XCTAssertEqual(trace.calls, 1, "\(name) : une trace")
        Memory.clearCache()

        let c = Case(
            input: name, language: language,
            mel: ParakeetFixtureTests.stats(features), encoder: ParakeetFixtureTests.stats(encoded),
            audio_tokens: audioTokens, prompt: trace.prompt, tokens: trace.tokens, text: text)
        return (c, "\(encoded.dtype)")
    }
}
