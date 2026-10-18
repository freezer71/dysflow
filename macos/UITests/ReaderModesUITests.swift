import XCTest

/// Le lecteur et le mini, fermer, rouvrir. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:SillageUITests/ReaderModesUITests`.
@MainActor
final class ReaderModesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        return app
    }

    /// Rouvre par le menu de la barre des menus.
    private func reopen(_ app: XCUIApplication) {
        app.statusItems.firstMatch.click()
        let item = app.menuItems["Afficher le lecteur"]
        XCTAssertTrue(item.waitForExistence(timeout: 2))
        item.click()
    }

    /// ⌘M : le mini, à sa taille ; ⌘M : le lecteur ; la lecture continue.
    func testCommandMSwitchesBetweenReaderAndMini() {
        let app = launch()
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2), "⌘M : pas de mini")
        let settled = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            abs(mini.frame.width - 340) <= 2 && abs(mini.frame.height - 108) <= 2
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 2), .completed, "taille du mini : \(mini.frame.size)")
        XCTAssertEqual(app.buttons["lecture"].label, "Pause", "réduire ne met pas en pause")
        app.typeKey("m", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2), "⌘M : pas de lecteur")
        XCTAssertEqual(app.buttons["lecture"].label, "Pause")
        app.typeKey(" ", modifierFlags: [])
    }

    /// ⌘W : plus rien à l'écran, ni lecteur ni mini ; rouvert, en pause.
    func testCommandWClosesAndPausesWithoutMini() {
        let app = launch()
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur"].waitForNonExistence(timeout: 2))
        XCTAssertFalse(app.groups["lecteur-mini"].exists, "fermer ne laisse jamais le mini")
        reopen(app)
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Lecture"), "la lecture s'est arrêtée")
    }

    /// Fermé en mini, rouvert en mini.
    func testReopeningGivesTheLastSize() {
        let app = launch()
        app.typeKey("m", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur-mini"].waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-mini"].waitForNonExistence(timeout: 2))
        reopen(app)
        XCTAssertTrue(app.groups["lecteur-mini"].waitForExistence(timeout: 2), "pas rouvert en mini")
    }

    /// Le mini : ⏮ ▶ ⏭ par phrase et « Agrandir », rien d'autre.
    func testTheMiniMovesBySentenceAndGrows() {
        let app = launch()
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2))
        for id in ["phrase-precedente", "lecture", "phrase-suivante", "agrandir"] {
            XCTAssertTrue(mini.buttons[id].exists, "\(id) absent du mini")
        }
        for id in ["mot-precedent", "mot-suivant", "ligne-precedente", "ligne-suivante", "reglages-voix", "sortie-audio"] {
            XCTAssertFalse(mini.descendants(matching: .any)[id].exists, "\(id) est revenu")
        }
        mini.buttons["agrandir"].click()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2))
    }
}
