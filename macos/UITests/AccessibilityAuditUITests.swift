import XCTest

/// Audit d'accessibilité du panneau et du lecteur agrandi. Écrit pour être
/// lancé à la main : `-only-testing:SillonUITests/AccessibilityAuditUITests`.
final class AccessibilityAuditUITests: XCTestCase {
    func testPanelAndExpandedPassTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        try app.performAccessibilityAuditOutsideTouchBar()
        panel.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2))
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}
