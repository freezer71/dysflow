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

    // MARK: - Bloc annoncé, largeur qui change, défilement à la main

    private func longText(_ words: Int) -> NSAttributedString {
        NSAttributedString(
            string: (1...words).map { "mot\($0)" }.joined(separator: " "),
            attributes: [.foregroundColor: Theme.paper.text, .font: NSFont.systemFont(ofSize: 20)])
    }

    private func range(of needle: String) -> NSRange {
        (view.textStorage!.string as NSString).range(of: needle)
    }

    /// Pendant qu'un bloc de code s'annonce, le déjà-lu reste atténué et la
    /// bande se pose sur le bloc ; sans rien à montrer, elle disparaît.
    func testAnnouncedBlockKeepsPastDimmedAndBandsTheBlock() {
        let r = SillonReader.silent()
        r.openText(markdown: "Avant.\n\n```rust\nfn a() {\n    b();\n}\n```\n\nAprès.", source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        let d = r.document()
        let layout = DocumentLayout.build(d, style: ReadingStyle(
            font: .literata, size: 20, lineSpacing: 1, ruler: true, theme: .paper))
        view.setDocument(layout.text)
        let avant = layout.wordRanges[d.blocks[0].sentences[0].words[0].index]!
        view.show(word: avant, sentence: layout.sentenceRanges[d.blocks[0].sentences[0].id],
                  pastUpTo: 0, follow: false)
        let wordBand = try! XCTUnwrap(view.lineBand)

        let block = layout.sentenceRanges[d.blocks[1].sentences[0].id]!
        view.show(word: nil, sentence: block, pastUpTo: block.location, follow: false)

        XCTAssertEqual(rendering(.foregroundColor, at: 0) as? NSColor, Theme.paper.past)
        XCTAssertNil(view.shown)
        let blockBand = try! XCTUnwrap(view.lineBand)
        XCTAssertGreaterThan(blockBand.height, wordBand.height * 2)
        XCTAssertGreaterThan(blockBand.minY, wordBand.minY)
        XCTAssertEqual(view.accessibilityValue(), "fn a() {\n    b();\n}")

        view.show(word: nil, sentence: nil, pastUpTo: 0, follow: false)
        XCTAssertNil(view.lineBand)
    }

    /// Le panneau s'élargit en pause : le surlignage et la bande suivent le
    /// mot là où il est maintenant, sans glisser, et la hauteur est renvoyée.
    func testWidthChangeReplacesTheHighlightAndResendsHeight() {
        var heights: [CGFloat] = []
        view.onContentHeight = { heights.append($0) }
        view.setDocument(longText(60))
        heights.removeAll()
        let word = range(of: "mot50")
        view.show(word: word, sentence: word, pastUpTo: 0, follow: false)
        let before = try! XCTUnwrap(view.shown)

        view.setFrameSize(NSSize(width: 250, height: view.frame.height))
        let after = try! XCTUnwrap(view.shown)
        XCTAssertGreaterThan(after.minY, before.minY)
        XCTAssertEqual(view.lineBand?.minY, after.minY - 2)
        XCTAssertEqual(heights.count, 1)
        // Posé d'un coup : il n'en bouge plus.
        RunLoop.main.run(until: Date().addingTimeInterval(0.15))
        XCTAssertEqual(view.shown, after)

        // Seule la hauteur change : rien n'est renvoyé, pas de boucle avec
        // le panneau qui se redimensionne.
        view.setFrameSize(NSSize(width: 250, height: view.frame.height + 40))
        XCTAssertEqual(heights.count, 1)
    }

    /// On relit en faisant défiler : la vue ne ramène plus la ligne lue,
    /// jusqu'à la prochaine commande de lecture.
    func testUserScrollStopsFollowingUntilResumed() {
        view.setDocument(longText(400))
        let clip = view.enclosingScrollView!.contentView
        view.show(word: range(of: "mot1 "), sentence: nil, pastUpTo: 0, follow: true)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        XCTAssertEqual(clip.bounds.origin.y, 0)

        view.userDidScroll()
        view.show(word: range(of: "mot300"), sentence: nil, pastUpTo: 0, follow: true)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        XCTAssertEqual(clip.bounds.origin.y, 0)

        view.resumeFollowing()
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        XCTAssertGreaterThan(clip.bounds.origin.y, 0)
    }
}
