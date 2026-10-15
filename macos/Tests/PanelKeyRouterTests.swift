import AppKit
import XCTest
@testable import Sillon

/// Ce que le panneau fait d'une touche, selon le focus et l'édition.
@MainActor
final class PanelKeyRouterTests: XCTestCase {
    private func route(_ keyCode: UInt16, command: Bool = false, option: Bool = false,
                       editing: Bool = false, focus: NSResponder? = nil,
                       action: ReaderAction? = .command(.togglePlay)) -> PanelKeyRouter.Route {
        PanelKeyRouter.route(keyCode: keyCode, command: command, option: option, editing: editing,
                             firstResponder: focus, action: { action })
    }

    func testCommandEnterFinishesEditing() {
        XCTAssertEqual(route(KeyCode.enter, command: true, editing: true), .finishEditing)
        XCTAssertEqual(route(KeyCode.enter, command: true, editing: false), .perform(.command(.togglePlay)))
    }

    func testATextFieldKeepsItsKeysButEscape() {
        let field = NSTextView()
        XCTAssertEqual(route(KeyCode.space, focus: field), .pass)
        XCTAssertEqual(route(KeyCode.escape, focus: field, action: .escape), .perform(.escape))
    }

    func testAListKeepsItsVerticalArrowsAndEnter() {
        let list = NSTableView()
        XCTAssertEqual(route(KeyCode.down, focus: list), .pass)
        XCTAssertEqual(route(KeyCode.enter, focus: list), .pass)
        XCTAssertEqual(route(KeyCode.down, option: true, focus: list), .perform(.command(.togglePlay)))
        XCTAssertEqual(route(KeyCode.space, focus: list), .perform(.command(.togglePlay)))
    }

    func testAKeyWithoutActionPasses() {
        XCTAssertEqual(route(KeyCode.space, action: nil), .pass)
    }
}
