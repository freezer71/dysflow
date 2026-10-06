import XCTest

/// La fenêtre des réglages au clavier et sous l'audit d'accessibilité.
/// Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/SettingsUITests`. Prérequis : « Navigation
/// au clavier » activée (Réglages Système › Clavier), sans quoi Tab ne
/// s'arrête que sur les champs de texte.
final class SettingsUITests: XCTestCase {
    private let tabs = [
        ("Voix", "onglet-voix"), ("Lecture", "onglet-lecture"), ("Modèles", "onglet-modeles"),
        ("Raccourcis", "onglet-raccourcis"), ("Avancé", "onglet-avance"), ("Général", "onglet-general"),
    ]

    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"] + arguments
        app.launch()
        return app
    }

    private func focused(in window: XCUIElement) -> XCUIElement {
        window.descendants(matching: .any)
            .matching(NSPredicate(format: "hasKeyboardFocus == true")).firstMatch
    }

    /// Chaque onglet s'ouvre, se parcourt au clavier et passe l'audit.
    func testEveryTabIsKeyboardReachableAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        for (title, marker) in tabs {
            window.toolbars.buttons[title].click()
            XCTAssertTrue(window.descendants(matching: .any)[marker].waitForExistence(timeout: 3), title)
            var seen = Set<String>()
            for _ in 0..<6 {
                window.typeKey("\t", modifierFlags: [])
                let element = focused(in: window)
                XCTAssertTrue(element.exists, "\(title) : aucun contrôle n'a le focus")
                seen.insert(element.debugDescription)
            }
            XCTAssertGreaterThan(seen.count, 1, "\(title) : Tab ne parcourt pas l'onglet")
            try app.performAccessibilityAuditOutsideTouchBar()
        }
    }

    /// L'écrou du panneau ouvre les réglages sur l'onglet Voix.
    func testTheGearOpensTheVoiceTab() throws {
        let app = launch(["--show-panel"])
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        let gear = panel.buttons["reglages-voix"]
        XCTAssertTrue(gear.exists)
        XCTAssertEqual(gear.label, "Réglages de la voix")
        gear.click()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-voix"].waitForExistence(timeout: 3))
    }

    /// `v` dans le panneau fait comme l'écrou.
    func testVFromThePanelOpensTheVoiceTab() throws {
        let app = launch(["--show-panel"])
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey("v", modifierFlags: [])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-voix"].waitForExistence(timeout: 3))
    }

    /// ⌘, depuis le panneau rouvre le dernier onglet utilisé.
    func testCommandCommaReopensTheLastTab() throws {
        let app = launch(["--show-panel", "--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.toolbars.buttons["Avancé"].click()
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
        window.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(window.waitForNonExistence(timeout: 3))
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
    }
}
