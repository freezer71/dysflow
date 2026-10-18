import XCTest
@testable import Sillage

/// La table de la spec 1.5, et rien d'autre.
final class KeyMapTests: XCTestCase {
    private func act(_ c: String, _ code: UInt16 = 0, option: Bool = false,
                     shift: Bool = false, command: Bool = false, control: Bool = false) -> ReaderAction? {
        KeyMap.action(for: KeyPress(characters: c, keyCode: code, option: option,
                                    shift: shift, command: command, control: control))
    }

    func testTheReadingKeys() {
        XCTAssertEqual(act(" ", KeyCode.space), .command(.togglePlay))
        XCTAssertEqual(act("", KeyCode.left), .command(.sentences(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right), .command(.sentences(delta: 1)))
        XCTAssertEqual(act("", KeyCode.up), .line(delta: -1))
        XCTAssertEqual(act("", KeyCode.down), .line(delta: 1))
        XCTAssertEqual(act("", KeyCode.home), .command(.start), "Début : revenir au début")
        XCTAssertEqual(act("n"), .command(.sections(delta: 1)))
        XCTAssertEqual(act("p"), .command(.sections(delta: -1)))
        XCTAssertEqual(act("+", shift: true), .command(.changeSpeed(delta: 0.1)))
        XCTAssertEqual(act("-"), .command(.changeSpeed(delta: -0.1)))
    }

    func testSearchSidebarEditAndHelp() {
        XCTAssertEqual(act("/"), .openSearch)
        XCTAssertEqual(act("f", command: true), .openSearch)
        XCTAssertEqual(act("N", shift: true), .command(.searchAdvance(delta: 1)))
        XCTAssertEqual(act("P", shift: true), .command(.searchAdvance(delta: -1)))
        XCTAssertEqual(act("t"), .toggleSidebar)
        XCTAssertEqual(act("e"), .edit)
        XCTAssertEqual(act("?", shift: true), .showHelp)
    }

    func testTheCommandKeys() {
        XCTAssertEqual(act("c", command: true), .copy)
        XCTAssertEqual(act("v", command: true), .paste)
        XCTAssertEqual(act("o", command: true), .openFile)
        XCTAssertEqual(act("m", command: true), .toggleSize)
        XCTAssertEqual(act("w", command: true), .close)
        XCTAssertEqual(act(",", command: true), .openSettings)
        XCTAssertEqual(act("", KeyCode.escape), .escape)
        // ⌘A et les autres restent au texte (tout sélectionner).
        XCTAssertNil(act("a", command: true))
    }

    /// Tout ce que la spec retire ne fait plus rien.
    func testRemovedKeysDoNothing() {
        for c in ["b", "B", "[", "]", "m", "r", "g", "v", "h", "=", "_", "1", "2", "3", "4", "&", "é", "\"", "'"] {
            XCTAssertNil(act(c), "« \(c) » devait être retiré")
        }
        for digit in ["1", "2", "3"] { XCTAssertNil(act(digit, command: true), "⌘\(digit)") }
        XCTAssertNil(act("", KeyCode.left, option: true), "⌥← (mot)")
        XCTAssertNil(act("", KeyCode.right, shift: true), "⇧→ (saut)")
        XCTAssertNil(act("", KeyCode.up, shift: true), "⇧↑ (bloc)")
        XCTAssertNil(act("", KeyCode.down, option: true), "⌥↓ (volume)")
        XCTAssertNil(act("", 119), "Fin")
        XCTAssertNil(act("", KeyCode.enter), "⏎")
    }

    func testOptionAndControlBlockTheCharacterKeys() {
        XCTAssertNil(act("n", option: true))
        XCTAssertNil(act("t", control: true))
        XCTAssertNil(act("c", option: true, command: true))
    }
}
