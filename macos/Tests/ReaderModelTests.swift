import XCTest
@testable import Sillon

@MainActor
final class ReaderModelTests: XCTestCase {
    private func model(_ text: String) -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        return m
    }

    func testDocumentFollowsRevision() {
        let m = model("Un. Deux. Trois.")
        XCTAssertEqual(m.document.sentenceCount, 3)
        m.open(text: "Autre.", source: "b")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 2 })
        XCTAssertEqual(m.document.sentenceCount, 1)
    }

    func testSendRefreshesSnapshot() {
        let m = model("Un. Deux. Trois.")
        m.send(.sentences(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.sentence == 1 })
        XCTAssertEqual(m.currentSentenceText, "Deux.")
    }

    func testClearingMessageOnOpen() {
        let m = model("Un.")
        m.message = "Sélectionnez ou copiez du texte"
        m.open(text: "Deux.", source: "b")
        XCTAssertNil(m.message)
    }
}

@MainActor
final class ReaderModelTickingTests: XCTestCase {
    private var clock = Date()

    private func hiddenModel() -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent(), now: { [unowned self] in self.clock })
        m.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        // La période de grâce passée, le modèle masqué au repos s'arrête.
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        return m
    }

    func testIdleHiddenModelDoesNotTick() {
        let m = hiddenModel()
        XCTAssertFalse(m.snapshot.playing)
        XCTAssertFalse(m.isTicking)
    }

    func testDisplayedTicksThenStopsWhenHidden() {
        let m = hiddenModel()
        m.isDisplayed = true
        XCTAssertTrue(m.isTicking)
        m.isDisplayed = false
        m.refresh()
        XCTAssertFalse(m.isTicking)
    }

    func testHiddenPlayIsSeenWithoutManualRefresh() {
        let m = hiddenModel()
        m.send(.togglePlay)
        // Aucun refresh manuel : seul le suivi à l'image peut voir `playing`.
        XCTAssertTrue(waitUntil { m.snapshot.playing })
        XCTAssertTrue(m.isTicking)
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { !m.snapshot.playing })
        // Encore dans la période de grâce : le suivi continue.
        XCTAssertTrue(m.isTicking)
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        XCTAssertFalse(m.isTicking)
    }

    func testDownloadInProgressKeepsTicking() {
        let running = DownloadView(id: "v", received: 1, total: 10, failed: nil)
        let failed = DownloadView(id: "v", received: 1, total: 10, failed: "réseau")
        func tick(_ d: DownloadView?, settings: Bool = false) -> Bool {
            ReaderModel.shouldTick(displayed: false, playing: false, grace: false,
                                   download: d, settingsVisible: settings)
        }
        XCTAssertTrue(tick(running))
        XCTAssertFalse(tick(failed))
        XCTAssertFalse(tick(nil))
        XCTAssertTrue(tick(nil, settings: true))
    }

    func testSettingsVisibleKeepsHiddenModelTicking() {
        let m = hiddenModel()
        XCTAssertFalse(m.isTicking)
        m.isSettingsVisible = true
        XCTAssertTrue(m.isTicking)
        m.isSettingsVisible = false
        XCTAssertFalse(m.isTicking)
    }
}
