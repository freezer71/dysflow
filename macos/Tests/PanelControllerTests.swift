import XCTest
@testable import Sillon

@MainActor
final class PanelControllerTests: XCTestCase {
    private var controller: PanelController!

    override func tearDown() {
        controller?.collapseOrHide()
        controller?.collapseOrHide()
        controller = nil
        super.tearDown()
    }

    private func shownPanel(_ text: String) -> PanelController {
        let model = ReaderModel(reader: SillonReader.silent())
        model.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.revision == 1 })
        let c = PanelController(model: model)
        controller = c
        c.show()
        settle(c)
        return c
    }

    /// Laisse tourner la boucle jusqu'à ce que le cadre ne bouge plus.
    private func settle(_ c: PanelController) {
        var last = CGRect.null
        var stable = 0
        XCTAssertTrue(waitUntil(8) {
            let f = c.window.frame
            stable = f == last ? stable + 1 : 0
            last = f
            if stable >= 30 { return true }
            RunLoop.main.run(until: Date().addingTimeInterval(0.02))
            return false
        }, "le cadre du panneau ne se stabilise pas")
    }

    func testNoAnnouncementWhenHiddenNorOnShowAfterHide() {
        let c = shownPanel("Un. Deux.")
        _ = c.announcementText(for: c.model.snapshot)
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.announcementText(for: c.model.snapshot), "Lecture")
        c.collapseOrHide()  // masque : pause
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien panneau masqué")
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        c.show()
        settle(c)
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien à la réouverture")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.announcementText(for: c.model.snapshot), "Lecture")
    }

    private func textView(_ c: PanelController) -> ReadingTextView? {
        func find(_ v: NSView) -> ReadingTextView? {
            if let t = v as? ReadingTextView { return t }
            for s in v.subviews { if let t = find(s) { return t } }
            return nil
        }
        return c.window.contentView.flatMap(find)
    }

    private var maxHeight: CGFloat {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame.height * 0.45
    }

    func testShortTextGivesAPanelThatFitsTheText() throws {
        let c = shownPanel("Une seule phrase, assez courte.")
        let text = try XCTUnwrap(textView(c))
        let height = c.window.frame.height
        let shown = text.enclosingScrollView?.contentSize.height ?? 0
        print("MESURE panneau court : \(height) (texte \(text.contentHeight), visible \(shown), max \(maxHeight))")
        XCTAssertLessThan(height, maxHeight - 100, "le panneau doit suivre le texte, pas le maximum")
        XCTAssertEqual(height, max(PanelGeometry.minimumHeight, text.contentHeight + PanelGeometry.chrome),
                       accuracy: 1)
        // Le texte tient en entier dans la vue : rien n'est rogné.
        let visible = try XCTUnwrap(text.enclosingScrollView).contentSize.height
        XCTAssertGreaterThanOrEqual(visible + 1, text.contentHeight, "le texte court est rogné")
    }

    func testLongTextGivesTheMaximumHeight() {
        let long = Array(repeating: "Une phrase qui revient pour remplir le panneau bien au-delà de sa hauteur.",
                         count: 30).joined(separator: " ")
        XCTAssertLessThan(long.count, 4000, "mesuré vraiment, pas l'estimation des longs textes")
        let c = shownPanel(long)
        print("MESURE panneau long : \(c.window.frame.height) (max \(maxHeight))")
        XCTAssertEqual(c.window.frame.height, maxHeight, accuracy: 1)
    }

    func testCollapsingFromExpandedMeasuresAgainAtPanelWidth() {
        let c = shownPanel("Une seule phrase, assez courte.")
        let before = c.window.frame
        c.expand()
        settle(c)
        XCTAssertEqual(c.mode, .expanded)
        c.collapseOrHide()
        settle(c)
        XCTAssertEqual(c.mode, .panel)
        XCTAssertEqual(c.window.frame.height, before.height, accuracy: 1)
        XCTAssertEqual(c.window.frame.width, before.width, accuracy: 1)
    }

    func testClosingTheWindowGoesThroughTheHidePath() {
        let c = shownPanel("Un. Deux.")
        XCTAssertTrue(c.model.isDisplayed)
        let commands = c.model.commandCount
        c.window.performClose(nil)
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertFalse(c.model.isDisplayed)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertEqual(c.model.commandCount, commands + 1, "la lecture doit être mise en pause")
    }

    func testCloseDirectlyAlsoHides() {
        let c = shownPanel("Un. Deux.")
        c.window.close()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertFalse(c.model.isDisplayed)
        // Réafficher fonctionne encore.
        c.show()
        XCTAssertEqual(c.mode, .panel)
        XCTAssertTrue(c.window.isVisible)
    }

    func testCopiedMessageFadesButNotAnotherOne() {
        let c = shownPanel("Un. Deux.")
        c.messageDuration = 0.2
        c.perform(.copySentence)
        XCTAssertEqual(c.model.message, "Phrase copiée")
        XCTAssertTrue(waitUntil(2) { c.model.message == nil })

        c.perform(.copySentence)
        c.model.message = "Autre chose"
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertEqual(c.model.message, "Autre chose")
    }

    /// Chaque « / » redemande le focus du champ, même déjà ouvert.
    func testEachSearchKeyAsksForTheField() {
        let c = shownPanel("Un. Deux.")
        c.perform(.openSearch)
        XCTAssertTrue(c.searchActive)
        XCTAssertEqual(c.openSearchRequests, 1)
        c.perform(.openSearch)
        XCTAssertEqual(c.openSearchRequests, 2)
    }
}

final class PlayerBarFormatTests: XCTestCase {
    func testSpeedUsesADecimalCommaWhenShownAndSpoken() {
        XCTAssertEqual(PlayerBar.speedText(1.4), "1,4×")
        XCTAssertEqual(PlayerBar.speedSpoken(1.4), "1,4 fois")
    }

    func testTimeSurvivesNonFiniteValues() {
        XCTAssertEqual(PlayerBar.time(nil), "–:––")
        XCTAssertEqual(PlayerBar.time(.nan), "–:––")
        XCTAssertEqual(PlayerBar.time(.infinity), "–:––")
        XCTAssertEqual(PlayerBar.time(-.infinity), "–:––")
        XCTAssertEqual(PlayerBar.time(75), "1:15")
    }

    func testTimelineClampsProgress() {
        XCTAssertEqual(Timeline.clamped(-0.2), 0)
        XCTAssertEqual(Timeline.clamped(1.7), 1)
        XCTAssertEqual(Timeline.clamped(.nan), 0)
        XCTAssertEqual(Timeline.clamped(0.4), 0.4)
    }
}
