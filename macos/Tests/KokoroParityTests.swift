import XCTest
import MLX
@testable import Dysflow

/// Parité du port Swift de Kokoro avec mlx-audio Python, la chaîne du banc
/// d'écoute : mêmes identifiants, même style, même graine, même son. Lance
/// le vrai modèle (MLX, Metal) : seulement avec `DYSFLOW_KOKORO_DIR`, le
/// dossier du modèle (`TEST_RUNNER_DYSFLOW_KOKORO_DIR` pour xcodebuild).
/// Référence : `scripts/kokoro-parite.py`.
final class KokoroParityTests: XCTestCase {
    struct Reference: Decodable {
        let ids: [Int32]
        let style: [Float]
        let durations: [Int32]
        let audio_i16_base64: String
    }

    func testTheSwiftPortSoundsLikeTheBenchChain() throws {
        guard let dir = ProcessInfo.processInfo.environment["DYSFLOW_KOKORO_DIR"] else {
            throw XCTSkip("DYSFLOW_KOKORO_DIR absent : pas de modèle Kokoro")
        }
        let fixture = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("Fixtures/kokoro-parite.json")
        let reference = try JSONDecoder().decode(Reference.self, from: Data(contentsOf: fixture))
        let expected = Data(base64Encoded: reference.audio_i16_base64)!.withUnsafeBytes {
            $0.bindMemory(to: Int16.self).map { Float(Int16(littleEndian: $0)) / 32767 }
        }

        let runner = try KokoroMLXCompute.loadRunner(URL(fileURLWithPath: dir))
        MLXRandom.seed(7)
        let out = try XCTUnwrap(runner.run(ids: reference.ids, style: reference.style, shouldStop: nil))

        XCTAssertEqual(out.durations, reference.durations)
        XCTAssertEqual(out.samples.count, expected.count)
        let n = min(out.samples.count, expected.count)
        let meanError = zip(out.samples.prefix(n), expected.prefix(n)).map { abs($0 - $1) }.reduce(0, +) / Float(n)
        // Mesuré : 1 à 5 × 10⁻³ une fois corrigé, 4 × 10⁻² avec la trame
        // finale du bloc upsample mise à zéro (niveau RMS du signal ≈ 0,07).
        XCTAssertLessThan(meanError, 0.01)
    }
}
