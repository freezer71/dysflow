import XCTest
import MLX
@testable import KokoroMLX

/// Le conteneur de Phonon-2 : une matrice à cinq valeurs écrite comme le fait
/// Fermion (`fermion_container.py`) se relit à l'identique en deux plans
/// 2 bits, et la couche qui en sort calcule le même produit que la matrice
/// dense. Sans le modèle : tout est fabriqué ici.
final class PhononContainerTests: XCTestCase {
    /// Écrit [rows, columns] au format `five_value` : trits (chiffre 1 =
    /// zéro, lignes complétées à l'octet), un bit par non-nul (1 = hi), puis
    /// lo et hi en float16.
    private func fiveValueBlob(signs: [[Int]], isHi: [[Bool]], lo: [Float16], hi: [Float16]) -> Data {
        let columns = signs[0].count
        let rowBytes = (columns + 4) / 5
        var data = Data()
        var bits: [Bool] = []
        for (r, row) in signs.enumerated() {
            for b in 0 ..< rowBytes {
                var byte = 0
                for k in (0 ..< 5).reversed() {
                    let c = b * 5 + k
                    // Le remplissage au-delà de la ligne : chiffre 2, que le lecteur doit ignorer.
                    byte = byte * 3 + (c < columns ? row[c] + 1 : 2)
                }
                data.append(UInt8(byte))
            }
            for c in 0 ..< columns where row[c] != 0 { bits.append(isHi[r][c]) }
        }
        var packed = [UInt8](repeating: 0, count: (bits.count + 7) / 8)
        for (i, bit) in bits.enumerated() where bit { packed[i / 8] |= 1 << UInt8(i % 8) }
        data.append(contentsOf: packed)
        for value in lo + hi {
            withUnsafeBytes(of: value.bitPattern.littleEndian) { data.append(contentsOf: $0) }
        }
        return data
    }

    func testFiveValueRowsRoundTripThroughTwoPlanes() throws {
        let rows = 3, columns = 256
        var rng = SystemRandomNumberGenerator()
        let signs = (0 ..< rows).map { _ in (0 ..< columns).map { _ in Int.random(in: -1 ... 1, using: &rng) } }
        let isHi = (0 ..< rows).map { _ in (0 ..< columns).map { _ in Bool.random(using: &rng) } }
        let lo: [Float16] = [0.25, 0.5, 0.125], hi: [Float16] = [1.5, 2.0, 0.75]
        let blob = fiveValueBlob(signs: signs, isHi: isHi, lo: lo, hi: hi)

        let planes = try PhononContainer.planes(blob, rows: rows, columns: columns, name: "essai")
        let linear = FiveValueLinear(planes)
        let dense = linear.dense().asType(.float32)

        var expected: [Float] = []
        for r in 0 ..< rows {
            for c in 0 ..< columns {
                expected.append(Float(signs[r][c]) * Float(isHi[r][c] ? hi[r] : lo[r]))
            }
        }
        XCTAssertEqual(dense.asArray(Float.self), expected, "plans relus à l'identique")

        let x = MLXRandom.normal([2, columns])
        let reference = matmul(x, MLXArray(expected, [rows, columns]).T)
        let difference = abs(linear(x) - reference).max().item(Float.self)
        // Le produit 2 bits de MLX calcule s·Σqx − s·Σx (échelles en float16) :
        // un écart de l'ordre de 1e-3 relatif, pas une erreur de poids.
        XCTAssertLessThan(difference, 1e-2 * abs(reference).max().item(Float.self))
    }

    func testABlobOfTheWrongSizeIsRefused() {
        let blob = fiveValueBlob(signs: [[Int](repeating: 1, count: 128)], isHi: [[Bool](repeating: false, count: 128)],
                                 lo: [1], hi: [2]) + Data([0])
        XCTAssertThrowsError(try PhononContainer.planes(blob, rows: 1, columns: 128, name: "essai"))
    }

