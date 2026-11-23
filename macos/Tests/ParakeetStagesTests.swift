import XCTest
import MLX
import MLXNN
@testable import KokoroMLX

/// Les étages internes de l'encodeur de Parakeet, écrits pour que le port
/// Rust relance chacun sur l'entrée exacte du Swift et les compare élément
/// par élément (`stt::mlx::parakeet::conformer::stages`, tâche 14). Pas une
/// fixture : un dossier hors du dépôt, ~2,4 Mo par modèle.
///
/// Ignoré sans `DYSFLOW_STT_MODELS` (le dossier qui contient
/// `parakeet_tdt_v3/` et `parakeet_tdt_v2/`) et `DYSFLOW_STAGES_OUT` (le
/// dossier où écrire `<modèle>-etages.safetensors`), chacun préfixé de
/// `TEST_RUNNER_` pour xcodebuild :
///
///   TEST_RUNNER_DYSFLOW_STT_MODELS=… TEST_RUNNER_DYSFLOW_STAGES_OUT=… \
///   ./scripts/test-macos.sh ParakeetStagesTests
///
/// Entrée : le français de la fixture. Étages : mel, sous-échantillonnage
/// (chaque convolution), positions, chaque sous-étage du bloc 0, chaque
/// opération du module de convolution, la sortie de l'encodeur.
final class ParakeetStagesTests: XCTestCase {
    func testWritesTheEncoderStages() throws {
        let env = ProcessInfo.processInfo.environment
        guard let root = env["DYSFLOW_STT_MODELS"], let out = env["DYSFLOW_STAGES_OUT"] else {
            throw XCTSkip("DYSFLOW_STT_MODELS ou DYSFLOW_STAGES_OUT absent : rien à écrire")
        }
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fr = try ParakeetFixtureTests.pcm16(repo.appendingPathComponent("macos/Tests/Fixtures/dictee-charge.wav"))
        for id in ["parakeet_tdt_v2", "parakeet_tdt_v3"] {
            let model = try ParakeetModel.fromDirectory(URL(fileURLWithPath: root).appendingPathComponent(id))
            var d: [String: MLXArray] = [:]
            let mel = ParakeetAudio.logMelSpectrogram(MLXArray(fr), config: model.preprocessConfig)
            d["mel"] = mel
            let x = mel.asType(model.computeDType)
            d["features"] = x
            let enc = model.encoder
            let lengths = MLXArray([Int32(x.shape[1])])
            let sub = enc.preEncodeDw!
            var s0 = relu(sub.conv0(x.expandedDimensions(axis: 3))); d["s_conv0"] = s0
            let dw0 = sub.depthwiseLayers[0](s0); d["s_dw0"] = dw0
            s0 = relu(sub.pointwiseLayers[0](dw0)); d["s_pw0"] = s0
            let dw1 = sub.depthwiseLayers[1](s0); d["s_dw1"] = dw1
            s0 = relu(sub.pointwiseLayers[1](dw1)); d["s_pw1"] = s0
            let (pre, _) = enc.preEncodeDw!(x, lengths: lengths)
            d["pre"] = pre
            let (h, pos) = enc.posEnc!(pre)
            d["pos_x"] = h
            d["pos"] = pos
            let l = enc.layers[0]
            let n1 = l.normFeedForward1(h); d["n1"] = n1
            let f1 = l.feedForward1(n1); d["f1"] = f1
            let y1 = h + MLXArray(Float(0.5)).asType(h.dtype) * f1; d["y1"] = y1
            let n2 = l.normSelfAtt(y1); d["n2"] = n2
            let att = l.relSelfAttn!(n2, n2, n2, posEmb: pos); d["att"] = att
            let y2 = y1 + att; d["y2"] = y2
            let n3 = l.normConv(y2); d["n3"] = n3
            let cv = l.conv(n3); d["cv"] = cv
            let c = l.conv
            let pw = c.pointwiseConv1(n3); d["c_pw1"] = pw
            let halves = pw.split(parts: 2, axis: 2)
            let gated = halves[0] * sigmoid(halves[1]); d["c_gated"] = gated
            let dwc = c.depthwiseConv(gated); d["c_dw"] = dwc
            let bn = c.batchNorm(dwc); d["c_bn"] = bn
            let si = silu(bn); d["c_silu"] = si
            d["c_pw2"] = c.pointwiseConv2(si)
            let y3 = y2 + cv; d["y3"] = y3
            let n4 = l.normFeedForward2(y3); d["n4"] = n4
            let f2 = l.feedForward2(n4); d["f2"] = f2
            let y4 = y3 + MLXArray(Float(0.5)).asType(y3.dtype) * f2; d["y4"] = y4
            let o0 = l.normOut(y4); d["layer0"] = o0
            let layer0Direct = l(h, posEmb: pos); d["layer0_direct"] = layer0Direct
            let (full, _) = model.encodeBatchFeatures(x)
            d["encoder"] = full
            eval(Array(d.values))
            try save(arrays: d, url: URL(fileURLWithPath: out).appendingPathComponent("\(id)-etages.safetensors"))
            Memory.clearCache()
        }
    }
}
