import XCTest
@testable import Sillon

final class TranscriptTests: XCTestCase {
    func testAVolatileResultReplacesThePreviousOne() {
        var t = Transcript()
        t.receive("Bonjour", isFinal: false)
        t.receive("Bonjour, je", isFinal: false)
        XCTAssertEqual(t.text, "Bonjour, je")
        XCTAssertEqual(t.settled, "")
    }

    func testAFinalResultIsKeptAndClearsTheVolatileOne() {
        var t = Transcript()
        t.receive("Bonjour, je", isFinal: false)
        t.receive("Bonjour, je voudrais.", isFinal: true)
        t.receive("Et puis", isFinal: false)
        XCTAssertEqual(t.settled, "Bonjour, je voudrais.")
        XCTAssertEqual(t.pending, "Et puis")
        XCTAssertEqual(t.text, "Bonjour, je voudrais. Et puis")
    }

    func testPiecesAreJoinedByOneSpace() {
        var t = Transcript()
        t.receive(" Un. ", isFinal: true)
        t.receive("  Deux.", isFinal: true)
        XCTAssertEqual(t.text, "Un. Deux.")
        t.receive("   ", isFinal: false)
        XCTAssertEqual(t.text, "Un. Deux.")
    }

    func testEmptyUntilSomethingIsHeard() {
        var t = Transcript()
        XCTAssertTrue(t.isEmpty)
        t.receive("", isFinal: true)
        XCTAssertTrue(t.isEmpty)
    }

    /// Un son pur à 440 Hz allume la bande qui le contient, et elle seule
    /// au plus fort.
    func testASineLightsUpItsOwnBand() {
        let rate = 48_000.0
        let analyzer = SpectrumAnalyzer(sampleRate: rate)
        let samples = (0..<9600).map { Float(0.1 * sin(2 * .pi * 440 * Double($0) / rate)) }
        let columns = samples.withUnsafeBufferPointer { analyzer.push($0) }
        XCTAssertGreaterThan(columns.count, 5, "40 tranches par seconde")
        let last = columns.last!
        let edges = SpectrumAnalyzer.bandEdges(bands: analyzer.bands, sampleRate: rate, size: SpectrumAnalyzer.size)
        let bin = Int((440 / (rate / Double(SpectrumAnalyzer.size))).rounded())
        let band = (0..<analyzer.bands).first { edges[$0] <= bin && bin < edges[$0 + 1] }!
        XCTAssertEqual(last.firstIndex(of: last.max()!), band)
        XCTAssertEqual(last[band], 1, accuracy: 0.01)
        XCTAssertLessThan(last[analyzer.bands - 1], 0.2, "les aigus restent éteints")
    }

    func testSilenceDrawsNothing() {
        let analyzer = SpectrumAnalyzer(sampleRate: 48_000)
        let silence = [Float](repeating: 0, count: 4800)
        let columns = silence.withUnsafeBufferPointer { analyzer.push($0) }
        XCTAssertTrue(columns.allSatisfy { $0.allSatisfy { $0 == 0 } })
    }

    @MainActor
    func testTheSpectrogramScrollsAtTheRecordingPace() {
        let feed = SpectrumFeed(capacity: 4, bands: 2)
        feed.push([[1, 1], [0.5, 0.5]])
        feed.advance(to: 0)
        XCTAssertEqual(feed.columns.last, [0, 0], "rien n'entre au premier instant")
        feed.advance(to: 1 / SpectrumAnalyzer.rate)
        XCTAssertEqual(feed.columns.last, [1, 1], "une colonne par tranche de temps")
        feed.advance(to: 10)
        XCTAssertEqual(feed.columns.last, [0.5, 0.5])
        XCTAssertEqual(feed.columns.count, 4, "la largeur ne change pas")
        XCTAssertLessThan(feed.phase, 1)
    }

