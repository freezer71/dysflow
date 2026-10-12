import XCTest
@testable import Sillon

@MainActor
final class KokoroWarmUpTests: XCTestCase {
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.kokoro.preparation"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testTheMetalKernelsArePreparedOncePerVersion() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults)?.value
        XCTAssertEqual(compute.loads, 1)
        XCTAssertEqual(compute.warmUps, 1)
        XCTAssertEqual(compute.releases, 1, "le modèle est libéré tout de suite")
        XCTAssertFalse(KokoroWarmUp.isRunning)
        XCTAssertNil(KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults))
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "2", defaults: defaults)?.value
        XCTAssertEqual(compute.warmUps, 2, "nouvelle version : nouvelle préparation")
    }

    func testAFailedPreparationIsTriedAgainAtTheNextLaunch() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        compute.failLoad = true
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults)?.value
        XCTAssertNil(defaults.string(forKey: KokoroWarmUp.versionKey))
        XCTAssertNotNil(KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults))
    }

    func testOnlyTheMLXPackageIsPrepared() {
        var settings = SettingsView.defaults
        settings.family = "kokoro"
        settings.modelPath = "/m/kokoro/kokoro-v1_0.safetensors"
        XCTAssertEqual(KokoroWarmUp.modelDirectory(settings: settings), "/m/kokoro")
        settings.modelPath = "/m/kokoro/model_quantized.onnx"
        XCTAssertNil(KokoroWarmUp.modelDirectory(settings: settings))
        settings.family = "supertonic"
        settings.modelPath = "/m/supertonic/supertonic-3-f16.gguf"
        XCTAssertNil(KokoroWarmUp.modelDirectory(settings: settings))
    }

    func testEspeakIsLookedForInsideTheApp() {
        AppDelegate.exportEspeakLocation(bundle: .main)
        let helper = ProcessInfo.processInfo.environment["SILLON_ESPEAK"] ?? ""
        XCTAssertTrue(helper.hasSuffix("Contents/Helpers/sillon-espeak"), helper)
        XCTAssertNotNil(ProcessInfo.processInfo.environment["SILLON_ESPEAK_DATA"])
    }

    /// La fin d'un téléchargement peut faire de Kokoro la voix active
    /// (proposition acceptée) : c'est le moment de préparer les noyaux.
    func testTheEndOfADownloadIsSeen() {
        let going = DownloadView(id: "kokoro_82m_mlx", received: 1, total: 2, failed: nil)
        let failed = DownloadView(id: "kokoro_82m_mlx", received: 1, total: 2, failed: "réseau")
        XCTAssertTrue(ReaderModel.downloadEnded(from: going, to: nil))
        XCTAssertTrue(ReaderModel.downloadEnded(from: going, to: failed))
        XCTAssertFalse(ReaderModel.downloadEnded(from: going, to: going))
        XCTAssertFalse(ReaderModel.downloadEnded(from: nil, to: nil))
        XCTAssertFalse(ReaderModel.downloadEnded(from: failed, to: nil))
        XCTAssertFalse(ReaderModel.downloadEnded(from: nil, to: going))
    }

    /// La clé change avec l'exécutable : une mise à jour prépare à nouveau.
    func testTheVersionFollowsTheExecutable() {
        let version = KokoroWarmUp.version(of: .main)
        XCTAssertEqual(version, KokoroWarmUp.version(of: .main))
        XCTAssertFalse(version.hasSuffix("-?"), version)
    }
}
