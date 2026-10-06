import XCTest
@testable import Sillon

final class AdvancedTabTests: XCTestCase {
    func testEnginesAndComputeAreNamedAndKeepAHandWrittenValue() {
        XCTAssertEqual(AdvancedTab.engineOptions(current: "auto").map(\.title),
                       ["Automatique", "audio.cpp", "Voix du système"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "metal").map(\.value), ["metal", "cpu"])
        XCTAssertEqual(AdvancedTab.computeOptions(current: "cuda").map(\.value), ["metal", "cpu", "cuda"])
    }

    func testZeroSessionsMeansTheModelDecides() {
        XCTAssertEqual(AdvancedTab.sessionsText(0), "Sessions : automatique")
        XCTAssertEqual(AdvancedTab.sessionsText(3), "Sessions : 3")
    }

    func testAnOptionIsAddedReplacedOrRefused() {
        let one = ExtraOptions.adding(key: " cfg_scale ", value: " 3 ", to: [])
        XCTAssertEqual(one, [ExtraOption(key: "cfg_scale", value: "3")])
        let replaced = ExtraOptions.adding(key: "cfg_scale", value: "4", to: one ?? [])
        XCTAssertEqual(replaced, [ExtraOption(key: "cfg_scale", value: "4")])
        XCTAssertNil(ExtraOptions.adding(key: "  ", value: "1", to: []), "clé vide")
        XCTAssertNil(ExtraOptions.adding(key: "a=b", value: "1", to: []), "« = » dans la clé")
        XCTAssertEqual(ExtraOptions.removing("cfg_scale", from: replaced ?? []), [])
    }

    func testReplacingKeepsThePlaceAndNoKeyIsDuplicated() {
        let start = [ExtraOption(key: "a", value: "1"), ExtraOption(key: "b", value: "2")]
        let next = ExtraOptions.adding(key: "a", value: "9", to: start)
        XCTAssertEqual(next, [ExtraOption(key: "a", value: "9"), ExtraOption(key: "b", value: "2")])
    }
}