    func testTheLanguageFollowsTheSettingThenTheSystem() {
        let defaults = UserDefaults(suiteName: "DictationLanguageTests")!
        defaults.removePersistentDomain(forName: "DictationLanguageTests")
        XCTAssertEqual(DictationLanguage.stored(defaults, preferred: ["en-GB"]), .english)
        XCTAssertEqual(DictationLanguage.stored(defaults, preferred: ["de-DE"]), .french)
        defaults.set("fr-FR", forKey: DictationLanguage.defaultsKey)
        XCTAssertEqual(DictationLanguage.stored(defaults, preferred: ["en-GB"]), .french)
        defaults.removePersistentDomain(forName: "DictationLanguageTests")
    }
}

// MARK: - Le déroulé

@MainActor
final class FakeDictationEngine: DictationEngine {
    var events: (@MainActor @Sendable (DictationEvent) -> Void)?
    var startError: DictationError?
    var started: [Locale] = []
    var finished = 0
    var cancelled = 0
    /// Ce que le moteur rend en finissant (le dernier résultat définitif).
    var finalText: String?

    /// Le micro ouvert, le démarrage attend cette porte (un modèle qui se
    /// charge), puis échoue avec `lateError` s'il y en a une.
    var loading: TestGate?
    var lateError: DictationError?

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        started.append(locale)
        if let startError { throw startError }
        self.events = events
        events(.listening)
        if let loading { await loading.wait() }
        if let lateError { throw lateError }
    }

    func finish() async {
        finished += 1
        if let finalText { events?(.text(finalText, isFinal: true)) }
    }

    func cancel() async { cancelled += 1 }

    func send(_ event: DictationEvent) { events?(event) }
}

@MainActor
final class FakePresenter: DictationPresenter {
    var visible = false
    var shows = 0
    func show() { visible = true; shows += 1 }
    func hide() { visible = false }
}

@MainActor
final class DictationControllerTests: XCTestCase {
    var engine: FakeDictationEngine!
    var presenter: FakePresenter!
    var inserted: [String] = []
    var insertedWhileVisible: [Bool] = []
    var outcome: InsertOutcome = .pasted

    func makeController() -> DictationController {
        engine = FakeDictationEngine()
        presenter = FakePresenter()
        let controller = DictationController(engine: engine, insert: { [unowned self] text in
            insertedWhileVisible.append(presenter.visible)
            inserted.append(text)
            return text.isEmpty ? .nothing : outcome
        }, language: { .french })
        controller.presenter = presenter
        controller.noticeDuration = .milliseconds(50)
        controller.failureDuration = .milliseconds(50)
        controller.closing = .milliseconds(20)
        return controller
    }

    func testThePressOpensTheWindowAndTheMicrophone() async {
        let c = makeController()
        var paused = false
        c.willStart = { paused = true }
        c.toggle()
        XCTAssertTrue(presenter.visible)
        XCTAssertTrue(paused, "la lecture se met en pause avant le micro")
        let listening = await eventuallyTrue { c.phase == .listening }
        XCTAssertTrue(listening)
        XCTAssertEqual(engine.started, [Locale(identifier: "fr-FR")])
    }

    func testWhatIsSaidShowsUpWhileSpeaking() async {
        let c = makeController()
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Bonjour", isFinal: false))
        XCTAssertEqual(c.transcript.text, "Bonjour")
        XCTAssertNotNil(c.listeningSince, "le chronomètre part avec le micro")
    }

    func testTheSecondPressInsertsTheTextAndClosesTheWindow() async {
        let c = makeController()
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Bonjour, je", isFinal: false))
        engine.finalText = "Bonjour, je voudrais."
        c.toggle()
        let done = await eventuallyTrue { c.phase == .hidden }
        XCTAssertTrue(done)
        XCTAssertEqual(engine.finished, 1)
        XCTAssertEqual(inserted, ["Bonjour, je voudrais."], "les derniers mots arrivent avant la saisie")
        XCTAssertEqual(insertedWhileVisible, [false], "la fenêtre part avant que le texte soit collé")
        XCTAssertFalse(presenter.visible)
    }

