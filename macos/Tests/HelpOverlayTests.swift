import XCTest
@testable import Sillon

@MainActor
final class HelpOverlayTests: XCTestCase {
    func testTheHelpListsTheDisplayModes() {
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘1 ⌘2 ⌘3", "Petit · moyen · grand") })
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("Échap", "Revenir · arrière-plan") })
        XCTAssertFalse(HelpOverlay.rows.contains { $0.1 == "Revenir · masquer" })
    }
}

@MainActor
final class HelpOverlayKeysTests: XCTestCase {
    func testTheHelpedCommandDigitsAreTheOnesTheKeyMapHandles() {
        let help = HelpOverlay.rows.first { $0.1 == "Petit · moyen · grand" }?.0
        XCTAssertEqual(help, "⌘1 ⌘2 ⌘3")
        let modes: [(String, DisplayMode)] = [("1", .small), ("2", .medium), ("3", .large)]
        for (digit, mode) in modes {
            XCTAssertEqual(KeyMap.action(for: KeyPress(characters: digit, command: true)), .display(mode))
        }
    }
}

@MainActor
final class MenuOpeningTests: XCTestCase {
    func testAMenuOpeningRefreshesOncePerOpening() {
        let center = NotificationCenter()
        var refreshes = 0
        let token = AppDelegate.refreshWhenAMenuOpens(center: center) { refreshes += 1 }
        defer { center.removeObserver(token) }
        XCTAssertEqual(refreshes, 0)
        center.post(name: NSMenu.didBeginTrackingNotification, object: nil)
        XCTAssertEqual(refreshes, 1)
        center.post(name: NSMenu.didBeginTrackingNotification, object: nil)
        XCTAssertEqual(refreshes, 2)
    }
}
