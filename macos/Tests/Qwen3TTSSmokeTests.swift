import XCTest
import KokoroMLX

/// Fumée de Qwen3-TTS copié dans le framework : le vrai modèle (MLX, Metal),
/// seulement avec `DYSFLOW_TTS_MODELS` (dossier contenant
/// `qwen3_tts_06b_base_8bit/` et `qwen3_tts_17b_customvoice_8bit/`, copie
/// de ~/Library/Application Support/dysflow/models ;
/// `TEST_RUNNER_DYSFLOW_TTS_MODELS` pour xcodebuild). Sans lui : ignoré.
final class Qwen3TTSSmokeTests: XCTestCase {
    func runner(_ id: String) throws -> Qwen3TTSRunner {
        guard let root = ProcessInfo.processInfo.environment["DYSFLOW_TTS_MODELS"] else {
            throw XCTSkip("DYSFLOW_TTS_MODELS absent")
        }
        let dir = URL(fileURLWithPath: root).appendingPathComponent(id)
        guard FileManager.default.fileExists(atPath: dir.path) else { throw XCTSkip("\(id) absent") }
        return try Qwen3TTSRunner.load(directory: dir)
    }

    func check(_ samples: [Float]?, file: StaticString = #filePath, line: UInt = #line) {
        guard let samples else { return XCTFail("arrêté sans raison", file: file, line: line) }
        let seconds = Double(samples.count) / Double(Qwen3TTSRunner.sampleRate)
        XCTAssertTrue((0.3...6).contains(seconds), "\(seconds) s", file: file, line: line)
        XCTAssertTrue(samples.allSatisfy(\.isFinite), file: file, line: line)
    }

    func testTheFixedFrenchVoiceSaysHello() throws {
        let r = try runner("qwen3_tts_06b_base_8bit")
        XCTAssertEqual(r.voices, ["siwis"])
        check(try r.synthesize(text: "Bonjour.", language: "fr", voice: "siwis", shouldStop: { false }))
    }

    func testCustomVoiceSaysHelloAndStopsWhenAsked() throws {
        let r = try runner("qwen3_tts_17b_customvoice_8bit")
        XCTAssertEqual(r.voices.count, 9)
        check(try r.synthesize(text: "Bonjour.", language: "fr", voice: "vivian", shouldStop: { false }))
        XCTAssertNil(try r.synthesize(text: "Une phrase qui ne finira pas.", language: "fr", voice: "eric",
                                      shouldStop: { true }))
        XCTAssertThrowsError(try r.synthesize(text: "x", language: "fr", voice: "inconnue", shouldStop: { false }))
    }
}
