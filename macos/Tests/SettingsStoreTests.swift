import XCTest
@testable import Sillon

@MainActor
final class SettingsStoreTests: XCTestCase {
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

    /// Supertonic → MagpieTTS : température apparaît, la vitesse disparaît.
    func testIgnoredSettingsFollowTheActiveModel() async {
        let backend = FakeSettingsBackend()
        let store = await loadedStore(backend)
        XCTAssertTrue(store.isShown(.speed))
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
        XCTAssertFalse(store.isShown(.speed))
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
}
