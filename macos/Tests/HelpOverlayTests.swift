import XCTest
@testable import Sillage

/// L'aide liste toutes les touches du lecteur, et chacune fait quelque chose.
@MainActor
final class HelpOverlayTests: XCTestCase {
    private let presses: [(String, [KeyPress])] = [
        ("Espace", [KeyPress(characters: " ", keyCode: KeyCode.space)]),
        ("← →", [KeyPress(characters: "", keyCode: KeyCode.left), KeyPress(characters: "", keyCode: KeyCode.right)]),
        ("↑ ↓", [KeyPress(characters: "", keyCode: KeyCode.up), KeyPress(characters: "", keyCode: KeyCode.down)]),
        ("Début", [KeyPress(characters: "", keyCode: KeyCode.home)]),
        ("n p", [KeyPress(characters: "n"), KeyPress(characters: "p")]),
        ("/ ⌘F", [KeyPress(characters: "/"), KeyPress(characters: "f", command: true)]),
        ("N P", [KeyPress(characters: "N", shift: true), KeyPress(characters: "P", shift: true)]),
        ("t", [KeyPress(characters: "t")]),
        ("e", [KeyPress(characters: "e")]),
        ("+ −", [KeyPress(characters: "+", shift: true), KeyPress(characters: "-")]),
        ("⌘C", [KeyPress(characters: "c", command: true)]),
        ("⌘V", [KeyPress(characters: "v", command: true)]),
        ("⌘O", [KeyPress(characters: "o", command: true)]),
        ("⌘M", [KeyPress(characters: "m", command: true)]),
        ("⌘W", [KeyPress(characters: "w", command: true)]),
        ("Échap", [KeyPress(characters: "", keyCode: KeyCode.escape)]),
        ("?", [KeyPress(characters: "?", shift: true)]),
        ("⌘,", [KeyPress(characters: ",", command: true)]),
    ]

    func testTheHelpListsEveryKeyAndEachOneWorks() {
        XCTAssertEqual(HelpOverlay.rows.map(\.0), presses.map(\.0))
        for (row, keys) in presses {
            for key in keys { XCTAssertNotNil(KeyMap.action(for: key), "\(row) : sans action") }
        }
    }

    /// Le pied de l'aide dit vrai : seules « ? » et Échap la ferment, les
    /// autres touches agissent dessous.
    func testTheFooterNamesTheKeysThatCloseTheHelp() {
        XCTAssertEqual(HelpOverlay.footer, "? ou Échap pour fermer")
        guard case .showHelp? = KeyMap.action(for: KeyPress(characters: "?", shift: true)) else {
            return XCTFail("« ? » ne bascule pas l'aide")
        }
        guard case .escape? = KeyMap.action(for: KeyPress(characters: "", keyCode: KeyCode.escape)) else {
            return XCTFail("Échap ne ferme pas l'aide")
        }
    }

    func testTheHelpSaysThatClosingStopsTheReading() {
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘W", "Fermer (la lecture s'arrête)") })
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘C", "Copier la sélection, sinon la phrase") })
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("Début", "Revenir au début") })
    }
}

@MainActor
final class MenuOpeningTests: XCTestCase {
    func testAMenuOpeningRefreshesOncePerOpening() {
        let center = NotificationCenter()
        var refreshes = 0
        let token = AppDelegate.refreshWhenAMenuOpens(center: center) { refreshes += 1 }
        defer { center.removeObserver(token) }
        XCTAssertEqual(refreshes, 0)
        center.post(name: NSMenu.didBeginTrackingNotification, object: nil)
        XCTAssertEqual(refreshes, 1)
        center.post(name: NSMenu.didBeginTrackingNotification, object: nil)
        XCTAssertEqual(refreshes, 2)
    }
}