    func testEscapeCancelsWithoutInsertingAnything() async {
        let c = makeController()
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Bonjour", isFinal: false))
        c.cancel()
        XCTAssertEqual(c.phase, .hidden)
        XCTAssertFalse(presenter.visible)
        _ = await eventuallyTrue { engine.cancelled == 1 }
        XCTAssertEqual(engine.cancelled, 1)
        engine.send(.text("trop tard", isFinal: true))
        XCTAssertEqual(c.transcript.text, "Bonjour", "une dictée annulée n'écoute plus rien")
        XCTAssertEqual(inserted, [])
    }

    func testCopiedTextIsSaidThenTheWindowLeaves() async {
        let c = makeController()
        outcome = .copied(.noAccessibility)
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Bonjour.", isFinal: true))
        c.toggle()
        let said = await eventuallyTrue { c.phase == .notice(CopyReason.noAccessibility.message) }
        XCTAssertTrue(said)
        XCTAssertEqual(c.missingPermission, .accessibility)
        let gone = await eventuallyTrue { c.phase == .hidden }
        XCTAssertTrue(gone)
    }

    /// Le modèle échoue à se charger après le second appui : le message
    /// d'échec reste, rien n'est saisi (la saisie ne passe pas par-dessus).
    func testALateStartFailureDuringFinishingIsShown() async {
        let c = makeController()
        let loading = TestGate()
        engine.loading = loading
        engine.lateError = .failed("chargement")
        c.closing = .milliseconds(300)
        c.failureDuration = .seconds(5)
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        c.toggle()
        XCTAssertEqual(c.phase, .finishing)
        loading.open()
        let failed = await eventuallyTrue { c.phase == .failed(DictationError.failed("chargement").message) }
        XCTAssertTrue(failed)
        try? await Task.sleep(for: .milliseconds(500))
        XCTAssertEqual(c.phase, .failed(DictationError.failed("chargement").message), "le message reste")
        XCTAssertTrue(inserted.isEmpty, "rien n'est saisi")
        XCTAssertTrue(presenter.visible)
    }

    func testARefusedMicrophoneIsSaid() async {
        let c = makeController()
        engine.startError = .microphoneDenied
        c.toggle()
        let failed = await eventuallyTrue { c.phase == .failed(DictationError.microphoneDenied.message) }
        XCTAssertTrue(failed)
        XCTAssertTrue(presenter.visible)
        XCTAssertEqual(c.missingPermission, .microphone, "la fenêtre propose d'autoriser le micro")
        let gone = await eventuallyTrue { c.phase == .hidden }
        XCTAssertTrue(gone)
    }

    func testAPermissionThatWasMissingIsForgottenOnTheNextDictation() async {
        let c = makeController()
        engine.startError = .microphoneDenied
        c.toggle()
        _ = await eventuallyTrue { c.missingPermission == .microphone }
        engine.startError = nil
        c.toggle()
        XCTAssertNil(c.missingPermission)
    }

    func testThePillShowsWhereTheTextWillGoAndFollowsTheFocus() {
        var front = DictationTarget(processIdentifier: 1, name: "Notes", icon: nil)
        let c = DictationController(engine: FakeDictationEngine(), insert: { _ in .pasted },
                                    language: { .french }, target: { front })
        c.targetMayHaveChanged()
        XCTAssertNil(c.target, "fenêtre retirée : rien ne se suit")
        c.toggle()
        XCTAssertEqual(c.target?.name, "Notes")
        front = DictationTarget(processIdentifier: 2, name: "Mail", icon: nil)
        c.targetMayHaveChanged()
        XCTAssertEqual(c.target?.name, "Mail")
    }

