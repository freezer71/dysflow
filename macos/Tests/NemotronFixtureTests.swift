import XCTest
import MLX
@testable import Dysflow
@testable import KokoroMLX

/// P1 du port de Nemotron 3.5 Streaming en Rust (procédure de port,
/// tâche 20) : le Nemotron Swift actuel (`NemotronASRModel.fromDirectory`,
/// puis une session de flux en tranches de 560 ms, enveloppée de
/// `NemotronSession` comme `NemotronSpeech` le fait dans l'app), sur les
/// entrées de la tâche, écrit la référence que le port Rust doit rendre,
/// `tests/fixtures/mlx/nemotron_35_asr_streaming_8bit.json`.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient
/// `nemotron_35_asr_streaming_8bit/`) et `DYSFLOW_FIXTURE_COMMIT` (le commit
/// d'origine, écrit dans la fixture). Pour xcodebuild, chacun préfixé de
/// `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh NemotronFixtureTests
///
/// Les entrées : les cinq de Parakeet (`ParakeetFixtureTests.inputs`). La
/// langue de l'invite est imposée : `en` pour l'anglais, `fr` pour le reste.
/// Pour chaque entrée :
/// - la forme et les moyennes du mel de tout l'audio (Float32, celui que la
///   session lit à `finish`) ;
/// - l'audio donné par pas de 80 ms (1 280 échantillons, le pas de la
///   dictée) : la forme et les moyennes de chaque tranche de l'encodeur
///   après l'invite (`chunkTraceEmitter`), puis, pas par pas, le texte
///   nouveau que `NemotronSession.step` rend et les jetons nouveaux de la
///   session (seulement les pas où l'un ou l'autre n'est pas vide), les
///   jetons nouveaux de `finish`, tous les jetons, le texte brut de la
///   session et le texte de `NemotronSession.finish` ;
/// - le même audio en d'autres découpages (`splits`) : les jetons et le
///   texte de `finish`, pour juger du « même texte quel que soit le
///   découpage » sur le Swift lui-même.
/// Décodage glouton : aucun tirage, aucune graine.
final class NemotronFixtureTests: XCTestCase {
    /// La révision épinglée du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let model = (id: "nemotron_35_asr_streaming_8bit", revision: "7279359e4481b5e9e185a318bd618e429c6d86cd")

    /// Le pas de la référence : 80 ms, celui de la dictée (`STREAM_STEP`).
    static let stepSamples = 1_280

    /// Les découpages comparés : des tailles de `feed`, reprises en boucle
    /// jusqu'à la fin de l'audio. `[]` : tout l'audio d'un coup.
    static let splits: [(name: String, sizes: [Int])] = [
        ("entier", []),
        ("tranches_560ms", [8_960]),
        ("irregulier", [1, 160, 1_279, 4_000, 37, 9_000, 2]),
    ]

    struct Delta: Codable {
        let step: Int
        /// Ce que `NemotronSession.step` rend : le texte réduit nouveau.
        let text: String
        /// Les jetons que la session a gardés pendant ce pas.
        let tokens: [Int]
    }

    struct Split: Codable {
        let split: String
        let tokens: [Int]
        let text: String
    }

    struct Case: Codable {
        let input: String
        let language: String
        let mel: ParakeetFixtureTests.Stats
        /// Chaque tranche de l'encodeur après l'invite, dans l'ordre.
        let chunks: [ParakeetFixtureTests.Stats]
        let steps: Int
        let deltas: [Delta]
        let finish_tokens: [Int]
        let tokens: [Int]
        /// `session.text`, tel quel.
        let session_text: String
        /// `NemotronSession.finish()` : réduit, sans espaces autour.
        let text: String
        let splits: [Split]
    }

    struct SplitDescription: Codable {
        let name: String
        let sizes: [Int]
    }

    struct Fixture: Codable {
        let model: String
        let revision: String
        let commit: String
        let generator: String
        let compute_dtype: String
        let quantization: String
        let chunk_ms: Int
        let step_samples: Int
        let seed: String?
        let inputs: [ParakeetFixtureTests.Input]
        let splits: [SplitDescription]
        let cases: [Case]
    }

    func testWritesTheNemotronFixture() throws {
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
            let model = try NemotronASRModel.fromDirectory(dir)
            var cases: [Case] = []
            for input in inputs {
                let language = input.name == "anglais" ? "en" : "fr"
                cases.append(Self.run(model, input.name, language, input.samples))
            }
            let fixture = Fixture(
                model: id, revision: revision, commit: commit,
                generator: "macos/Tests/NemotronFixtureTests.swift",
                compute_dtype: "\(model.computeDType)", quantization: quantization,
                chunk_ms: NemotronSpeech.chunkMs, step_samples: Self.stepSamples, seed: nil,
                inputs: ParakeetFixtureTests.described(inputs),
                splits: Self.splits.map { SplitDescription(name: $0.name, sizes: $0.sizes) },
                cases: cases)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            var data = try encoder.encode(fixture)
            data.append(0x0A)
            try data.write(to: repo.appendingPathComponent("tests/fixtures/mlx/\(id).json"))
        }
        Memory.clearCache()
    }

    /// `samples` coupés selon `sizes`, reprises en boucle ; `[]` : d'un coup.
    static func pieces(_ samples: [Float], _ sizes: [Int]) -> [[Float]] {
        guard !sizes.isEmpty else { return [samples] }
        var out: [[Float]] = []
        var at = 0, i = 0
        while at < samples.count {
            let end = min(at + sizes[i % sizes.count], samples.count)
            out.append(Array(samples[at..<end]))
            at = end
            i += 1
        }
        return out
    }

    /// Une entrée : le mel de tout l'audio, puis la session de l'app par pas
    /// de 80 ms (tranches, textes et jetons de chaque pas), puis les autres
    /// découpages.
    static func run(_ model: NemotronASRModel, _ name: String, _ language: String,
                    _ samples: [Float]) -> Case {
        let mel = NemotronASRAudio.logMelSpectrogram(MLXArray(samples), config: model.preprocessConfig)
        eval(mel)

        let session = model.makeStreamSession(language: language, chunkMs: NemotronSpeech.chunkMs)
        var chunks: [ParakeetFixtureTests.Stats] = []
        session.chunkTraceEmitter = { prompted in
            chunks.append(ParakeetFixtureTests.stats(prompted))
        }
        let app = NemotronSession(session)
        var deltas: [Delta] = []
        let steps = pieces(samples, [stepSamples])
        for (index, piece) in steps.enumerated() {
            let before = session.tokens.count
            let text = app.step(piece)
            let tokens = Array(session.tokens[before...])
            if !text.isEmpty || !tokens.isEmpty {
                deltas.append(Delta(step: index, text: text, tokens: tokens))
            }
        }
        let before = session.tokens.count
        let text = app.finish()
        let finishTokens = Array(session.tokens[before...])
        session.chunkTraceEmitter = nil

        var splits: [Split] = []
        for split in Self.splits {
            let other = model.makeStreamSession(language: language, chunkMs: NemotronSpeech.chunkMs)
            let otherApp = NemotronSession(other)
            for piece in pieces(samples, split.sizes) {
                _ = otherApp.step(piece)
            }
            let otherText = otherApp.finish()
            splits.append(Split(split: split.name, tokens: other.tokens, text: otherText))
        }

        return Case(
            input: name, language: language, mel: ParakeetFixtureTests.stats(mel),
            chunks: chunks, steps: steps.count, deltas: deltas, finish_tokens: finishTokens,
            tokens: session.tokens, session_text: session.text, text: text, splits: splits)
    }
}
