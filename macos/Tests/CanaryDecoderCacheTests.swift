import XCTest
import MLX
@testable import KokoroMLX

/// Le décodage pas à pas de Canary (cache d'auto-attention, attention
/// croisée calculée une fois) rend les mêmes logits que le décodage de toute
/// la suite à chaque jeton, celui de l'amont. Petit décodeur aux poids
/// tirés au hasard, en Float32 : seul l'ordre des additions diffère.
final class CanaryDecoderCacheTests: XCTestCase {
    private func makeDecoder() -> CanaryDecoder {
        MLXRandom.seed(7)
        let config = CanaryDecoderConfig(numLayers: 2, hiddenSize: 32, numAttentionHeads: 4, innerSize: 64)
        let decoder = CanaryDecoder(config: config, vocabSize: 50, hiddenSize: 32)
        decoder.train(false)
        return decoder
    }

    /// Les logits de la dernière position, toute la suite repassée.
    private func full(_ decoder: CanaryDecoder, _ tokens: [Int], encoder: MLXArray, mask: MLXArray) -> MLXArray {
        let ids = MLXArray(tokens.map(Int32.init)).reshaped(1, tokens.count)
        let logits = decoder(ids, encoderOutput: encoder, encoderMask: mask)
        return logits[0, logits.dim(1) - 1]
    }

    private func step(_ decoder: CanaryDecoder, _ tokens: [Int], state: CanaryDecoderState) -> MLXArray {
        let ids = MLXArray(tokens.map(Int32.init)).reshaped(1, tokens.count)
        let logits = decoder(ids, state: state)
        return logits[0, logits.dim(1) - 1]
    }

    private func assertClose(_ a: MLXArray, _ b: MLXArray, _ message: String, file: StaticString = #filePath, line: UInt = #line) {
        let gap = MLX.abs(a - b).max().item(Float.self)
        XCTAssertLessThan(gap, 1e-4, message, file: file, line: line)
        XCTAssertEqual(a.argMax().item(Int.self), b.argMax().item(Int.self), message, file: file, line: line)
    }

    func testStepByStepMatchesTheFullPass() {
        let decoder = makeDecoder()
        let encoder = MLXRandom.normal([1, 20, 32])
        let mask = MLXArray.ones([1, 20])
        let tokens = (0..<16).map { ($0 * 7 + 3) % 50 }
        let state = decoder.makeState(encoderOutput: encoder, encoderMask: mask)
        assertClose(step(decoder, Array(tokens[..<5]), state: state), full(decoder, Array(tokens[..<5]), encoder: encoder, mask: mask),
                    "l'invite")
        for k in 6...tokens.count {
            assertClose(step(decoder, [tokens[k - 1]], state: state), full(decoder, Array(tokens[..<k]), encoder: encoder, mask: mask),
                        "jeton \(k)")
        }
        XCTAssertEqual(state.offset, tokens.count)
    }

    /// Au-delà de la table des positions (1 024), la dernière est répétée,
    /// comme dans la suite entière.
    func testPositionsPastTheTableRepeatTheLastOne() {
        let decoder = makeDecoder()
        let encoder = MLXRandom.normal([1, 8, 32])
        let mask = MLXArray.ones([1, 8])
        let tokens = (0..<1030).map { ($0 * 11 + 1) % 50 }
        let state = decoder.makeState(encoderOutput: encoder, encoderMask: mask)
        _ = step(decoder, Array(tokens[..<1020]), state: state)
        for k in 1021...tokens.count {
            assertClose(step(decoder, [tokens[k - 1]], state: state), full(decoder, Array(tokens[..<k]), encoder: encoder, mask: mask),
                        "position \(k - 1)")
        }
    }
}
