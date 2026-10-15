import SwiftUI
import XCTest
@testable import Sillon

@MainActor
final class BarControlsTests: XCTestCase {
    func testSpeedsAreWrittenTheFrenchWayWithoutUselessZeros() {
        XCTAssertEqual(SpeedMenu.choices.map(SpeedFormat.text), ["0,75×", "1×", "1,25×", "1,5×", "2×"])
        XCTAssertEqual(SpeedFormat.text(1.1), "1,1×", "une vitesse hors du menu, après +")
        XCTAssertEqual(SpeedFormat.text(1 + 0.1 + 0.1 + 0.1), "1,3×", "les flottants n'ajoutent pas de chiffres")
        XCTAssertEqual(SpeedFormat.spoken(1.25), "1,25 fois")
    }

    /// Un clic sur la barre : la phrase sous le pointeur, bornée au document.
    func testAClickOnTheTimelineGivesTheSentenceUnderThePointer() {
        XCTAssertEqual(SeekableTimeline.fraction(x: 50, width: 200), 0.25)
        XCTAssertEqual(SeekableTimeline.fraction(x: -10, width: 200), 0)
        XCTAssertEqual(SeekableTimeline.fraction(x: 400, width: 200), 1)
        XCTAssertEqual(SeekableTimeline.fraction(x: 10, width: 0), 0)
        XCTAssertEqual(ProgressSeek.sentence(fraction: 0.25, sentenceCount: 8), 2)
        XCTAssertEqual(ProgressSeek.sentence(fraction: 1, sentenceCount: 8), 7, "la fin : la dernière phrase")
        XCTAssertEqual(ProgressSeek.sentence(fraction: 0, sentenceCount: 8), 0)
        XCTAssertNil(ProgressSeek.sentence(fraction: 0.5, sentenceCount: 0), "document vide : rien")
        XCTAssertNil(ProgressSeek.sentence(fraction: .nan, sentenceCount: 8))
    }

    /// ⏮ et ⏭ font ce que font ← et → : une phrase.
    func testTheSentenceButtonsSendTheArrowActions() {
        XCTAssertEqual(SentenceTransport.previous.action,
                       KeyMap.action(for: KeyPress(characters: "", keyCode: KeyCode.left)))
        XCTAssertEqual(SentenceTransport.next.action,
                       KeyMap.action(for: KeyPress(characters: "", keyCode: KeyCode.right)))
        var sent: [ReaderAction] = []
        TransportButton(item: SentenceTransport.next) { sent.append($0) }.fire()
        XCTAssertEqual(sent, [.command(.sentences(delta: 1))])
    }

    func testEveryNewBarButtonHasAComfortableTarget() {
        for view in [AnyView(TransportButton(item: SentenceTransport.previous) { _ in }),
                     AnyView(BarIconButton(symbol: "xmark", label: "Fermer", help: "Fermer (⌘W)",
                                           identifier: "fermer") {})] {
            let size = NSHostingView(rootView: view).fittingSize
            XCTAssertGreaterThanOrEqual(size.width, 32)
            XCTAssertGreaterThanOrEqual(size.height, 32)
        }
    }

    func testTheEmptyReaderInvitesToPasteOrDrop() {
        XCTAssertEqual(PanelInvitation.text, "Collez un texte (⌘V) ou déposez un fichier")
    }
}
