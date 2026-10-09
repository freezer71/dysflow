import XCTest

/// Les quatre modes au clavier et l'audit du mode petit. Lancé à la main,
/// avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/DisplayModeUITests`.
final class DisplayModeUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 5))
        return app
    }

    /// Passe en mode petit et attend la barre, avec son contenu réel.
    private func showSmall(_ app: XCUIApplication) -> XCUIElement {
        app.typeKey("1", modifierFlags: .command)
        let small = app.groups["lecteur-reduit"]
        XCTAssertTrue(small.waitForExistence(timeout: 2), "⌘1 : pas de barre")
        XCTAssertTrue(app.descendants(matching: .any)["barre-mini"].waitForExistence(timeout: 2), "⌘1 : barre vide")
        XCTAssertTrue(app.buttons["lecture"].waitForExistence(timeout: 2))
        // Le passage moyen → petit est animé : on attend la taille finale
        // (SmallPlacement.size, 340 × 88, hors de portée de la cible de tests).
        let settled = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                abs(small.frame.width - 340) <= 2 && abs(small.frame.height - 88) <= 2
            }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 2), .completed,
                       "⌘1 : la barre n'a pas atteint 340 × 88 (\(small.frame.size))")
        return small
    }

    func testCommandDigitsSwitchModesAndEscapeHides() {
        let app = launch()
        _ = showSmall(app)
        for id in ["ligne-precedente", "mot-precedent", "mot-suivant", "ligne-suivante", "reglages-voix"] {
            XCTAssertTrue(app.buttons[id].exists, "\(id) absent de la barre")
        }
        // Un document est ouvert : la rangée des temps est là.
        XCTAssertTrue(app.staticTexts["temps-ecoule"].exists || app.descendants(matching: .any)["temps-ecoule"].exists)
        // Les touches de lecture restent actives dans la barre.
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Lecture"))

        app.typeKey("2", modifierFlags: .command)
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2), "⌘2 : pas de panneau")
        app.typeKey("3", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2), "⌘3 : pas de lecteur agrandi")

        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2), "Échap : d'abord le moyen")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForNonExistence(timeout: 2), "Échap : puis masqué")
    }

    func testEscapeFromTheSmallBarHides() {
        let app = launch()
        let small = showSmall(app)
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(small.waitForNonExistence(timeout: 2))
    }

    func testTheSmallBarPassesTheAudit() throws {
        let app = launch()
        let small = showSmall(app)
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: small)
    }
}
