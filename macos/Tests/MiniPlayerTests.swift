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
    private var inner: CGFloat { SmallPlacement.size.width - 2 * MiniBarMetrics.margin }

    /// Rangée 1, aux pires valeurs (« 99:59 », « -99:59 ») : la timeline
    /// garde au moins 30 pt.
    func testTheTimeRowLeavesRoomForTheTimeline() {
        let fixed = HStack(spacing: 8) {
            Text("99:59").monospacedDigit().fixedSize()
            Text("-99:59").monospacedDigit().fixedSize()
        }
        .font(.system(size: 13))
        let ideal = NSHostingView(rootView: fixed).fittingSize
        let room = inner - 8 - ideal.width - 8
        XCTAssertGreaterThanOrEqual(room, 30, "reste \(room) pt pour la timeline")
        let row = NSHostingView(rootView: MiniTimeRow(progress: 0.5, elapsed: "99:59", remaining: "-99:59",
                                                      spokenRemaining: "99:59", value: "50 %", theme: .paper)
            .font(.system(size: 13)))
        XCTAssertLessThanOrEqual(row.fittingSize.height, 20)
    }

    /// Rangée 2 : le transport tient entre vitesse et écrou (« 10,0× »), au
    /// centre, dans la largeur, et la bar entière dans ses 76 pt.
    func testTheControlsRowFitsAtTheWorstSpeed() {
        let model = ReaderModel(reader: SillonReader.silent())
        let speed = NSHostingView(rootView: SpeedLabel(speed: 10).fixedSize().font(.system(size: 13)))
        XCTAssertLessThanOrEqual(speed.fittingSize.width, MiniBarMetrics.side)
        let row = NSHostingView(rootView: MiniControlsRow(model: model, speed: 10) { _ in }
            .font(.system(size: 13)))
        let ideal = row.fittingSize
        XCTAssertLessThanOrEqual(ideal.width, inner, "la rangée déborde de \(ideal.width - inner) pt")
        let bar = ideal.height + 16 + 6 + 2 * 10
        XCTAssertLessThanOrEqual(bar, SmallPlacement.size.height)
    }

    func testTheBarIs320By76() {
        XCTAssertEqual(SmallPlacement.size, CGSize(width: 320, height: 76))
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
