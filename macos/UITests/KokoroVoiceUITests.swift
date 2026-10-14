import XCTest

/// L'onglet Voix simplifié (14 oct. 2026), et son audit. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/KokoroVoiceUITests`.
/// Le lecteur des tests est muet : `--ui-testing-kokoro` lui prête les
/// capacités de Kokoro (`UITestStaging`).
final class KokoroVoiceUITests: XCTestCase {
    private func voiceTab(_ arguments: [String]) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"] + arguments
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.buttons["Voix"].click()
        return (app, window)
    }

    /// Rien de ce qui a été retiré ne revient : modèle actif, liste de voix,
    /// voix clonée, expression, langue par défaut, réglages ignorés.
    private func assertNothingRemovedIsBack(in window: XCUIElement) {
        let any = window.descendants(matching: .any)
        for identifier in ["modele-actif", "liste-voix", "langue", "intonation", "ecouter"] {
            XCTAssertFalse(any[identifier].exists, "\(identifier) est revenu")
        }
        XCTAssertFalse(window.checkBoxes["Afficher aussi les réglages ignorés par ce modèle"].exists)
        XCTAssertFalse(window.buttons["Choisir un fichier WAV…"].exists)
        XCTAssertFalse(window.sliders["Hauteur"].exists)
        XCTAssertFalse(window.sliders["Énergie"].exists)
    }

    /// Kokoro : une voix et un « Écouter » par langue, la langue du
    /// document, la vitesse, le volume.
    func testKokoroShowsOneVoicePerLanguageAndTheDocumentLanguage() throws {
        continueAfterFailure = false
        let (app, window) = voiceTab(["--ui-testing-kokoro"])
        let french = window.popUpButtons["voix-francaise"]
        XCTAssertTrue(french.waitForExistence(timeout: 3), "voix française absente")
        XCTAssertEqual(french.value as? String, "p-fenrir60 · française, recommandée")
        let english = window.popUpButtons["voix-anglaise"]
        XCTAssertEqual(english.value as? String, "af_bella · américaine, femme, recommandée")
        english.click()
        XCTAssertEqual(app.menuItems.matching(NSPredicate(format: "title CONTAINS ' · américaine'")).count, 3)
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(window.buttons["ecouter-fr"].exists)
        XCTAssertTrue(window.buttons["ecouter-en"].exists)
        XCTAssertTrue(window.sliders["vitesse"].exists)
        XCTAssertTrue(window.sliders["Volume"].exists)
        assertNothingRemovedIsBack(in: window)
        let picker = window.popUpButtons["langue-document"]
        XCTAssertTrue(picker.waitForExistence(timeout: 3), "langue du document absente")
        XCTAssertEqual(picker.value as? String, "Automatique (français)")
        picker.click()
        app.menuItems["Anglais"].click()
        XCTAssertTrue(picker.waitForValue("Anglais"))
        try app.performAccessibilityAuditOutsideTouchBar()
    }

    /// Sans Kokoro (voix du système) : vitesse et volume, rien d'autre.
    func testWithoutKokoroOnlySpeedAndVolumeRemain() throws {
        continueAfterFailure = false
        let (app, window) = voiceTab([])
        XCTAssertTrue(window.sliders["vitesse"].waitForExistence(timeout: 3), "vitesse absente")
        XCTAssertTrue(window.sliders["Volume"].exists)
        XCTAssertEqual(window.sliders.count, 2)
        XCTAssertEqual(window.popUpButtons.count, 0, "ni voix ni langue sans Kokoro")
        assertNothingRemovedIsBack(in: window)
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }
}

private extension XCUIElement {
    func waitForValue(_ expected: String, timeout: TimeInterval = 3) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if value as? String == expected { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        return false
    }
}

/// L'offre, l'avis et le bandeau « Configuration illisible », posés par les
/// arguments de test (`UITestStaging`) : atteints au clavier, puis audités.
/// Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/KokoroNoticesUITests`.
final class KokoroNoticesUITests: XCTestCase {
    func testTheOfferNoticeAndBannerAreReachableAndAudited() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings", "--ui-testing-offre",
                               "--ui-testing-avis", "--ui-testing-config-illisible"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.buttons["Voix"].click()
        let any = window.descendants(matching: .any)
        XCTAssertTrue(any["configuration-illisible"].waitForExistence(timeout: 3), "bandeau absent")
        XCTAssertTrue(any["avis-voix"].exists, "avis absent")
        let accept = window.buttons["passer-nouvelle-voix"]
        XCTAssertTrue(accept.exists, "offre absente")
        XCTAssertEqual(accept.title, "Passer à la nouvelle voix (342 Mo)")
        XCTAssertTrue(window.buttons["plus-tard"].exists)
        XCTAssertTrue(window.buttons["remplacer-configuration"].exists)
        // Au clavier : Tab finit par donner le focus au bouton de l'offre
        // (exige « Navigation au clavier », comme SettingsUITests).
        let mode = UserDefaults.standard.persistentDomain(forName: UserDefaults.globalDomain)?["AppleKeyboardUIMode"] as? Int ?? 0
        if mode & 2 != 0 {
            var reached = false
            for _ in 0..<30 where !reached {
                window.typeKey(.tab, modifierFlags: [])
                reached = (accept.value(forKey: "hasKeyboardFocus") as? Bool) == true
            }
            XCTAssertTrue(reached, "l'offre n'est pas atteinte au clavier")
        }
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }
}