    func testIntSixRowsAreScaled() throws {
        // Deux lignes de quatre : q = −32, −1, 0, 31 puis 1, 2, 3, 4 (décalés de 32).
        let q: [Int] = [-32, -1, 0, 31, 1, 2, 3, 4]
        var data = Data()
        for group in stride(from: 0, to: q.count, by: 4) {
            var packed: UInt32 = 0
            for k in 0 ..< 4 { packed |= UInt32(q[group + k] + 32) << UInt32(6 * k) }
            data.append(contentsOf: [UInt8(packed & 0xFF), UInt8(packed >> 8 & 0xFF), UInt8(packed >> 16 & 0xFF)])
        }
        for scale: Float16 in [0.5, 2] {
            withUnsafeBytes(of: scale.bitPattern.littleEndian) { data.append(contentsOf: $0) }
        }
        let values = try PhononContainer.intSix(data, shape: [2, 4]).asArray(Float.self)
        XCTAssertEqual(values, [-16, -0.5, 0, 15.5, 2, 4, 6, 8])
    }

    /// Fumée du vrai modèle (MLX, Metal) : seulement avec `SILLON_STT_MODELS`
    /// (`TEST_RUNNER_SILLON_STT_MODELS` pour xcodebuild), dossier contenant
    /// `phonon_2/phonon-2.bps.tar.zst`. Sans lui : ignoré.
    func testTheRealArchiveLoadsAndTranscribesSilence() throws {
        guard let root = ProcessInfo.processInfo.environment["SILLON_STT_MODELS"] else {
            throw XCTSkip("SILLON_STT_MODELS absent")
        }
        let model = try ParakeetModel.fromPhonon(URL(fileURLWithPath: root).appendingPathComponent("phonon_2"))
        let output = model.generate(audio: MLXArray.zeros([16_000]), generationParameters: model.defaultGenerationParameters)
        XCTAssertNotNil(output.text)
    }

    /// Les noms de transformers deviennent ceux du graphe Swift (ceux de
    /// mlx-community, puis `ParakeetModel.remapKey`).
    func testTransformersNamesMapToTheSwiftGraph() {
        let cases = [
            "encoder.subsampling.layers.0.weight": "encoder.pre_encode.conv0.weight",
            "encoder.subsampling.layers.2.bias": "encoder.pre_encode.depthwise_layers.0.bias",
            "encoder.subsampling.layers.3.weight": "encoder.pre_encode.pointwise_layers.0.weight",
            "encoder.subsampling.layers.5.weight": "encoder.pre_encode.depthwise_layers.1.weight",
            "encoder.subsampling.layers.6.weight": "encoder.pre_encode.pointwise_layers.1.weight",
            "encoder.subsampling.linear.weight": "encoder.pre_encode.out.weight",
            "encoder.layers.3.self_attn.q_proj": "encoder.layers.3.self_attn.linear_q",
            "encoder.layers.3.self_attn.o_proj": "encoder.layers.3.self_attn.linear_out",
            "encoder.layers.3.self_attn.relative_k_proj": "encoder.layers.3.self_attn.linear_pos",
            "encoder.layers.3.self_attn.bias_u": "encoder.layers.3.self_attn.posBiasU",
            "encoder.layers.3.conv.norm.running_var": "encoder.layers.3.conv.batch_norm.running_var",
            "encoder.layers.3.conv.pointwise_conv1": "encoder.layers.3.conv.pointwise_conv1",
            "encoder.layers.3.feed_forward2.linear1": "encoder.layers.3.feed_forward2.linear1",
            "decoder.embedding.weight": "decoder.prediction.embed.weight",
            "decoder.lstm.weight_ih_l1": "decoder.prediction.dec_rnn.lstm.1.Wx",
            "decoder.lstm.weight_hh_l0": "decoder.prediction.dec_rnn.lstm.0.Wh",
            "decoder.decoder_projector.bias": "joint.pred.bias",
            "encoder_projector.weight": "joint.enc.weight",
            "joint.head.weight": "joint.joint_net.weight",
        ]
        for (source, target) in cases {
            XCTAssertEqual(PhononContainer.graphKey(source), target, source)
        }
    }
}
