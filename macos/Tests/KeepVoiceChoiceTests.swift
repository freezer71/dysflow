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

    /// Une durée écrite à la main s'affiche telle quelle, à sa place.
    func testAnUnlistedValueIsKeptAsIs() {
        XCTAssertEqual(KeepVoiceChoice(minutes: 7), .minutes(7))
        XCTAssertEqual(KeepVoiceChoice(minutes: 7).label, "7 minutes")
        XCTAssertEqual(KeepVoiceChoice.options(stored: 7),
                       [.end, .minutes(2), .minutes(5), .minutes(7), .minutes(10), .minutes(30), .always])
        XCTAssertEqual(KeepVoiceChoice.options(stored: 10), KeepVoiceChoice.allCases)
        XCTAssertEqual(KeepVoiceChoice.options(stored: -1), KeepVoiceChoice.allCases)
    }
}
