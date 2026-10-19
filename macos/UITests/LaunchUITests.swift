import XCTest

@MainActor
final class LaunchUITests: XCTestCase {
    func testAppLaunchesInTestMode() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"]
        app.launchInFrench()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5)
                      || app.wait(for: .runningBackground, timeout: 5))
    }

    /// Lancé à la main sans rien à lire : le lecteur, vide, qui invite à
    /// coller ou déposer.
    func testAManualLaunchOpensTheEmptyReader() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-vide", "--ui-testing-lancement-manuel"]
        app.launchInFrench()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5), "pas de lecteur")
        let invitation = app.staticTexts["invitation"]
        XCTAssertTrue(invitation.waitForExistence(timeout: 2))
        XCTAssertEqual(invitation.value as? String ?? invitation.label, "Collez un texte (⌘V) ou déposez un fichier")
        XCTAssertFalse(app.groups["lecteur-mini"].exists)
        XCTAssertFalse(app.buttons["bouton-modifier"].isEnabled, "rien à modifier")
    }
}
