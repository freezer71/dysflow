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
    /// La barre tient dans ses 44 pt, avec ou sans document.
    func testTheBarFitsInItsHeight() {
        let model = ReaderModel(reader: SillonReader.silent())
        let empty = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper))
        empty.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        empty.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(empty.fittingSize.height, SmallPlacement.size.height)

        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        let full = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper))
        full.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        full.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(full.fittingSize.height, SmallPlacement.size.height)
    }
}
