import XCTest
@testable import Sillon

final class PanelGeometryTests: XCTestCase {
    let screen = CGRect(x: 0, y: 0, width: 1800, height: 1100)

    func testPanelIsCenteredAndFollowsShortText() {
        let f = PanelGeometry.panelFrame(contentHeight: 80, screen: screen)
        XCTAssertEqual(f.midX, screen.midX, accuracy: 1)
        XCTAssertEqual(f.height, 80 + PanelGeometry.chrome, accuracy: 1)
        XCTAssertLessThanOrEqual(f.width, 760)
    }

    func testPanelHeightIsBoundedForLongText() {
        let f = PanelGeometry.panelFrame(contentHeight: .greatestFiniteMagnitude, screen: screen)
        XCTAssertEqual(f.height, screen.height * 0.45, accuracy: 1)
    }

    func testPanelNeverShrinksBelowMinimum() {
        let f = PanelGeometry.panelFrame(contentHeight: 0, screen: screen)
        XCTAssertEqual(f.height, PanelGeometry.minimumHeight, accuracy: 1)
    }

    func testExpandedFillsTheScreenWithAMargin() {
        let f = PanelGeometry.expandedFrame(screen: screen)
        XCTAssertEqual(f, screen.insetBy(dx: 24, dy: 24))
    }
}
