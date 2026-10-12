import XCTest

/// La langue du document dans l'onglet Voix, et son audit. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/KokoroVoiceUITests`.
/// Le lecteur des tests est muet (pas de Kokoro) : la langue du document,
/// elle, vaut pour toutes les voix.
final class KokoroVoiceUITests: XCTestCase {
    func testTheDocumentLanguageIsShownAndChangeable() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.toolbars.buttons["Voix"].click()
        let picker = window.popUpButtons["langue-document"]
        XCTAssertTrue(picker.waitForExistence(timeout: 3), "langue du document absente")
        XCTAssertEqual(picker.value as? String, "Automatique (français)")
        picker.click()
        app.menuItems["Anglais"].click()
        XCTAssertTrue(picker.waitForValue("Anglais"))
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}

private extension XCUIElement {
    func waitForValue(_ expected: String, timeout: TimeInterval = 3) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if value as? String == expected { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        return false
    }
}
