import XCTest
import MLX
import KokoroMLX

/// Fumée de Nemotron 3.5 ASR Streaming copié dans le framework MLX : le
/// modèle se charge d'un dossier local, lit une seconde de silence d'un bloc,
/// puis une session de flux en français reçoit la même seconde par blocs de
/// 80 ms (le pas de la dictée) et se termine sans planter.
/// Lance le vrai modèle (MLX, Metal) : seulement avec `DYSFLOW_STT_MODELS`
/// (`TEST_RUNNER_DYSFLOW_STT_MODELS` pour xcodebuild).
final class NemotronSmokeTests: XCTestCase {
    /// `DYSFLOW_STT_MODELS` : dossier contenant
    /// `nemotron_35_asr_streaming_8bit/` (copie de
    /// ~/Library/Application Support/dysflow/models). Sans lui : ignoré.
    func testReadsAndStreamsOneSecondOfSilenceWithoutCrashing() throws {
        let model = try NemotronASRModel.fromDirectory(try SmokeModels.folder("nemotron_35_asr_streaming_8bit"))
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: model.defaultGenerationParameters)
        XCTAssertNotNil(output.text)

        // 560 ms : la tranche de la dictée (`NemotronSpeech.chunkMs`).
        let session = model.makeStreamSession(language: "fr", chunkMs: 560)
        let silence = [Float](repeating: 0, count: 16_000)
        for start in stride(from: 0, to: silence.count, by: 1_280) {
            session.step(Array(silence[start..<min(start + 1_280, silence.count)]))
        }
        session.finish()
        XCTAssertTrue(session.isFinished)
        // `finish()` scelle le flux : un bloc de plus n'est plus lu.
        let afterFinish = session.step([Float](repeating: 0, count: 1_280))
        XCTAssertEqual(afterFinish.text, "")
        XCTAssertEqual(afterFinish.tokenIds, [])
        Memory.clearCache()
    }
}
