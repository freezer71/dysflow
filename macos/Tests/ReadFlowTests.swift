import XCTest
@testable import Sillon

@MainActor
final class ReadFlowTests: XCTestCase {
    private var log: [String] = []

    private func flow(capture: CaptureOutcome, clipboard: ClipboardRead,
                      imageOpens: Bool = true) -> ReadFlow {
        ReadFlow(
            prepare: { self.log.append("voix") },
            capture: { self.log.append("capture"); return capture },
            open: { self.log.append("ouvre:\($0):\($1)") },
            readClipboard: { self.log.append("presse-papiers"); return clipboard },
            openImage: {
                self.log.append("image")
                if !imageOpens { throw SillonError.Failed(message: "illisible") }
            },
            say: { self.log.append("dit:\($0)") },
            show: { self.log.append("montre") })
    }

    func testSelectionWinsAndVoiceStartsFirst() async {
        await flow(capture: .text("texte"), clipboard: .empty).run()
        XCTAssertEqual(log, ["voix", "capture", "ouvre:texte:sélection", "montre"])
    }

    func testUnsupportedFallsBackToClipboardText() async {
        await flow(capture: .unsupported, clipboard: .text("copié")).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "ouvre:copié:presse-papiers", "montre"])
    }

    func testNoSelectionFallsBackToClipboard() async {
        await flow(capture: .noSelection, clipboard: .text("copié")).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "ouvre:copié:presse-papiers", "montre"])
    }

    /// Seule une image passe par le moteur (reconnaissance de texte).
    func testImageGoesToTheEngine() async {
        await flow(capture: .unsupported, clipboard: .image).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "image", "montre"])
    }

    func testUnreadableImageSaysSo() async {
        await flow(capture: .unsupported, clipboard: .image, imageOpens: false).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "image",
                             "dit:Sélectionnez ou copiez du texte", "montre"])
    }

    func testNothingToReadSaysSoAndStillShows() async {
        await flow(capture: .unsupported, clipboard: .empty).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers",
                             "dit:Sélectionnez ou copiez du texte", "montre"])
    }

    func testSecureVetoReadsNothingAndSaysSo() async {
        await flow(capture: .veto(.secure), clipboard: .text("x")).run()
        XCTAssertEqual(log, ["voix", "capture", "dit:Champ sécurisé : rien n'est lu", "montre"])
    }

    func testOwnAppVetoReadsNothingAndSaysWhy() async {
        await flow(capture: .veto(.ownApp), clipboard: .text("x")).run()
        XCTAssertEqual(log, ["voix", "capture",
                             "dit:sillon est au premier plan : sélectionnez dans une autre application",
                             "montre"])
    }

    func testConcealedClipboardReadsNothingAndSaysSo() async {
        await flow(capture: .noSelection, clipboard: .concealed).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers",
                             "dit:Presse-papiers protégé : rien n'est lu", "montre"])
    }

    /// L'OCR d'une image tourne hors de l'acteur principal : le panneau ne
    /// paraît qu'une fois le texte ouvert, jamais avant.
    func testShowWaitsForAsyncImage() async {
        let flow = ReadFlow(
            prepare: {}, capture: { .unsupported }, open: { _, _ in },
            readClipboard: { .image },
            openImage: {
                try await Task.sleep(for: .milliseconds(50))
                await MainActor.run { self.log.append("image") }
            },
            say: { _ in }, show: { self.log.append("montre") })
        await flow.run()
        XCTAssertEqual(log, ["image", "montre"])
    }

    func testConcealedTypeIsDetectedOnAnyItem() {
        let plain = [SavedItem(types: [SavedType(type: "public.utf8-plain-text", data: Data())])]
        let hidden = plain + [SavedItem(types: [SavedType(type: concealedType, data: Data())])]
        XCTAssertFalse(isConcealed(plain))
        XCTAssertTrue(isConcealed(hidden))
    }
}

/// Le presse-papiers lu d'un seul coup : ce qui est vérifié est ce qui est lu.
final class ClipboardSnapshotTests: XCTestCase {
    private func board(_ types: [String: String]) -> FakePasteboard {
        let pb = FakePasteboard()
        pb.items = [SavedItem(types: types.map { SavedType(type: $0.key, data: Data($0.value.utf8)) })]
        return pb
    }

    func testText() {
        XCTAssertEqual(ClipboardSnapshot(pasteboard: board(["public.utf8-plain-text": "bonjour"])).read(),
                       .text("bonjour"))
    }

    func testBlankIsEmpty() {
        XCTAssertEqual(ClipboardSnapshot(pasteboard: board(["public.utf8-plain-text": " \n "])).read(), .empty)
        let none = FakePasteboard(); none.items = []
        XCTAssertEqual(ClipboardSnapshot(pasteboard: none).read(), .empty)
    }

    func testConcealedWinsOverText() {
        let pb = board(["public.utf8-plain-text": "secret", concealedType: ""])
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .concealed)
    }

    func testImage() {
        XCTAssertEqual(ClipboardSnapshot(pasteboard: board(["public.png": "x"])).read(), .image)
    }

    /// Une copie arrive entre la vérification et la lecture : on ne lit rien
    /// plutôt qu'un contenu jamais vérifié.
    func testClipboardChangingDuringReadIsEmpty() {
        let pb = board(["public.utf8-plain-text": "vérifié"])
        pb.lateCopy = (afterReads: 2, text: "arrivé entre-temps")
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .empty)
    }
}

/// Deux appuis rapprochés : une seule lecture à la fois, sinon la seconde
/// capture sauvegarderait la copie de la première comme presse-papiers.
@MainActor
final class ReadGateTests: XCTestCase {
    func testSecondRunIsRefusedWhileFirstIsBusy() async {
        let gate = ReadGate()
        var runs = 0
        let first = Task { @MainActor in
            await gate.run { runs += 1; try? await Task.sleep(for: .milliseconds(100)) }
        }
        await Task.yield()
        try? await Task.sleep(for: .milliseconds(20))
        let second = await gate.run { runs += 1 }
        let accepted = await first.value
        XCTAssertTrue(accepted)
        XCTAssertFalse(second)
        XCTAssertEqual(runs, 1)
        let third = await gate.run { runs += 1 }
        XCTAssertTrue(third)
        XCTAssertEqual(runs, 2)
    }
}

final class LoginItemTests: XCTestCase {
    func testOnlyAnInstalledReleaseIsEnabledAtFirstLaunch() {
        XCTAssertTrue(LoginItem.eligible(bundlePath: "/Applications/Sillon.app", debug: false))
        XCTAssertFalse(LoginItem.eligible(bundlePath: "/Applications/Sillon.app", debug: true))
        XCTAssertFalse(LoginItem.eligible(
            bundlePath: "/Users/x/Library/Developer/Xcode/DerivedData/B/Debug/Sillon.app", debug: false))
        XCTAssertFalse(LoginItem.eligible(bundlePath: "/ApplicationsX/Sillon.app", debug: false))
    }
}
