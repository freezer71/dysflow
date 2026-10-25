import XCTest
import MLX
import KokoroMLX

/// Fumée de Voxtral Realtime copié dans le framework MLX : le modèle se
/// charge d'un dossier local, et une session de flux reçoit 0,5 s de silence
/// par blocs de 80 ms (un jeton, 1 280 échantillons à 16 kHz) puis se termine
/// sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `DYSFLOW_STT_MODELS`
/// (`TEST_RUNNER_DYSFLOW_STT_MODELS` pour xcodebuild).
final class VoxtralSmokeTests: XCTestCase {
    /// `DYSFLOW_STT_MODELS` : dossier contenant `voxtral_realtime_4bit/` (copie
    /// de ~/Library/Application Support/dysflow/models). Sans lui : ignoré.
    func testStreamsHalfASecondOfSilenceThenFinishesWithoutCrashing() throws {
        guard let root = ProcessInfo.processInfo.environment["DYSFLOW_STT_MODELS"] else {
            throw XCTSkip("DYSFLOW_STT_MODELS absent")
        }
        let model = try VoxtralRealtimeModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent("voxtral_realtime_4bit"))
        let session = model.makeStreamSession()
        let silence = [Float](repeating: 0, count: 8_000)
        for start in stride(from: 0, to: silence.count, by: 1_280) {
            session.step(Array(silence[start..<min(start + 1_280, silence.count)]))
        }
        session.finish()
        // `finish()` scelle le flux : un bloc de plus n'est plus lu.
        let afterFinish = session.step([Float](repeating: 0, count: 1_280))
        XCTAssertEqual(afterFinish.text, "")
        XCTAssertEqual(afterFinish.tokenIds, [])
        Memory.clearCache()
    }
}
