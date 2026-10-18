import XCTest
import MLX
import KokoroMLX

/// Fumée de Parakeet v3 copié dans le framework MLX : le modèle se charge
/// d'un dossier local et transcrit une seconde de silence sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `SILLAGE_STT_MODELS`
/// (`TEST_RUNNER_SILLAGE_STT_MODELS` pour xcodebuild).
final class ParakeetSmokeTests: XCTestCase {
    /// `SILLAGE_STT_MODELS` : dossier contenant `parakeet_tdt_v3/` (copie de
    /// ~/Library/Application Support/sillage/models). Sans lui : ignoré.
    func testTranscribesOneSecondOfSilenceWithoutCrashing() throws {
        guard let root = ProcessInfo.processInfo.environment["SILLAGE_STT_MODELS"] else {
            throw XCTSkip("SILLAGE_STT_MODELS absent")
        }
        let model = try ParakeetModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent("parakeet_tdt_v3"))
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: model.defaultGenerationParameters)
        XCTAssertNotNil(output.text)
    }
}
