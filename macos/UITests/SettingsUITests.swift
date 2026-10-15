import XCTest

/// La page des réglages, au clavier et sous l'audit. Lancé à la main, avec
/// l'accord de l'utilisateur : `-only-testing:SillonUITests/SettingsUITests`.
/// Le parcours au clavier exige « Navigation au clavier » (Réglages Système
/// › Clavier) et se saute sans elle ; l'audit tourne toujours.
@MainActor
final class SettingsUITests: XCTestCase {
    private let present = ["vitesse", "sortie-audio", "police-lecture", "taille-texte", "interlignage",
                           "regle-lecture", "raccourci-lecture-selection", "raccourci-afficher", "ouvrir-connexion"]
    private let removed = ["page-voix", "page-modeles", "page-avance", "page-general", "moteur", "garder-voix",
                           "blocs-code", "langue-document", "espace-libre", "offre-voix"]

    /// Une seule page : tout ce que la spec garde, rien de ce qu'elle retire ;
    /// l'audit en haut et en bas.
    func testEverythingFitsOnOnePageAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let any = window.descendants(matching: .any)
        for id in present { XCTAssertTrue(any[id].waitForExistence(timeout: 3), "\(id) absent") }
        for id in removed { XCTAssertFalse(any[id].exists, "\(id) est revenu") }
        XCTAssertFalse(window.buttons["Avancé"].exists, "plus de barre latérale")
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
        let page = window.scrollViews.firstMatch
        if page.exists {
            page.scroll(byDeltaX: 0, deltaY: -5000)
            try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
        }
    }

    /// La page se parcourt au clavier.
    func testThePageIsKeyboardReachable() throws {
        guard fullKeyboardAccess else {
            throw XCTSkip("Activer Réglages Système › Clavier › Navigation au clavier pour ce test")
        }
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        var seen = Set<String>()
        for _ in 0..<8 {
            window.typeKey("\t", modifierFlags: [])
            let element = focused(in: window)
            XCTAssertTrue(element.exists, "aucun contrôle n'a le focus")
            seen.insert(identity(element))
        }
        XCTAssertGreaterThan(seen.count, 4, "Tab ne parcourt pas la page")
    }

    /// ⌘, depuis le lecteur ouvre la page.
    func testCommandCommaFromTheReaderOpensTheSettings() throws {
        let app = launch(["--show-panel"])
        let reader = app.groups["lecteur"]
        XCTAssertTrue(reader.waitForExistence(timeout: 5))
        reader.typeKey(",", modifierFlags: .command)
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["page-reglages"].waitForExistence(timeout: 3))
    }

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

    /// Identité d'un contrôle : identifiant, libellé et cadre (pas la
    /// description de débogage, qui peut se répéter ou changer).
    private func identity(_ element: XCUIElement) -> String {
        "\(element.identifier)|\(element.label)|\(element.frame)"
    }

    /// Bit 2 de `AppleKeyboardUIMode` : « Navigation au clavier » active.
    private var fullKeyboardAccess: Bool {
        let mode = UserDefaults.standard.persistentDomain(forName: UserDefaults.globalDomain)?["AppleKeyboardUIMode"] as? Int ?? 0
        return mode & 2 != 0
    }
}
