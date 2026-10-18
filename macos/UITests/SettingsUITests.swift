import XCTest

/// La fenêtre des réglages : la liste des sections à gauche, la section
/// choisie à droite ; au clavier et sous l'audit. Lancé à la main, avec
/// l'accord de l'utilisateur : `-only-testing:SillageUITests/SettingsUITests`.
/// Le parcours au clavier exige « Navigation au clavier » (Réglages Système
/// › Clavier) et se saute sans elle ; l'audit tourne toujours.
@MainActor
final class SettingsUITests: XCTestCase {
    /// Ce que chaque section garde ; la Dictée, sous macOS 26 seulement.
    private let present: [(section: String, ids: [String])] = [
        ("voix", ["vitesse", "sortie-audio", "place-modeles"]),
        ("affichage", ["police-lecture", "taille-texte", "interlignage", "regle-lecture"]),
        ("raccourcis", ["raccourci-lecture-selection", "raccourci-afficher"]),
        ("autorisations", []),
        ("demarrage", ["ouvrir-connexion"]),
    ]
    private let removed = ["page-voix", "page-modeles", "page-avance", "page-general", "moteur", "garder-voix",
                           "blocs-code", "langue-document", "espace-libre", "offre-voix"]

    /// Chaque section a sa ligne et son volet, et seulement les siens ; rien
    /// de ce que la spec retire ; l'audit sur chaque volet, en haut et en bas.
    func testEverySectionHasItsPaneAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let any = window.descendants(matching: .any)
        XCTAssertTrue(any["page-reglages"].waitForExistence(timeout: 3))
        XCTAssertTrue(any["sections-reglages"].exists, "liste des sections absente")
        var sections = present
        if #available(macOS 26, *) {
            sections.insert(("dictee", ["raccourci-dictee", "langue-dictee"]), at: 2)
        } else {
            XCTAssertFalse(any["section-dictee"].exists, "pas de Dictée avant macOS 26")
        }
        let others = Set(sections.flatMap(\.ids))
        for (section, ids) in sections {
            window.selectSettingsSection(section)
            for id in ids { XCTAssertTrue(any[id].waitForExistence(timeout: 3), "\(id) absent de \(section)") }
            for id in others.subtracting(ids) { XCTAssertFalse(any[id].exists, "\(id) affiché dans \(section)") }
            for id in removed { XCTAssertFalse(any[id].exists, "\(id) est revenu") }
            try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
            let pane = any["volet-\(section)"].scrollViews.firstMatch
            XCTAssertTrue(pane.exists, "le volet \(section) ne défile pas : l'audit du bas serait sauté")
            pane.scroll(byDeltaX: 0, deltaY: -5000)
            try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
        }
        XCTAssertFalse(window.buttons["Avancé"].exists, "plus d'onglet Avancé")
    }

    /// ↓ et ↑ changent de section dans la liste.
    func testArrowKeysChangeTheSection() {
        let app = launch(["--show-settings", "--section-reglages=voix"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.selectSettingsSection("voix")
        window.typeKey(.downArrow, modifierFlags: [])
        XCTAssertTrue(window.descendants(matching: .any)["volet-affichage"].waitForExistence(timeout: 3))
        window.typeKey(.upArrow, modifierFlags: [])
        XCTAssertTrue(window.descendants(matching: .any)["volet-voix"].waitForExistence(timeout: 3))
    }

    /// La dernière section choisie revient au lancement suivant.
    func testTheLastSectionIsRememberedAcrossLaunches() {
        var app = launch(["--show-settings"])
        var window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.selectSettingsSection("raccourcis")
        app.terminate()
        // Sans `--ui-testing-reglages-neufs` : la section gardée reste.
        app = launch(["--show-settings"], keepingSection: true)
        window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["volet-raccourcis"].waitForExistence(timeout: 3))
        XCTAssertTrue(window.descendants(matching: .any)["raccourci-afficher"].exists)
    }

    /// `--section-reglages=` ouvre à la section demandée (`SettingsOpener.open(_:)`).
    func testOpeningAtARequestedSection() {
        let app = launch(["--show-settings", "--section-reglages=demarrage"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["volet-demarrage"].waitForExistence(timeout: 3))
        XCTAssertTrue(window.descendants(matching: .any)["ouvrir-connexion"].exists)
    }

    /// La liste et le volet se parcourent au clavier.
    func testThePageIsKeyboardReachable() throws {
        guard fullKeyboardAccess else {
            throw XCTSkip("Activer Réglages Système › Clavier › Navigation au clavier pour ce test")
        }
        let app = launch(["--show-settings", "--section-reglages=affichage"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        var seen = Set<String>()
        for _ in 0..<8 {
            window.typeKey("\t", modifierFlags: [])
            let element = focused(in: window)
            XCTAssertTrue(element.exists, "aucun contrôle n'a le focus")
            seen.insert(identity(element))
        }
        XCTAssertGreaterThan(seen.count, 4, "Tab ne parcourt pas la fenêtre")
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

    /// Chaque test part de réglages neufs (la section gardée par les tests,
    /// dans leur suite à part, est oubliée) ; `keepingSection` la garde.
    private func launch(_ arguments: [String], keepingSection: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"] + (keepingSection ? [] : ["--ui-testing-reglages-neufs"]) + arguments
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
