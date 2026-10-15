import XCTest

/// La fenêtre des réglages au clavier et sous l'audit d'accessibilité.
/// Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/SettingsUITests`. Le parcours au clavier
/// exige « Navigation au clavier » (Réglages Système › Clavier) et se saute
/// sans elle ; l'audit tourne toujours.
@MainActor
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

    /// Ce que chaque onglet doit montrer pour qu'on le dise chargé.
    private func waitForContent(of title: String, marker: String, in window: XCUIElement) {
        let any = window.descendants(matching: .any)
        XCTAssertTrue(any[marker].waitForExistence(timeout: 3), "\(title) : \(marker) absent")
        switch title {
        case "Voix":
            // Le lecteur muet n'a pas Kokoro : vitesse et volume seulement.
            XCTAssertTrue(any["vitesse"].waitForExistence(timeout: 3), "Voix : vitesse absente")
        case "Lecture":
            XCTAssertTrue(any["police-lecture"].waitForExistence(timeout: 3), "Lecture : police absente")
        case "Modèles":
            // Selon la compilation et les modèles installés : l'espace libre,
            // le message du catalogue vide, ou au moins une ligne de modèle.
            let empty = window.staticTexts["Aucun modèle proposé par cette version : la voix du système lit."]
            let deadline = Date().addingTimeInterval(3)
            while Date() < deadline, !(any["espace-libre"].exists || empty.exists || self.modelButtons(in: window) > 0) {
                RunLoop.current.run(until: Date().addingTimeInterval(0.1))
            }
            XCTAssertTrue(any["espace-libre"].exists || empty.exists || modelButtons(in: window) > 0,
                          "Modèles : ni espace libre, ni catalogue, ni message")
        default:
            break
        }
    }

    /// Boutons d'action des lignes de modèles (Installer, Utiliser, …).
    private func modelButtons(in window: XCUIElement) -> Int {
        window.buttons.matching(NSPredicate(
            format: "label BEGINSWITH 'Installer' OR label BEGINSWITH 'Utiliser' OR label BEGINSWITH 'Supprimer' OR label BEGINSWITH 'Annuler'"
        )).count
    }

    /// Chaque onglet s'ouvre, montre son contenu et passe l'audit, en haut
    /// puis en bas (un onglet long dépasse la fenêtre).
    func testEveryTabShowsItsContentAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        for (title, marker) in tabs {
            window.buttons[title].click()
            waitForContent(of: title, marker: marker, in: window)
            try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
            let form = window.scrollViews.firstMatch
            if form.exists {
                form.scroll(byDeltaX: 0, deltaY: -5000)
                try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
                form.scroll(byDeltaX: 0, deltaY: 5000)
            }
        }
    }

    /// Chaque onglet se parcourt au clavier.
    func testEveryTabIsKeyboardReachable() throws {
        guard fullKeyboardAccess else {
            throw XCTSkip("Activer Réglages Système › Clavier › Navigation au clavier pour ce test")
        }
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        for (title, marker) in tabs {
            window.buttons[title].click()
            waitForContent(of: title, marker: marker, in: window)
            // Le catalogue peut ne rien proposer : rien à parcourir alors.
            if title == "Modèles", modelButtons(in: window) == 0 { continue }
            var seen = Set<String>()
            for _ in 0..<6 {
                window.typeKey("\t", modifierFlags: [])
                let element = focused(in: window)
                guard element.exists else {
                    XCTFail("\(title) : aucun contrôle n'a le focus")
                    continue
                }
                seen.insert(identity(element))
            }
            XCTAssertGreaterThan(seen.count, 1, "\(title) : Tab ne parcourt pas l'onglet")
        }
    }

    /// L'écrou du panneau ouvre les réglages sur l'onglet Voix.
    func testTheGearOpensTheVoiceTab() throws {
        let app = launch(["--show-panel"])
        let panel = app.groups["lecteur"]
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
        let panel = app.groups["lecteur"]
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
        window.buttons["Avancé"].click()
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
        window.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(window.waitForNonExistence(timeout: 3))
        let panel = app.groups["lecteur"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        panel.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["onglet-avance"].waitForExistence(timeout: 3))
    }
}
