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