    /// Un essai du micro tient le micro (réglages) : la dictée ne s'ouvre
    /// pas, le dit, et la lecture ne se met pas en pause.
    func testABusyMicrophoneIsSaidWithoutStarting() async {
        let c = makeController()
        var paused = false
        c.willStart = { paused = true }
        c.busy = { "Un essai du micro est en cours" }
        c.toggle()
        XCTAssertEqual(c.phase, .failed("Un essai du micro est en cours"))
        XCTAssertFalse(paused)
        XCTAssertEqual(engine.started, [])
        let gone = await eventuallyTrue { c.phase == .hidden }
        XCTAssertTrue(gone)
        c.busy = { nil }
        c.toggle()
        let listening = await eventuallyTrue { c.phase == .listening }
        XCTAssertTrue(listening)
    }

    func testTheLastDictationCanBeCopiedAgain() async {
        let c = makeController()
        var copied: [String] = []
        c.copy = { copied.append($0) }
        XCTAssertNil(c.lastText, "rien à copier avant la première dictée")
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Mauvais champ.", isFinal: true))
        c.toggle()
        _ = await eventuallyTrue { c.phase == .hidden }
        c.copyLast()
        XCTAssertEqual(copied, ["Mauvais champ."])
        // Une dictée annulée se garde aussi ; une dictée vide ne l'efface pas.
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Annulée.", isFinal: true))
        c.cancel()
        XCTAssertEqual(c.lastText, "Annulée.")
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        c.cancel()
        XCTAssertEqual(c.lastText, "Annulée.")
    }

    func testANewDictationStartsFromAnEmptyText() async {
        let c = makeController()
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        engine.send(.text("Avant.", isFinal: true))
        c.cancel()
        c.toggle()
        _ = await eventuallyTrue { c.phase == .listening }
        XCTAssertTrue(c.transcript.isEmpty)
    }

    func testDownloadProgressIsShownWhilePreparing() {
        let c = makeController()
        c.toggle()
        XCTAssertEqual(c.phase, .preparing(download: nil))
        XCTAssertEqual(DictationLine.status(.preparing(download: 0.42)), "Installation de la langue… 42 %")
        XCTAssertNil(DictationLine.status(.listening), "à l'écoute : les barres seules")
    }

    func testTheLastTwoLinesStayVisible() {
        XCTAssertEqual(DictationLine.cut("Bonjour", width: 300), 0, "tout tient")
        let long = String(repeating: "un mot de plus ", count: 30) + "et la fin"
        let start = DictationLine.cut(long, width: 300)
        XCTAssertGreaterThan(start, 0)
        XCTAssertTrue(String(long.dropFirst(start)).hasSuffix("et la fin"))
        XCTAssertEqual(Array(long)[start - 1], " ", "on coupe au début d'un mot")
    }

    func testTheCutKeepsSettledAndPendingColors() {
        var t = Transcript()
        t.receive("Un deux trois.", isFinal: true)
        t.receive("quatre cinq", isFinal: false)
        let shown = DictationLine.styled(t, theme: .paper, from: 4)
        XCTAssertEqual(String(shown.characters), "…deux trois. quatre cinq")
        XCTAssertEqual(String(DictationLine.styled(t, theme: .paper).characters), "Un deux trois. quatre cinq")
    }

    func testTheBarsMakeAHillWithTheLowBandsInTheMiddle() {
        let column: [Float] = [1, 1, 0.8, 0.8, 0.6, 0.6, 0.4, 0.4, 0.2, 0.2, 0, 0]
        let levels = VoiceBars.levels(column)
        XCTAssertEqual(levels.count, VoiceBars.count)
        XCTAssertEqual(levels.max(), levels[3], "la bande la plus grave au centre")
        XCTAssertEqual(VoiceBars.levels([]), [Float](repeating: 0, count: VoiceBars.count))
    }

    func testTheBarsRiseFastAndFallSlowly() {
        let motion = BarMotion()
        let up = [Float](repeating: 1, count: VoiceBars.count)
        _ = motion.step(toward: up, at: 0, smooth: true)
        XCTAssertEqual(motion.levels.first, 1, "la première image va droit au but")
        let rise = motion.step(toward: [], at: 0.05, smooth: true).first!
        XCTAssertGreaterThan(rise, 0.4, "la retombée est douce")
        XCTAssertLessThan(rise, 1)
    }
}

