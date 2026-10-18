import XCTest

/// La ligne d'état de la voix : rien quand elle va bien ; l'échec et
/// « Réessayer » dans le lecteur et en bas des réglages. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillageUITests/VoiceStatusUITests`.
@MainActor
final class VoiceStatusUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testNothingIsSaidWhenTheVoiceIsFine() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["etat-voix"].exists)
    }

    func testAFailedInstallationOffersToRetryInTheReaderAndTheSettings() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel", "--show-settings", "--ui-testing-echec-voix"]
        app.launch()
        let reader = app.groups["lecteur"]
        XCTAssertTrue(reader.waitForExistence(timeout: 5))
        XCTAssertTrue(reader.buttons["reessayer-voix"].waitForExistence(timeout: 2), "pas de Réessayer dans le lecteur")
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.buttons["reessayer-voix"].waitForExistence(timeout: 2), "pas de Réessayer dans les réglages")
        // Visible quelle que soit la section.
        window.selectSettingsSection("demarrage")
        XCTAssertTrue(window.buttons["reessayer-voix"].exists, "Réessayer disparaît hors de la section Voix")
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }

    func testTheDownloadShowsItsProgressAndPassesTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel", "--ui-testing-telechargement"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        let line = app.staticTexts["etat-voix"]
        XCTAssertTrue(line.waitForExistence(timeout: 2))
        XCTAssertTrue((line.value as? String ?? line.label).hasPrefix("Installation de la voix"))
        XCTAssertFalse(app.buttons["reessayer-voix"].exists)
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}
