import XCTest
@testable import Sillon

func text(_ s: String) -> SavedItem {
    SavedItem(types: [SavedType(type: "public.utf8-plain-text", data: Data(s.utf8))])
}

/// Envoyé à la tâche de la copie tardive (`FakeKeys.delay`) : le test ne
/// le touche plus pendant ce temps.
final class FakePasteboard: PasteboardAccess, @unchecked Sendable {
    private var count = 0
    private var reads = 0
    /// Simule une copie qui arrive pile entre la fin de l'attente et le dernier coup d'œil.
    var lateCopy: (afterReads: Int, text: String)?
    var changeCount: Int {
        get {
            reads += 1
            if let late = lateCopy, reads == late.afterReads {
                items = [text(late.text)]
                count += 1
            }
            return count
        }
        set { count = newValue }
    }
    var items: [SavedItem] = [text("ancien")]
    func save() -> [SavedItem] { items }
    func restore(_ saved: [SavedItem]) { items = saved; changeCount += 1 }
    func string() -> String? {
        items.first?.types.first { $0.type == "public.utf8-plain-text" }
            .flatMap { String(data: $0.data, encoding: .utf8) }
    }
    var isConcealed: Bool { Sillon.isConcealed(items) }
    func imageData() -> Data? {
        items.lazy.compactMap { $0.types.first { imageTypes.contains($0.type) }?.data }.first
    }
}

struct FakeKeys: KeystrokeSender {
    let pasteboard: FakePasteboard
    let copied: String?
    var delay: Duration? = nil   // copie tardive, après `sendCopy`
    /// La copie porte le marqueur « dissimulé » (gestionnaire de mots de passe).
    var concealed = false
    let calls = Counter()
    func sendCopy() {
        calls.n += 1
        guard let copied else { return }
        if let delay {
            let pb = pasteboard
            Task {
                try? await Task.sleep(for: delay)
                pb.items = [text(copied)]
                pb.changeCount += 1
            }
            return
        }
        var item = text(copied)
        if concealed { item.types.append(SavedType(type: concealedType, data: Data())) }
        pasteboard.items = [item]
        pasteboard.changeCount += 1
    }
}

final class Counter: @unchecked Sendable { var n = 0 }

struct Stub: TextSource {
    let outcome_: CaptureOutcome
    init(_ o: CaptureOutcome) { outcome_ = o }
    func outcome() async -> CaptureOutcome { outcome_ }
}

