import XCTest
import KokoroMLX

/// Fumée de Magpie copié dans le framework : le vrai modèle, seulement avec
/// `SILLAGE_TTS_MODELS` (dossier contenant `magpie_tts_357m_8bit/`).
final class MagpieSmokeTests: XCTestCase {
    func runner() throws -> MagpieRunner {
        guard let root = ProcessInfo.processInfo.environment["SILLAGE_TTS_MODELS"] else {
            throw XCTSkip("SILLAGE_TTS_MODELS absent")
        }
        let dir = URL(fileURLWithPath: root).appendingPathComponent("magpie_tts_357m_8bit")
        guard FileManager.default.fileExists(atPath: dir.path) else { throw XCTSkip("magpie absent") }
        return try MagpieRunner.load(directory: dir)
    }

    func testSaysHelloInFrenchAndEnglish() throws {
        let r = try runner()
        for (text, language) in [("Bonjour.", "fr"), ("Hello.", "en")] {
            let samples = try XCTUnwrap(try r.synthesize(text: text, language: language, voice: "sofia", shouldStop: { false }))
            let seconds = Double(samples.count) / Double(r.sampleRate)
            XCTAssertTrue((0.2...5).contains(seconds), "\(text) : \(seconds) s")
            XCTAssertTrue(samples.allSatisfy(\.isFinite))
        }
        XCTAssertNil(try r.synthesize(text: "Une phrase arrêtée.", language: "fr", voice: "leo", shouldStop: { true }))
    }
}
