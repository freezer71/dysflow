import XCTest
@testable import Sillon

final class AdvancedTabTests: XCTestCase {
    func testEnginesAndComputeAreNamedAndKeepAHandWrittenValue() {
        XCTAssertEqual(AdvancedTab.engineOptions(current: "auto").map(\.title),
                       ["Automatique", "audio.cpp", "Voix du système"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "metal").map(\.value), ["metal", "cpu"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "cuda").map(\.value), ["metal", "cpu", "cuda"])
    }

    func testZeroSessionsMeansTheModelDecides() {
        XCTAssertEqual(AdvancedTab.sessionsText(0), "Sessions : automatique")
        XCTAssertEqual(AdvancedTab.sessionsText(3), "Sessions : 3")
    }

    func testAnOptionIsAddedReplacedOrRefused() {
        let one = ExtraOptions.adding(key: " cfg_scale ", value: " 3 ", to: [])
        XCTAssertEqual(one, [ExtraOption(key: "cfg_scale", value: "3")])
        let replaced = ExtraOptions.adding(key: "cfg_scale", value: "4", to: one ?? [])
        XCTAssertEqual(replaced, [ExtraOption(key: "cfg_scale", value: "4")])
        XCTAssertNil(ExtraOptions.adding(key: "  ", value: "1", to: []), "clé vide")
        XCTAssertNil(ExtraOptions.adding(key: "a=b", value: "1", to: []), "« = » dans la clé")
        XCTAssertEqual(ExtraOptions.removing("cfg_scale", from: replaced ?? []), [])
    }

    func testReplacingKeepsThePlaceAndNoKeyIsDuplicated() {
        let start = [ExtraOption(key: "a", value: "1"), ExtraOption(key: "b", value: "2")]
        let next = ExtraOptions.adding(key: "a", value: "9", to: start)
        XCTAssertEqual(next, [ExtraOption(key: "a", value: "9"), ExtraOption(key: "b", value: "2")])
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
}
