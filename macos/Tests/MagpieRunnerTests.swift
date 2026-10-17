import XCTest
@testable import KokoroMLX

/// La porte de Magpie, sans modèle.
final class MagpieRunnerTests: XCTestCase {
    /// Relevé §3.c : 500 trames (≈ 23 s) coupaient une phrase longue. La
    /// borne suit le texte, sans dépasser le contexte du décodeur.
    func testMaxStepsGrowsWithTheText() {
        XCTAssertEqual(MagpieRunner.maxSteps(textBytes: 10, maxLen: 2048, bakedT: 110), 90)
        XCTAssertGreaterThan(MagpieRunner.maxSteps(textBytes: 400, maxLen: 2048, bakedT: 110), 500,
                             "400 octets ≈ 30 s de parole : plus que les 500 trames d'origine")
        XCTAssertEqual(MagpieRunner.maxSteps(textBytes: 10_000, maxLen: 2048, bakedT: 110), 2048 - 110 - 1)
    }

    /// Un « é » décomposé (PDF) donnerait deux octets inattendus en byT5.
    func testFrenchIsComposedBeforeItsBytes() {
        let nfd = MagpieRunner.prepare("E\u{301}te\u{301}").lowercased()
        XCTAssertEqual(MagpieByT5Encoder.encode(nfd, language: .french),
                       MagpieByT5Encoder.encode("été", language: .french))
    }

    func testTheFiveVoicesAreKnownByTheirCatalogIds() {
        for id in ["sofia", "aria", "jason", "leo", "john_van_stan"] {
            XCTAssertNotNil(MagpieSpeaker(named: id), id)
        }
        XCTAssertNil(MagpieSpeaker(named: "vivian"))
    }

    func testTheEnglishDictionaryShipsInTheFramework() {
        XCTAssertNotNil(Bundle(for: MagpieRunner.self).url(forResource: "cmudict_ipa_en", withExtension: "txt"))
    }
}
