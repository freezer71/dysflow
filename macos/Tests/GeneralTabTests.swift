import XCTest
@testable import Sillon

final class GeneralTabTests: XCTestCase {
    private func bookmark(_ document: String, _ label: String, _ sentence: UInt32) -> StoredBookmarkView {
        StoredBookmarkView(document: document, label: label, sentence: sentence,
                           sectionTitle: "Titre", snippet: "Phrase \(sentence).", when: "2026-10-06 10:00")
    }

    func testBookmarksAreGroupedByDocumentInReadingOrder() {
        let groups = BookmarkGroups.group([
            bookmark("/a/cours.md", "cours.md", 9),
            bookmark("sélection:01", "sélection", 2),
            bookmark("/a/cours.md", "cours.md", 3),
        ])
        XCTAssertEqual(groups.map(\.label), ["cours.md", "sélection"])
        XCTAssertEqual(groups[0].items.map(\.sentence), [3, 9])
    }

    /// Deux fichiers de même nom, deux dossiers : deux groupes.
    func testTwoDocumentsWithTheSameNameStayApart() {
        let groups = BookmarkGroups.group([
            bookmark("/a/notes.md", "notes.md", 1),
            bookmark("/b/notes.md", "notes.md", 1),
        ])
        XCTAssertEqual(groups.count, 2)
    }

    func testTheCacheReportSaysWhatWasFreed() {
        XCTAssertEqual(CacheReport.text(freed: 0), "Le cache était déjà vide")
        XCTAssertTrue(CacheReport.text(freed: 12_000_000).hasSuffix("libérés"))
    }
}
