import XCTest
import KeyboardShortcuts
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

    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.affichage.panneau"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    private func shownPanel(_ text: String, defaults: UserDefaults? = nil,
                            frames: FrameSource? = nil) -> PanelController {
        let model = ReaderModel(reader: SillonReader.silent(), frames: frames)
        model.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.revision == 1 })
        let c = PanelController(model: model, store: DisplayModeStore(defaults: defaults ?? freshDefaults()))
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

    /// L'annonceur a deux lecteurs : la vue (`onChange` de l'instantané,
    /// pendant que la boucle tourne) et ce test. Un état n'est dit qu'une
    /// fois : on compte ce que le vrai chemin (`announce`) poste, quel que
    /// soit celui qui a vu le changement le premier.
    func testNoAnnouncementWhenHiddenNorOnShowAfterHide() {
        let c = shownPanel("Un. Deux.")
        var posted: [String] = []
        c.post = { posted.append($0) }
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.announce(c.model.snapshot)  // sans effet si la vue l'a déjà dit
        XCTAssertEqual(posted, ["Lecture"], "« Lecture » dit une fois")
        c.collapseOrHide()  // arrière-plan : la lecture continue
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien en arrière-plan")
        c.model.send(.pause)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien en arrière-plan")
        c.show()
        settle(c)
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien à la réouverture")
        XCTAssertEqual(posted, ["Lecture"], "rien dit en arrière-plan ni à la réouverture")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.announce(c.model.snapshot)
        XCTAssertEqual(posted, ["Lecture", "Lecture"])
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

    /// ↓ vise le premier mot de la ligne affichée suivante, d'un saut absolu.
    func testDownSeeksToTheFirstWordOfTheNextDisplayedLine() throws {
        let words = (1...60).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".")
        let text = try XCTUnwrap(textView(c))
        let string = text.string as NSString
        // Le cadre de chaque mot affiché, dans l'ordre du texte.
        var ranges: [NSRange] = []
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length),
                                   options: .byWords) { _, range, _, _ in ranges.append(range) }
        let top = { (r: NSRange) in text.rectForTesting(r)!.minY }
        let next = try XCTUnwrap(ranges.firstIndex { top($0) > top(ranges[0]) + 1 },
                                 "le texte doit tenir sur plusieurs lignes")
        let docWords = c.model.document.blocks.flatMap(\.sentences).flatMap(\.words)
        let expected = docWords[next].index

        c.perform(.line(delta: 1))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.word == expected },
                      "mot visé : \(expected), lu : \(String(describing: c.model.snapshot.word))")
        XCTAssertEqual(c.model.snapshot.sentence, 0)
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
        XCTAssertEqual(c.mode, .large)
        c.collapseOrHide()
        settle(c)
        XCTAssertEqual(c.mode, .medium)
        XCTAssertEqual(c.window.frame.height, before.height, accuracy: 1)
        XCTAssertEqual(c.window.frame.width, before.width, accuracy: 1)
    }

    /// ⌘W : l'arrière-plan, sans pause (la spec : la lecture continue).
    func testClosingTheWindowGoesToTheBackgroundWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        XCTAssertEqual(c.model.presence, .full)
        let commands = c.model.commandCount
        c.window.performClose(nil)
        XCTAssertEqual(c.mode, .background)
        XCTAssertEqual(c.model.presence, .hidden)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertEqual(c.model.commandCount, commands, "aucune pause envoyée")
        XCTAssertEqual(c.store.state.current, .background)
    }

    func testCloseDirectlyAlsoHides() {
        let c = shownPanel("Un. Deux.")
        c.window.close()
        XCTAssertEqual(c.mode, .background)
        XCTAssertEqual(c.model.presence, .hidden)
        // Réafficher fonctionne encore.
        c.show()
        XCTAssertEqual(c.mode, .medium)
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

    /// Le pas de l'avance rapide est celui des réglages, pas un 5 en dur.
    func testShiftArrowUsesTheSeekStepOfTheSettings() {
        let c = shownPanel("Un. Deux.")
        let key = KeyPress(characters: "", keyCode: KeyCode.right, shift: true)
        XCTAssertEqual(c.action(for: key), .command(.sentences(delta: 5)), "valeur par défaut")
        c.reading.seekStep = 12
        XCTAssertEqual(c.action(for: key), .command(.sentences(delta: 12)))
        XCTAssertEqual(c.action(for: KeyPress(characters: "", keyCode: KeyCode.left, shift: true)),
                       .command(.sentences(delta: -12)))
    }

    /// Les réglages ouverts, le panneau passe sous eux, puis flotte à nouveau.
    func testThePanelStepsBackWhileTheSettingsAreOpen() {
        let c = shownPanel("Un. Deux.")
        XCTAssertEqual(c.window.level, .floating)
        let settings = NSWindow()
        settings.setAccessibilityIdentifier("Réglages")
        let other = NSWindow()
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: other)
        XCTAssertEqual(c.window.level, .floating, "une autre fenêtre n'y change rien")
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: settings)
        XCTAssertEqual(c.window.level, .normal)
        XCTAssertTrue(c.window.isVisible, "le panneau reste affiché")
        NotificationCenter.default.post(name: NSWindow.willCloseNotification, object: settings)
        XCTAssertEqual(c.window.level, .floating)
    }

    func testPanelLevelIsDecidedBySettingsVisibility() {
        XCTAssertEqual(PanelLevel.level(settingsVisible: false), .floating)
        XCTAssertEqual(PanelLevel.level(settingsVisible: true), .normal)
    }

    private var screen: CGRect { (NSScreen.main ?? NSScreen.screens[0]).visibleFrame }

    func testCommandDigitsGiveEachModeItsSize() {
        let c = shownPanel("Une seule phrase, assez courte.")
        let medium = c.window.frame
        c.perform(.display(.small))
        settle(c)
        XCTAssertEqual(c.mode, .small)
        XCTAssertEqual(c.window.frame.width, SmallPlacement.size.width, accuracy: 1)
        XCTAssertEqual(c.window.frame.height, SmallPlacement.size.height, accuracy: 1)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-reduit")
        XCTAssertFalse(c.window.styleMask.contains(.resizable), "la barre a une taille fixe")
        XCTAssertEqual(c.window.standardWindowButton(.closeButton)?.isHidden, true)
        XCTAssertEqual(c.window.standardWindowButton(.zoomButton)?.isHidden, true)
        XCTAssertEqual(c.window.standardWindowButton(.miniaturizeButton)?.isHidden, true)
        XCTAssertTrue(c.window.titlebarAppearsTransparent)
        c.perform(.display(.large))
        settle(c)
        XCTAssertEqual(c.mode, .large)
        XCTAssertEqual(c.window.frame, PanelGeometry.expandedFrame(screen: screen))
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-agrandi")
        c.perform(.display(.medium))
        settle(c)
        XCTAssertEqual(c.mode, .medium)
        XCTAssertEqual(c.window.frame.width, medium.width, accuracy: 1)
        XCTAssertEqual(c.window.frame.height, medium.height, accuracy: 1)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "panneau-lecture")
        XCTAssertTrue(c.window.styleMask.contains(.resizable))
        XCTAssertEqual(c.window.standardWindowButton(.closeButton)?.isHidden, true)
        XCTAssertEqual(c.window.standardWindowButton(.zoomButton)?.isHidden, true)
        XCTAssertEqual(c.window.standardWindowButton(.miniaturizeButton)?.isHidden, true)
        XCTAssertTrue(c.window.titlebarAppearsTransparent)
    }

    func testEscapeGoesFromLargeToMediumThenToTheBackgroundWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.large))
        settle(c)
        c.perform(.command(.togglePlay))
        let commands = c.model.commandCount
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .medium)
        settle(c)
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertEqual(c.model.commandCount, commands, "la lecture continue en arrière-plan")
    }

    func testEscapeFromTheSmallBarGoesToTheBackground() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.window.isVisible)
    }

    /// ⌃⌥S : le dernier mode visible ; ⏎ depuis la barre : le grand mode.
    func testShowBringsBackTheLastVisibleMode() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.collapseOrHide()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .small)
        XCTAssertTrue(c.window.isVisible)
        c.perform(.expand)
        settle(c)
        XCTAssertEqual(c.mode, .large)
    }

    /// ⌃⌥Espace garde le mode ; depuis l'arrière-plan, il ouvre le moyen.
    func testReadingASelectionKeepsTheModeOrOpensTheMediumOne() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.apply(.readSelection)
        XCTAssertEqual(c.mode, .small)
        c.collapseOrHide()
        XCTAssertEqual(c.mode, .background)
        c.apply(.readSelection)
        XCTAssertEqual(c.mode, .medium)
        XCTAssertTrue(c.window.isVisible)
    }

    func testTheSmallBarKeepsItsPlaceFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        c.perform(.display(.small))
        settle(c)
        // Placée par le code (animation comprise) : rien n'est retenu.
        XCTAssertNil(c.store.smallOrigin)
        let main = NSScreen.screens[0].visibleFrame
        let spot = CGPoint(x: main.minX + 40, y: main.minY + 40)
        c.window.setFrameOrigin(spot)
        XCTAssertTrue(waitUntil { c.store.smallOrigin == spot }, "le déplacement n'est pas retenu")
        c.collapseOrHide()
        c.show()
        settle(c)
        XCTAssertEqual(c.window.frame.minX, spot.x, accuracy: 1)
        XCTAssertEqual(c.window.frame.minY, spot.y, accuracy: 1)
        // Relance : un nouveau contrôleur, mêmes préférences.
        c.collapseOrHide()
        let again = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        controller = again
        again.show()
        settle(again)
        XCTAssertEqual(again.mode, .small)
        XCTAssertEqual(again.window.frame.minX, spot.x, accuracy: 1)
        XCTAssertEqual(again.window.frame.minY, spot.y, accuracy: 1)
    }

    func testASmallBarLeftOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).smallOrigin = CGPoint(x: -50_000, y: -50_000)
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.display(.small))
        settle(c)
        XCTAssertTrue(NSScreen.screens[0].visibleFrame.contains(c.window.frame),
                      "barre hors écran : \(c.window.frame)")
    }

    /// Relance : le mode visible retenu reparaît ; l'arrière-plan, rien.
    func testTheModeComesBackAfterARelaunchButNotTheBackground() {
        let defaults = freshDefaults()
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.display(.large))
        settle(c)
        let relaunched = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        relaunched.restore()
        settle(relaunched)
        XCTAssertEqual(relaunched.mode, .large)
        XCTAssertTrue(relaunched.window.isVisible)
        relaunched.collapseOrHide()
        relaunched.collapseOrHide()
        let third = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        third.restore()
        XCTAssertEqual(third.mode, .background)
        XCTAssertFalse(third.window.isVisible)
        third.show()
        XCTAssertEqual(third.mode, .medium, "Échap du grand mode avait retenu le moyen")
        third.collapseOrHide()
    }

    /// Les réglages ouverts, aucun mode ne remet le panneau au-dessus.
    func testModesKeepThePanelUnderTheOpenSettings() {
        let c = shownPanel("Un. Deux.")
        let settings = NSWindow()
        settings.setAccessibilityIdentifier("Réglages")
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: settings)
        for mode in [DisplayMode.small, .large, .medium] {
            c.perform(.display(mode))
            settle(c)
            XCTAssertEqual(c.window.level, .normal, "\(mode)")
        }
        c.collapseOrHide()
        c.show()
        XCTAssertEqual(c.window.level, .normal, "réaffiché")
        NotificationCenter.default.post(name: NSWindow.willCloseNotification, object: settings)
        XCTAssertEqual(c.window.level, .floating)
    }

    /// ↑ ↓ dans la barre : la ligne suit le texte même invisible, aux
    /// lignes du mode moyen.
    func testLinesFollowTheTextEvenInTheSmallBar() throws {
        let words = (1...60).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".")
        let text = try XCTUnwrap(textView(c))
        let string = text.string as NSString
        var ranges: [NSRange] = []
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length),
                                   options: .byWords) { _, range, _, _ in ranges.append(range) }
        let top = { (r: NSRange) in text.rectForTesting(r)!.minY }
        let next = try XCTUnwrap(ranges.firstIndex { top($0) > top(ranges[0]) + 1 },
                                 "le texte doit tenir sur plusieurs lignes")
        let expected = c.model.document.blocks.flatMap(\.sentences).flatMap(\.words)[next].index

        c.perform(.display(.small))
        settle(c)
        XCTAssertTrue(waitUntil { self.textView(c).map { $0 !== text } ?? false },
                      "la copie cachée de la vue de lecture manque")
        c.perform(.line(delta: 1))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.word == expected },
                      "mot visé : \(expected), lu : \(String(describing: c.model.snapshot.word))")
    }

    /// La barre en pause ne coûte rien : ni coup d'œil lent, ni lien
    /// d'affichage. Elle suit la cadence « lit sans être affiché ».
    func testAPausedSmallBarCostsNothing() {
        let c = shownPanel("Un. Deux.")
        XCTAssertEqual(c.model.cadence, .glance, "moyen, en pause : le coup d'œil lent")
        c.perform(.display(.small))
        settle(c)
        XCTAssertEqual(c.model.presence, .bar)
        XCTAssertTrue(waitUntil { c.model.cadence == .none }, "cadence : \(c.model.cadence)")
        XCTAssertFalse(c.model.isTicking)
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.model.cadence, .glance, "en lecture, la barre relit l'état à 4 Hz")
        c.model.send(.pause)
        XCTAssertTrue(waitUntil { c.model.cadence == .none }, "cadence : \(c.model.cadence)")
        XCTAssertFalse(c.model.isTicking)
    }

    /// En lecture, la cadence suit le mode : à chaque image au moyen, 4 Hz
    /// pour la barre, une fois par seconde en arrière-plan — sans lien
    /// d'affichage hors du moyen et du grand.
    func testThePlayingCadenceFollowsTheMode() {
        let frames = ManualFrames()
        let words = (1...200).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".", frames: frames)
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.model.cadence, .screen)
        XCTAssertTrue(frames.isRunning)
        c.perform(.display(.small))
        settle(c)
        XCTAssertEqual(c.model.cadence, .glance)
        XCTAssertFalse(frames.isRunning)
        c.collapseOrHide()
        XCTAssertEqual(c.model.cadence, .second)
        XCTAssertFalse(frames.isRunning)
        // La grâce passée, toujours la seconde (la minuterie seule relit :
        // ReaderModelTests.testHiddenPlayIsSeenWithoutManualRefresh).
        RunLoop.main.run(until: Date().addingTimeInterval(ReaderModel.gracePeriod + 0.6))
        XCTAssertTrue(c.model.snapshot.playing)
        XCTAssertEqual(c.model.cadence, .second)
        XCTAssertFalse(frames.isRunning)
        c.model.send(.pause)
    }

    /// L'arrière-plan ne garde aucune vue de lecture dans la fenêtre
    /// retirée : rien n'y est remis à jour pendant l'écoute, aucun trait ne
    /// tourne, et le modèle ne s'y croit plus affiché (rien en pause).
    func testTheBackgroundKeepsNothingDisplayed() throws {
        let c = shownPanel("Un. Deux.")
        let text = try XCTUnwrap(textView(c))
        XCTAssertEqual(c.model.cadence, .glance)
        c.collapseOrHide()
        XCTAssertEqual(c.model.presence, .hidden)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertTrue(waitUntil { self.textView(c) == nil }, "une vue de lecture reste en arrière-plan")
        XCTAssertNil(text.window)
        XCTAssertFalse(text.isAnimatingLine)
        XCTAssertTrue(waitUntil { c.model.cadence == .none }, "cadence : \(c.model.cadence)")
        XCTAssertFalse(c.model.isTicking)
        // Réaffiché, le texte revient.
        c.show()
        XCTAssertTrue(waitUntil { self.textView(c) != nil })
    }

    /// En arrière-plan la recherche est annulée : le grand mode ne revient
    /// pas avec un champ vide et un compteur « n / m » périmé.
    func testGoingToTheBackgroundCancelsAnActiveSearch() {
        let c = shownPanel("Un. Deux.")
        c.perform(.openSearch)
        XCTAssertTrue(c.searchActive)
        c.collapseOrHide()
        c.collapseOrHide()
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.searchActive)
        XCTAssertNil(c.model.snapshot.search)
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

    func testVolumeShowsOnlyWhenItDiffersFromFull() {
        XCTAssertNil(PlayerBar.volumeText(volume: 1, muted: false))
        XCTAssertEqual(PlayerBar.volumeText(volume: 0.6, muted: false), "60 %")
        XCTAssertEqual(PlayerBar.volumeText(volume: 0.6, muted: true), "Muet")
        XCTAssertEqual(PlayerBar.volumeText(volume: 1, muted: true), "Muet")
    }

    func testSkippedSentencesAreCounted() {
        XCTAssertNil(PlayerBar.skippedText(0))
        XCTAssertEqual(PlayerBar.skippedText(1), "1 phrase sautée")
        XCTAssertEqual(PlayerBar.skippedText(3), "3 phrases sautées")
    }

    func testTimelineClampsProgress() {
        XCTAssertEqual(Timeline.clamped(-0.2), 0)
        XCTAssertEqual(Timeline.clamped(1.7), 1)
        XCTAssertEqual(Timeline.clamped(.nan), 0)
        XCTAssertEqual(Timeline.clamped(0.4), 0.4)
    }
}

