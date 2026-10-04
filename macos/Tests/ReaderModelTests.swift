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
    private func hiddenModel() -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
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

    func testPlayingTicksWhileHidden() {
        let m = hiddenModel()
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.playing })
        XCTAssertTrue(m.isTicking)
    }
}
