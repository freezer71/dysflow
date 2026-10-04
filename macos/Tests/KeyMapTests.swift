import XCTest
@testable import Sillon

final class KeyMapTests: XCTestCase {
    private func act(_ c: String, _ code: UInt16 = 0, option: Bool = false,
                     shift: Bool = false, command: Bool = false) -> ReaderAction? {
        KeyMap.action(for: KeyPress(characters: c, keyCode: code, option: option,
                                    shift: shift, command: command))
    }

    func testPlayAndWords() {
        XCTAssertEqual(act(" ", KeyCode.space), .command(.togglePlay))
        XCTAssertEqual(act("", KeyCode.left, option: true), .command(.words(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right, option: true), .command(.words(delta: 1)))
    }

    func testSentencesBlocksSections() {
        XCTAssertEqual(act("", KeyCode.left), .command(.sentences(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right), .command(.sentences(delta: 1)))
        XCTAssertEqual(act("", KeyCode.right, shift: true), .command(.sentences(delta: 5)))
        XCTAssertEqual(act("", KeyCode.up), .command(.blocks(delta: -1)))
        XCTAssertEqual(act("", KeyCode.down), .command(.blocks(delta: 1)))
        XCTAssertEqual(act("n"), .command(.sections(delta: 1)))
        XCTAssertEqual(act("p"), .command(.sections(delta: -1)))
    }

    func testSpeedVolumeAndMisc() {
        XCTAssertEqual(act("+"), .command(.changeSpeed(delta: 0.1)))
        XCTAssertEqual(act("-"), .command(.changeSpeed(delta: -0.1)))
        XCTAssertEqual(act("3"), .command(.setSpeed(speed: 1.5)))
        XCTAssertEqual(act("]"), .command(.changeVolume(delta: 0.05)))
        XCTAssertEqual(act("m"), .command(.toggleMute))
        XCTAssertEqual(act("r"), .command(.restartSentence))
        XCTAssertEqual(act("b"), .command(.toggleBookmark))
        XCTAssertEqual(act("g"), .command(.start))
        XCTAssertEqual(act("", KeyCode.end), .command(.end))
    }

    func testCaseMattersForSearchAndBookmarks() {
        XCTAssertEqual(act("N"), .command(.searchAdvance(delta: 1)))
        XCTAssertEqual(act("P"), .command(.searchAdvance(delta: -1)))
        XCTAssertEqual(act("B"), .toggleBookmarks)
        XCTAssertEqual(act("t"), .toggleSidebar)
        XCTAssertEqual(act("/"), .openSearch)
    }

    func testWindowGestures() {
        XCTAssertEqual(act("\r", KeyCode.enter), .expand)
        XCTAssertEqual(act("", KeyCode.escape), .collapseOrHide)
        XCTAssertEqual(act("c", command: true), .copySentence)
        XCTAssertEqual(act(",", command: true), .openSettings)
        XCTAssertEqual(act("?"), .showHelp)
    }

    func testUnknownKeyIsIgnored() {
        XCTAssertNil(act("z"))
        XCTAssertNil(act("x", command: true))
    }
}
