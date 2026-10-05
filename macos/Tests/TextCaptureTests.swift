import XCTest
@testable import Sillon

final class FakePasteboard: PasteboardAccess {
    var changeCount = 0
    var items: [[String: Data]] = [["public.utf8-plain-text": Data("ancien".utf8)]]
    func save() -> [[String: Data]] { items }
    func restore(_ saved: [[String: Data]]) { items = saved; changeCount += 1 }
    func string() -> String? {
        items.first?["public.utf8-plain-text"].flatMap { String(data: $0, encoding: .utf8) }
    }
}

struct FakeKeys: KeystrokeSender {
    let pasteboard: FakePasteboard
    let copied: String?
    func sendCopy() {
        guard let copied else { return }
        pasteboard.items = [["public.utf8-plain-text": Data(copied.utf8)]]
        pasteboard.changeCount += 1
    }
}

struct Stub: TextSource {
    let value: String?
    func text() async -> String? { value }
}

final class TextCaptureTests: XCTestCase {
    func testCopyReadsThenRestoresThePasteboard() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "sélection"),
                                trusted: { true })
        let text = await source.text()
        XCTAssertEqual(text, "sélection")
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testCopyThatChangesNothingTimesOutAndTouchesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: nil),
                                trusted: { true }, timeout: .milliseconds(50))
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.string(), "ancien")
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testCopyWithoutPermissionDoesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "x"),
                                trusted: { false })
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testBlankCopyCountsAsNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "  \n"),
                                trusted: { true })
        let text = await source.text()
        XCTAssertNil(text)
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testChainTakesTheFirstSourceThatAnswers() async {
        let chain = CaptureChain(sources: [Stub(value: nil), Stub(value: "deux"), Stub(value: "trois")])
        let text = await chain.capture()
        XCTAssertEqual(text, "deux")
        let empty = await CaptureChain(sources: [Stub(value: nil)]).capture()
        XCTAssertNil(empty)
    }
}
