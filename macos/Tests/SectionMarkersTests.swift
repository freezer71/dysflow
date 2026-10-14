import XCTest
@testable import Sillon

@MainActor
final class SectionMarkersTests: XCTestCase {
    private func doc(_ markdown: String) -> DocView {
        let r = SillonReader.silent()
        r.openText(markdown: markdown, source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return r.document()
    }

    func testOneMarkerPerSectionAtItsFirstSentence() {
        let d = doc("# A\n\nUn. Deux.\n\n# B\n\nTrois.")
        let m = SectionMarkers.positions(d)
        XCTAssertEqual(m.count, 2)
        XCTAssertEqual(m[0], 0, accuracy: 0.001)
        XCTAssertEqual(m[1], 3.0 / 5.0, accuracy: 0.001)
    }

    func testBreadcrumbJoinsParentAndChild() {
        let d = doc("# Monades\n\n## Exemples\n\nTexte.")
        let child = d.sections.first { $0.title == "Exemples" }!
        XCTAssertEqual(SectionMarkers.breadcrumb(d, section: child.id), "Monades ▸ Exemples")
    }

    /// La section implicite du début n'a pas de phrase tant qu'aucune prose
    /// ne précède le premier titre : le sommaire ne la montre pas.
    func testTableOfContentsSkipsEmptyIntroduction() {
        let d = doc("# Monades\n\n## Exemples\n\nTexte.")
        XCTAssertEqual(SectionMarkers.tableOfContents(d).map(\.title), ["Monades", "Exemples"])
    }
}
