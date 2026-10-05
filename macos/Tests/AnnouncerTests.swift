import XCTest
@testable import Sillon

@MainActor
final class AnnouncerTests: XCTestCase {
    private func setup() -> (SillonReader, DocView) {
        let r = SillonReader.silent()
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
}
