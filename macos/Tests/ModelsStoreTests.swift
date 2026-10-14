import XCTest
@testable import Sillon

@MainActor
final class ModelsStoreTests: XCTestCase {
    func testTheActiveModelIsRefusedWithoutAnAlert() {
        let store = ModelsStore(backend: FakeModelsBackend())
        let active = VoiceModel.sample(id: "a", name: "Kokoro 82M · MLX", active: true)
        guard case .refuse(let reason) = ModelsStore.deletionRequest(for: active) else {
            return XCTFail("le modèle actif ne se supprime pas")
        }
        XCTAssertTrue(reason.contains("modèle actif"))
        store.askDeletion(active)
        XCTAssertNil(store.pendingDeletion, "aucune alerte")
        XCTAssertEqual(store.error, reason)
    }

    func testAnInstalledModelAsksForConfirmation() {
        let store = ModelsStore(backend: FakeModelsBackend())
        let model = VoiceModel.sample(id: "b", name: "Kokoro 82M · int8", bytes: 108_000_000)
        guard case .confirm(let title, let message) = ModelsStore.deletionRequest(for: model) else {
            return XCTFail("confirmation attendue")
        }
        XCTAssertEqual(title, "Supprimer Kokoro 82M · int8 ?")
        XCTAssertTrue(message.hasPrefix("108 Mo seront libérés"), message)
        store.askDeletion(model)
        XCTAssertEqual(store.pendingDeletion?.id, "b")
    }

