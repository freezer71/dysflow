import XCTest

@MainActor
final class PanelUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
    }

    // La vue hôte SwiftUI se présente comme un groupe, pas « other ».
    // Les touches vont à l'app : la fenêtre clé les reçoit, quel que soit
    // l'identifiant du moment (panneau ou lecteur agrandi).
    func testKeyboardOnlyFlow() {
        let panel = app.groups["panneau-lecture"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))

        let play = app.buttons["lecture"]
        XCTAssertEqual(play.label, "Lecture")
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))

        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2))

        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2))

        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForNonExistence(timeout: 2))
    }
}

@MainActor
extension XCUIElement {
    func waitForLabel(_ label: String, timeout: TimeInterval = 2) -> Bool {
        let predicate = NSPredicate(format: "label == %@", label)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [exp], timeout: timeout) == .completed
    }
}
