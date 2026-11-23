import XCTest
import MLX
@testable import KokoroMLX

/// P1 du port de Whisper en Rust (procédure de port, tâche 17) : le Whisper
/// Swift actuel (`WhisperModel.fromDirectory`, puis `generate`, le chemin de
/// `WhisperSpeech` dans l'app), lancé sur les entrées de la tâche, écrit la
/// référence que le port Rust doit rendre, `tests/fixtures/mlx/<modèle>.json`,
/// pour `whisper_tiny_4bit`, `whisper_small_4bit` et
/// `whisper_large_v3_turbo_4bit`.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient les trois paquets) et
/// `DYSFLOW_FIXTURE_COMMIT` (le commit d'origine, écrit dans la fixture). Pour
/// xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh WhisperFixtureTests
///
/// Les entrées : les cinq de Parakeet (`ParakeetFixtureTests.inputs`), plus
/// `trente_cinq_secondes`, plus longue qu'une fenêtre de 30 s : elle passe
/// par deux fenêtres, lues chacune seule puis collées (`generate`). La langue
/// est imposée, jamais devinée : `en` pour l'anglais, `fr` pour le reste.
///
/// Pour chaque entrée et chaque fenêtre : la forme et les moyennes des
/// caractéristiques (le mel `[1, 3000, bandes]` que voit l'encodeur) et de la
/// sortie de l'encodeur, l'invite et les jetons gardés (`windowTraceEmitter`) ;
/// puis le texte de `generate`, sans espaces autour. Décodage glouton
/// (température 0) : aucun tirage, aucune graine.
final class WhisperFixtureTests: XCTestCase {
    /// Les révisions épinglées du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let models: [(id: String, revision: String)] = [
        ("whisper_tiny_4bit", "064afea920b0373f8e19d51dc698bfbb91f501ff"),
        ("whisper_small_4bit", "1ac90abbb1b2f53863bfd2a483a3c31d792c3e5b"),
        ("whisper_large_v3_turbo_4bit", "0f058d38170d183f9fdee07908f5b515d91793a8"),
    ]

    struct Window: Codable {
        let mel: ParakeetFixtureTests.Stats
        let encoder: ParakeetFixtureTests.Stats
        /// Les jetons gardés de la fenêtre (sans `<|endoftext|>`).
        let tokens: [Int]
    }

    struct Case: Codable {
        let input: String
        let language: String
        let prompt: [Int]
        let windows: [Window]
        /// Le texte que l'app garde : celui de `generate`, sans espaces autour.
        let text: String
    }

    struct Fixture: Codable {
        let model: String
        let revision: String
        let commit: String
        let generator: String
        let encoder_dtype: String
        let quantization: String
        let seed: String?
        let inputs: [ParakeetFixtureTests.Input]
        let cases: [Case]
    }

    final class Traces {
        var all: [(prompt: [Int], tokens: [Int])] = []
    }

    func testWritesTheWhisperFixtures() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try Self.inputs(repo)

        for (id, revision) in Self.models {
            let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
            let configText = try String(contentsOf: dir.appendingPathComponent("config.json"), encoding: .utf8)
            let quantization = try ParakeetFixtureTests.quantization(configText)
            try autoreleasepool {
                let model = try WhisperModel.fromDirectory(dir)
                var cases: [Case] = []
                var encoderDType = ""
                for input in inputs {
                    let (c, dtype) = try Self.run(model, input.name, Self.language(input.name), input.samples)
                    cases.append(c)
                    encoderDType = dtype
                }
                let fixture = Fixture(
                    model: id, revision: revision, commit: commit,
                    generator: "macos/Tests/WhisperFixtureTests.swift",
                    encoder_dtype: encoderDType, quantization: quantization, seed: nil,
                    inputs: ParakeetFixtureTests.described(inputs), cases: cases)
                let encoder = JSONEncoder()
                encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
                var data = try encoder.encode(fixture)
                data.append(0x0A)
                try data.write(to: repo.appendingPathComponent("tests/fixtures/mlx/\(id).json"))
            }
            Memory.clearCache()
        }
    }

    /// Les cinq entrées de Parakeet, plus `trente_cinq_secondes` : deux
    /// fenêtres de 30 s (la seconde de 5 s).
    static func inputs(_ repo: URL) throws -> [(name: String, source: String, samples: [Float])] {
        var inputs = try ParakeetFixtureTests.inputs(repo)
        let fr = inputs[0].samples, en = inputs[1].samples
        inputs.append((
            "trente_cinq_secondes",
            "francais, anglais, francais… à la suite, coupé à 560 000 échantillons (35 s : deux fenêtres de 30 s)",
            ParakeetFixtureTests.cycled([fr, en], count: 560_000)))
        return inputs
    }

    /// La langue imposée : `en` pour l'anglais, `fr` pour le reste.
    static func language(_ input: String) -> String {
        input == "anglais" ? "en" : "fr"
    }

    /// Une entrée : chaque fenêtre (découpée comme `generate`) par ses
    /// étages, puis le texte et les jetons par `generate`, le chemin de l'app.
    static func run(_ model: WhisperModel, _ name: String, _ language: String,
                    _ samples: [Float]) throws -> (Case, String) {
        let window = WhisperAudioConfig.chunkLengthSamples
        let pieces: [[Float]] = samples.count <= window
            ? [samples]
            : stride(from: 0, to: samples.count, by: window).map {
                Array(samples[$0..<min($0 + window, samples.count)])
            }
        var stages: [(ParakeetFixtureTests.Stats, ParakeetFixtureTests.Stats)] = []
        var dtype = ""
        for piece in pieces {
            let features = WhisperAudio.encoderFeatures(audio: MLXArray(piece), nMels: model.config.numMelBins)
            eval(features)
            let encoded = model.model.encoder(features)
            eval(encoded)
            dtype = "\(encoded.dtype)"
            stages.append((ParakeetFixtureTests.stats(features), ParakeetFixtureTests.stats(encoded)))
        }

        let traces = Traces()
        model.windowTraceEmitter = { prompt, tokens in traces.all.append((prompt, tokens)) }
        let defaults = model.defaultGenerationParameters
        let parameters = STTGenerateParameters(
            maxTokens: defaults.maxTokens, temperature: 0, topP: 1, language: language,
            chunkDuration: defaults.chunkDuration, minChunkDuration: defaults.minChunkDuration)
        let text = model.generate(samples: samples, generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
        model.windowTraceEmitter = nil

        XCTAssertEqual(traces.all.count, pieces.count, "\(name) : une trace par fenêtre")
        let prompt = traces.all.first?.prompt ?? []
        XCTAssertTrue(traces.all.allSatisfy { $0.prompt == prompt }, "\(name) : la même invite partout")
        let windows = zip(stages, traces.all).map { stage, trace in
            Window(mel: stage.0, encoder: stage.1, tokens: trace.tokens)
        }
        return (Case(input: name, language: language, prompt: prompt, windows: windows, text: text), dtype)
    }
}
