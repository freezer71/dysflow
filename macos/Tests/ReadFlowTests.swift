import XCTest
@testable import Dysflow

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
            openImage: { data in
                self.log.append("image:\(String(decoding: data, as: UTF8.self))")
                if !imageOpens { throw DysflowError.Failed(message: "illisible") }
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

    /// Seule une image passe par le moteur (reconnaissance de texte), avec
    /// les octets vérifiés : le moteur ne relit pas le presse-papiers.
    func testImageGoesToTheEngine() async {
        await flow(capture: .unsupported, clipboard: .image(Data("png".utf8))).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "image:png", "montre"])
    }

    func testUnreadableImageSaysSo() async {
        await flow(capture: .unsupported, clipboard: .image(Data("png".utf8)), imageOpens: false).run()
        XCTAssertEqual(log, ["voix", "capture", "presse-papiers", "image:png",
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
                             "dit:dysflow est au premier plan : sélectionnez dans une autre application",
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
        let ocr = TestGate()
        let flow = ReadFlow(
            prepare: {}, capture: { .unsupported }, open: { _, _ in },
            readClipboard: { .image(Data()) },
            openImage: { _ in
                await ocr.wait()
                await MainActor.run { self.log.append("image") }
            },
            say: { _ in }, show: { self.log.append("montre") })
        let running = Task { await flow.run() }
        // L'OCR est en cours (suspendue à la porte) : rien n'est montré.
        let started = await eventuallyTrue { ocr.waiting == 1 }
        XCTAssertTrue(started)
        XCTAssertEqual(log, [])
        ocr.open()
        await running.value
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
        XCTAssertEqual(ClipboardSnapshot(pasteboard: board(["public.png": "x"])).read(),
                       .image(Data("x".utf8)))
    }

    /// Numbers, Excel, Keynote, Pages posent une image à côté du texte : le
    /// texte l'emporte, la reconnaissance ne sert qu'à défaut.
    func testTextWinsOverAnAccompanyingImage() {
        let pb = board(["public.utf8-plain-text": "A1\tB1", "public.png": "x"])
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .text("A1\tB1"))
    }

    /// Une image copiée depuis un navigateur porte souvent son adresse en
    /// texte : l'adresse seule ne vaut pas le contenu de l'image.
    func testAnImageWhoseTextIsOnlyItsAddressIsAnImage() {
        let pb = board(["public.utf8-plain-text": " https://exemple.fr/schema.png\n", "public.tiff": "x"])
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .image(Data("x".utf8)))
    }

    func testAnAddressWithoutImageIsText() {
        let pb = board(["public.utf8-plain-text": "https://exemple.fr"])
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .text("https://exemple.fr"))
    }

    /// L'image change pendant la lecture : rien n'est passé au moteur.
    func testImageChangingDuringReadIsEmpty() {
        let pb = board(["public.png": "x"])
        pb.lateCopy = (afterReads: 2, text: "arrivé entre-temps")
        XCTAssertEqual(ClipboardSnapshot(pasteboard: pb).read(), .empty)
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
        let release = TestGate()
        var runs = 0
        let first = Task { @MainActor in
            await gate.run { runs += 1; await release.wait() }
        }
        // La première lecture est entrée et attend : la porte est prise.
        let busy = await eventuallyTrue { release.waiting == 1 }
        XCTAssertTrue(busy)
        let second = await gate.run { runs += 1 }
        release.open()
        let accepted = await first.value
        XCTAssertTrue(accepted)
        XCTAssertFalse(second)
        XCTAssertEqual(runs, 1)
        let third = await gate.run { runs += 1 }
        XCTAssertTrue(third)
        XCTAssertEqual(runs, 2)
    }
}

/// En mode mesure (`--measure`, `--measure-charge`), le modèle et le panneau
/// ne sont jamais créés : « Lire le presse-papiers » plantait (déballage forcé).
@MainActor
final class ReadWithoutModelTests: XCTestCase {
    func testReadingWithoutAModelDoesNothing() async {
        let delegate = AppDelegate()
        XCTAssertNil(delegate.model)
        XCTAssertNil(delegate.panel)
        await delegate.read()
        await delegate.read(capture: { .unsupported })
    }
}

final class LoginItemTests: XCTestCase {
    func testOnlyAnInstalledReleaseIsEnabledAtFirstLaunch() {
        XCTAssertTrue(LoginItem.eligible(bundlePath: "/Applications/Dysflow.app", debug: false))
        XCTAssertFalse(LoginItem.eligible(bundlePath: "/Applications/Dysflow.app", debug: true))
        XCTAssertFalse(LoginItem.eligible(
            bundlePath: "/Users/x/Library/Developer/Xcode/DerivedData/B/Debug/Dysflow.app", debug: false))
        XCTAssertFalse(LoginItem.eligible(bundlePath: "/ApplicationsX/Dysflow.app", debug: false))
    }
}