    func testAConfirmedDeletionRemovesAndRereadsTheCatalogue() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "b")]
        let store = ModelsStore(backend: backend)
        await store.reload()
        store.askDeletion(backend.catalogue[0])
        await store.delete(backend.catalogue[0])
        XCTAssertEqual(backend.removed, ["b"])
        XCTAssertTrue(store.catalogue.isEmpty)
        XCTAssertNil(store.pendingDeletion)
        XCTAssertEqual(backend.loads, 2)
    }

    func testAFailedDeletionSaysWhyAndRereadsTheCatalogue() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "b")]
        backend.failRemoval = "suppression de model_quantized.onnx : permission refusée"
        let store = ModelsStore(backend: backend)
        await store.reload()
        await store.delete(backend.catalogue[0])
        XCTAssertEqual(store.error, "suppression de model_quantized.onnx : permission refusée")
        XCTAssertEqual(backend.loads, 2, "catalogue relu")
        XCTAssertEqual(store.catalogue.count, 1)
    }

    func testTheFreeSpaceIsRead() async {
        let backend = FakeModelsBackend()
        let store = ModelsStore(backend: backend)
        await store.reload()
        XCTAssertEqual(store.freeSpace, 10_000_000_000)
    }

    func testUsingAModelTellsTheSettingsToReload() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a", active: true), .sample(id: "b")]
        var changes = 0
        let store = ModelsStore(backend: backend, onActiveModelChanged: { changes += 1 })
        await store.reload()
        XCTAssertEqual(changes, 0, "un simple relevé ne change rien")
        await store.use("b")
        XCTAssertEqual(backend.used, ["b"])
        XCTAssertEqual(changes, 1)
        XCTAssertEqual(store.catalogue.first(where: \.active)?.id, "b")
    }

    func testADownloadThatActivatesAModelTellsTheSettingsToReload() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a", active: true), .sample(id: "b", installed: false)]
        var changes = 0
        let store = ModelsStore(backend: backend, onActiveModelChanged: { changes += 1 })
        await store.reload()
        await store.downloadSettled()
        XCTAssertEqual(changes, 0, "le modèle actif n'a pas bougé")
        backend.finishDownloadActivating("b")
        await store.downloadSettled()
        XCTAssertEqual(changes, 1)
        XCTAssertFalse(store.cancelling)
    }

    func testARefusedDeletionIsShown() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a")]
        backend.failRemoval = "mode d'essai : rien ne s'écrit"
        let store = ModelsStore(backend: backend)
        await store.reload()
        await store.delete(backend.catalogue[0])
        XCTAssertEqual(store.error, "mode d'essai : rien ne s'écrit")
    }

    func testARefusedUseAndARefusedInstallAreShown() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a"), .sample(id: "b", installed: false)]
        backend.failUse = "un téléchargement est en cours"
        backend.failInstall = "mode d'essai : rien ne se télécharge"
        let store = ModelsStore(backend: backend)
        await store.reload()
        await store.use("a")
        XCTAssertEqual(store.error, "un téléchargement est en cours")
        await store.install("b")
        XCTAssertEqual(store.error, "mode d'essai : rien ne se télécharge")
    }

    func testARepeatedRefusalIsAnnouncedEveryTime() {
        var said: [String] = []
        let store = ModelsStore(backend: FakeModelsBackend(), announce: { said.append($0) })
        let active = VoiceModel.sample(id: "a", name: "Kokoro", active: true)
        store.askDeletion(active)
        store.askDeletion(active)
        XCTAssertEqual(said.count, 2)
    }

    func testAFailedCancelDoesNotLeaveCancellingOn() async {
        let backend = FakeModelsBackend()
        backend.failCancel = "rien à annuler"
        let store = ModelsStore(backend: backend, isDownloading: { true })
        await store.cancelDownload()
        XCTAssertFalse(store.cancelling)
        XCTAssertEqual(store.error, "rien à annuler")
    }

    func testCancellingAfterTheDownloadEndedDoesNotStick() async {
        let store = ModelsStore(backend: FakeModelsBackend(), isDownloading: { false })
        await store.cancelDownload()
        XCTAssertFalse(store.cancelling)
    }

    func testCancellingStaysWhileTheTransferStillRuns() async {
        let store = ModelsStore(backend: FakeModelsBackend(), isDownloading: { true })
        await store.cancelDownload()
        XCTAssertTrue(store.cancelling, "le transfert voit l'annulation à son prochain bloc")
    }

    func testCancellingTheAlertClearsAStaleRefusal() {
        let store = ModelsStore(backend: FakeModelsBackend())
        store.askDeletion(.sample(id: "a", active: true))
        XCTAssertNotNil(store.error)
        store.askDeletion(.sample(id: "b"))
        XCTAssertNil(store.error)
        store.cancelDeletion()
        XCTAssertNil(store.pendingDeletion)
    }

    /// P4 : la fin d'un téléchargement relit les réglages quel que soit
    /// l'onglet affiché, même si l'onglet Modèles n'a jamais été ouvert.
    func testADownloadEndingWithoutTheModelsTabStillReloadsTheSettings() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a", active: true), .sample(id: "b", installed: false)]
        let settingsBackend = FakeSettingsBackend()
        let settings = SettingsStore(backend: settingsBackend)
        await settings.load()
        let store = ModelsStore(backend: backend, onActiveModelChanged: { await settings.reload() })
        await store.downloadChanged(downloading: true)
        settingsBackend.capabilities = Capabilities.kokoro
        backend.finishDownloadActivating("b")
        await store.downloadChanged(downloading: false)
        XCTAssertEqual(settings.capabilities.family, "kokoro")
    }

    /// Seule la fin d'un téléchargement compte, et seulement si elle change
    /// le modèle actif.
    func testOnlyTheEndOfADownloadThatChangesTheModelCounts() async {
        let backend = FakeModelsBackend()
        backend.catalogue = [.sample(id: "a", active: true), .sample(id: "b", installed: false)]
        var changes = 0
        let store = ModelsStore(backend: backend, onActiveModelChanged: { changes += 1 })
        await store.downloadChanged(downloading: false)
        XCTAssertEqual(changes, 0, "aucun téléchargement n'a commencé")
        await store.downloadChanged(downloading: true)
        await store.downloadChanged(downloading: false)
        XCTAssertEqual(changes, 0, "le modèle actif n'a pas bougé")
        await store.downloadChanged(downloading: true)
        backend.finishDownloadActivating("b")
        await store.downloadChanged(downloading: false)
        XCTAssertEqual(changes, 1)
    }
}
