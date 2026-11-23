import XCTest
import MLX
@testable import Dysflow
@testable import KokoroMLX

/// P1 du port de Voxtral Realtime en Rust (procédure de port, tâche 21) : le
/// Voxtral Swift actuel (`VoxtralRealtimeModel.fromDirectory`, puis une
/// session de flux au retard de 480 ms, comme `VoxtralSpeech.makeSession`
/// la crée dans l'app), sur les entrées de la tâche, écrit la référence que
/// le port Rust doit rendre, `tests/fixtures/mlx/voxtral_realtime_4bit.json`.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient `voxtral_realtime_4bit/`)
/// et `DYSFLOW_FIXTURE_COMMIT` (le commit d'origine, écrit dans la
/// fixture). Pour xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh VoxtralFixtureTests
///
/// Les entrées : celles de Whisper (`WhisperFixtureTests.inputs`, les cinq
/// de Parakeet plus `trente_cinq_secondes`, qui passe les 256 lignes du
/// cache du décodeur et la fenêtre de 750 trames de l'encodeur). Voxtral
/// n'a pas de langue : aucune n'est donnée. Pour chaque entrée :
/// - l'audio donné par pas de 80 ms (1 280 échantillons, un jeton, le pas
///   de la dictée) : la forme et les moyennes de toutes les colonnes de mel
///   que la session calcule (mises bout à bout), et de chaque bloc de lignes
///   de l'adaptateur (la sortie de l'encodeur) avec son pas ; puis, pas par
///   pas, le texte nouveau et les jetons nouveaux (`Delta`, seulement les
///   pas où l'un ou l'autre n'est pas vide), le texte et les jetons de
///   `finish`, tous les jetons, le texte de la session et celui que
///   `VoxtralSession.finish` rend (rogné) ;
/// - le même audio en d'autres découpages (`splits`) : les jetons et le
///   texte, pour juger du « même texte quel que soit le découpage » sur le
///   Swift lui-même.
/// Décodage glouton (température 0) : aucun tirage, aucune graine.
final class VoxtralFixtureTests: XCTestCase {
    /// La révision épinglée du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let model = (id: "voxtral_realtime_4bit", revision: "fdebf7b2af834a1db4b8a3c99ab7480b333adf9e")

    /// Le pas de la référence : 80 ms, celui de la dictée (`STREAM_STEP`).
    static let stepSamples = 1_280

    /// Les découpages comparés : des tailles de `step`, reprises en boucle
    /// jusqu'à la fin de l'audio. `[]` : tout l'audio d'un coup.
    static let splits: [(name: String, sizes: [Int])] = [
        ("entier", []),
        ("tranches_480ms", [7_680]),
        ("irregulier", [1, 160, 1_279, 4_000, 37, 9_000, 2]),
    ]

    struct Delta: Codable {
        let step: Int
        let text: String
        let tokens: [Int]
    }

    struct Block: Codable {
        let step: Int
        let stats: ParakeetFixtureTests.Stats
    }

    struct Split: Codable {
        let split: String
        let tokens: [Int]
        let text: String
    }

    struct Case: Codable {
        let input: String
        /// Toutes les colonnes de mel de la session, bout à bout, `[128, n]`.
        let mel: ParakeetFixtureTests.Stats
        /// Chaque bloc de lignes de l'adaptateur, avec le pas qui l'a rendu
        /// (`steps` pour `finish`).
        let adapter: [Block]
        let steps: Int
        let deltas: [Delta]
        let finish_text: String
        let finish_tokens: [Int]
        let tokens: [Int]
        let finished: Bool
        /// `session.text`, tel quel.
        let session_text: String
        /// `VoxtralSession.finish()` : `session.text` rogné.
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
        let quantization: String
        let delay_ms: Int
        let delay_tokens: Int
        let max_tokens: Int
        let step_samples: Int
        let seed: String?
        let inputs: [ParakeetFixtureTests.Input]
        let splits: [SplitDescription]
        let cases: [Case]
    }

    func testWritesTheVoxtralFixture() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try WhisperFixtureTests.inputs(repo)

        let (id, revision) = Self.model
        let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
        let configText = try String(contentsOf: dir.appendingPathComponent("config.json"), encoding: .utf8)
        let quantization = try ParakeetFixtureTests.quantization(configText)
        try autoreleasepool {
            let model = try VoxtralRealtimeModel.fromDirectory(dir)
            var cases: [Case] = []
            for input in inputs {
                cases.append(Self.run(model, input.name, input.samples))
            }
            let fixture = Fixture(
                model: id, revision: revision, commit: commit,
                generator: "macos/Tests/VoxtralFixtureTests.swift",
                quantization: quantization, delay_ms: VoxtralSpeech.delayMs,
                delay_tokens: model.numDelayTokens(VoxtralSpeech.delayMs),
                max_tokens: model.defaultGenerationParameters.maxTokens,
                step_samples: Self.stepSamples, seed: nil,
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

    /// La session de l'app : `VoxtralSpeech.makeSession` (retard de 480 ms,
    /// 4 096 jetons au plus, température 0).
    static func session(_ model: VoxtralRealtimeModel) -> VoxtralRealtimeStreamSession {
        model.makeStreamSession(transcriptionDelayMs: VoxtralSpeech.delayMs)
    }

    /// Une entrée : la session de l'app par pas de 80 ms (mel, adaptateur,
    /// textes et jetons de chaque pas), puis les autres découpages.
    static func run(_ model: VoxtralRealtimeModel, _ name: String, _ samples: [Float]) -> Case {
        let session = session(model)
        var mels: [MLXArray] = []
        var adapter: [Block] = []
        var step = 0
        session.melTraceEmitter = { mel in
            eval(mel)
            mels.append(mel)
        }
        session.adapterTraceEmitter = { rows in
            adapter.append(Block(step: step, stats: ParakeetFixtureTests.stats(rows)))
        }
        var deltas: [Delta] = []
        let steps = NemotronFixtureTests.pieces(samples, [stepSamples])
        for (index, piece) in steps.enumerated() {
            step = index
            let delta = session.step(piece)
            if !delta.text.isEmpty || !delta.tokenIds.isEmpty {
                deltas.append(Delta(step: index, text: delta.text, tokens: delta.tokenIds))
            }
        }
        step = steps.count
        let finish = session.finish()
        session.melTraceEmitter = nil
        session.adapterTraceEmitter = nil
        let mel = MLX.concatenated(mels, axis: 1)

        var splits: [Split] = []
        for split in Self.splits {
            let other = Self.session(model)
            for piece in NemotronFixtureTests.pieces(samples, split.sizes) {
                other.step(piece)
            }
            other.finish()
            splits.append(Split(split: split.name, tokens: other.tokens,
                                text: other.text.trimmingCharacters(in: .whitespacesAndNewlines)))
        }

        return Case(
            input: name, mel: ParakeetFixtureTests.stats(mel), adapter: adapter,
            steps: steps.count, deltas: deltas,
            finish_text: finish.text, finish_tokens: finish.tokenIds,
            tokens: session.tokens, finished: session.isFinished,
            session_text: session.text,
            text: session.text.trimmingCharacters(in: .whitespacesAndNewlines),
            splits: splits)
    }
}
