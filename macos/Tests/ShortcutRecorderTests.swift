import Carbon.HIToolbox
import KeyboardShortcuts
import XCTest
@testable import Dysflow

/// L'enregistrement des raccourcis : une touche seule n'est acceptée que si
/// c'est une touche d'action (modification ou fonction) ; une lettre ou un
/// chiffre ne vaut qu'avec ⌘, ⌥ ou ⌃.
final class ShortcutRecorderTests: XCTestCase {
    private let option = UInt16(kVK_RightOption)

    func testAModifierAloneIsTheDictationTrigger() {
        var reader = ShortcutCaptureReader(allowsModifierAlone: true)
        XCTAssertNil(reader.flagsChanged(keyCode: option, modifiers: .option), "enfoncée : on attend le relâché")
        XCTAssertEqual(reader.flagsChanged(keyCode: option, modifiers: []), .modifier(.rightOption))
        XCTAssertNil(reader.flagsChanged(keyCode: UInt16(kVK_Function), modifiers: .function))
        XCTAssertEqual(reader.flagsChanged(keyCode: UInt16(kVK_Function), modifiers: []), .modifier(.function))
    }

    func testAModifierWithALetterIsACombination() {
        var reader = ShortcutCaptureReader(allowsModifierAlone: true)
        _ = reader.flagsChanged(keyCode: option, modifiers: .option)
        XCTAssertEqual(reader.keyDown(keyCode: UInt16(kVK_ANSI_D), modifiers: .option),
                       .combo(KeyboardShortcuts.Shortcut(.d, modifiers: .option)))
        XCTAssertNil(reader.flagsChanged(keyCode: option, modifiers: []), "le relâché après ⌥D n'est pas une touche seule")
    }

    func testTwoModifiersTogetherAreNotATouchAlone() {
        var reader = ShortcutCaptureReader(allowsModifierAlone: true)
        _ = reader.flagsChanged(keyCode: option, modifiers: .option)
        XCTAssertNil(reader.flagsChanged(keyCode: UInt16(kVK_Command), modifiers: [.option, .command]))
        XCTAssertNil(reader.flagsChanged(keyCode: UInt16(kVK_Command), modifiers: .option))
        XCTAssertNil(reader.flagsChanged(keyCode: option, modifiers: []))
    }

    func testALetterOrADigitAloneIsRefused() {
        var reader = ShortcutCaptureReader(allowsModifierAlone: true)
        XCTAssertEqual(reader.keyDown(keyCode: UInt16(kVK_ANSI_D), modifiers: []), .refused)
        XCTAssertEqual(reader.keyDown(keyCode: UInt16(kVK_ANSI_5), modifiers: []), .refused)
        XCTAssertEqual(reader.keyDown(keyCode: UInt16(kVK_ANSI_D), modifiers: .shift), .refused, "Maj + lettre : une majuscule")
        XCTAssertEqual(reader.keyDown(keyCode: UInt16(kVK_F5), modifiers: []),
                       .combo(KeyboardShortcuts.Shortcut(.f5)), "une touche de fonction seule est une touche d'action")
    }

    func testOtherShortcutsDoNotTakeAModifierAlone() {
        var reader = ShortcutCaptureReader(allowsModifierAlone: false)
        _ = reader.flagsChanged(keyCode: option, modifiers: .option)
        XCTAssertNil(reader.flagsChanged(keyCode: option, modifiers: []))
    }

    @MainActor
    func testTheTriggerSaysWhatIsActive() {
        XCTAssertEqual(DictationTrigger.label(modifier: .rightOption, combo: "⌃⌥D"), "Option droite ou ⌃⌥D")
        XCTAssertEqual(DictationTrigger.label(modifier: .none, combo: "⌥D"), "⌥D")
        XCTAssertEqual(DictationTrigger.label(modifier: .leftCommand, combo: nil), "Commande gauche")
        XCTAssertNil(DictationTrigger.label(modifier: .none, combo: nil))
    }

    func testEveryModifierKeyIsRecognisedOnEitherSide() {
        for key in DictationModifier.allCases where key != .none {
            XCTAssertEqual(DictationModifier(keyCode: key.keyCode!), key)
            var tap = ModifierTapDetector(key: key)
            XCTAssertFalse(tap.flagsChanged(keyCode: key.keyCode!, flags: key.flag, at: 0))
            XCTAssertTrue(tap.flagsChanged(keyCode: key.keyCode!, flags: [], at: 0.1), "\(key.label) : un appui bref")
        }
    }
}
