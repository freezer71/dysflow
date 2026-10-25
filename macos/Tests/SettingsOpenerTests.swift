import XCTest
@testable import Dysflow

/// ⌘, ouvre les réglages par l'action SwiftUI `openSettings`, prise dans
/// une vue : une demande faite avant que la vue ne l'ait donnée attend.
@MainActor
final class SettingsOpenerTests: XCTestCase {
    func testOpensThroughTheCapturedActionAfterActivating() {
        var log: [String] = []
        let opener = SettingsOpener(activate: { log.append("active") }, bringToFront: {})
        opener.install { log.append("réglages") }
        opener.open()
        XCTAssertEqual(log, ["active", "réglages"])
    }

    /// L'action de l'icône de la barre des menus, donnée la première, reste.
    func testTheFirstInstalledActionIsKept() {
        var log: [String] = []
        let opener = SettingsOpener(activate: {}, bringToFront: {})
        opener.install { log.append("icône") }
        opener.install { log.append("autre vue") }
        opener.open()
        XCTAssertEqual(log, ["icône"])
    }

    func testARequestBeforeTheActionWaitsForIt() {
        var log: [String] = []
        let opener = SettingsOpener(activate: { log.append("active") }, bringToFront: {})
        opener.open()
        XCTAssertEqual(log, ["active"])
        opener.install { log.append("réglages") }
        XCTAssertEqual(log, ["active", "réglages"])
        // Servie une fois : une nouvelle vue qui redonne l'action n'ouvre rien.
        opener.install { log.append("réglages") }
        XCTAssertEqual(log, ["active", "réglages"])
    }
}

/// Ouvrir les réglages à une section : la liste y passe, puis la fenêtre
/// s'ouvre ; sans section, la dernière choisie reste.
@MainActor
final class SettingsOpenerSectionTests: XCTestCase {
    private func navigation() -> SettingsNavigation {
        let name = "dysflow.tests.reglages.ouverture"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return SettingsNavigation(defaults: defaults, dictationSupported: true)
    }

    func testOpeningAtASectionSelectsItBeforeOpening() {
        var log: [String] = []
        let navigation = navigation()
        let opener = SettingsOpener(activate: {}, bringToFront: {}, navigation: navigation)
        opener.install { log.append("réglages : \(navigation.selection.rawValue)") }
        opener.open(.dictee)
        XCTAssertEqual(log, ["réglages : dictee"])
        XCTAssertEqual(navigation.selection, .dictee)
    }

    func testOpeningWithoutASectionKeepsTheLastOne() {
        let navigation = navigation()
        navigation.select(.demarrage)
        let opener = SettingsOpener(activate: {}, bringToFront: {}, navigation: navigation)
        opener.install {}
        opener.open()
        XCTAssertEqual(navigation.selection, .demarrage)
    }

    /// Une demande faite avant que l'action n'existe garde sa section.
    func testAnEarlyRequestKeepsItsSection() {
        let navigation = navigation()
        let opener = SettingsOpener(activate: {}, bringToFront: {}, navigation: navigation)
        opener.open(.autorisations)
        var seen: SettingsSection?
        opener.install { seen = navigation.selection }
        XCTAssertEqual(seen, .autorisations)
    }
}
