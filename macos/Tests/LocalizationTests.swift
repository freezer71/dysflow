import XCTest
@testable import Dysflow

/// Les tests tournent en français (`-AppleLanguages (fr)` du schéma) : le
/// catalogue donne le français, et l'app l'a dit au moteur au lancement.
final class LocalizationTests: XCTestCase {
    func testTheTestHostRunsInFrench() {
        XCTAssertEqual(Bundle.main.preferredLocalizations.first, "fr")
    }

    /// La clé est l'anglais du code ; le français vient du catalogue.
    func testTheCatalogueGivesTheFrenchText() {
        XCTAssertEqual(String(localized: "Play"), "Lecture")
        XCTAssertEqual(String(localized: "Pause"), "Pause")
    }

    /// `AppDelegate` a donné la langue de l'app au moteur avant de le créer :
    /// la voix du système porte son nom français.
    func testTheEngineSpeaksTheLanguageOfTheApp() {
        let system = DysflowReader.silent().voices().first(where: \.system)
        XCTAssertEqual(system?.name, "Voix de macOS")
    }
}
