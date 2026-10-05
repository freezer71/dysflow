import XCTest

final class SettingsUITests: XCTestCase {
    func testSettingsReachableAndKeyboardNavigable() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.popUpButtons["police-lecture"].exists)
        XCTAssertTrue(window.popUpButtons["garder-voix"].exists)
        window.typeKey("\t", modifierFlags: [])
        try app.performAccessibilityAudit()
    }
}
