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
}
