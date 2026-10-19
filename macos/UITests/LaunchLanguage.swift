import XCTest

extension XCUIApplication {
    /// Lance l'app en français, quelle que soit la langue du Mac : les tests
    /// d'interface cherchent les libellés français (`Réglages`, `Lecture`…).
    /// Les arguments s'ajoutent à ceux du test.
    func launchInFrench() {
        launchArguments += ["-AppleLanguages", "(fr)", "-AppleLocale", "fr_FR"]
        launch()
    }
}
