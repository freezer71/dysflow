import XCTest

/// Audit d'accessibilité du lecteur et du mini. Écrit pour être lancé à la
/// main : `-only-testing:SillageUITests/AccessibilityAuditUITests`.
@MainActor
final class AccessibilityAuditUITests: XCTestCase {
    func testReaderAndMiniPassTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        try app.performAccessibilityAuditOutsideTouchBar()
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2))
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: mini)
    }
}