@MainActor
final class TextCaptureTests: XCTestCase {
    func testCopyReadsThenRestoresThePasteboard() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "sélection"),
                                trusted: { true }, secureInput: { false }, ownAppFrontmost: { false })
        let text = await source.outcome()
        XCTAssertEqual(text, .text("sélection"))
        XCTAssertEqual(pb.string(), "ancien")
    }

    /// 1Password au premier plan, un élément choisi : le ⌘C copie le mot de
    /// passe marqué « dissimulé ». Il n'est ni lu ni laissé dans le presse-papiers.
    func testConcealedCopyIsVetoedAndClipboardRestored() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "mot de passe", concealed: true)
        let source = CopySource(pasteboard: pb, keys: keys, trusted: { true },
                                secureInput: { false }, ownAppFrontmost: { false })
        let outcome = await source.outcome()
        XCTAssertEqual(outcome, .veto(.secure))
        XCTAssertEqual(pb.string(), "ancien")
        XCTAssertFalse(pb.isConcealed)
    }

    func testCopyThatChangesNothingTimesOutAndTouchesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: nil),
                                trusted: { true }, timeout: .milliseconds(50),
                                secureInput: { false }, ownAppFrontmost: { false })
        let text = await source.outcome()
        XCTAssertNotEqual(text, .text("x")); XCTAssertFalse(text.isText)
        XCTAssertEqual(pb.string(), "ancien")
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testCopyWithoutPermissionDoesNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "x"),
                                trusted: { false }, secureInput: { false }, ownAppFrontmost: { false })
        let text = await source.outcome()
        XCTAssertNotEqual(text, .text("x")); XCTAssertFalse(text.isText)
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testBlankCopyCountsAsNothing() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "  \n"),
                                trusted: { true }, secureInput: { false }, ownAppFrontmost: { false })
        let text = await source.outcome()
        XCTAssertNotEqual(text, .text("x")); XCTAssertFalse(text.isText)
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testChainTakesTheFirstSourceThatAnswers() async {
        let chain = CaptureChain(sources: [Stub(.unsupported), Stub(.text("deux")), Stub(.text("trois"))])
        let t = await chain.capture()
        XCTAssertEqual(t, .text("deux"))
        let empty = await CaptureChain(sources: [Stub(.unsupported)]).capture()
        XCTAssertEqual(empty, .unsupported)
    }

    func testVetoStopsTheChainAndNoCopyIsSent() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "secret")
        let chain = CaptureChain(sources: [
            SelectionSource(trusted: { true }, secureInput: { false }, read: { .secureField }),
            CopySource(pasteboard: pb, keys: keys, trusted: { true },
                       secureInput: { false }, ownAppFrontmost: { false }),
        ])
        let t = await chain.capture()
        XCTAssertEqual(t, .veto(.secure))
        XCTAssertEqual(keys.calls.n, 0)
        XCTAssertEqual(pb.changeCount, 0)
    }

    func testOwnProcessAbortsTheWholeCapture() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "x")
        let chain = CaptureChain(sources: [
            SelectionSource(trusted: { true }, secureInput: { false }, read: { .ownProcess }),
            CopySource(pasteboard: pb, keys: keys, trusted: { true },
                       secureInput: { false }, ownAppFrontmost: { false }),
        ])
        let t = await chain.capture()
        XCTAssertEqual(t, .veto(.ownApp))
        XCTAssertEqual(keys.calls.n, 0)
    }

    func testCopyRefusesWhenOwnAppIsFrontmost() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "x")
        let source = CopySource(pasteboard: pb, keys: keys, trusted: { true },
                                secureInput: { false }, ownAppFrontmost: { true })
        let o = await source.outcome()
        XCTAssertEqual(o, .veto(.ownApp))
        XCTAssertEqual(keys.calls.n, 0)
    }

    func testSecureEventInputVetoesBothSources() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "x")
        let sel = await SelectionSource(trusted: { true }, secureInput: { true },
                                        read: { .selected("a") }).outcome()
        let copy = await CopySource(pasteboard: pb, keys: keys, trusted: { true },
                                    secureInput: { true }, ownAppFrontmost: { false }).outcome()
        XCTAssertEqual(sel, .veto(.secure))
        XCTAssertEqual(copy, .veto(.secure))
        XCTAssertEqual(keys.calls.n, 0)
    }

    func testEmptyAXSelectionStopsBeforeCopy() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "ligne entière")
        let chain = CaptureChain(sources: [
            SelectionSource(trusted: { true }, secureInput: { false }, read: { .empty }),
            CopySource(pasteboard: pb, keys: keys, trusted: { true },
                       secureInput: { false }, ownAppFrontmost: { false }),
        ])
        let t = await chain.capture()
        XCTAssertEqual(t, .noSelection)
        XCTAssertEqual(keys.calls.n, 0)
    }

    func testUnsupportedAXFallsBackToCopy() async {
        let pb = FakePasteboard()
        let keys = FakeKeys(pasteboard: pb, copied: "via copie")
        let chain = CaptureChain(sources: [
            SelectionSource(trusted: { true }, secureInput: { false }, read: { .unsupported }),
            CopySource(pasteboard: pb, keys: keys, trusted: { true },
                       secureInput: { false }, ownAppFrontmost: { false }),
        ])
        let t = await chain.capture()
        XCTAssertEqual(t, .text("via copie"))
        XCTAssertEqual(keys.calls.n, 1)
    }

    func testRestoreKeepsTypeOrderAndAddsTransientMarker() {
        let item = SavedItem(types: [
            SavedType(type: "public.png", data: Data([1])),
            SavedType(type: "public.utf8-plain-text", data: Data("a".utf8)),
            SavedType(type: "org.nspasteboard.ConcealedType", data: Data()),
        ])
        let second = text("b")
        let out = itemsForRestore([item, second])
        XCTAssertEqual(out.count, 2)
        XCTAssertEqual(out[0].types.map(\.type), [
            "public.png", "public.utf8-plain-text", "org.nspasteboard.ConcealedType", transientType])
        XCTAssertEqual(out[0].types.last?.data, Data())
        XCTAssertEqual(out[1], second)
        // pas de doublon si déjà présent
        XCTAssertEqual(itemsForRestore(out).first?.types.filter { $0.type == transientType }.count, 1)
    }

    func testEmptyOriginalClipboardRestoresEmpty() async {
        let pb = FakePasteboard()
        pb.items = []
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: "sélection"),
                                trusted: { true }, secureInput: { false }, ownAppFrontmost: { false })
        let o = await source.outcome()
        XCTAssertEqual(o, .text("sélection"))
        XCTAssertTrue(pb.items.isEmpty)
        XCTAssertTrue(itemsForRestore([]).isEmpty)
    }

    func testLateCopyAfterTimeoutIsRestored() async {
        let pb = FakePasteboard()
        // Délai nul : lecture 1 dans la boucle (inchangé), lecture 2 au dernier coup d'œil (changé).
        pb.lateCopy = (afterReads: 2, text: "tardif")
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: nil),
                                trusted: { true }, timeout: .zero, secureInput: { false },
                                ownAppFrontmost: { false })
        _ = await source.outcome()
        XCTAssertEqual(pb.string(), "ancien")
    }

    func testTimeoutWithNoChangeLeavesClipboardUntouched() async {
        let pb = FakePasteboard()
        let source = CopySource(pasteboard: pb, keys: FakeKeys(pasteboard: pb, copied: nil),
                                trusted: { true }, timeout: .milliseconds(30),
                                secureInput: { false }, ownAppFrontmost: { false })
        _ = await source.outcome()
        XCTAssertEqual(pb.changeCount, 0)
        XCTAssertEqual(pb.string(), "ancien")
    }
}

extension CaptureOutcome {
    var isText: Bool { if case .text = self { return true } else { return false } }
}
