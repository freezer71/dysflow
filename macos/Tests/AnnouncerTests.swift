import XCTest
@testable import Sillage

@MainActor
final class AnnouncerTests: XCTestCase {
    private func setup() -> (SillageReader, DocView) {
        let r = SillageReader.silent()
        r.openText(markdown: "# A\n\nUn. Deux.\n\n# B\n\nTrois.", source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return (r, r.document())
    }

    func testPlayPauseAndSectionAreAnnouncedOnce() {
        let (r, doc) = setup()
        var a = Announcer()
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc), "état initial : rien")

        r.command(command: .togglePlay)
        XCTAssertTrue(waitUntil { r.snapshot().playing })
        XCTAssertEqual(a.announcement(for: r.snapshot(), doc: doc), "Lecture")
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc), "pas deux fois")

        r.command(command: .sections(delta: 1))
        // Les titres sont des phrases : « A » 0, « Un. » 1, « Deux. » 2, « B » 3.
        XCTAssertTrue(waitUntil { r.snapshot().sentence == 3 })
        XCTAssertEqual(a.announcement(for: r.snapshot(), doc: doc), "Section : B")
    }

    func testWordChangesAreNeverAnnounced() {
        let (r, doc) = setup()
        var a = Announcer()
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.command(command: .words(delta: 1))
        XCTAssertTrue(waitUntil { r.snapshot().word != nil })
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: doc))
    }

    func testPauseIsAnnounced() {
        let (r, doc) = setup()
        var a = Announcer()
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.command(command: .togglePlay)
        XCTAssertTrue(waitUntil { r.snapshot().playing })
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.command(command: .togglePlay)
        XCTAssertTrue(waitUntil { !r.snapshot().playing })
        XCTAssertEqual(a.announcement(for: r.snapshot(), doc: doc), "Pause")
    }

    func testNewDocumentReseedsInsteadOfAnnouncingSection() {
        let (r, doc) = setup()
        var a = Announcer()
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.command(command: .sections(delta: 1))
        XCTAssertTrue(waitUntil { r.snapshot().sentence == 3 })
        _ = a.announcement(for: r.snapshot(), doc: doc)
        r.openText(markdown: "# C\n\nUn.\n\n# D\n\nDeux.", source: "u")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 2 })
        XCTAssertNil(a.announcement(for: r.snapshot(), doc: r.document()))
    }

    func testSectionBecomingNilAnnouncesNothing() {
        var s = SillageReader.silent().snapshot()
        var a = Announcer()
        let doc = DocView(title: "", complete: true, sentenceCount: 0, sections: [], blocks: [])
        s.section = 1
        _ = a.announcement(for: s, doc: doc)
        s.section = nil
        XCTAssertNil(a.announcement(for: s, doc: doc))
    }

    func testPlayAndSectionTogetherAnnouncesPlayOnly() {
        var s = SillageReader.silent().snapshot()
        var a = Announcer()
        let doc = DocView(title: "", complete: true, sentenceCount: 0,
                          sections: [SectionView(id: 2, level: 1, title: "B", firstSentence: 3)], blocks: [])
        s.playing = false; s.section = 1
        _ = a.announcement(for: s, doc: doc)
        s.playing = true; s.section = 2
        XCTAssertEqual(a.announcement(for: s, doc: doc), "Lecture")
        XCTAssertNil(a.announcement(for: s, doc: doc))
    }
}
