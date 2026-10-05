import XCTest

final class ExpandedUITests: XCTestCase {
    // La vue hôte SwiftUI se présente comme un groupe ; les touches vont à
    // l'app, donc à la fenêtre clé (voir PanelUITests).
    func testSidebarSearchAndBack() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: [])
        let reader = app.groups["lecteur-agrandi"]
        XCTAssertTrue(reader.waitForExistence(timeout: 2))
        let sommaire = app.outlines["sommaire"]
        XCTAssertTrue(sommaire.waitForExistence(timeout: 2))

        app.typeKey("t", modifierFlags: [])
        XCTAssertTrue(sommaire.waitForNonExistence(timeout: 2))

        app.typeKey("/", modifierFlags: [])
        let field = app.textFields["recherche"]
        XCTAssertTrue(field.waitForExistence(timeout: 2))
        field.typeText("promesses\r")
        XCTAssertTrue(app.staticTexts["resultats-recherche"].waitForExistence(timeout: 2))

        app.typeKey(.escape, modifierFlags: [])
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2))
    }

    /// Au clavier seul : Tab entre dans le sommaire, ↓ choisit le titre
    /// suivant, Entrée y va ; le fil d'Ariane le montre.
    func testSidebarKeyboardNavigation() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2))
        let crumb = app.staticTexts["fil-ariane"]
        XCTAssertTrue(crumb.waitForExistence(timeout: 2))
        XCTAssertEqual(crumb.value as? String ?? crumb.label, "Les monades")

        app.typeKey(.tab, modifierFlags: [])
        app.typeKey(.downArrow, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
        let predicate = NSPredicate(format: "value == %@ OR label == %@",
                                    "Les monades ▸ Exemples", "Les monades ▸ Exemples")
        let moved = XCTNSPredicateExpectation(predicate: predicate, object: crumb)
        XCTAssertEqual(XCTWaiter.wait(for: [moved], timeout: 2), .completed)
        // Toujours agrandi : Entrée est allée à la liste, pas au panneau.
        XCTAssertTrue(app.groups["lecteur-agrandi"].exists)
    }
}
