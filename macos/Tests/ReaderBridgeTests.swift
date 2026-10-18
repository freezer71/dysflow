import XCTest
@testable import Sillage

@MainActor
final class ReaderBridgeTests: XCTestCase {
    func testSilentReaderOpensTextAndMoves() {
        let reader = SillageReader.silent()
        reader.openText(markdown: "# Titre\n\nUn. Deux.", source: "test")
        XCTAssertTrue(waitUntil { reader.snapshot().revision == 1 })
        XCTAssertEqual(reader.document().sentenceCount, 3)
        reader.command(command: .sentences(delta: 2))
        XCTAssertTrue(waitUntil { reader.snapshot().sentence == 2 })
    }
}
