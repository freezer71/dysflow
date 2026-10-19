import XCTest

/// L'app en anglais : lancée en anglais, ou dans une langue qu'elle ne
/// traduit pas (l'allemand), elle parle anglais, sans un mot de français.
/// Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillageUITests/EnglishUITests`.
@MainActor
final class EnglishUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Les sections des réglages, par leur identifiant ; la Dictée, sous
    /// macOS 26 seulement.
    private var sections: [String] {
        var all = ["voix", "affichage", "raccourcis", "autorisations", "demarrage"]
        if #available(macOS 26, *) { all.insert("dictee", at: 2) }
        return all
    }

    /// Lance l'app dans `language`, quelle que soit la langue du Mac (même
    /// mécanisme que `launchInFrench()` : le test lance l'app lui-même, les
    /// arguments du schéma ne s'y appliquent pas).
    private func launch(_ arguments: [String], language: String, locale: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"] + arguments + ["-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        app.launch()
        return app
    }

    /// Le bouton de lecture dit « Play » ou « Pause », pas « Lecture ».
    private func checkReader(language: String, locale: String) {
        let app = launch(["--show-panel"], language: language, locale: locale)
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5), "pas de lecteur")
        let play = app.buttons["lecture"]
        XCTAssertTrue(play.waitForExistence(timeout: 3), "pas de bouton de lecture")
        XCTAssertTrue(["Play", "Pause"].contains(play.label), "bouton de lecture : « \(play.label) »")
        app.terminate()
    }

    /// Aucun texte visible des réglages ne contient d'accent français ni
    /// « Réglages ». Le titre de la fenêtre est cherché par son identifiant
    /// (`Réglages`, gardé en français pour les tests).
    private func checkSettings(language: String, locale: String) {
        let app = launch(["--ui-testing-reglages-neufs", "--show-settings"], language: language, locale: locale)
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5), "pas de fenêtre des réglages")
        var seen = 0
        for section in sections {
            window.selectSettingsSection(section)
            for text in window.staticTexts.allElementsBoundByIndex where text.exists {
                let shown = [text.label, text.value as? String ?? ""]
                for string in shown {
                    seen += string.isEmpty ? 0 : 1
                    XCTAssertNil(string.rangeOfCharacter(from: CharacterSet(charactersIn: "éèà")),
                                 "texte français dans « \(section) » : « \(string) »")
                    XCTAssertFalse(string.contains("Réglages"), "« Réglages » dans « \(section) » : « \(string) »")
                }
            }
        }
        XCTAssertGreaterThan(seen, 0, "aucun texte lu dans les réglages")
        app.terminate()
    }

    func testReaderSpeaksEnglish() {
        checkReader(language: "en", locale: "en_US")
    }

    func testSettingsSpeakEnglish() {
        checkSettings(language: "en", locale: "en_US")
    }

    /// Une langue non traduite (l'allemand) tombe sur l'anglais.
    func testGermanFallsBackToEnglishInTheReader() {
        checkReader(language: "de", locale: "de_DE")
    }

    func testGermanFallsBackToEnglishInSettings() {
        checkSettings(language: "de", locale: "de_DE")
    }
}
