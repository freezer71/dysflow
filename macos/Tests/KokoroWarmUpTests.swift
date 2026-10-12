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
        let retry = KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1", defaults: defaults)
        XCTAssertNotNil(retry)
        // `isRunning` est partagé : la tâche finit ici, pas pendant un autre test.
        await retry?.value
        XCTAssertFalse(KokoroWarmUp.isRunning)
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

    /// En publication, la clé suit les numéros de version ; en
    /// développement (ou sans numéro de build), l'UUID de l'exécutable.
    func testTheVersionFollowsTheNumbersOrTheExecutableUUID() {
        XCTAssertEqual(KokoroWarmUp.version(short: "0.2.0", build: "1", uuid: "U", development: false), "0.2.0(1)")
        XCTAssertEqual(KokoroWarmUp.version(short: "0.2.0", build: "1", uuid: "U", development: true), "0.2.0(1)-U")
        XCTAssertEqual(KokoroWarmUp.version(short: nil, build: nil, uuid: "U", development: false), "?(?)-U")
        let uuid = KokoroWarmUp.executableUUID(of: .main)
        XCTAssertNotNil(uuid.flatMap(UUID.init(uuidString:)), "LC_UUID de l'app hôte")
        XCTAssertEqual(KokoroWarmUp.version(of: .main), KokoroWarmUp.version(of: .main))
        XCTAssertTrue(KokoroWarmUp.version(of: .main).hasPrefix("0.2.0(1)"), KokoroWarmUp.version(of: .main))
    }
}
