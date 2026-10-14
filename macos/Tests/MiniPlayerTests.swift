import KeyboardShortcuts
import SwiftUI
import XCTest
@testable import Sillon

@MainActor
final class PlaybackTimeTests: XCTestCase {
    func testTheRemainingTimeIsShownWithAMinus() {
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 56, total: 176), "-2:00")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: nil, total: 176), "–:––")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 200, total: 176), "-0:00", "jamais négatif")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 5, total: .nan), "–:––")
    }

    func testTheRemainingTimeIsSpokenWithoutTheMinus() {
        XCTAssertEqual(PlaybackTime.spokenRemaining(elapsed: 56, total: 176), "2:00")
        XCTAssertEqual(PlaybackTime.spokenRemaining(elapsed: nil, total: nil), "inconnu")
    }

    /// La timeline annonce « 0:56 sur 2:56 » ; sans estimation, le pourcentage.
    func testTheTimelineValue() {
        XCTAssertEqual(PlaybackTime.timelineValue(elapsed: 56, total: 176, progress: 0.3), "0:56 sur 2:56")
        XCTAssertEqual(PlaybackTime.timelineValue(elapsed: nil, total: nil, progress: 0.4), "40 %")
    }

    func testTheMenuLine() {
        XCTAssertEqual(PlaybackTime.menuLine(elapsed: 56, total: 176), "Reste environ 2:00")
        XCTAssertNil(PlaybackTime.menuLine(elapsed: nil, total: 176))
    }

    func testNothingToReadNamesTheChosenShortcut() {
        let space = KeyboardShortcuts.Shortcut(.space, modifiers: [.control, .option])
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: space), "Rien à lire — ⌃⌥Espace")
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: nil), "Rien à lire — raccourci : ⌘,")
    }
}

@MainActor
final class MiniPlayerViewTests: XCTestCase {
    /// La barre entière, montée avec un document et les pires valeurs
    /// (« 99:59 », « -99:59 », « 10,0× »), tient dans sa fenêtre ; la
    /// timeline garde au moins 30 pt.
    func testTheAssembledBarFitsAtTheWorstValues() {
        let model = ReaderModel(reader: SillonReader.silent())
        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        var worst = model.snapshot
        worst.elapsed = 0
        worst.total = 99 * 60 + 59
        worst.speed = 10
        let bar = MiniPlayerBar(model: model, snapshot: worst, message: nil, theme: .paper) { _ in }
        let host = NSHostingView(rootView: bar)
        host.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        host.layoutSubtreeIfNeeded()
        let ideal = host.fittingSize
        XCTAssertLessThanOrEqual(ideal.width, SmallPlacement.size.width, "déborde de \(ideal.width - SmallPlacement.size.width) pt")
        XCTAssertLessThanOrEqual(ideal.height, SmallPlacement.size.height)
        // Les textes fixes de la rangée du haut laissent 30 pt à la timeline.
        let fixed = HStack(spacing: 8) {
            Text("99:59").monospacedDigit().fixedSize()
            Text("-99:59").monospacedDigit().fixedSize()
        }.font(.system(size: 13))
        let room = SmallPlacement.size.width - 2 * MiniBarMetrics.margin - 16 - NSHostingView(rootView: fixed).fittingSize.width
        XCTAssertGreaterThanOrEqual(room, 30)
        let speed = NSHostingView(rootView: SpeedLabel(speed: 10).fixedSize().font(.system(size: 13)))
        XCTAssertLessThanOrEqual(speed.fittingSize.width, MiniBarMetrics.side)
    }

    /// Chaque bouton de barre offre une cible d'au moins 32 × 32 pt.
    func testHoverBackgroundOnlyOnAnEnabledButton() {
        XCTAssertTrue(BarButtonMetrics.showsHover(hovering: true, enabled: true))
        XCTAssertFalse(BarButtonMetrics.showsHover(hovering: true, enabled: false))
        XCTAssertFalse(BarButtonMetrics.showsHover(hovering: false, enabled: true))
    }

    func testEveryBarButtonHasAComfortableTarget() {
        XCTAssertGreaterThanOrEqual(BarButtonMetrics.target, 32)
        XCTAssertGreaterThanOrEqual(BarButtonMetrics.disc, 36)
        XCTAssertTrue((16...17).contains(BarButtonMetrics.glyph))
        let model = ReaderModel(reader: SillonReader.silent())
        for view in [AnyView(PlayButton(model: model)), AnyView(VoiceSettingsButton()),
                     AnyView(MiniTransportButton(item: MiniTransport.after[0]) { _ in })] {
            let size = NSHostingView(rootView: view).fittingSize
            XCTAssertGreaterThanOrEqual(size.width, 32)
            XCTAssertGreaterThanOrEqual(size.height, 32)
        }
        let disc = NSHostingView(rootView: PlayButton(model: model, prominent: true)).fittingSize
        XCTAssertGreaterThanOrEqual(disc.width, 36)
        XCTAssertGreaterThanOrEqual(disc.height, 36)
    }

