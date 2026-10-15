import XCTest
@testable import Sillon

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
