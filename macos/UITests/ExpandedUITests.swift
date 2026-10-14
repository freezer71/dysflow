import XCTest

@MainActor
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

    /// Après une recherche, le focus quitte le champ (Espace lit, N avance) ;
    /// « / » y revient ; Échap ferme et vide le champ.
    func testSearchAgainAndNextResult() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 5))
        // Depuis le panneau, « / » agrandit puis ouvre le champ.
        app.typeKey("/", modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2), "« / » n'a pas agrandi")
        let field = app.textFields["recherche"]
        XCTAssertTrue(field.waitForExistence(timeout: 2), "pas de champ")
        field.typeText("promesses\r")
        let results = app.staticTexts["resultats-recherche"]
        XCTAssertTrue(results.waitForExistence(timeout: 2))

        // Le champ n'a plus le focus : Espace lance la lecture.
        let unfocused = NSPredicate(format: "hasKeyboardFocus == false")
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: unfocused, object: field)],
                                      timeout: 2), .completed)
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Lecture"))

        // « / » rend le focus au champ : ce qu'on tape y va.
        app.typeKey("/", modifierFlags: [])
        app.typeKey("a", modifierFlags: .command)
        app.typeText("monade\r")
        // La recherche part de la phrase lue : trois résultats, peu importe
        // lequel est le premier.
        XCTAssertTrue(results.waitForText { $0.hasSuffix(" / 3") },
                      "la nouvelle recherche n'a pas eu lieu : \(results.label) champ=\(field.value ?? "")")
        let before = (results.value as? String) ?? results.label
        // N passe au résultat suivant.
        app.typeKey("n", modifierFlags: .shift)
        XCTAssertTrue(results.waitForText { $0 != before && $0.hasSuffix(" / 3") })

        // Échap ferme la recherche ; rouverte, le champ est vide.
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(field.waitForNonExistence(timeout: 2))
        app.typeKey("/", modifierFlags: [])
        XCTAssertTrue(field.waitForExistence(timeout: 2))
        XCTAssertEqual(field.value as? String ?? "", "")
    }

    /// Le sommaire suit la section lue tant qu'on ne le parcourt pas.
    func testSidebarFollowsTheCurrentSection() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: [])
        let outline = app.outlines["sommaire"]
        XCTAssertTrue(outline.waitForExistence(timeout: 2))
        let first = outline.outlineRows.element(boundBy: 0)
        let second = outline.outlineRows.element(boundBy: 1)
        XCTAssertTrue(first.waitForSelected(true))

        app.typeKey("n", modifierFlags: [])
        XCTAssertTrue(second.waitForSelected(true), "la ligne ne suit pas la section")
        XCTAssertFalse(first.isSelected)
    }
}

@MainActor
extension XCUIElement {
    func waitForText(timeout: TimeInterval = 2, _ ok: @escaping (String) -> Bool) -> Bool {
        let predicate = NSPredicate { object, _ in
            guard let e = object as? XCUIElement else { return false }
            let text = (e.value as? String).flatMap { $0.isEmpty ? nil : $0 } ?? e.label
            return ok(text)
        }
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [exp], timeout: timeout) == .completed
    }

    func waitForSelected(_ selected: Bool, timeout: TimeInterval = 2) -> Bool {
        let predicate = NSPredicate(format: "isSelected == %@", NSNumber(value: selected))
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [exp], timeout: timeout) == .completed
    }
}
