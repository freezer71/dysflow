import AppKit
import XCTest

/// ⌘V dans le lecteur lit le presse-papiers. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:SillonUITests/PasteUITests`. Le
/// presse-papiers de l'utilisateur est sauvé, puis rendu.
@MainActor
final class PasteUITests: XCTestCase {
    private var saved: String?

    override func setUp() {
        continueAfterFailure = false
        saved = NSPasteboard.general.string(forType: .string)
    }

    override func tearDown() {
        NSPasteboard.general.clearContents()
        if let saved { NSPasteboard.general.setString(saved, forType: .string) }
    }

    func testCommandVReadsTheClipboard() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("Bonjour du presse-papiers. Deuxième phrase.", forType: .string)
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-vide", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["invitation"].waitForExistence(timeout: 2))
        app.typeKey("v", modifierFlags: .command)
        XCTAssertTrue(app.staticTexts["invitation"].waitForNonExistence(timeout: 3), "rien n'a été lu")
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForText(timeout: 3) { $0.contains("Bonjour du presse-papiers") },
                      "le texte copié n'est pas dans le lecteur")
    }
}
