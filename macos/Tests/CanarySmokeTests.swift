import XCTest
import MLX
import KokoroMLX

/// Fumée de Canary 1b-v2 copié dans le framework MLX : le modèle se charge
/// d'un dossier local, trouve son vocabulaire dans `config.json` et transcrit
/// une seconde de silence en français sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `SILLON_STT_MODELS`
/// (`TEST_RUNNER_SILLON_STT_MODELS` pour xcodebuild).
final class CanarySmokeTests: XCTestCase {
    /// `SILLON_STT_MODELS` : dossier contenant `canary_1b_v2_q8/` (copie de
    /// ~/Library/Application Support/sillon/models). Sans lui : ignoré.
    func testTranscribesOneSecondOfSilenceInFrenchWithoutCrashing() throws {
        guard let root = ProcessInfo.processInfo.environment["SILLON_STT_MODELS"] else {
            throw XCTSkip("SILLON_STT_MODELS absent")
        }
        let model = try CanaryModel.fromModelDirectory(URL(fileURLWithPath: root).appendingPathComponent("canary_1b_v2_q8"))
        // Sans vocabulaire, la sortie serait des ids bruts (« <12><34> »).
        XCTAssertNotNil(model.tokenizer, "vocabulaire (tokenizer.model_base64 de config.json) non lu")
        let parameters = STTGenerateParameters(maxTokens: 1024, temperature: 0, topP: 1, language: "fr")
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: parameters)
        XCTAssertEqual(output.language, "fr")
        XCTAssertLessThan(output.generationTokens, 1024, "la génération n'a pas trouvé de fin sur du silence")
    }
}
