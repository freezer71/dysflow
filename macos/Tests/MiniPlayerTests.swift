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
    /// Les éléments fixes de la barre, aux pires valeurs (« -99:59 »,
    /// « 10,0× »), laissent à la timeline au moins 30 pt, et la barre
    /// tient dans ses 44 pt.
    func testTheFixedElementsLeaveRoomForTheTimeline() {
        let model = ReaderModel(reader: SillonReader.silent())
        let fixed = HStack(spacing: 10) {
            PlayButton(model: model)
            Text(PlaybackTime.shortRemaining(elapsed: 0, total: 99 * 60 + 59))
                .monospacedDigit().fontWeight(.medium).fixedSize()
            SpeedLabel(speed: 10).fixedSize()
            VoiceSettingsButton()
        }
        .font(.system(size: 13))
        let host = NSHostingView(rootView: fixed)
        let ideal = host.fittingSize
        // Marge de 14 pt de chaque côté, et un espacement de plus pour la timeline.
        let room = SmallPlacement.size.width - 2 * 14 - 10 - ideal.width
        XCTAssertGreaterThanOrEqual(room, 30, "reste \(room) pt pour la timeline")
        XCTAssertLessThanOrEqual(ideal.height, SmallPlacement.size.height)
    }

    func testTheEmptyBarFitsItsHeight() {
        let model = ReaderModel(reader: SillonReader.silent())
        let host = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper))
        host.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        host.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(host.fittingSize.height, SmallPlacement.size.height)
    }
}
