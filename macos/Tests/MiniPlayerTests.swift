import KeyboardShortcuts
import SwiftUI
import XCTest
@testable import Sillage

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
        let model = ReaderModel(reader: SillageReader.silent())
        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        var worst = model.snapshot
        worst.elapsed = 0
        worst.total = 99 * 60 + 59
        worst.speed = 10
        let bar = MiniPlayerBar(model: model, snapshot: worst, message: nil, theme: .paper) { _ in }
        let host = NSHostingView(rootView: bar)
        host.frame = CGRect(origin: .zero, size: MiniPlacement.size)
        host.layoutSubtreeIfNeeded()
        let ideal = host.fittingSize
        XCTAssertLessThanOrEqual(ideal.width, MiniPlacement.size.width, "déborde de \(ideal.width - MiniPlacement.size.width) pt")
        XCTAssertLessThanOrEqual(ideal.height, MiniPlacement.size.height)
        // Les textes fixes de la rangée du haut laissent 30 pt à la timeline.
        let fixed = HStack(spacing: 8) {
            Text("99:59").monospacedDigit().fixedSize()
            Text("-99:59").monospacedDigit().fixedSize()
        }.font(.system(size: 13))
        let room = MiniPlacement.size.width - 2 * MiniBarMetrics.margin - 16 - NSHostingView(rootView: fixed).fittingSize.width
        XCTAssertGreaterThanOrEqual(room, 30)
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
        let model = ReaderModel(reader: SillageReader.silent())
        for view in [AnyView(PlayButton(model: model)),
                     AnyView(TransportButton(item: SentenceTransport.next) { _ in })] {
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
        let model = ReaderModel(reader: SillageReader.silent())
        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        let bar = MiniPlayerBar(model: model, snapshot: model.snapshot,
                                message: "Aucune ligne de ce côté", theme: .paper) { _ in }
        let host = NSHostingView(rootView: bar)
        host.frame = CGRect(origin: .zero, size: MiniPlacement.size)
        host.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(host.fittingSize.height, MiniPlacement.size.height)
        XCTAssertLessThanOrEqual(host.fittingSize.width, MiniPlacement.size.width)
    }

    func testTheBarIs340By128() {
        XCTAssertEqual(MiniPlacement.size, CGSize(width: 340, height: 128))
    }

    /// Le titre de ce qui est lu ; sans document, le nom de l'app.
    func testTheMiniShowsWhatIsRead() {
        XCTAssertEqual(MiniTitle.text(hasDocument: true, title: "Le sillage"), "Le sillage")
        XCTAssertEqual(MiniTitle.text(hasDocument: true, title: "  "), "sillage")
        XCTAssertEqual(MiniTitle.text(hasDocument: false, title: "Le sillage"), "sillage")
    }

    /// Le mini avance par phrase, comme le lecteur, et s'agrandit.
    func testTheMiniMovesBySentenceAndGrows() {
        XCTAssertEqual(MiniControlsRow.transport.map(\.identifier), ["phrase-precedente", "phrase-suivante"])
        XCTAssertEqual(MiniControlsRow.transport.map(\.action),
                       [.command(.sentences(delta: -1)), .command(.sentences(delta: 1))])
        XCTAssertEqual(MiniControlsRow.growAction, .toggleSize)
        // « Coller » fait ce que fait ⌘V.
        XCTAssertEqual(MiniControlsRow.pasteAction, .paste)
        // « Fermer » fait ce que fait ⌘W : la lecture s'arrête.
        XCTAssertEqual(MiniPlayerBar.closeAction, .close)
        XCTAssertEqual(MiniPlayerBar.minimizeAction, .minimize, "le Dock, la lecture continue")
    }

    func testTheEmptyBarFitsItsHeight() {
        let model = ReaderModel(reader: SillageReader.silent())
        let host = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper) { _ in })
        host.frame = CGRect(origin: .zero, size: MiniPlacement.size)
        host.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(host.fittingSize.height, MiniPlacement.size.height)
    }
}
