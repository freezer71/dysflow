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
}
