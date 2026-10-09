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
