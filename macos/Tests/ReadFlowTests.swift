import XCTest
@testable import Sillon

@MainActor
final class ReadFlowTests: XCTestCase {
    private var log: [String] = []

    private func flow(capture: CaptureOutcome, clipboard: Bool,
                      concealed: Bool = false) -> ReadFlow {
        ReadFlow(
            prepare: { self.log.append("voix") },
            capture: { self.log.append("capture"); return capture },
            open: { self.log.append("ouvre:\($0)") },
            clipboardIsConcealed: { concealed },
            openClipboard: {
                self.log.append("presse-papiers")
                if !clipboard { throw SillonError.Failed(message: "vide") }
            },
            say: { self.log.append("dit:\($0)") },
            show: { self.log.append("montre") })
    }

    func testSelectionWinsAndVoiceStartsFirst() async {
        await flow(capture: .text("texte"), clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "ouvre:texte", "montre"])
    }

    func testFallsBackToClipboard() async {
        await flow(capture: .unsupported, clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "montre"])
    }

    func testNoSelectionFallsBackToClipboard() async {
        await flow(capture: .noSelection, clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "montre"])
    }

    func testNothingToReadSaysSoAndStillShows() async {
        await flow(capture: .unsupported, clipboard: false).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers",
                             "dit:Sélectionnez ou copiez du texte", "montre"])
    }

    func testSecureVetoReadsNothingAndSaysSo() async {
        await flow(capture: .veto(.secure), clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture", "dit:Champ sécurisé : rien n'est lu", "montre"])
    }

    func testOwnAppVetoReadsNothingAndSaysWhy() async {
        await flow(capture: .veto(.ownApp), clipboard: true).run()
        XCTAssertEqual(log, ["voix", "capture",
                             "dit:sillon est au premier plan : sélectionnez dans une autre application",
                             "montre"])
    }

    func testConcealedClipboardReadsNothingAndSaysSo() async {
        await flow(capture: .noSelection, clipboard: true, concealed: true).run()
        XCTAssertEqual(log, ["voix", "capture", "dit:Presse-papiers protégé : rien n'est lu", "montre"])
    }

    /// Le presse-papiers s'ouvre hors de l'acteur principal : le panneau ne
    /// paraît qu'une fois le texte ouvert, jamais avant.
    func testShowWaitsForAsyncClipboard() async {
        let flow = ReadFlow(
            prepare: {},
            capture: { .unsupported },
            open: { _ in },
            clipboardIsConcealed: { false },
            openClipboard: {
                try await Task.sleep(for: .milliseconds(50))
                await MainActor.run { self.log.append("presse-papiers") }
            },
            say: { _ in },
            show: { self.log.append("montre") })
        await flow.run()
        XCTAssertEqual(log, ["presse-papiers", "montre"])
    }

    func testConcealedTypeIsDetectedOnAnyItem() {
        let plain = [SavedItem(types: [SavedType(type: "public.utf8-plain-text", data: Data())])]
        let hidden = plain + [SavedItem(types: [SavedType(type: concealedType, data: Data())])]
        XCTAssertFalse(isConcealed(plain))
        XCTAssertTrue(isConcealed(hidden))
    }
}
