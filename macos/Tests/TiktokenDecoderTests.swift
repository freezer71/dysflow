import XCTest
@testable import KokoroMLX

final class TiktokenDecoderTests: XCTestCase {
    func fixture() -> URL {
        Bundle(for: Self.self).url(forResource: "mini", withExtension: "tiktoken")!
    }

    func testDecodesBytesInOrder() throws {
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([0, 1], skipSpecial: true), "Hello world")
    }

    func testJoinsUTF8SplitAcrossTokens() throws {
        // « é » = C3 A9 : un jeton peut n'en porter qu'une partie.
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([0, 2], skipSpecial: true), "Hello é")
    }

    func testSpecialIdsAreSkipped() throws {
        let decoder = try TiktokenDecoder(contentsOf: fixture())
        XCTAssertEqual(decoder.decode([50258, 0, 50257], skipSpecial: true), "Hello")
    }

    func testTurboShiftsTheTaskTokens() {
        let base = WhisperSpecialTokens(vocabularySize: 51865)
        let turbo = WhisperSpecialTokens(vocabularySize: 51866)
        XCTAssertEqual(base.transcribe, 50359)
        XCTAssertEqual(turbo.transcribe, 50360)
        XCTAssertEqual(turbo.noTimestamps, 50364)
        XCTAssertEqual(base.french, 50265)
        XCTAssertEqual(turbo.french, 50265)
    }
}
