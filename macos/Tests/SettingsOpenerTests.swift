import XCTest
@testable import Sillon

/// ⌘, ouvre les réglages par l'action SwiftUI `openSettings`, prise dans
/// une vue : une demande faite avant que la vue ne l'ait donnée attend.
@MainActor
final class SettingsOpenerTests: XCTestCase {
    func testOpensThroughTheCapturedActionAfterActivating() {
        var log: [String] = []
        let opener = SettingsOpener(activate: { log.append("active") })
        opener.install { log.append("réglages") }
        opener.open()
        XCTAssertEqual(log, ["active", "réglages"])
    }

    /// L'action de l'icône de la barre des menus, donnée la première, reste.
    func testTheFirstInstalledActionIsKept() {
        var log: [String] = []
        let opener = SettingsOpener(activate: {})
        opener.install { log.append("icône") }
        opener.install { log.append("autre vue") }
        opener.open()
        XCTAssertEqual(log, ["icône"])
    }

    func testARequestBeforeTheActionWaitsForIt() {
        var log: [String] = []
        let opener = SettingsOpener(activate: { log.append("active") })
        opener.open()
        XCTAssertEqual(log, ["active"])
        opener.install { log.append("réglages") }
        XCTAssertEqual(log, ["active", "réglages"])
        // Servie une fois : une nouvelle vue qui redonne l'action n'ouvre rien.
        opener.install { log.append("réglages") }
        XCTAssertEqual(log, ["active", "réglages"])
    }
    /// Des préférences à part, vidées : les tests ne touchent pas à celles
    /// de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.reglages"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    /// L'écrou du panneau ouvre l'onglet Voix ; ⌘, rouvre le dernier.
    func testOpeningOnATabSelectsItAndPlainOpeningKeepsTheLast() {
        let nav = SettingsNavigation(defaults: freshDefaults())
        let opener = SettingsOpener(activate: {}, navigation: nav)
        opener.install {}
        nav.select(.advanced)
        opener.open()
        XCTAssertEqual(nav.tab, .advanced, "⌘, : le dernier onglet")
        opener.open(tab: .voice)
        XCTAssertEqual(nav.tab, .voice)
    }

    func testTheLastTabSurvivesARelaunch() {
        let defaults = freshDefaults()
        SettingsNavigation(defaults: defaults).select(.models)
        XCTAssertEqual(SettingsNavigation(defaults: defaults).tab, .models)
        defaults.set("inconnu", forKey: SettingsNavigation.key)
        XCTAssertEqual(SettingsNavigation(defaults: defaults).tab, .voice, "valeur illisible : l'onglet Voix")
    }

    /// Sous les tests (unitaires ou d'interface), l'onglet retenu ne va
    /// jamais dans les préférences réelles de l'utilisateur.
    func testUnderTestsTheTabIsNotStoredInTheUserDefaults() {
        let defaults = SettingsNavigation.processDefaults()
        XCTAssertFalse(defaults === UserDefaults.standard)
        let before = UserDefaults.standard.string(forKey: SettingsNavigation.key)
        let other: SettingsTab = before == SettingsTab.general.rawValue ? .shortcuts : .general
        SettingsNavigation(defaults: defaults).select(other)
        XCTAssertEqual(UserDefaults.standard.string(forKey: SettingsNavigation.key), before)
    }
}