/// La ligne de message du panneau : ce que l'app dit, sinon ce que le
/// lecteur dit, sinon la voix de repli.
@MainActor
final class PanelStatusTests: XCTestCase {
    private func snapshot(status: String = "", voice: VoiceState = .ready) -> Snapshot {
        var s = SillonReader.silent().snapshot()
        s.hasDocument = true
        s.status = status
        s.voice = voice
        return s
    }

    func testAReloadingVoiceSaysSo() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .loading)),
                       "Voix en chargement…")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Signet ajouté", voice: .loading)),
                       "Signet ajouté", "le lecteur parle d'abord")
    }

    /// La toute première préparation des noyaux Metal : le texte de la spec,
    /// plutôt que « Voix en chargement… », même voix déjà chargée.
    func testAFirstPreparationSaysSo() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .loading), preparing: true),
                       "Voix en préparation (première fois)…")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .ready), preparing: true),
                       "Voix en préparation (première fois)…")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Signet ajouté", voice: .loading),
                                        preparing: true),
                       "Signet ajouté", "le lecteur parle d'abord")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .fallback(reason: "x")), preparing: true),
                       "Voix du système — installer une voix : ⌘,", "en repli, c'est la voix du système qui parle")
    }

    func testAppMessageWins() {
        XCTAssertEqual(PanelStatus.line(message: "Phrase copiée",
                                        snapshot: snapshot(status: "Signet ajouté")), "Phrase copiée")
    }

    func testReaderStatusShows() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Aucun mot de ce côté")),
                       "Aucun mot de ce côté")
    }

    func testFallbackVoiceSaysHowToInstallOne() {
        let s = snapshot(voice: .fallback(reason: "modèle absent"))
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: s),
                       "Voix du système — installer une voix : ⌘,")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Signet ajouté",
                                                                          voice: .fallback(reason: "x"))),
                       "Signet ajouté")
    }

    func testNothingToSay() {
        XCTAssertNil(PanelStatus.line(message: nil, snapshot: snapshot()))
    }
}

/// L'invitation sans document dit le raccourci réellement choisi.
@MainActor
final class PanelInvitationTests: XCTestCase {
    func testInvitationNamesTheChosenShortcut() {
        let custom = KeyboardShortcuts.Shortcut(.r, modifiers: [.control, .option])
        XCTAssertEqual(PanelInvitation.text(for: custom), "Sélectionnez ou copiez du texte, puis ⌃⌥R")
    }

    func testTheSpaceBarIsNamedInFrench() {
        let space = KeyboardShortcuts.Shortcut(.space, modifiers: [.control, .option])
        XCTAssertEqual(PanelInvitation.text(for: space), "Sélectionnez ou copiez du texte, puis ⌃⌥Espace")
    }

    func testWithoutShortcutItPointsToTheSettings() {
        XCTAssertEqual(PanelInvitation.text(for: nil),
                       "Sélectionnez ou copiez du texte, puis choisissez un raccourci : ⌘,")
    }
}
