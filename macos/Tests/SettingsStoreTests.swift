import XCTest
@testable import Sillon

@MainActor
final class SettingsStoreTests: XCTestCase {
    /// Un moteur à portes : libéré en fin de test quoi qu'il arrive, et test borné.
    private func gated() -> GatedSettingsBackend {
        executionTimeAllowance = 20
        let backend = GatedSettingsBackend()
        addTeardownBlock { @MainActor in backend.releaseAll() }
        return backend
    }

    private func loadedStore(_ backend: FakeSettingsBackend,
                             delay: Duration = .milliseconds(20)) async -> SettingsStore {
        let store = SettingsStore(backend: backend, delay: delay)
        await store.load()
        return store
    }

    func testOnlyTheTouchedFieldIsSent() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.set(\.speed, 1.4)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        var expected = SettingsPatch.empty
        expected.speed = 1.4
        XCTAssertEqual(backend.patches, [expected])
    }

    /// Point d'attention 2 : une valeur écrite à la main ne repart jamais
    /// tant qu'on n'y touche pas.
    func testOpeningTheTabRewritesNothing() async throws {
        let backend = FakeSettingsBackend()
        backend.stored.codeBlocks = "lire"
        backend.stored.language = "ja"
        backend.stored.keepVoiceMinutes = 7
        let store = await loadedStore(backend)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
        XCTAssertEqual(store.values.codeBlocks, "lire")
        store.set(\.speed, 1.2)
        _ = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertNil(backend.patches.first?.codeBlocks)
        XCTAssertNil(backend.patches.first?.language)
        XCTAssertNil(backend.patches.first?.keepVoiceMinutes)
    }

    func testNothingIsSentBeforeLoading() async throws {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10))
        store.set(\.speed, 2)
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertTrue(backend.patches.isEmpty)
    }

    /// Supertonic → MagpieTTS : température apparaît, la hauteur reste ignorée.
    func testIgnoredSettingsFollowTheActiveModel() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        XCTAssertFalse(store.isShown(.temperature))
        XCTAssertTrue(store.isIgnored(.pitch))
        store.showIgnored = true
        XCTAssertTrue(store.isShown(.temperature), "montré sur demande")
        store.showIgnored = false
        backend.capabilities = Capabilities.magpie
        await store.reloadCapabilities()
        XCTAssertTrue(store.isShown(.temperature))
        XCTAssertTrue(store.isShown(.topK))
        XCTAssertTrue(store.isShown(.guidance))
    }

    func testTheValueShownIsTheOneApplied() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.set(\.lineSpacing, 4)
        let clamped = await eventuallyTrue { store.values.lineSpacing == 3 }
        XCTAssertTrue(clamped, "ramené dans les bornes et affiché tel qu'appliqué")
    }

    /// Point d'attention 5 : la voix choisie part avant l'aperçu.
    func testListeningSendsThePendingChangeFirst() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend, delay: .seconds(30))
        store.set(\.voice, "F2")
        await store.listen()
        XCTAssertEqual(backend.log, ["patch", "écoute"])
        XCTAssertEqual(backend.patches.last?.voice, "F2")
    }

    func testAFailedWriteKeepsTheValueAndSaysWhy() async {
        let backend = FakeSettingsBackend()
        backend.failNext = "écriture de config.toml impossible"
        let store = await loadedStore(backend)
        store.set(\.speed, 1.4)
        let said = await eventuallyTrue { store.error != nil }
        XCTAssertTrue(said)
        XCTAssertEqual(store.error, "écriture de config.toml impossible")
        XCTAssertEqual(store.values.speed, 1.4, "gardé à l'écran")
    }

    func testNothingLeavesWhileASliderIsDragged() async throws {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.dragChanged(true)
        store.set(\.speed, 1.1)
        store.set(\.speed, 1.3)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
        store.dragChanged(false)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        XCTAssertEqual(backend.patches.first?.speed, 1.3)
    }

    /// Même règle pour chaque curseur qui resynthétise : une seule écriture au relâchement.
    func testEverySamplingSliderWritesOnceOnRelease() async throws {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.dragChanged(true)
        store.set(\.pitch, 0.5)
        store.set(\.energy, 1.5)
        store.set(\.temperature, 0.6)
        store.set(\.pitch, 0.7)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
        store.dragChanged(false)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        XCTAssertEqual(backend.patches.first?.pitch, 0.7)
        XCTAssertEqual(backend.patches.first?.energy, 1.5)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(backend.patches.count, 1)
    }

    func testReleasingWithoutChangeWritesNothing() async throws {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.dragChanged(true)
        store.dragChanged(false)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
    }

    /// P4 : `reload` relit réglages et capacités, sans rien écrire.
    func testReloadRereadsSettingsAndCapabilities() async throws {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        backend.stored.voice = "Aria"
        backend.capabilities = Capabilities.magpie
        await store.reload()
        XCTAssertEqual(store.values.voice, "Aria")
        XCTAssertEqual(store.capabilities.family, "magpie_tts")
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(backend.patches.isEmpty)
    }

    /// `reload` n'avale pas un changement qui attend encore sa pause.
    func testReloadKeepsAPendingChange() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend, delay: .seconds(30))
        store.set(\.speed, 1.6)
        await store.reload()
        XCTAssertEqual(backend.patches.last?.speed, 1.6)
        XCTAssertEqual(store.values.speed, 1.6)
    }

    /// Course : un changement fait pendant la relecture survit à l'écran, et
    /// seul ce champ part, par-dessus la config relue.
    func testChangeDuringReloadSurvivesAndWritesOnlyThatField() async throws {
        let backend = gated()
        let store = SettingsStore(backend: backend, delay: .milliseconds(20))
        await store.load()
        backend.stored.voice = "Aria"        // changé ailleurs (nouveau modèle)
        backend.stored.language = "de"
        backend.gateLoads = true
        let reloading = Task { await store.reload() }
        guard await eventuallyTrue(.seconds(5), { backend.waitingLoads == 1 }) else { return XCTFail("précondition jamais atteinte") }
        store.set(\.speed, 1.7)
        backend.releaseLoad()
        await reloading.value
        XCTAssertEqual(store.values.speed, 1.7, "le changement reste à l'écran")
        XCTAssertEqual(store.values.voice, "Aria")
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        var expected = SettingsPatch.empty
        expected.speed = 1.7
        XCTAssertEqual(backend.patches, [expected], "pas d'ancienne voix ni langue réécrites")
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(backend.patches.count, 1)
    }

    /// Deux relectures qui se chevauchent : seule la dernière s'applique.
    func testOnlyTheLatestOverlappingReloadApplies() async {
        let backend = gated()
        let store = SettingsStore(backend: backend, delay: .milliseconds(20))
        await store.load()
        backend.gateLoads = true
        backend.stored.voice = "ancienne"
        let first = Task { await store.reload() }
        guard await eventuallyTrue(.seconds(5), { backend.waitingLoads == 1 }) else { return XCTFail("précondition jamais atteinte") }
        backend.stored.voice = "récente"
        let second = Task { await store.reload() }
        guard await eventuallyTrue(.seconds(5), { backend.waitingLoads == 2 }) else { return XCTFail("précondition jamais atteinte") }
        backend.releaseLoad()    // la première rend « ancienne » (lue avant)
        await first.value
        XCTAssertNotEqual(store.values.voice, "ancienne")
        backend.releaseLoad()
        await second.value
        XCTAssertEqual(store.values.voice, "récente")
    }

    /// Une relecture pendant un glissé remet le glissé à zéro.
    func testReloadResetsTheDragState() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.dragChanged(true)
        await store.reload()
        store.set(\.speed, 1.5)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent, "plus de glissé en cours : le changement part")
    }

    /// `flush` public : ce qui attend sa pause part tout de suite.
    func testStoreFlushWritesAtOnce() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend, delay: .seconds(30))
        store.set(\.speed, 1.9)
        await store.flush()
        XCTAssertEqual(backend.patches.count, 1)
    }

    /// Un `flush` pendant une écriture qui porte déjà la valeur n'en ajoute pas.
    func testFlushDuringTheSameWriteAddsNothing() async {
        let backend = gated()
        backend.gateWrites = true
        let store = SettingsStore(backend: backend, delay: .zero)
        await store.load()
        store.set(\.speed, 2.2)
        guard await eventuallyTrue(.seconds(5), { backend.waitingWrites == 1 }) else { return XCTFail("précondition jamais atteinte") }
        let flushing = Task { await store.flush() }
        try? await Task.sleep(for: .milliseconds(50))
        backend.releaseWrite()
        await flushing.value
        XCTAssertEqual(backend.patches.count, 1)
    }

    /// Trois changements pendant une écriture en vol : écritures en série, la dernière gagne.
    func testWritesAreSerialisedAndTheLastWins() async throws {
        let backend = gated()
        backend.gateWrites = true
        let store = SettingsStore(backend: backend, delay: .milliseconds(5))
        await store.load()
        store.set(\.speed, 1.1)
        guard await eventuallyTrue(.seconds(5), { backend.waitingWrites == 1 }) else { return XCTFail("précondition jamais atteinte") }
        store.set(\.speed, 1.2)
        try await Task.sleep(for: .milliseconds(30))
        store.set(\.speed, 1.3)
        try await Task.sleep(for: .milliseconds(30))
        store.set(\.speed, 1.4)
        try await Task.sleep(for: .milliseconds(30))
        XCTAssertEqual(backend.patches.count, 1, "rien ne part tant que la première n'est pas finie")
        backend.releaseWrite()
        guard await eventuallyTrue(.seconds(5), { backend.waitingWrites == 1 }) else { return XCTFail("précondition jamais atteinte") }
        backend.releaseWrite()
        guard await eventuallyTrue(.seconds(5), { backend.patches.count == 2 && backend.running == 0 }) else { return XCTFail("précondition jamais atteinte") }
        try await Task.sleep(for: .milliseconds(60))
        XCTAssertEqual(backend.patches.count, 2)
        XCTAssertEqual(backend.patches.last?.speed, 1.4)
        XCTAssertEqual(backend.maxRunning, 1, "jamais deux écritures en même temps")
        XCTAssertEqual(backend.stored.speed, 1.4)
    }

    /// Vitesse et volume changés depuis le panneau (touches + - 1-4, ⌥↑↓) :
    /// l'onglet Voix les reprend sans rien écrire, et revenir à l'ancienne
    /// valeur écrit quand même.
    func testSpeedAndVolumeChangedFromThePanelAreAdopted() async throws {
        let backend = FakeSettingsBackend()
        backend.stored.speed = 1.0
        backend.stored.volume = 1.0
        let store = await loadedStore(backend)
        // Le panneau a écrit : le moteur a déjà la nouvelle valeur.
        backend.stored.speed = 1.5
        backend.stored.volume = 0.8
        store.followReader(speed: 1.5, volume: 0.8)
        XCTAssertEqual(store.values.speed, 1.5)
        XCTAssertEqual(store.values.volume, 0.8)
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertTrue(backend.patches.isEmpty, "reprendre n'écrit rien")
        store.set(\.speed, 1.0)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent, "revenir à l'ancienne valeur écrit")
        XCTAssertEqual(backend.patches.first?.speed, 1.0)
        XCTAssertNil(backend.patches.first?.volume)
    }

    /// Un changement qui attend sa pause, ou un geste en cours, l'emporte sur
    /// ce que dit le panneau.
    func testThePanelDoesNotOverrideAnEditInProgress() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend, delay: .seconds(30))
        store.set(\.speed, 2.0)
        store.followReader(speed: 1.5, volume: store.values.volume)
        XCTAssertEqual(store.values.speed, 2.0, "écriture en attente")

        let other = await loadedStore(FakeSettingsBackend())
        other.dragChanged(true)
        other.set(\.speed, 2.5)
        other.followReader(speed: 1.5, volume: other.values.volume)
        XCTAssertEqual(other.values.speed, 2.5, "geste en cours")
        other.endDragging()
    }

    func testAVoicePerLanguageIsWrittenAlone() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.setVoice("af_sky", language: "en")
        await store.flush()
        XCTAssertEqual(backend.patches.last?.voiceForLanguage, LanguageVoice(language: "en", voice: "af_sky"))
        XCTAssertNil(backend.patches.last?.voice, "la voix des autres modèles ne bouge pas")
        XCTAssertEqual(store.values.voicesByLanguage.first(where: { $0.language == "en" })?.voice, "af_sky")
    }

    func testListeningInALanguageWritesFirstThenSpeaksIt() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        store.setVoice("ff_siwis", language: "fr")
        await store.listen(language: "fr")
        XCTAssertEqual(backend.log, ["patch", "écoute fr"])
    }

    func testARefusedListeningInALanguageIsSaid() async {
        let backend = FakeSettingsBackend()
        backend.failPreview = "sortie audio indisponible"
        let store = await loadedStore(backend)
        await store.listen(language: "en")
        XCTAssertEqual(store.previewNote, "sortie audio indisponible")
    }
}
