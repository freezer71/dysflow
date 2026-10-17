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
    /// Un morceau tient dans la borne de trames : 4 trames par octet + 50
    /// ne dépassent jamais le contexte du décodeur (relevé §3.c), donc la
    /// borne n'atteint pas le plafond qui couperait en plein mot.
    func testAChunkNeverReachesTheFrameCeiling() {
        let limit = MagpieRunner.maxChunkBytes(maxLen: 2048, bakedT: 110)
        XCTAssertGreaterThan(limit, 300)
        XCTAssertLessThanOrEqual(4 * limit + 50, 2048 - 110 - 1)
        XCTAssertLessThan(limit, 2048, "et bien sous le contexte de l'encodeur de texte")
    }

    func testAShortSentenceStaysWhole() {
        XCTAssertEqual(MagpieRunner.chunks("Bonjour, le monde.", maxBytes: 471), ["Bonjour, le monde."])
    }

    /// Relevé de revue : au-delà de 2 048 jetons, l'encodeur lirait hors de
    /// sa table de positions. Un texte de 3 000 octets est découpé en
    /// morceaux sous la borne, aux ponctuations ou aux espaces, sans mot coupé.
    func testALongFrenchTextIsSplitAtWordBoundaries() {
        let phrase = "L'été dernier, à Besançon, nous avons lu « Les Misérables » ; c'était long… "
        var text = ""
        while text.utf8.count < 3000 { text += phrase }
        let limit = 471
        let parts = MagpieRunner.chunks(text, maxBytes: limit)
        XCTAssertGreaterThan(parts.count, 6)
        for part in parts {
            XCTAssertLessThanOrEqual(part.utf8.count, limit)
            XCTAssertFalse(part.isEmpty)
        }
        let words = { (s: String) in s.split(whereSeparator: \.isWhitespace).map(String.init) }
        XCTAssertEqual(parts.flatMap(words), words(text), "aucun mot coupé ni perdu")
        XCTAssertTrue(parts.dropLast().allSatisfy { $0.last.map { ".…;!?".contains($0) } ?? false },
                      "coupé après une ponctuation forte quand il y en a une")
    }

    /// Un « mot » plus long que la borne (une URL…) est coupé quand même,
    /// entre deux caractères, jamais au milieu d'un octet UTF-8.
    func testAWordLongerThanTheLimitIsCutBetweenCharacters() {
        let word = String(repeating: "é", count: 300)  // 600 octets
        let parts = MagpieRunner.chunks(word, maxBytes: 471)
        XCTAssertEqual(parts.joined(), word)
        XCTAssertTrue(parts.allSatisfy { $0.utf8.count <= 471 })
    }
}
