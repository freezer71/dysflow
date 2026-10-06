import XCTest
@testable import Sillon

final class ReadingTabTests: XCTestCase {
    func testTheThreeCodePoliciesAreNamedInFrench() {
        let options = ReadingTab.codeOptions(current: "announce")
        XCTAssertEqual(options.map(\.value), ["announce", "skip", "read"])
        XCTAssertEqual(options.map(\.title), ["Annoncer", "Sauter", "Lire"])
    }

    /// `code_blocks = "lire"`, accepté par le moteur, s'affiche tel quel.
    func testAHandWrittenPolicyStaysInTheMenu() {
        let options = ReadingTab.codeOptions(current: "lire")
        XCTAssertEqual(options.map(\.value), ["announce", "skip", "read", "lire"])
        XCTAssertEqual(options.last?.title, "lire (écrit à la main)")
    }

    func testSeekStepIsPluralOnlyAboveOne() {
        XCTAssertEqual(ReadingTab.seekText(1), "1 phrase")
        XCTAssertEqual(ReadingTab.seekText(2), "2 phrases")
        XCTAssertEqual(ReadingTab.seekText(50), "50 phrases")
    }
}
