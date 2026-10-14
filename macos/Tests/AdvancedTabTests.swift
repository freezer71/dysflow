import XCTest
@testable import Sillon

final class AdvancedTabTests: XCTestCase {
    func testEnginesAreNamedAndKeepAHandWrittenValue() {
        XCTAssertEqual(AdvancedTab.engineOptions(current: "auto").map(\.title),
                       ["Automatique", "Voix du système"])
        XCTAssertEqual(AdvancedTab.engineOptions(current: "maison").map(\.value), ["auto", "system", "maison"])
    }

    func testZeroSessionsMeansTheModelDecides() {
        XCTAssertEqual(AdvancedTab.sessionsText(0), "Sessions : automatique")
        XCTAssertEqual(AdvancedTab.sessionsText(3), "Sessions : 3")
    }
}

@MainActor
final class AdvancedStoreSafetyTests: XCTestCase {
    func testABurstOfThreadPressesIsOneWriteAfterTheEngineDelay() async throws {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10), engineDelay: .milliseconds(300))
        await store.load()
        for n in UInt32(5)...UInt32(9) {
            store.set(\.threads, n, reloadsVoice: true)
            try await Task.sleep(for: .milliseconds(40))
        }
        XCTAssertTrue(backend.patches.isEmpty, "pas d'écriture pendant la rafale")
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        try await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(backend.patches.count, 1)
        XCTAssertEqual(backend.patches.first?.threads, 9)
    }

    func testAStuckDragIsClearedAfterTheTimeout() async {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10), dragTimeout: .milliseconds(80))
        await store.load()
        store.dragChanged(true)
        store.set(\.speed, 1.5)
        XCTAssertTrue(store.isDragging)
        let cleared = await eventuallyTrue { !store.isDragging }
        XCTAssertTrue(cleared)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent, "la valeur posée pendant le geste part à l'abandon")
    }

    func testLeavingTheTabClearsADrag() async {
        let store = SettingsStore(backend: FakeSettingsBackend(), delay: .milliseconds(10))
        await store.load()
        store.dragChanged(true)
        store.endDragging()
        XCTAssertFalse(store.isDragging)
    }

    func testSliderValuesAreRoundedToTwoDecimals() {
        XCTAssertEqual(SettingFormat.rounded(0.35000002), 0.35)
    }

    func testAnOrdinaryEditDoesNotShortenTheEnginePause() async throws {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(20), engineDelay: .milliseconds(400))
        await store.load()
        store.set(\.threads, 6, reloadsVoice: true)
        try await Task.sleep(for: .milliseconds(100))
        store.set(\.lineSpacing, 2)
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertTrue(backend.patches.isEmpty, "pas de rechargement en pleine rafale")
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
        XCTAssertEqual(backend.patches.first?.threads, 6)
        XCTAssertEqual(backend.patches.first?.lineSpacing, 2)
    }

    func testFlushStillSendsAtOnceDuringTheEnginePause() async {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(20), engineDelay: .seconds(30))
        await store.load()
        store.set(\.threads, 6, reloadsVoice: true)
        await store.flush()
        XCTAssertEqual(backend.patches.count, 1)
    }

    func testASlowDragIsNotCutShortWhileItMoves() async throws {
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10), dragTimeout: .milliseconds(150))
        await store.load()
        store.dragChanged(true)
        for n in 0..<12 {
            store.set(\.speed, 1 + Float(n) * 0.05)
            try await Task.sleep(for: .milliseconds(40))
        }
        XCTAssertTrue(store.isDragging, "le geste dure plus que le délai, mais bouge")
        XCTAssertTrue(backend.patches.isEmpty)
        store.dragChanged(false)
        let sent = await eventuallyTrue { backend.patches.count == 1 }
        XCTAssertTrue(sent)
    }
}
