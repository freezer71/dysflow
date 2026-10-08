import XCTest
@testable import Sillon

/// ↑ ↓ d'une ligne affichée à l'autre : le mot visé est le premier de la
/// ligne voisine, telle que TextKit 2 l'a vraiment coupée.
@MainActor
final class LineNavigationTests: XCTestCase {
    private var window: NSWindow!
    private var view: ReadingTextView!
    private var layout: DocumentLayout!
    private var navigator: LineNavigator!

    private let style = ReadingStyle(font: .literata, size: 20, lineSpacing: 1,
                                     ruler: true, theme: .paper)

    override func tearDown() async throws {
        window?.orderOut(nil)
        window = nil
        view = nil
        layout = nil
        navigator = nil
    }

    /// Met le texte en page dans une vue étroite, qui force les retours à la ligne.
    private func lay(_ markdown: String, width: CGFloat = 220) {
        let r = SillonReader.silent()
        r.openText(markdown: markdown, source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        layout = DocumentLayout.build(r.document(), style: style)
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: width, height: 300))
        view = ReadingTextView()
        view.frames = ManualFrames()
        view.frame = NSRect(origin: .zero, size: scroll.contentSize)
        scroll.documentView = view
        window = NSWindow(contentRect: scroll.frame, styleMask: [.borderless],
                          backing: .buffered, defer: false)
        window.contentView = scroll
        view.setDocument(layout.text)
        navigator = LineNavigator()
        navigator.attach(view, layout: layout)
    }

    // MARK: - Ce que l'œil voit, mesuré mot par mot

    /// Le haut de la ligne de chaque mot, d'après son cadre à l'écran.
    private func top(_ w: WordPlace) -> CGFloat {
        view.rectForTesting(w.range)!.minY
    }

    /// Les lignes affichées qui portent un mot, de haut en bas : leur premier mot.
    private var lineStarts: [WordPlace] {
        var starts: [WordPlace] = []
        for w in layout.words where starts.last.map({ top(w) > top($0) + 1 }) ?? true {
            starts.append(w)
        }
        return starts
    }

    private func text(_ w: WordPlace?) -> String? {
        w.map { (layout.text.string as NSString).substring(with: $0.range) }
    }

    private func move(from w: WordPlace, _ delta: Int) -> WordPlace? {
        navigator.target(word: w.index, sentence: w.sentence, delta: delta)
    }

    // MARK: - Le mot de début de ligne, sans mise en page

    func testTheLineStartsAtItsFirstWord() {
        let words = [WordPlace(range: NSRange(location: 2, length: 3), sentence: 0, word: 0, index: 0),
                     WordPlace(range: NSRange(location: 6, length: 4), sentence: 0, word: 1, index: 1)]
        // « • » puis le premier mot : la puce n'est pas un mot.
        XCTAssertEqual(LineNavigator.word(on: NSRange(location: 0, length: 11), in: words), words[0])
        XCTAssertEqual(LineNavigator.word(on: NSRange(location: 5, length: 6), in: words), words[1])
    }

    func testALineThatBeginsMidWordTakesThatWord() {
        let words = [WordPlace(range: NSRange(location: 0, length: 30), sentence: 3, word: 2, index: 9)]
        XCTAssertEqual(LineNavigator.word(on: NSRange(location: 12, length: 18), in: words), words[0])
    }

    func testALineWithoutWordGivesNothing() {
        let words = [WordPlace(range: NSRange(location: 0, length: 3), sentence: 0, word: 0, index: 0),
                     WordPlace(range: NSRange(location: 40, length: 3), sentence: 1, word: 0, index: 1)]
        XCTAssertNil(LineNavigator.word(on: NSRange(location: 10, length: 20), in: words))
    }

    // MARK: - Dans une vraie mise en page

    func testDownAndUpWalkAParagraphWrappedOnSeveralLines() throws {
        lay("Un deux trois quatre cinq six sept huit neuf dix onze douze treize quatorze quinze.")
        let starts = lineStarts
        XCTAssertGreaterThanOrEqual(starts.count, 3, "le paragraphe doit tenir sur trois lignes")
        let first = layout.words[0]
        XCTAssertEqual(move(from: first, 1), starts[1])
        XCTAssertEqual(move(from: starts[1], 1), starts[2])
        // Depuis le milieu d'une ligne : le début de la suivante.
        let middle = try XCTUnwrap(layout.words.first { top($0) == top(starts[1]) && $0 != starts[1] })
        XCTAssertEqual(move(from: middle, 1), starts[2])
        XCTAssertEqual(move(from: middle, -1), starts[0])
        XCTAssertEqual(move(from: starts[2], -1), starts[1])
    }

    func testNothingAboveTheFirstLineNorBelowTheLast() {
        lay("Un deux trois quatre cinq six sept huit neuf dix onze douze.")
        XCTAssertNil(move(from: layout.words[0], -1))
        XCTAssertNil(move(from: layout.words.last!, 1))
    }

    func testDownGoesFromOneBulletToTheNextFirstWord() {
        lay("- Un deux trois quatre cinq six sept.\n- Huit neuf dix.\n- Onze.")
        let starts = lineStarts
        let huit = layout.words.first { text($0) == "Huit" }!
        XCTAssertEqual(text(starts.first { $0 == huit }), "Huit", "« Huit » ouvre une ligne")
        let before = starts[starts.firstIndex(of: huit)! - 1]
        XCTAssertEqual(text(move(from: before, 1)), "Huit")
        XCTAssertEqual(move(from: before, 1)?.word, 0)
        XCTAssertEqual(text(move(from: huit, 1)), "Onze.")
        XCTAssertEqual(move(from: huit, -1), before)
    }

    func testDownCrossesTheSpacingBetweenTwoParagraphs() {
        lay("Un deux trois quatre cinq six sept huit.\n\nNeuf dix onze.")
        let neuf = layout.words.first { text($0) == "Neuf" }!
        let starts = lineStarts
        let lastOfFirst = starts[starts.firstIndex(of: neuf)! - 1]
        XCTAssertEqual(move(from: lastOfFirst, 1), neuf)
        XCTAssertEqual(neuf.word, 0)
        XCTAssertEqual(move(from: neuf, -1), lastOfFirst)
    }

    func testLinesWithoutWordsAreSkipped() {
        lay("Avant.\n\n```\nfn main() {}\nlet x = 1;\n```\n\nAprès.", width: 400)
        let avant = layout.words.first { text($0) == "Avant." }!
        let apres = layout.words.first { text($0) == "Après." }!
        XCTAssertEqual(move(from: avant, 1), apres)
        XCTAssertEqual(move(from: apres, -1), avant)
    }
}
