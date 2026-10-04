import XCTest
@testable import Sillon

/// La vue de lecture, hors SwiftUI : ce qu'elle atténue, souligne et
/// surligne, sans jamais toucher au texte lui-même.
@MainActor
final class ReadingTextViewTests: XCTestCase {
    private var window: NSWindow!
    private var view: ReadingTextView!

    override func setUp() async throws {
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 500, height: 300))
        view = ReadingTextView()
        view.frame = NSRect(origin: .zero, size: scroll.contentSize)
        scroll.documentView = view
        window = NSWindow(contentRect: scroll.frame, styleMask: [.borderless],
                          backing: .buffered, defer: false)
        window.contentView = scroll
        window.orderFront(nil)
        view.setDocument(NSAttributedString(
            string: "Un deux trois. Quatre cinq six.",
            attributes: [.foregroundColor: Theme.paper.text,
                         .font: NSFont.systemFont(ofSize: 20)]))
    }

    override func tearDown() async throws {
        window.orderOut(nil)
        window = nil
        view = nil
    }

    /// L'attribut de rendu posé à `location`, ou nil.
    private func rendering(_ key: NSAttributedString.Key, at location: Int) -> Any? {
        let tlm = view.textLayoutManager!
        let tcm = tlm.textContentManager!
        let start = tcm.location(tcm.documentRange.location, offsetBy: location)!
        var value: Any?
        tlm.enumerateRenderingAttributes(from: start, reverse: false) { _, attrs, range in
            if range.contains(start) { value = attrs[key] }
            return false
        }
        return value
    }

    func testPastIsDimmedWithoutTouchingTheText() {
        let second = NSRange(location: 15, length: 16)
        view.show(word: NSRange(location: 15, length: 6), sentence: second,
                  pastUpTo: second.location, follow: false)

        XCTAssertEqual(rendering(.foregroundColor, at: 0) as? NSColor, Theme.paper.past)
        XCTAssertEqual(rendering(.foregroundColor, at: 15) as? NSColor, Theme.paper.wordText)
        XCTAssertNil(rendering(.foregroundColor, at: 23))
        XCTAssertNotNil(rendering(.underlineStyle, at: 23))
        XCTAssertNil(rendering(.underlineStyle, at: 3))
        // Le texte garde sa couleur : seul le rendu change.
        XCTAssertEqual(view.textStorage!.attribute(.foregroundColor, at: 0, effectiveRange: nil)
                       as? NSColor, Theme.paper.text)
    }

    func testNewDocumentStartsClean() {
        view.show(word: NSRange(location: 15, length: 6), sentence: NSRange(location: 15, length: 16),
                  pastUpTo: 15, follow: false)
        view.setDocument(NSAttributedString(string: "Autre texte, tout neuf."))

        XCTAssertNil(rendering(.foregroundColor, at: 0))
        XCTAssertNil(rendering(.underlineStyle, at: 16))
        XCTAssertNil(view.shown)
    }

    func testHighlightGlidesToTheNextWord() {
        view.show(word: NSRange(location: 0, length: 2), sentence: NSRange(location: 0, length: 14),
                  pastUpTo: 0, follow: false)
        let first = try! XCTUnwrap(view.shown)
        view.show(word: NSRange(location: 3, length: 4), sentence: NSRange(location: 0, length: 14),
                  pastUpTo: 0, follow: false)

        // Il part de « Un » sans sauter…
        XCTAssertEqual(view.shown, first)
        // … et le glissement dure 80 ms : à 150 ms, il est posé sur « deux »
        // et n'en bouge plus.
        RunLoop.main.run(until: Date().addingTimeInterval(0.15))
        let settled = try! XCTUnwrap(view.shown)
        XCTAssertGreaterThan(settled.minX, first.maxX)
        RunLoop.main.run(until: Date().addingTimeInterval(0.15))
        XCTAssertEqual(view.shown, settled)
    }
}
