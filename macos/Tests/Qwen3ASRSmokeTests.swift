import XCTest
import MLX
import KokoroMLX

/// Fumée de Qwen3-ASR copié dans le framework MLX : chaque modèle (0.6B,
/// 1.7B) se charge d'un dossier local, avec notre vocabulaire BPE, et lit
/// une seconde de silence sans langue imposée (il la reconnaît seul) sans
/// planter. Le gabarit garde un `<|audio_pad|>` par jeton audio.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `SILLAGE_STT_MODELS`
/// (`TEST_RUNNER_SILLAGE_STT_MODELS` pour xcodebuild).
final class Qwen3ASRSmokeTests: XCTestCase {
    func testTheSmallModelReadsOneSecondOfSilenceWithoutCrashing() throws {
        try readsSilence("qwen3_asr_06b_8bit")
    }

    func testTheLargeModelReadsOneSecondOfSilenceWithoutCrashing() throws {
        try readsSilence("qwen3_asr_17b_4bit")
    }

    private func readsSilence(_ id: String) throws {
        let model = try Qwen3ASRModel.fromModelDirectory(try SmokeModels.folder(id))
        let prompt = model.buildPrompt(numAudioTokens: 13)
        let ids = prompt.asArray(Int32.self)
        XCTAssertEqual(ids.filter { Int($0) == model.config.audioTokenId }.count, 13, id)
        let output = model.generate(audio: MLXArray.zeros([16_000]), language: nil)
        XCTAssertNotNil(output.text, id)
        Memory.clearCache()
    }
}
