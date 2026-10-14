import XCTest

final class LaunchUITests: XCTestCase {
    func testAppLaunchesInTestMode() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5)
                      || app.wait(for: .runningBackground, timeout: 5))
    }
}
