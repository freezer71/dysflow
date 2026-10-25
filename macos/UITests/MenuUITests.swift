import XCTest

/// Le menu de la barre des menus et ⌃⌥S. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:DysflowUITests/MenuUITests`. Quitter
/// d'abord le dysflow installé : il tiendrait ⌃⌥S et aurait aussi une icône.
@MainActor
final class MenuUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testTheMenuHasFiveLines() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"]
        app.launchInFrench()
        app.statusItems.firstMatch.click()
        let titles = app.menuItems.allElementsBoundByIndex.map(\.title).filter { !$0.isEmpty }
            .filter { $0 != "Autoriser la lecture de la sélection…" }
        XCTAssertEqual(titles, ["Lire la sélection", "Ouvrir un fichier…", "Afficher le lecteur",
                                "Réglages…", "Quitter Dysflow"])
        app.typeKey(.escape, modifierFlags: [])
    }

    /// ⌃⌥S affiche le lecteur, puis le masque.
    func testControlOptionSShowsAndHidesTheReader() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-raccourcis"]
        app.launchInFrench()
        XCTAssertFalse(app.groups["lecteur"].waitForExistence(timeout: 1), "rien au lancement")
        app.typeKey("s", modifierFlags: [.control, .option])
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 3), "⌃⌥S n'a rien affiché")
        app.typeKey("s", modifierFlags: [.control, .option])
        XCTAssertTrue(app.groups["lecteur"].waitForNonExistence(timeout: 3), "⌃⌥S n'a pas masqué")
    }
}
