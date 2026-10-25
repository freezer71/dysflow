import XCTest
@testable import Dysflow

final class MiniPlacementTests: XCTestCase {
    private let main = CGRect(x: 0, y: 0, width: 1440, height: 875)
    private let side = CGRect(x: 1440, y: 0, width: 1920, height: 1055)

    func testWithoutASavedPlaceTheBarSitsOnTheMainScreen() {
        let frame = MiniPlacement.frame(savedOrigin: nil, screens: [main, side], main: main)
        XCTAssertEqual(frame, MiniPlacement.defaultFrame(on: main))
        XCTAssertEqual(frame.size, MiniPlacement.size)
        XCTAssertTrue(main.contains(frame))
    }

    func testASavedPlaceOnAScreenIsKept() {
        let frame = MiniPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main, side], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1600, y: 500))
    }

    func testAPlaceOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let frame = MiniPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main], main: main)
        XCTAssertEqual(frame, MiniPlacement.defaultFrame(on: main))
    }

    func testABarHalfOffTheEdgeIsPulledBackOnScreen() {
        let frame = MiniPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1440 - 340, y: 100))
        XCTAssertTrue(main.contains(frame))
        // Deux écrans : celui qui en montre le plus la reçoit.
        let shared = MiniPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main, side], main: main)
        XCTAssertEqual(shared.origin, CGPoint(x: 1440, y: 100))
    }

    func testWithoutAnyScreenTheMainFrameIsUsed() {
        XCTAssertEqual(MiniPlacement.frame(savedOrigin: CGPoint(x: 5, y: 5), screens: [], main: main),
                       MiniPlacement.defaultFrame(on: main))
    }

    func testASecondScreenWithANegativeOriginKeepsItsPlace() {
        let left = CGRect(x: -1920, y: -100, width: 1920, height: 1080)
        let origin = CGPoint(x: -1000, y: 300)
        let frame = MiniPlacement.frame(savedOrigin: origin, screens: [main, left], main: main)
        XCTAssertEqual(frame, CGRect(origin: origin, size: MiniPlacement.size))
        // À cheval sur x = 0 : l'écran qui en montre le plus la reçoit, entière.
        let straddling = CGPoint(x: -MiniPlacement.size.width * 0.75, y: 300)
        let pulled = MiniPlacement.frame(savedOrigin: straddling, screens: [main, left], main: main)
        XCTAssertEqual(pulled.origin, CGPoint(x: -MiniPlacement.size.width, y: 300))
        XCTAssertTrue(left.contains(pulled))
        let other = CGPoint(x: -MiniPlacement.size.width * 0.25, y: 300)
        let onMain = MiniPlacement.frame(savedOrigin: other, screens: [main, left], main: main)
        XCTAssertEqual(onMain.origin, CGPoint(x: 0, y: 300))
        XCTAssertTrue(main.contains(onMain))
    }

    func testABarNearTheTopEdgeStaysBelowTheMenuBar() {
        // visibleFrame : la barre des menus (24 pt) est déjà retranchée.
        let visible = CGRect(x: 0, y: 0, width: 1440, height: 851)
        let frame = MiniPlacement.frame(savedOrigin: CGPoint(x: 200, y: 851 - 10), screens: [visible], main: visible)
        XCTAssertEqual(frame.maxY, visible.maxY)
        XCTAssertTrue(visible.contains(frame))
        XCTAssertEqual(frame.size, MiniPlacement.size)
    }

    func testAPlaceFarOutsideEveryScreenReturnsToTheDefaultFrame() {
        let frame = MiniPlacement.frame(savedOrigin: CGPoint(x: 9000, y: -7000), screens: [main, side], main: main)
        XCTAssertEqual(frame, MiniPlacement.defaultFrame(on: main))
    }
}

final class ReaderPlacementTests: XCTestCase {
    private let main = CGRect(x: 0, y: 0, width: 1440, height: 875)
    private let side = CGRect(x: 1440, y: 0, width: 1920, height: 1055)

    func testWithoutASavedFrameTheReaderSitsOnTheMainScreen() {
        let frame = ReaderPlacement.frame(saved: nil, screens: [main, side], main: main)
        XCTAssertEqual(frame, ReaderPlacement.defaultFrame(on: main))
        XCTAssertTrue(main.contains(frame))
        XCTAssertEqual(frame.midX, main.midX, accuracy: 1)
        XCTAssertLessThanOrEqual(frame.width, PanelGeometry.maximumWidth)
    }

    func testASavedFrameOnAScreenIsKept() {
        let saved = CGRect(x: 1600, y: 300, width: 900, height: 600)
        XCTAssertEqual(ReaderPlacement.frame(saved: saved, screens: [main, side], main: main), saved)
    }

    func testAReaderFrameOffEveryScreenComesBackOnTheMainScreen() {
        let saved = CGRect(x: 1600, y: 300, width: 900, height: 600)
        XCTAssertEqual(ReaderPlacement.frame(saved: saved, screens: [main], main: main),
                       ReaderPlacement.defaultFrame(on: main))
    }

    func testAReaderFrameLargerThanTheScreenIsShrunk() {
        let saved = CGRect(x: -100, y: -100, width: 5000, height: 3000)
        let frame = ReaderPlacement.frame(saved: saved, screens: [main], main: main)
        XCTAssertEqual(frame, main)
    }

    func testATinyFrameGrowsToTheMinimum() {
        let saved = CGRect(x: 100, y: 100, width: 50, height: 40)
        let frame = ReaderPlacement.frame(saved: saved, screens: [main], main: main)
        XCTAssertEqual(frame.size, ReaderPlacement.minimumSize)
        XCTAssertTrue(main.contains(frame))
    }
}

@MainActor
final class PanelSizeLimitsTests: XCTestCase {
    /// Le lecteur ne se réduit pas sous sa taille minimale (l'en-tête et la
    /// barre se chevaucheraient) ; le mini retrouve la sienne.
    func testTheReaderCannotShrinkBelowItsMinimumAndTheMiniGetsItsOwn() {
        let name = "dysflow.tests.affichage.limites"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        // Jamais montrée : la fenêtre n'existe que pour ses limites.
        let window = NSWindow(contentRect: CGRect(origin: .zero, size: MiniPlacement.size),
                              styleMask: [.borderless], backing: .buffered, defer: true)
        let placement = PanelPlacement(window: window, store: DisplayModeStore(defaults: defaults), mode: { .reader })
        placement.applySizeLimits(for: .reader)
        XCTAssertEqual(window.minSize, ReaderPlacement.minimumSize)
        XCTAssertTrue(window.styleMask.contains(.resizable))
        placement.applySizeLimits(for: .mini)
        XCTAssertEqual(window.minSize, MiniPlacement.size)
        XCTAssertFalse(window.styleMask.contains(.resizable))
    }
}