// MARK: - La saisie

final class WritablePasteboard: PasteboardWriting, @unchecked Sendable {
    var changeCount = 0
    var items: [SavedItem] = [text("ancien")]
    func save() -> [SavedItem] { items }
    func restore(_ saved: [SavedItem]) { items = saved; changeCount += 1 }
    func string() -> String? {
        items.first?.types.first { $0.type == "public.utf8-plain-text" }
            .flatMap { String(data: $0.data, encoding: .utf8) }
    }
    var isConcealed: Bool { Sillon.isConcealed(items) }
    func imageData() -> Data? { nil }
    var transient: Bool { items.first?.types.contains { $0.type == transientType } ?? false }
    var concealed: Bool { items.first?.types.contains { $0.type == concealedType } ?? false }
    /// Le niveau de la dernière écriture (`.currentHostOnly` compris).
    private(set) var lastPrivacy: PasteboardPrivacy?

    func write(_ s: String, privacy: PasteboardPrivacy) {
        var item = text(s)
        if privacy != .normal { item.types.append(SavedType(type: transientType, data: Data())) }
        if privacy == .concealed { item.types.append(SavedType(type: concealedType, data: Data())) }
        items = [item]
        lastPrivacy = privacy
        changeCount += 1
    }
}

final class PasteRecorder: PasteKeystroke, @unchecked Sendable {
    var pasted: [String?] = []
    let pasteboard: WritablePasteboard
    /// L'utilisateur copie autre chose juste après le ⌘V.
    var userCopiesAfter = false
    init(_ pasteboard: WritablePasteboard) { self.pasteboard = pasteboard }
    func sendPaste() {
        pasted.append(pasteboard.string())
        if userCopiesAfter { pasteboard.write("copie de l'utilisateur", privacy: .normal) }
    }
}

@MainActor
final class TextInserterTests: XCTestCase {
    func inserter(_ board: WritablePasteboard, _ keys: PasteRecorder, trusted: Bool = true,
                  secure: Bool = false, own: Bool = false) -> TextInserter {
        TextInserter(pasteboard: board, keys: keys, trusted: { trusted }, secureInput: { secure },
                     ownAppHasKeyboard: { own }, restoreDelay: .milliseconds(1))
    }

    func testTheTextIsPastedThenTheClipboardComesBack() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        let outcome = await inserter(board, keys).insert("  Bonjour.\n")
        XCTAssertEqual(outcome, .pasted)
        XCTAssertEqual(keys.pasted, ["Bonjour."], "le ⌘V colle le texte dicté")
        XCTAssertEqual(board.string(), "ancien", "le presse-papiers revient")
    }

    func testTheDictatedTextIsMarkedTransient() async {
        let board = WritablePasteboard()
        var markedWhilePasting = false
        let check = PasteCheck(board) { markedWhilePasting = $0 }
        _ = await TextInserter(pasteboard: board, keys: check, trusted: { true }, secureInput: { false },
                               ownAppHasKeyboard: { false }, restoreDelay: .milliseconds(1))
            .insert("Bonjour")
        XCTAssertTrue(markedWhilePasting, "les historiques de presse-papiers ne le gardent pas")
    }

    func testACopyMadeMeanwhileIsKept() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        keys.userCopiesAfter = true
        _ = await inserter(board, keys).insert("Bonjour")
        XCTAssertEqual(board.string(), "copie de l'utilisateur")
    }

    func testWithoutAccessibilityTheTextIsLeftInTheClipboard() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        let outcome = await inserter(board, keys, trusted: false).insert("Bonjour")
        XCTAssertEqual(outcome, .copied(.noAccessibility))
        XCTAssertEqual(keys.pasted, [])
        XCTAssertEqual(board.string(), "Bonjour")
        XCTAssertFalse(board.transient, "gardé : c'est là que l'utilisateur le reprend")
        XCTAssertEqual(board.lastPrivacy, .normal)
    }

    func testTheTextPastedStaysOnThisMac() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        _ = await inserter(board, keys).insert("Bonjour")
        // Écrit en éphémère, sur ce Mac seulement (pas de presse-papiers universel).
        XCTAssertEqual(keys.pasted, ["Bonjour"])
        XCTAssertEqual(board.lastPrivacy, .transient)
    }

    func testTextForAPasswordFieldIsConcealed() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        let outcome = await inserter(board, keys, secure: true).insert("motdepasse")
        XCTAssertEqual(outcome, .copied(.secureInput))
        XCTAssertEqual(board.lastPrivacy, .concealed, "ni historique, ni autre appareil")
        XCTAssertTrue(board.concealed)
        XCTAssertTrue(board.transient)
    }

    func testSillonsOwnWindowAndSecureInputGetACopy() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        let own = await inserter(board, keys, own: true).insert("Bonjour")
        XCTAssertEqual(own, .copied(.ownWindow))
        let secure = await inserter(board, keys, secure: true).insert("Bonjour")
        XCTAssertEqual(secure, .copied(.secureInput))
        XCTAssertEqual(keys.pasted, [])
    }

    func testNothingSaidTouchesNothing() async {
        let board = WritablePasteboard()
        let keys = PasteRecorder(board)
        let outcome = await inserter(board, keys).insert("  \n")
        XCTAssertEqual(outcome, .nothing)
        XCTAssertEqual(board.changeCount, 0)
        XCTAssertEqual(keys.pasted, [])
    }
}

