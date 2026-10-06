import XCTest
@testable import Sillon

final class KeyMapTests: XCTestCase {
    private func act(_ c: String, _ code: UInt16 = 0, option: Bool = false,
                     shift: Bool = false, command: Bool = false, control: Bool = false) -> ReaderAction? {
        KeyMap.action(for: KeyPress(characters: c, keyCode: code, option: option,
                                    shift: shift, command: command, control: control))
    }

    func testVOpensTheVoiceSettings() {
        XCTAssertEqual(KeyMap.action(for: KeyPress(characters: "v")), .openVoiceSettings)
        XCTAssertNil(KeyMap.action(for: KeyPress(characters: "v", option: true)))
    }

    func testTheSeekStepComesFromTheSettings() {
        let key = KeyPress(characters: "", keyCode: KeyCode.right, shift: true)
        XCTAssertEqual(KeyMap.action(for: key, seekStep: 12), .command(.sentences(delta: 12)))
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

    func testLayoutIndependentVolume() {
        // ⌥↑ et ⌥↓ pour le volume (indépendant du clavier)
        XCTAssertEqual(act("", KeyCode.up, option: true), .command(.changeVolume(delta: 0.05)))
        XCTAssertEqual(act("", KeyCode.down, option: true), .command(.changeVolume(delta: -0.05)))
        // Les flèches sans Option sont toujours des blocs
        XCTAssertEqual(act("", KeyCode.up), .command(.blocks(delta: -1)))
        XCTAssertEqual(act("", KeyCode.down), .command(.blocks(delta: 1)))
        // [ et ] restent disponibles pour QWERTY
        XCTAssertEqual(act("["), .command(.changeVolume(delta: -0.05)))
        XCTAssertEqual(act("]"), .command(.changeVolume(delta: 0.05)))
    }

    func testAZERTYSpeedAliases() {
        // AZERTY non-décalées correspondent aux vitesses
        XCTAssertEqual(act("&"), .command(.setSpeed(speed: 0.75)))
        XCTAssertEqual(act("é"), .command(.setSpeed(speed: 1.0)))
        XCTAssertEqual(act("\""), .command(.setSpeed(speed: 1.5)))
        XCTAssertEqual(act("'"), .command(.setSpeed(speed: 2.0)))
        // QWERTY décalées restent aussi
        XCTAssertEqual(act("1"), .command(.setSpeed(speed: 0.75)))
        XCTAssertEqual(act("2"), .command(.setSpeed(speed: 1.0)))
        XCTAssertEqual(act("3"), .command(.setSpeed(speed: 1.5)))
        XCTAssertEqual(act("4"), .command(.setSpeed(speed: 2.0)))
    }

    func testCharacterShortcutsBlockedByOptionAndControl() {
        // Les raccourcis caractères sont bloqués par Option et Control
        XCTAssertNil(act("n", option: true))
        XCTAssertNil(act("b", control: true))
        XCTAssertNil(act("/", option: true))
        // Mais les flèches conservent leur traitement avec Option
        XCTAssertEqual(act("", KeyCode.left, option: true), .command(.words(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right, option: true), .command(.words(delta: 1)))
        XCTAssertEqual(act("", KeyCode.left, shift: true), .command(.sentences(delta: -5)))
        // ⌥⇧← : Option a la priorité
        XCTAssertEqual(act("", KeyCode.left, option: true, shift: true), .command(.words(delta: -1)))
    }
}
