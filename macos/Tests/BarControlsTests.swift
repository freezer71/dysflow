import SwiftUI
import XCTest
@testable import Sillage

@MainActor
final class BarControlsTests: XCTestCase {
    func testSpeedsAreWrittenTheFrenchWayWithoutUselessZeros() {
        XCTAssertEqual(SpeedMenu.choices.map(SpeedFormat.text), ["0,75×", "1×", "1,25×", "1,5×", "2×"])
        XCTAssertEqual(SpeedFormat.text(1.1), "1,1×", "une vitesse hors du menu, après +")
        XCTAssertEqual(SpeedFormat.text(1 + 0.1 + 0.1 + 0.1), "1,3×", "les flottants n'ajoutent pas de chiffres")
        XCTAssertEqual(SpeedFormat.spoken(1.25), "1,25 fois")
    }

    /// Le volume : son icône suit le niveau, barrée quand le son est coupé.
    func testTheVolumeButtonShowsTheLevel() {
        XCTAssertEqual(VolumeFormat.text(1), "100 %")
        XCTAssertEqual(VolumeFormat.text(0.35), "35 %")
        XCTAssertEqual(VolumeFormat.symbol(volume: 1, muted: true), "speaker.slash.fill")
        XCTAssertEqual(VolumeFormat.symbol(volume: 0, muted: false), "speaker.slash.fill")
        XCTAssertEqual(VolumeFormat.symbol(volume: 0.2, muted: false), "speaker.wave.1.fill")
        XCTAssertEqual(VolumeFormat.symbol(volume: 0.5, muted: false), "speaker.wave.2.fill")
        XCTAssertEqual(VolumeFormat.symbol(volume: 1.5, muted: false), "speaker.wave.3.fill")
    }

    /// Le curseur du volume : la valeur sous le pointeur, au pas de 5 %.
    func testTheVolumeSliderGivesTheValueUnderThePointer() {
        XCTAssertEqual(VolumeFormat.value(x: 90, width: 180), 1)
        XCTAssertEqual(VolumeFormat.value(x: 50, width: 180), 0.55, accuracy: 0.0001, "arrondi au pas de 5 %")
        XCTAssertEqual(VolumeFormat.value(x: -20, width: 180), 0)
        XCTAssertEqual(VolumeFormat.value(x: 400, width: 180), 2)
        XCTAssertEqual(VolumeFormat.value(x: 10, width: 0), 0)
        XCTAssertEqual(VolumeFormat.snapped(1.98 + 0.05), 2, "borné à 200 %")
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
