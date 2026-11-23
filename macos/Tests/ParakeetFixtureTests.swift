import CryptoKit
import XCTest
import MLX
@testable import KokoroMLX

/// P1 du port de Parakeet en Rust (procédure de port, tâche 14) : le Parakeet
/// Swift actuel, lancé sur les entrées de la tâche, écrit la référence que le
/// port Rust doit rendre, `tests/fixtures/mlx/<modèle>.json`.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient `parakeet_tdt_v3/` et
/// `parakeet_tdt_v2/`) et `DYSFLOW_FIXTURE_COMMIT` (le commit d'origine,
/// écrit dans la fixture). Pour xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh ParakeetFixtureTests
///
/// Le chemin de l'app (`ParakeetSpeech`) : `fromDirectory` (calcul en
/// bfloat16, quantification lue dans `config.json`), puis `generate` sur
/// l'audio entier. Pour chaque entrée : le texte, les jetons, chaque pas du
/// décodeur TDT, et la forme et la moyenne du mel et de la sortie de
/// l'encodeur. Le décodage est glouton : aucun tirage, aucune graine.
final class ParakeetFixtureTests: XCTestCase {
    /// Les révisions épinglées du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let models: [(id: String, revision: String)] = [
        ("parakeet_tdt_v3", "18498133db8b4c8753bf98b3c5b6639b2a791be3"),
        ("parakeet_tdt_v2", "8ae155301e23d820d82aa60d24817c900e69e487"),
    ]

    struct Stats: Codable {
        let shape: [Int]
        let mean: Float
        let mean_abs: Float
    }

    struct Input: Codable {
        let name: String
        let source: String
        let samples: Int
        /// SHA-256 des échantillons `f32` en petit-boutiste, tels que donnés
        /// au modèle.
        let sha256: String
    }

    struct Case: Codable {
        let input: String
        let mel: Stats
        let encoder: Stats
        let encoded_length: Int
        /// `[temps, jeton, indice de durée]` de chaque pas du décodeur.
        let steps: [[Int]]
        /// Les jetons gardés (hors blancs et jetons spéciaux), dans l'ordre.
        let tokens: [Int]
        /// Le texte que l'app garde : celui de `generate`, sans espaces autour.
        let text: String
    }

    struct Fixture: Codable {
        let model: String
        let revision: String
        let commit: String
        let generator: String
        let compute_dtype: String
        let quantization: String
        let seed: String?
        let inputs: [Input]
        let cases: [Case]
    }

    struct FixtureFailure: Error {
        let message: String
    }

    final class Steps: @unchecked Sendable {
        var all: [[Int]] = []
    }

    func testWritesTheParakeetFixtures() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let output = repo.appendingPathComponent("tests/fixtures/mlx")

        let fr = try Self.pcm16(repo.appendingPathComponent("macos/Tests/Fixtures/dictee-charge.wav"))
        let en = try Self.pcm16(repo.appendingPathComponent("tests/fixtures/mlx/dictee-en.wav"))
        let inputs: [(name: String, source: String, samples: [Float])] = [
            ("francais", "macos/Tests/Fixtures/dictee-charge.wav", fr),
            ("anglais", "tests/fixtures/mlx/dictee-en.wav", en),
            ("douze_secondes", "francais, anglais, francais… à la suite, coupé à 192 000 échantillons",
             Self.cycled([fr, en], count: 192_000)),
            ("silence", "16 000 zéros (1 s)", [Float](repeating: 0, count: 16_000)),
            ("dixieme", "francais[16 000 ..< 17 600] (0,1 s)", Array(fr[16_000..<17_600])),
        ]

        for (id, revision) in Self.models {
            let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
            let configText = try String(contentsOf: dir.appendingPathComponent("config.json"), encoding: .utf8)
            let quantization = try Self.quantization(configText)
            try autoreleasepool {
                let model = try ParakeetModel.fromDirectory(dir)
                var cases: [Case] = []
                for input in inputs {
                    cases.append(try Self.run(model, input.name, input.samples))
                }
                let fixture = Fixture(
                    model: id, revision: revision, commit: commit,
                    generator: "macos/Tests/ParakeetFixtureTests.swift",
                    compute_dtype: "\(model.computeDType)", quantization: quantization, seed: nil,
                    inputs: inputs.map {
                        Input(name: $0.name, source: $0.source, samples: $0.samples.count, sha256: Self.sha256($0.samples))
                    },
                    cases: cases)
                let encoder = JSONEncoder()
                encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
                var data = try encoder.encode(fixture)
                data.append(0x0A)
                try data.write(to: output.appendingPathComponent("\(id).json"))
            }
            Memory.clearCache()
        }
    }

    /// Une entrée : les étages comme `decodeTDT` les enchaîne, puis le texte
    /// par `generate`, le chemin de `ParakeetSpeech`.
    static func run(_ model: ParakeetModel, _ name: String, _ samples: [Float]) throws -> Case {
        let mel = ParakeetAudio.logMelSpectrogram(MLXArray(samples), config: model.preprocessConfig)
        eval(mel)
        let (encoded, lengths) = model.encodeBatchFeatures(mel.asType(model.computeDType))
        eval(encoded, lengths)

        let steps = Steps()
        model.tdtTraceEmitter = { step in
            steps.all.append([step.time, step.token, step.decisionIndex])
        }
        let result = model.decode(mel: mel)[0]
        model.tdtTraceEmitter = nil

        let text = model.generate(samples: samples, generationParameters: model.defaultGenerationParameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
        return Case(
            input: name, mel: stats(mel), encoder: stats(encoded),
            encoded_length: Int(lengths[0].item(Int32.self)),
            steps: steps.all,
            tokens: result.sentences.flatMap { $0.tokens.map(\.id) },
            text: text)
    }

    static func stats(_ x: MLXArray) -> Stats {
        let f = x.asType(.float32)
        let mean = f.mean(), meanAbs = MLX.abs(f).mean()
        eval(mean, meanAbs)
        return Stats(shape: x.shape, mean: mean.item(Float.self), mean_abs: meanAbs.item(Float.self))
    }

    /// `quantization` de `config.json`, telle que lue : `bits`, `group_size`.
    static func quantization(_ configText: String) throws -> String {
        var text = configText
        for token in ["-Infinity", "Infinity", "NaN"] {
            text = text.replacingOccurrences(of: token, with: "null")
        }
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        guard let q = json["quantization"] as? [String: Any] else { return "aucune" }
        return "bits \(q["bits"] ?? "?"), group_size \(q["group_size"] ?? "?")"
    }

    /// `clips` à la suite, recommencés, jusqu'à `count` échantillons.
    static func cycled(_ clips: [[Float]], count: Int) -> [Float] {
        var out: [Float] = []
        out.reserveCapacity(count)
        while out.count < count {
            for clip in clips where out.count < count {
                out.append(contentsOf: clip.prefix(count - out.count))
            }
        }
        return out
    }

    static func sha256(_ samples: [Float]) -> String {
        let bytes = samples.withUnsafeBufferPointer { Data(buffer: $0) }
        return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }

    /// Un WAV PCM 16 bits mono 16 kHz, chunk après chunk, divisé par 32 768
    /// (comme `stt::file::Wav` en Rust).
    static func pcm16(_ url: URL) throws -> [Float] {
        let bytes = [UInt8](try Data(contentsOf: url))
        func u32(_ at: Int) -> Int { Int(bytes[at]) | Int(bytes[at + 1]) << 8 | Int(bytes[at + 2]) << 16 | Int(bytes[at + 3]) << 24 }
        func u16(_ at: Int) -> Int { Int(bytes[at]) | Int(bytes[at + 1]) << 8 }
        XCTAssertEqual(Array(bytes[0..<4]), Array("RIFF".utf8))
        var at = 12
        var format: (channels: Int, rate: Int, bits: Int)?
        while at + 8 <= bytes.count {
            let id = String(decoding: bytes[at..<at + 4], as: UTF8.self), size = u32(at + 4)
            if id == "fmt " {
                format = (u16(at + 10), u32(at + 12), u16(at + 22))
            } else if id == "data" {
                let f = try XCTUnwrap(format, "fmt absent")
                XCTAssertEqual(f.channels, 1)
                XCTAssertEqual(f.rate, 16_000)
                XCTAssertEqual(f.bits, 16)
                return stride(from: at + 8, to: at + 8 + size - 1, by: 2).map {
                    Float(Int16(bitPattern: UInt16(u16($0)))) / 32768
                }
            }
            at += 8 + size + size % 2
        }
        throw FixtureFailure(message: "\(url.path) : WAV sans données")
    }
}
