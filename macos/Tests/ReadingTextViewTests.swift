import XCTest
@testable import Sillage

/// La vue de lecture, hors SwiftUI : ce qu'elle atténue, encre et trace
/// sous la ligne, sans jamais toucher au texte lui-même.
@MainActor
final class ReadingTextViewTests: XCTestCase {
    private var window: NSWindow!
    private var view: ReadingTextView!
    /// Les images du trait, données à la main : un écran en veille n'en envoie pas.
    private var frames: ManualFrames!

    override func setUp() async throws {
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 500, height: 300))
        view = ReadingTextView()
        frames = ManualFrames()
        view.frames = frames
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
        frames = nil
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

    /// Le déjà-lu s'atténue mot par mot ; le mot lu prend une encre plus
    /// sombre ; rien n'est souligné, rien n'est touché dans le texte.
    func testPastIsDimmedWordByWordWithoutTouchingTheText() {
        let cinq = NSRange(location: 22, length: 4)
        view.show(word: cinq, sentence: NSRange(location: 15, length: 16),
                  pastUpTo: cinq.location, follow: false)

        XCTAssertEqual(rendering(.foregroundColor, at: 0) as? NSColor, Theme.paper.past)
        // « Quatre », dans la phrase lue, est déjà dit.
        XCTAssertEqual(rendering(.foregroundColor, at: 15) as? NSColor, Theme.paper.past)
        XCTAssertEqual(rendering(.foregroundColor, at: 22) as? NSColor, Theme.paper.wordText)
        XCTAssertNil(rendering(.foregroundColor, at: 27))
        // Plus de soulignement de phrase : le trait le remplace.
        XCTAssertNil(rendering(.underlineStyle, at: 22))
        XCTAssertNil(rendering(.underlineStyle, at: 27))
        // Le texte garde sa couleur : seul le rendu change.
        XCTAssertEqual(view.textStorage!.attribute(.foregroundColor, at: 0, effectiveRange: nil)
                       as? NSColor, Theme.paper.text)
        XCTAssertEqual(view.accessibilityValue(), "Quatre cinq six.")

        // Le mot suivant : l'ancien passe au déjà-lu.
        view.show(word: NSRange(location: 27, length: 4), sentence: NSRange(location: 15, length: 16),
                  pastUpTo: 27, follow: false)
        XCTAssertEqual(rendering(.foregroundColor, at: 22) as? NSColor, Theme.paper.past)
        XCTAssertEqual(rendering(.foregroundColor, at: 27) as? NSColor, Theme.paper.wordText)
    }

    func testNewDocumentStartsClean() {
        view.playing = true
        view.show(word: NSRange(location: 15, length: 6), sentence: NSRange(location: 15, length: 16),
                  pastUpTo: 15, follow: false)
        view.setDocument(NSAttributedString(string: "Autre texte, tout neuf."))

        XCTAssertNil(rendering(.foregroundColor, at: 0))
        XCTAssertNil(view.wordRect)
        XCTAssertNil(view.cursorLine)
        XCTAssertFalse(view.isAnimatingLine)
    }

    // MARK: - Le trait sous la ligne

    /// Pendant la lecture, le trait va du début de la ligne au point de la
    /// voix, et avance avec elle à chaque image ; en pause, il disparaît et
    /// plus rien ne tourne.
    func testTheLineFollowsTheVoiceWhilePlaying() {
        var progress: Float = 0.25
        view.reduceMotion = { false }
        view.liveProgress = { progress }
        let deux = NSRange(location: 3, length: 4)
        view.show(word: deux, sentence: NSRange(location: 0, length: 14), pastUpTo: 3, follow: false)
        XCTAssertNil(view.cursorLine, "en pause, pas de trait")
        XCTAssertFalse(view.isAnimatingLine)

        view.playing = true
        XCTAssertTrue(view.isAnimatingLine)
        let word = try! XCTUnwrap(view.wordRect)
        let un = try! XCTUnwrap(view.rectForTesting(NSRange(location: 0, length: 2)))
        frames.tick()
        let line = try! XCTUnwrap(view.cursorLine)
        XCTAssertEqual(line.maxX, word.minX + word.width * 0.25, accuracy: 0.5)
        XCTAssertEqual(line.minX, un.minX, accuracy: 0.5)

        progress = 0.75
        frames.tick()
        XCTAssertEqual(try! XCTUnwrap(view.cursorLine).maxX, word.minX + word.width * 0.75, accuracy: 0.5)

        view.playing = false
        XCTAssertNil(view.cursorLine)
        XCTAssertFalse(view.isAnimatingLine)
    }

    /// Fenêtre retirée (l'arrière-plan garde la vue dans sa fenêtre) : le
    /// trait s'arrête aussitôt ; la fenêtre remise, il reprend.
    func testTheLineStopsWhileTheWindowIsOrderedOut() {
        view.reduceMotion = { false }
        view.liveProgress = { 0.5 }
        view.show(word: NSRange(location: 3, length: 4), sentence: NSRange(location: 0, length: 14),
                  pastUpTo: 3, follow: false)
        view.playing = true
        XCTAssertTrue(view.isAnimatingLine)
        window.orderOut(nil)
        XCTAssertNotNil(view.window)
        XCTAssertFalse(view.isAnimatingLine, "lien d'affichage fenêtre retirée")
        window.orderFront(nil)
        XCTAssertTrue(view.isAnimatingLine, "le trait reprend")
    }

    /// Au premier mot d'une ligne, le trait repart de zéro : il ne traîne
    /// pas de la ligne d'avant.
    func testTheLineRestartsOnEachLine() {
        view.setDocument(longText(60))
        view.reduceMotion = { false }
        view.liveProgress = { 0 }
        view.playing = true
        // Le premier mot de la deuxième ligne.
        let first = try! XCTUnwrap(view.rectForTesting(range(of: "mot1 ")))
        let second = (1...60).map { range(of: "mot\($0) ") }.first {
            (view.rectForTesting($0)?.minY ?? 0) > first.minY
        }!
        view.show(word: second, sentence: nil, pastUpTo: second.location, follow: false)
        XCTAssertTrue(waitUntil(1) { view.cursorLine != nil })
        let wordRect = try! XCTUnwrap(view.wordRect)
        XCTAssertEqual(view.cursorLine!.minX, wordRect.minX, accuracy: 0.5)
        XCTAssertEqual(view.cursorLine!.width, 0, accuracy: 0.5)
        XCTAssertGreaterThan(view.cursorLine!.minY, first.maxY - CursorLine.lift)
    }

    /// Réduire les animations : le trait se pose au bout du mot lu, mot par
    /// mot, et aucun lien d'affichage ne tourne.
    func testReduceMotionMovesTheLineWordByWord() {
        view.reduceMotion = { true }
        view.liveProgress = { 0.1 }
        view.playing = true
        view.show(word: NSRange(location: 3, length: 4), sentence: NSRange(location: 0, length: 14),
                  pastUpTo: 3, follow: false)
        XCTAssertFalse(view.isAnimatingLine)
        let word = try! XCTUnwrap(view.wordRect)
        XCTAssertEqual(view.cursorLine!.maxX, word.maxX, accuracy: 0.5)
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
        let r = SillageReader.silent()
        r.openText(markdown: "Avant.\n\n```rust\nfn a() {\n    b();\n}\n```\n\nAprès.", source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        let d = r.document()
        let layout = DocumentLayout.build(d, style: ReadingStyle(
            font: .literata, size: 20, lineSpacing: 1, ruler: true, theme: .paper))
        view.setDocument(layout.text)
        view.playing = true
        let avant = layout.wordRanges[d.blocks[0].sentences[0].words[0].index]!
        view.show(word: avant, sentence: layout.sentenceRanges[d.blocks[0].sentences[0].id],
                  pastUpTo: 0, follow: false)
        let wordBand = try! XCTUnwrap(view.lineBand)

        let block = layout.sentenceRanges[d.blocks[1].sentences[0].id]!
        view.show(word: nil, sentence: block, pastUpTo: block.location, follow: false)

        XCTAssertEqual(rendering(.foregroundColor, at: 0) as? NSColor, Theme.paper.past)
        XCTAssertNil(view.wordRect)
        XCTAssertNil(view.cursorLine, "pas de trait sous un bloc annoncé")
        XCTAssertFalse(view.isAnimatingLine)
        let blockBand = try! XCTUnwrap(view.lineBand)
        XCTAssertGreaterThan(blockBand.height, wordBand.height * 2)
        XCTAssertGreaterThan(blockBand.minY, wordBand.minY)
        XCTAssertEqual(view.accessibilityValue(), "fn a() {\n    b();\n}")

        view.show(word: nil, sentence: nil, pastUpTo: 0, follow: false)
        XCTAssertNil(view.lineBand)
    }

    /// Le panneau s'élargit en pause : le mot lu et la bande sont repris là
    /// où le mot est maintenant, et la hauteur est renvoyée.
    func testWidthChangeReplacesTheHighlightAndResendsHeight() {
        var heights: [CGFloat] = []
        view.onContentHeight = { heights.append($0) }
        view.setDocument(longText(60))
        heights.removeAll()
        let word = range(of: "mot50")
        view.show(word: word, sentence: word, pastUpTo: 0, follow: false)
        let before = try! XCTUnwrap(view.wordRect)

        view.setFrameSize(NSSize(width: 250, height: view.frame.height))
        let after = try! XCTUnwrap(view.wordRect)
        XCTAssertGreaterThan(after.minY, before.minY)
        XCTAssertEqual(view.lineBand?.minY, after.minY - 2)
        XCTAssertEqual(heights.count, 1)

        // Seule la hauteur change : rien n'est renvoyé, pas de boucle avec
        // le panneau qui se redimensionne.
        view.setFrameSize(NSSize(width: 250, height: view.frame.height + 40))
        XCTAssertEqual(heights.count, 1)
    }

    /// On relit en faisant défiler : la vue ne ramène plus la ligne lue,
    /// jusqu'à la prochaine commande de lecture.
    func testUserScrollStopsFollowingUntilResumed() {
        // Sans animation : le défilement se pose tout de suite, sans dépendre
        // de l'écran (une animation n'avance pas quand il est en veille).
        view.reduceMotion = { true }
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

    /// En lecture, le texte se sélectionne et ne se modifie pas.
    func testTheTextIsSelectableButNotEditable() {
        XCTAssertTrue(view.isSelectable)
        XCTAssertFalse(view.isEditable)
        view.setSelectedRange(NSRange(location: 3, length: 4))
        XCTAssertEqual(view.selectedText, "deux")
        view.setSelectedRange(NSRange(location: 0, length: 0))
        XCTAssertNil(view.selectedText)
    }

    /// La voix avance, la vue suit (trait, couleur, défilement) : la
    /// sélection reste.
    func testFollowingTheVoiceKeepsTheSelection() {
        let selection = NSRange(location: 15, length: 6)
        view.setSelectedRange(selection)
        view.playing = true
        for (i, word) in [NSRange(location: 0, length: 2), NSRange(location: 3, length: 4),
                          NSRange(location: 8, length: 5), NSRange(location: 15, length: 6)].enumerated() {
            view.show(word: word, sentence: NSRange(location: i < 3 ? 0 : 15, length: 14),
                      pastUpTo: word.location, follow: true)
            frames.tick()
        }
        view.setFrameSize(NSSize(width: 420, height: view.frame.height))
        XCTAssertEqual(view.selectedRange(), selection)
        view.playing = false
    }

    /// Clic droit : « Copier », « Lire à partir d'ici » (à l'endroit cliqué).
    func testTheContextMenuOffersCopyAndReadFromHere() throws {
        var readFrom: Int?
        var copies = 0
        view.onReadFrom = { readFrom = $0 }
        view.onCopy = { copies += 1 }
        let word = try XCTUnwrap(view.rectForTesting(NSRange(location: 15, length: 6)))
        let inWindow = view.convert(CGPoint(x: word.midX, y: word.midY), to: nil)
        let event = try XCTUnwrap(NSEvent.mouseEvent(with: .rightMouseDown, location: inWindow, modifierFlags: [],
                                                     timestamp: 0, windowNumber: window.windowNumber, context: nil,
                                                     eventNumber: 0, clickCount: 1, pressure: 1))
        let menu = try XCTUnwrap(view.menu(for: event))
        XCTAssertEqual(menu.items.map(\.title), ["Copier", "Lire à partir d'ici"])
        menu.performActionForItem(at: 1)
        XCTAssertTrue((15..<21).contains(readFrom ?? -1), "endroit : \(String(describing: readFrom))")
        menu.performActionForItem(at: 0)
        XCTAssertEqual(copies, 1)
    }

    /// Un clic droit sur la moitié droite de la dernière lettre d'un mot
    /// désigne ce mot, pas l'insertion qui le suit (le mot suivant).
    func testReadFromHereOnTheLastLetterStaysOnTheWord() throws {
        var readFrom: Int?
        view.onReadFrom = { readFrom = $0 }
        let last = try XCTUnwrap(view.rectForTesting(NSRange(location: 20, length: 1)))
        let inWindow = view.convert(CGPoint(x: last.maxX - 0.5, y: last.midY), to: nil)
        let event = try XCTUnwrap(NSEvent.mouseEvent(with: .rightMouseDown, location: inWindow, modifierFlags: [],
                                                     timestamp: 0, windowNumber: window.windowNumber, context: nil,
                                                     eventNumber: 0, clickCount: 1, pressure: 1))
        let menu = try XCTUnwrap(view.menu(for: event))
        menu.performActionForItem(at: 1)
        XCTAssertEqual(readFrom, 20, "endroit : \(String(describing: readFrom))")
    }
}