    /// Le temps écoulé annonce sa valeur à VoiceOver (l'arbre d'accessibilité
    /// d'une vue hors écran est vide : on éprouve ce que la vue déclare).
    func testTheElapsedTimeCarriesItsValue() {
        XCTAssertEqual(MiniTimeRow.elapsedLabel, "Temps écoulé")
        XCTAssertEqual(MiniTimeRow.elapsedValue("0:56"), "0:56")
    }

    /// Un message passager prend la place des temps (ou de « Rien à lire »).
    func testAMessageReplacesTheTimes() {
        XCTAssertEqual(MiniTop.choose(message: nil, hasDocument: true), .times)
        XCTAssertEqual(MiniTop.choose(message: nil, hasDocument: false), .empty)
        XCTAssertEqual(MiniTop.choose(message: "Aucune ligne de ce côté", hasDocument: true),
                       .message("Aucune ligne de ce côté"))
        let model = ReaderModel(reader: SillonReader.silent())
        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        let bar = MiniPlayerBar(model: model, snapshot: model.snapshot,
                                message: "Aucune ligne de ce côté", theme: .paper) { _ in }
        let host = NSHostingView(rootView: bar)
        host.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        host.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(host.fittingSize.height, SmallPlacement.size.height)
        XCTAssertLessThanOrEqual(host.fittingSize.width, SmallPlacement.size.width)
    }

    func testTheBarIs340By88() {
        XCTAssertEqual(SmallPlacement.size, CGSize(width: 340, height: 88))
    }

    /// Ligne précédente, mot précédent, [lecture], mot suivant, ligne suivante.
    func testTheTransportButtonsSendTheRightActions() {
        let all = MiniTransport.before + MiniTransport.after
        XCTAssertEqual(all.map(\.identifier), ["ligne-precedente", "mot-precedent", "mot-suivant", "ligne-suivante"])
        XCTAssertEqual(all.map(\.action), [.line(delta: -1), .command(.words(delta: -1)),
                                           .command(.words(delta: 1)), .line(delta: 1)])
        XCTAssertEqual(MiniTransport.before.count, 2)
    }

    func testTheTransportButtonsHaveLabelsAndHelp() {
        let all = MiniTransport.before + MiniTransport.after
        XCTAssertEqual(all.map(\.label), ["Ligne précédente", "Mot précédent", "Mot suivant", "Ligne suivante"])
        XCTAssertEqual(all.map(\.help), ["Ligne précédente (↑)", "Mot précédent (⌥←)",
                                         "Mot suivant (⌥→)", "Ligne suivante (↓)"])
    }

    /// Ces actions sont celles des touches : ↑ ↓ et ⌥← ⌥→.
    func testTheButtonsMatchTheKeys() {
        let all = MiniTransport.before + MiniTransport.after
        let keys: [Sillon.KeyPress] = [Sillon.KeyPress(characters: "", keyCode: KeyCode.up),
                                Sillon.KeyPress(characters: "", keyCode: KeyCode.left, option: true),
                                Sillon.KeyPress(characters: "", keyCode: KeyCode.right, option: true),
                                Sillon.KeyPress(characters: "", keyCode: KeyCode.down)]
        XCTAssertEqual(all.map(\.action), keys.map { KeyMap.action(for: $0) })
    }

    func testAButtonSendsItsActionOnClick() {
        var sent: [ReaderAction] = []
        let item = MiniTransport.after[0]
        MiniTransportButton(item: item) { sent.append($0) }.fire()
        XCTAssertEqual(sent, [.command(.words(delta: 1))])
    }

    func testTheTransportIsDisabledWithoutADocument() {
        let model = ReaderModel(reader: SillonReader.silent())
        XCTAssertFalse(model.snapshot.hasDocument)
        XCTAssertFalse(MiniTransport.enabled(snapshot: model.snapshot))
    }

    func testTheEmptyBarFitsItsHeight() {
        let model = ReaderModel(reader: SillonReader.silent())
        let host = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper) { _ in })
        host.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        host.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(host.fittingSize.height, SmallPlacement.size.height)
    }
}
