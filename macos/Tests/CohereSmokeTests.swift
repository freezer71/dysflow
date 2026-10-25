import XCTest
import MLX
import KokoroMLX

/// Fumée de Cohere Transcribe copié dans le framework MLX : le modèle
/// (8 bits, beshkenadze) se charge d'un dossier local et lit une seconde de
/// silence en français (la langue est imposée, jamais devinée) sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `DYSFLOW_STT_MODELS`
/// (`TEST_RUNNER_DYSFLOW_STT_MODELS` pour xcodebuild).
final class CohereSmokeTests: XCTestCase {
    func testReadsOneSecondOfSilenceInFrenchWithoutCrashing() throws {
        let model = try CohereTranscribeModel.fromDirectory(try SmokeModels.folder("cohere_transcribe_8bit"))
        let base = model.defaultGenerationParameters
        let parameters = STTGenerateParameters(maxTokens: base.maxTokens, temperature: 0, topP: base.topP,
                                               topK: base.topK, verbose: false, language: "fr",
                                               chunkDuration: 30, minChunkDuration: base.minChunkDuration)
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: parameters)
        XCTAssertNotNil(output.text)
        XCTAssertEqual(output.language, "fr")
        Memory.clearCache()
    }
}
