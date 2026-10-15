import XCTest
import KeyboardShortcuts
@testable import Sillon

@MainActor
final class PanelControllerTests: XCTestCase {
    private var controller: PanelController!

    override func tearDown() async throws {
        controller?.close()
        controller = nil
        try await super.tearDown()
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
        // Fermer met en pause et ne dit rien de ce qui suit.
        c.close()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien fenêtre masquée")
        c.show()
        settle(c)
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien à la réouverture")
        XCTAssertEqual(posted, ["Lecture"], "rien dit fenêtre masquée ni à la réouverture")
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

    func testCloseDirectlyAlsoHides() {
        let c = shownPanel("Un. Deux.")
        c.window.close()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertEqual(c.model.presence, .hidden)
        // Réafficher fonctionne encore.
        c.show()
        XCTAssertEqual(c.mode, .reader)
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

    private var mainScreen: CGRect { NSScreen.screens[0].visibleFrame }

    /// Le lecteur à sa place ; ⌘M : le mini, à sa taille fixe, puis retour.
    func testCommandMSwitchesBetweenReaderAndMiniWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        XCTAssertEqual(c.mode, .reader)
        XCTAssertEqual(c.window.frame, ReaderPlacement.defaultFrame(on: mainScreen))
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur")
        XCTAssertTrue(c.window.styleMask.contains(.resizable))
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.mode, .mini)
        XCTAssertEqual(c.window.frame.size, MiniPlacement.size)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-mini")
        XCTAssertFalse(c.window.styleMask.contains(.resizable), "le mini a une taille fixe")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertTrue(c.model.snapshot.playing, "réduire et agrandir ne touchent pas à la lecture")
        c.model.send(.pause)
    }

