import XCTest

/// La section Voix de la page des réglages, et son audit. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/KokoroVoiceUITests`.
/// Le lecteur des tests est muet : `--ui-testing-kokoro` lui prête les
/// capacités de Kokoro (`UITestStaging`).
@MainActor
final class KokoroVoiceUITests: XCTestCase {
    private func settings(_ arguments: [String]) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"] + arguments
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        return (app, window)
    }

    /// Rien de ce qui a été retiré ne revient : modèle actif, liste de voix,
    /// voix clonée, expression, langue par défaut, langue du document,
    /// réglages ignorés.
    private func assertNothingRemovedIsBack(in window: XCUIElement) {
        let any = window.descendants(matching: .any)
        for identifier in ["modele-actif", "liste-voix", "langue", "intonation", "ecouter", "langue-document"] {
            XCTAssertFalse(any[identifier].exists, "\(identifier) est revenu")
        }
        XCTAssertFalse(window.checkBoxes["Afficher aussi les réglages ignorés par ce modèle"].exists)
        XCTAssertFalse(window.buttons["Choisir un fichier WAV…"].exists)
        XCTAssertFalse(window.sliders["Hauteur"].exists)
        XCTAssertFalse(window.sliders["Énergie"].exists)
        XCTAssertFalse(window.sliders["Volume"].exists)
    }

    /// Kokoro : une voix pour toutes les langues, son « Écouter », la vitesse.
    func testKokoroShowsOneVoiceForEveryLanguage() throws {
        continueAfterFailure = false
        let (app, window) = settings(["--ui-testing-kokoro"])
        let voice = window.popUpButtons["voix-unique"]
        XCTAssertTrue(voice.waitForExistence(timeout: 3), "voix absente")
        voice.click()
        XCTAssertEqual(app.menuItems.matching(NSPredicate(format: "title CONTAINS ' · '")).count, 4,
                       "toutes les voix du modèle, quelle que soit leur langue")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(window.buttons["ecouter-voix"].exists)
        XCTAssertFalse(window.popUpButtons["voix-francaise"].exists, "plus de voix par langue")
        XCTAssertFalse(window.popUpButtons["voix-anglaise"].exists)
        XCTAssertTrue(window.sliders["vitesse"].exists)
        assertNothingRemovedIsBack(in: window)
        try app.performAccessibilityAuditOutsideTouchBar()
    }

    /// Sans Kokoro : la vitesse et la sortie audio, jamais de voix par langue.
    func testWithoutKokoroThereIsNoVoicePerLanguage() throws {
        continueAfterFailure = false
        let (app, window) = settings([])
        XCTAssertTrue(window.sliders["vitesse"].waitForExistence(timeout: 3), "vitesse absente")
        XCTAssertTrue(window.popUpButtons["sortie-audio"].exists, "sortie audio absente")
        XCTAssertFalse(window.popUpButtons["voix-francaise"].exists, "pas de voix par langue")
        XCTAssertFalse(window.popUpButtons["voix-anglaise"].exists)
        XCTAssertFalse(window.buttons["ecouter-fr"].exists)
        assertNothingRemovedIsBack(in: window)
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }

    /// config.toml illisible : le bandeau est atteignable et passe l'audit.
    func testTheUnreadableConfigBannerIsAudited() throws {
        let (app, window) = settings(["--ui-testing-config-illisible"])
        XCTAssertTrue(window.descendants(matching: .any)["configuration-illisible"].waitForExistence(timeout: 3))
        XCTAssertTrue(window.buttons["remplacer-configuration"].exists)
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }
}
