import XCTest
import MLX
import KokoroMLX

/// Fumée de Whisper copié dans le framework MLX : le modèle se charge d'un
/// dossier local, sans rien télécharger, trouve son vocabulaire dans
/// `multilingual.tiktoken` et transcrit une seconde de silence en français
/// sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `SILLON_STT_MODELS`
/// (`TEST_RUNNER_SILLON_STT_MODELS` pour xcodebuild).
final class WhisperSmokeTests: XCTestCase {
    /// `SILLON_STT_MODELS` : dossier contenant `whisper_tiny_4bit/` (copie de
    /// ~/Library/Application Support/sillon/models). Sans lui : ignoré.
    func testTranscribesOneSecondOfSilenceInFrenchWithoutCrashing() throws {
        guard let root = ProcessInfo.processInfo.environment["SILLON_STT_MODELS"] else {
            throw XCTSkip("SILLON_STT_MODELS absent")
        }
        let model = try WhisperModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent("whisper_tiny_4bit"))
        let defaults = model.defaultGenerationParameters
        let parameters = STTGenerateParameters(
            maxTokens: defaults.maxTokens, temperature: 0, topP: 1, language: "fr",
            chunkDuration: defaults.chunkDuration, minChunkDuration: defaults.minChunkDuration
        )
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: parameters)
        // Invite : <|startoftranscript|> <|fr|> <|transcribe|> <|notimestamps|>.
        // Sans les ids spéciaux en dur, la langue et la tâche manqueraient (2).
        XCTAssertEqual(output.promptTokens, 4)
        XCTAssertEqual(output.language, "fr")
        XCTAssertLessThan(output.generationTokens, defaults.maxTokens, "la génération n'a pas trouvé de fin sur du silence")
        Memory.clearCache()
    }
}
