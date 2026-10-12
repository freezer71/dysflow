import XCTest

/// La langue du document dans l'onglet Voix, et son audit. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/KokoroVoiceUITests`.
/// Le lecteur des tests est muet (pas de Kokoro) : la langue du document,
/// elle, vaut pour toutes les voix.
final class KokoroVoiceUITests: XCTestCase {
    func testTheDocumentLanguageIsShownAndChangeable() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        window.toolbars.buttons["Voix"].click()
        let picker = window.popUpButtons["langue-document"]
        XCTAssertTrue(picker.waitForExistence(timeout: 3), "langue du document absente")
        XCTAssertEqual(picker.value as? String, "Automatique (français)")
        picker.click()
        app.menuItems["Anglais"].click()
        XCTAssertTrue(picker.waitForValue("Anglais"))
        try app.performAccessibilityAuditOutsideTouchBar()
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
        window.toolbars.buttons["Voix"].click()
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