/// Regarde, au moment du ⌘V, si le presse-papiers porte le marqueur « transitoire ».
final class PasteCheck: PasteKeystroke, @unchecked Sendable {
    let board: WritablePasteboard
    let seen: (Bool) -> Void
    init(_ board: WritablePasteboard, seen: @escaping (Bool) -> Void) {
        self.board = board
        self.seen = seen
    }
    func sendPaste() { seen(board.transient) }
}

final class ModifierTapTests: XCTestCase {
    let option = UInt16(61)   // Option droite
    let leftOption = UInt16(58)

    func testABriefTapOnTheKeyAloneTriggers() {
        var d = ModifierTapDetector(key: .rightOption)
        XCTAssertFalse(d.flagsChanged(keyCode: option, flags: .option, at: 0))
        XCTAssertTrue(d.flagsChanged(keyCode: option, flags: [], at: 0.15))
    }

    func testOptionWithALetterIsTypingNotATap() {
        var d = ModifierTapDetector(key: .rightOption)
        _ = d.flagsChanged(keyCode: option, flags: .option, at: 0)
        d.keyDown()   // ⌥ + ( pour écrire {
        XCTAssertFalse(d.flagsChanged(keyCode: option, flags: [], at: 0.1))
    }

    func testAHeldKeyIsNotATap() {
        var d = ModifierTapDetector(key: .rightOption)
        _ = d.flagsChanged(keyCode: option, flags: .option, at: 0)
        XCTAssertFalse(d.flagsChanged(keyCode: option, flags: [], at: 0.9))
    }

    func testOtherModifiersOrTheLeftKeyDoNothing() {
        var d = ModifierTapDetector(key: .rightOption)
        _ = d.flagsChanged(keyCode: leftOption, flags: .option, at: 0)
        XCTAssertFalse(d.flagsChanged(keyCode: leftOption, flags: [], at: 0.1), "Option gauche")
        _ = d.flagsChanged(keyCode: 59, flags: .control, at: 1)
        _ = d.flagsChanged(keyCode: option, flags: [.control, .option], at: 1.05)
        XCTAssertFalse(d.flagsChanged(keyCode: option, flags: .control, at: 1.1), "⌃ tenue")
        var none = ModifierTapDetector(key: .none)
        _ = none.flagsChanged(keyCode: option, flags: .option, at: 0)
        XCTAssertFalse(none.flagsChanged(keyCode: option, flags: [], at: 0.1), "réglage « Aucune »")
    }
}

