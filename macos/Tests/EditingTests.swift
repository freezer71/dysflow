import XCTest
@testable import Sillon

@MainActor
final class EditingTests: XCTestCase {
    private var controller: PanelController!

    override func tearDown() async throws {
        controller?.close()
        controller = nil
        try await super.tearDown()
    }

    private func shownPanel(_ text: String) -> PanelController {
        let name = "sillon.tests.edition"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        let model = ReaderModel(reader: SillonReader.silent())
        model.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.revision == 1 })
        let c = PanelController(model: model, store: DisplayModeStore(defaults: defaults))
        controller = c
        c.apply(.choose(.reader))
        return c
    }

    func testTheEKeyEdits() {
        XCTAssertEqual(KeyMap.action(for: KeyPress(characters: "e")), .edit)
        XCTAssertTrue(HelpOverlay.rows.contains { $0.1 == "Modifier le texte" })
    }

    func testEditingStartsFromTheOpenedText() {
        let c = shownPanel("# Titre\n\nUn. Deux.")
        c.beginEditing()
        XCTAssertTrue(c.editing)
        XCTAssertEqual(c.draft, "# Titre\n\nUn. Deux.")
    }

    func testTheEditedTextReplacesTheDocumentAtTheSameSentence() {
        let c = shownPanel("Un. Deux. Trois. Quatre.")
        c.model.send(.seekTo(sentence: 2))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 2 })
        c.beginEditing()
        c.draft = "Un. Deux. Trois corrigée. Quatre."
        c.finishEditing()
        XCTAssertFalse(c.editing)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.revision == 2 })
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.pendingResume == nil })
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 2 },
                      "phrase \(c.model.snapshot.sentence)")
        XCTAssertEqual(c.model.sourceText, "Un. Deux. Trois corrigée. Quatre.")
        XCTAssertTrue(c.model.currentSentenceText.contains("corrigée"))
    }

    func testAShorterTextResumesAtItsLastSentence() {
        let c = shownPanel("Un. Deux. Trois. Quatre.")
        c.model.send(.seekTo(sentence: 3))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 3 })
        c.beginEditing()
        c.draft = "Un. Deux."
        c.finishEditing()
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.pendingResume == nil })
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 1 },
                      "phrase \(c.model.snapshot.sentence)")
    }

    func testEscapeValidatesAndKeepsThePanel() {
        let c = shownPanel("Un. Deux.")
        c.beginEditing()
        c.draft = "Un. Trois."
        c.perform(.escape)
        XCTAssertFalse(c.editing)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.revision == 2 })
    }

    func testNothingReloadsWhenNothingChanged() {
        let c = shownPanel("Un. Deux.")
        c.beginEditing()
        c.finishEditing()
        c.beginEditing()
        c.draft = "Autre chose."
        c.cancelEditing()
        c.model.refresh()
        XCTAssertEqual(c.model.snapshot.revision, 1)
        XCTAssertEqual(c.model.sourceText, "Un. Deux.")
    }

    func testTheMiniOpensTheReaderToEdit() {
        let c = shownPanel("Un. Deux.")
        c.apply(.choose(.mini))
        c.perform(.edit)
        XCTAssertTrue(c.editing)
        XCTAssertEqual(c.mode, .reader)
    }

    /// Une image passée à l'OCR n'a pas de texte d'origine : le document est
    /// réécrit en Markdown, titres et listes compris.
    func testTheDocumentIsRewrittenWhenThereIsNoSource() {
        let c = shownPanel("# Titre\n\n- un\n- deux\n\nTexte final.")
        let markdown = EditableText.markdown(from: c.model.document)
        XCTAssertTrue(markdown.hasPrefix("# Titre"), markdown)
        XCTAssertTrue(markdown.contains("- un"), markdown)
        XCTAssertTrue(markdown.contains("Texte final."), markdown)
    }
}