    /// ⌘W, Échap ou la fermeture de la fenêtre : masqué et en pause, jamais le mini.
    func testClosingPausesAndNeverLeavesTheMini() {
        for close in [{ (c: PanelController) in c.perform(.close) },
                      { (c: PanelController) in c.perform(.collapseOrHide) },
                      { (c: PanelController) in c.window.performClose(nil) }] {
            let c = shownPanel("Un. Deux.")
            c.sidebarVisible = false
            c.perform(.command(.togglePlay))
            XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
            close(c)
            XCTAssertEqual(c.mode, .hidden)
            XCTAssertFalse(c.window.isVisible)
            XCTAssertEqual(c.model.presence, .hidden)
            XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing }, "la lecture continue")
            XCTAssertEqual(c.store.state.current, .hidden)
        }
    }

    /// Le document et la position restent : la réouverture reprend là.
    func testReopeningAfterClosingKeepsThePlace() {
        let c = shownPanel("Un. Deux. Trois.")
        c.perform(.command(.sentences(delta: 2)))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 2 })
        c.close()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertEqual(c.model.snapshot.sentence, 2)
    }

    /// Toute ouverture rend la dernière taille : le mini fermé revient mini.
    func testShowGivesTheLastSize() {
        let c = shownPanel("Un. Deux.")
        c.perform(.toggleSize)
        settle(c)
        c.close()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .mini)
        XCTAssertTrue(c.window.isVisible)
        // Déjà visible : « show » ne change rien.
        c.show()
        XCTAssertEqual(c.mode, .mini)
    }

    /// ⌃⌥S : affiche la dernière taille, ou masque (en pause).
    func testToggleVisibilityShowsOrHides() {
        let c = shownPanel("Un. Deux.")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.toggleVisibility()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        c.toggleVisibility()
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertTrue(c.window.isVisible)
    }

    /// Une modification en cours au moment de fermer : le texte est gardé,
    /// la voix reste en pause (elle lisait avant l'édition).
    func testClosingWhileEditingKeepsTheTextAndStaysPaused() {
        let c = shownPanel("Un. Deux.")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.beginEditing()
        XCTAssertTrue(c.editing)
        c.draft = "Trois. Quatre."
        c.close()
        XCTAssertFalse(c.editing)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.editableText == "Trois. Quatre." })
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        c.model.refresh()
        XCTAssertFalse(c.model.snapshot.playing, "la reprise différée ne relance pas la voix")
    }

    func testTheMiniKeepsItsPlaceFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        c.perform(.toggleSize)
        settle(c)
        // Placé par le code (animation comprise) : rien n'est retenu.
        XCTAssertNil(c.store.miniOrigin)
        let spot = CGPoint(x: mainScreen.minX + 40, y: mainScreen.minY + 40)
        c.window.setFrameOrigin(spot)
        XCTAssertTrue(waitUntil { c.store.miniOrigin == spot }, "le déplacement n'est pas retenu")
        c.close()
        // Relance : un nouveau contrôleur, mêmes préférences ; rien ne s'affiche
        // avant qu'on le demande, puis le mini, à sa place.
        let again = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        controller = again
        XCTAssertEqual(again.store.state.current, .hidden)
        again.show()
        settle(again)
        XCTAssertEqual(again.mode, .mini)
        XCTAssertEqual(again.window.frame.origin.x, spot.x, accuracy: 1)
        XCTAssertEqual(again.window.frame.origin.y, spot.y, accuracy: 1)
    }

    func testAMiniLeftOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).miniOrigin = CGPoint(x: -50_000, y: -50_000)
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.toggleSize)
        settle(c)
        XCTAssertTrue(mainScreen.contains(c.window.frame), "mini hors écran : \(c.window.frame)")
    }

    /// Le lecteur redimensionné et déplacé par l'utilisateur garde ce cadre.
    func testTheReaderKeepsItsFrameFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        let wanted = CGRect(x: mainScreen.minX + 30, y: mainScreen.minY + 30, width: 600, height: 400)
        c.window.setFrame(wanted, display: true)
        XCTAssertTrue(waitUntil { c.store.readerFrame == wanted }, "le cadre n'est pas retenu")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.store.readerFrame, wanted, "le passage au mini ne compte pas")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.window.frame, wanted)
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

    /// Les réglages ouverts, aucun mode ne remet le panneau au-dessus.
    func testModesKeepThePanelUnderTheOpenSettings() {
        let c = shownPanel("Un. Deux.")
        let settings = NSWindow()
        settings.setAccessibilityIdentifier("Réglages")
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: settings)
        for _ in 0..<3 { c.perform(.toggleSize); settle(c); XCTAssertEqual(c.window.level, .normal) }
        c.close()
        c.show()
        XCTAssertEqual(c.window.level, .normal, "réaffiché")
        NotificationCenter.default.post(name: NSWindow.willCloseNotification, object: settings)
        XCTAssertEqual(c.window.level, .floating)
    }

    /// ↑ ↓ dans le mini : la ligne suit le texte même invisible, aux
    /// lignes du lecteur.
    func testLinesFollowTheTextEvenInTheMini() throws {
        let words = (1...60).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".")
        // La copie cachée a la largeur du texte du lecteur sans sommaire.
        c.sidebarVisible = false
        settle(c)
        let text = try XCTUnwrap(textView(c))
        let string = text.string as NSString
        var ranges: [NSRange] = []
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length),
                                   options: .byWords) { _, range, _, _ in ranges.append(range) }
        let top = { (r: NSRange) in text.rectForTesting(r)!.minY }
        let next = try XCTUnwrap(ranges.firstIndex { top($0) > top(ranges[0]) + 1 },
                                 "le texte doit tenir sur plusieurs lignes")
        let expected = c.model.document.blocks.flatMap(\.sentences).flatMap(\.words)[next].index

        c.perform(.toggleSize)
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
        XCTAssertEqual(c.model.cadence, .glance, "lecteur, en pause : le coup d'œil lent")
        c.perform(.toggleSize)
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

    /// En lecture, la cadence suit la forme : à chaque image dans le lecteur,
    /// 4 Hz pour le mini, une fois par seconde fenêtre masquée — sans lien
    /// d'affichage hors du lecteur.
    func testThePlayingCadenceFollowsTheMode() {
        let frames = ManualFrames()
        let words = (1...200).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".", frames: frames)
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.model.cadence, .screen)
        XCTAssertTrue(frames.isRunning)
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.model.cadence, .glance)
        XCTAssertFalse(frames.isRunning)
        c.close()
        // La pause envoyée garde la seconde le temps de la grâce, puis plus rien.
        XCTAssertTrue(waitUntil { c.model.cadence == .none }, "fermer met en pause : plus rien ne tourne")
        XCTAssertFalse(frames.isRunning)
        // Les touches média relancent la voix fenêtre masquée : la seconde.
        c.model.send(.togglePlay)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.model.cadence, .second)
        XCTAssertFalse(frames.isRunning)
        c.model.send(.pause)
    }

    /// La fenêtre masquée ne garde aucune vue de lecture : rien n'y est remis
    /// à jour, aucun trait ne tourne, et le modèle ne s'y croit plus affiché
    /// (rien en pause).
    func testAHiddenWindowKeepsNothingDisplayed() throws {
        let c = shownPanel("Un. Deux.")
        let text = try XCTUnwrap(textView(c))
        XCTAssertEqual(c.model.cadence, .glance)
        c.close()
        XCTAssertEqual(c.model.presence, .hidden)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertTrue(waitUntil { self.textView(c) == nil }, "une vue de lecture reste fenêtre masquée")
        XCTAssertNil(text.window)
        XCTAssertFalse(text.isAnimatingLine)
        XCTAssertTrue(waitUntil { c.model.cadence == .none }, "cadence : \(c.model.cadence)")
        XCTAssertFalse(c.model.isTicking)
        // Réaffiché, le texte revient.
        c.show()
        XCTAssertTrue(waitUntil { self.textView(c) != nil })
    }

    /// Fermer annule la recherche : le lecteur ne revient
    /// pas avec un champ vide et un compteur « n / m » périmé.
    func testClosingCancelsAnActiveSearch() {
        let c = shownPanel("Un. Deux.")
        c.perform(.openSearch)
        XCTAssertTrue(c.searchActive)
        c.close()
        XCTAssertEqual(c.mode, .hidden)
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

@MainActor
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

    /// Une voix dont aucune session ne s'ouvre le dit, au lieu de rester muette.
    func testAnUnavailableVoiceSaysWhy() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(voice: .unavailable(reason: "x"))),
                       "Voix indisponible : x")
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Voix indisponible : x",
                                                                          voice: .unavailable(reason: "x"))),
                       "Voix indisponible : x", "le statut du lecteur le dit déjà")
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
