import AppKit
import XCTest

/// Sélectionner et copier sans entrer en édition ; l'édition seulement par
/// le bouton Modifier. Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:DysflowUITests/SelectionUITests`. Le presse-papiers de
/// l'utilisateur est sauvé, puis rendu.
@MainActor
final class SelectionUITests: XCTestCase {
    private var saved: String?

    override func setUp() async throws {
        continueAfterFailure = false
        saved = NSPasteboard.general.string(forType: .string)
    }

    override func tearDown() async throws {
        NSPasteboard.general.clearContents()
        if let saved { NSPasteboard.general.setString(saved, forType: .string) }
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launchInFrench()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        return app
    }

    /// Tout sélectionner puis ⌘C : tout le texte, pas seulement la phrase ;
    /// aucune édition.
    func testSelectionAndCommandCCopyWithoutEditing() {
        let app = launch()
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 2))
        text.click()
        app.typeKey("a", modifierFlags: .command)
        NSPasteboard.general.clearContents()
        app.typeKey("c", modifierFlags: .command)
        let copied = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            (NSPasteboard.general.string(forType: .string) ?? "").contains("promesses")
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [copied], timeout: 2), .completed,
                       "copié : \(NSPasteboard.general.string(forType: .string) ?? "rien")")
        XCTAssertFalse(app.descendants(matching: .any)["barre-edition"].exists, "⌘C est entré en édition")
    }

    /// Le double clic sélectionne un mot ; il n'entre plus en édition.
    func testDoubleClickDoesNotEdit() {
        let app = launch()
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 2))
        text.doubleClick()
        XCTAssertFalse(app.descendants(matching: .any)["barre-edition"].waitForExistence(timeout: 1))
    }

    /// Le bouton Modifier ouvre l'édition ; Terminé la referme.
    func testTheEditButtonEdits() {
        let app = launch()
        app.buttons["bouton-modifier"].click()
        XCTAssertTrue(app.descendants(matching: .any)["barre-edition"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.textViews["edition-texte"].exists)
        app.buttons["edition-terminer"].click()
        XCTAssertTrue(app.descendants(matching: .any)["barre-edition"].waitForNonExistence(timeout: 2))
    }
}
