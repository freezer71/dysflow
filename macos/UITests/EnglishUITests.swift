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

    /// Les traductions françaises du catalogue : texte français → clé anglaise.
    /// Un nom propre (une voix du système comme « Amélie », un modèle) n'y est
    /// pas, il ne fait donc jamais échouer le test. Si le catalogue ne se lit
    /// pas, on retombe sur les lettres accentuées.
    private static let frenchTexts: [String: String]? = {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().appendingPathComponent("Resources/Localizable.xcstrings")
        guard let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let strings = root["strings"] as? [String: [String: Any]] else { return nil }
        var texts: [String: String] = [:]
        for (key, entry) in strings {
            let localizations = entry["localizations"] as? [String: Any]
            let french = (localizations?["fr"] as? [String: Any])?["stringUnit"] as? [String: Any]
            if let value = french?["value"] as? String, value != key { texts[value] = key }
        }
        return texts
    }()

    /// Ce texte est-il du français d'interface (et non un nom propre) ?
    private func isFrench(_ string: String) -> Bool {
        if string.contains("Réglages") { return true }
        guard let texts = Self.frenchTexts else {
            return string.rangeOfCharacter(from: CharacterSet(charactersIn: "éèà")) != nil
        }
        return texts[string] != nil
    }

    /// Aucun texte visible des réglages n'est une traduction française du
    /// catalogue, ni ne contient « Réglages ». Le titre de la fenêtre est
    /// cherché par son identifiant (`Réglages`, gardé en français pour les
    /// tests).
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
                    XCTAssertFalse(isFrench(string), "texte français dans « \(section) » : « \(string) »")
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
