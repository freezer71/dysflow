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

    /// Chaque activation de Kokoro (chaque lancement, chaque retour à
    /// Kokoro) prépare : au second lancement aussi, le premier passage d'un
    /// processus dépassait 0,3 s. « Première fois » ne se dit qu'une fois
    /// par version.
    func testTheMetalKernelsArePreparedAtEveryActivation() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        let preparation = VoicePreparation()
        let first = KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                             defaults: defaults, preparation: preparation)
        XCTAssertTrue(preparation.isRunning, "première fois pour cette version : dite")
        await first?.value
        XCTAssertEqual(compute.loads, 1)
        XCTAssertEqual(compute.warmUps, 1)
        XCTAssertEqual(compute.releases, 1, "le modèle est libéré tout de suite")
        XCTAssertFalse(preparation.isRunning)

        let again = KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                             defaults: defaults, preparation: preparation)
        XCTAssertNotNil(again, "même version : préparée quand même")
        XCTAssertFalse(preparation.isRunning, "mais sans « première fois »")
        await again?.value
        XCTAssertEqual(compute.warmUps, 2)
        XCTAssertEqual(compute.releases, 2)
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "2",
                                       defaults: defaults, preparation: preparation)?.value
        XCTAssertEqual(compute.warmUps, 3)
    }

    func testAFailedPreparationIsTriedAgainAtTheNextLaunch() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        compute.failLoad = true
        let preparation = VoicePreparation()
        await KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                       defaults: defaults, preparation: preparation)?.value
        XCTAssertNil(defaults.string(forKey: KokoroWarmUp.versionKey))
        let retry = KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                             defaults: defaults, preparation: preparation)
        XCTAssertNotNil(retry)
        await retry?.value
        XCTAssertFalse(preparation.isRunning)
    }

    /// L'onglet Voix dit « Voix en préparation (première fois)… » pendant
    /// la préparation : l'état s'observe, et une seconde demande pendant
    /// qu'elle tourne ne relance rien.
    func testThePreparationIsObservableWhileItRuns() async {
        let defaults = freshDefaults()
        let compute = FakeKokoroCompute()
        let preparation = VoicePreparation()
        nonisolated(unsafe) var seen: [Bool] = []
        withObservationTracking { _ = preparation.isRunning } onChange: { seen.append(true) }
        let task = KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                            defaults: defaults, preparation: preparation)
        XCTAssertTrue(preparation.isRunning)
        XCTAssertEqual(seen, [true], "le changement est signalé aux vues")
        XCTAssertNil(KokoroWarmUp.runIfNeeded(compute: compute, modelDirectory: "/m/kokoro", version: "1",
                                              defaults: defaults, preparation: preparation))
        await task?.value
        XCTAssertFalse(preparation.isRunning)
        XCTAssertEqual(compute.warmUps, 1)
    }

    func testOnlyTheMLXPackageIsPrepared() {
        var settings = SettingsView.defaults
        settings.family = "kokoro"
        settings.modelPath = "/m/kokoro/kokoro-v1_0.safetensors"
        XCTAssertEqual(KokoroWarmUp.modelDirectory(settings: settings), "/m/kokoro")
        settings.modelPath = "/m/kokoro/model_quantized.onnx"
        XCTAssertNil(KokoroWarmUp.modelDirectory(settings: settings))
        settings.family = ""
        settings.modelPath = ""
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
