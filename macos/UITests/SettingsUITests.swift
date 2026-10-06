import XCTest

final class SettingsUITests: XCTestCase {
    func testSettingsReachableAndKeyboardNavigable() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        // La fenêtre rouvre le dernier onglet choisi : on va sur chacun.
        window.toolbars.buttons["Lecture"].click()
        XCTAssertTrue(window.popUpButtons["police-lecture"].waitForExistence(timeout: 2))
        window.typeKey("\t", modifierFlags: [])
        try app.performAccessibilityAuditOutsideTouchBar()
        window.toolbars.buttons["Modèles"].click()
        XCTAssertTrue(window.descendants(matching: .any)["onglet-modeles"].waitForExistence(timeout: 2))
        // Le catalogue (ou, sans audio.cpp, son message) et l'espace libre se lisent.
        XCTAssertTrue(window.descendants(matching: .any)["espace-libre"].waitForExistence(timeout: 2)
                      || window.staticTexts["Aucun modèle proposé : cette version est compilée sans audio.cpp."].exists
                      || window.buttons["Annuler"].exists || window.staticTexts["Actif"].exists)
        try app.performAccessibilityAuditOutsideTouchBar()
        window.toolbars.buttons["Voix"].click()
        XCTAssertTrue(window.popUpButtons["modele-actif"].waitForExistence(timeout: 2))
        XCTAssertTrue(window.buttons["ecouter"].exists)
        try app.performAccessibilityAuditOutsideTouchBar()
    }

    /// ⌘, depuis le panneau déjà affiché : la fenêtre des réglages paraît.
    func testCommandCommaFromThePanelOpensSettings() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey(",", modifierFlags: .command)
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.toolbars.buttons["Lecture"].exists)
    }
}
