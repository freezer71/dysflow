import XCTest
@testable import Sillon

final class KeepVoiceChoiceTests: XCTestCase {
    func testRoundTrip() {
        for choice in KeepVoiceChoice.allCases {
            XCTAssertEqual(KeepVoiceChoice(minutes: choice.minutes), choice)
        }
    }

    func testSpecialValues() {
        XCTAssertEqual(KeepVoiceChoice(minutes: -1), .always)
        XCTAssertEqual(KeepVoiceChoice(minutes: 0), .end)
        XCTAssertEqual(KeepVoiceChoice(minutes: 10).label, "10 minutes")
    }

    func testUnlistedValueSnapsToNearest() {
        XCTAssertEqual(KeepVoiceChoice(minutes: 7), .minutes(5))
        XCTAssertEqual(KeepVoiceChoice(minutes: 300), .minutes(30))
    }
}

final class KeepVoiceSaveTests: XCTestCase {
    func testNothingSavedBeforeLoad() {
        XCTAssertFalse(KeepVoiceChoice.needsSave(choice: .minutes(10), stored: nil))
    }

    func testUnlistedStoredValueIsNotRewritten() {
        // 7 s'affiche « 5 minutes » : l'affichage seul n'écrit rien.
        XCTAssertFalse(KeepVoiceChoice.needsSave(choice: .minutes(5), stored: 7))
        XCTAssertTrue(KeepVoiceChoice.needsSave(choice: .minutes(10), stored: 7))
        XCTAssertTrue(KeepVoiceChoice.needsSave(choice: .always, stored: 10))
    }
}
