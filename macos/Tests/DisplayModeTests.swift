import XCTest
@testable import Sillage

final class DisplayModeTests: XCTestCase {
    private let start = DisplayState.initial

    func testAtFirstNothingIsShownAndTheReaderComesBack() {
        XCTAssertEqual(start.current, .hidden)
        XCTAssertEqual(start.lastVisible, .reader)
    }

    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière
    /// taille ; une forme visible ne change pas.
    func testShowGivesTheLastSizeAndKeepsAVisibleForm() {
        XCTAssertEqual(start.applying(.show).current, .reader)
        let mini = start.applying(.choose(.mini))
        XCTAssertEqual(mini.applying(.show), mini)
        XCTAssertEqual(mini.applying(.close).applying(.show).current, .mini)
    }

    /// ⌘M : du lecteur au mini et retour ; masqué, rien.
    func testToggleSizeSwitchesBetweenReaderAndMini() {
        let reader = start.applying(.show)
        XCTAssertEqual(reader.applying(.toggleSize).current, .mini)
        XCTAssertEqual(reader.applying(.toggleSize).lastVisible, .mini)
        XCTAssertEqual(reader.applying(.toggleSize).applying(.toggleSize).current, .reader)
        XCTAssertEqual(start.applying(.toggleSize), start)
    }

    /// Fermer : masqué, jamais le mini ; la taille fermée revient ensuite.
    func testClosingNeverLeavesTheMini() {
        let closed = start.applying(.show).applying(.close)
        XCTAssertEqual(closed.current, .hidden)
        XCTAssertEqual(closed.lastVisible, .reader)
        XCTAssertEqual(closed.applying(.close), closed)
    }

    /// ⌃⌥S : affiche la dernière taille, ou masque.
    func testToggleVisibilityShowsOrHides() {
        let mini = start.applying(.choose(.mini))
        let hidden = mini.applying(.toggleVisibility)
        XCTAssertEqual(hidden.current, .hidden)
        XCTAssertEqual(hidden.applying(.toggleVisibility).current, .mini)
    }

    /// Fermer et ⌃⌥S qui masque arrêtent la lecture ; réduire et agrandir non.
    func testOnlyHidingStopsTheReading() {
        let reader = start.applying(.show)
        XCTAssertTrue(reader.hides(reader.applying(.close)))
        XCTAssertTrue(reader.hides(reader.applying(.toggleVisibility)))
        XCTAssertFalse(reader.hides(reader.applying(.toggleSize)))
        XCTAssertFalse(start.hides(start.applying(.close)), "déjà masqué")
    }

    /// Le dernier mode visible n'est jamais « masqué ».
    func testLastVisibleIsNeverHidden() {
        let s = DisplayState(current: .hidden, lastVisible: .hidden)
        XCTAssertEqual(s.lastVisible, .reader)
        XCTAssertEqual(start.applying(.choose(.hidden)).lastVisible, .reader)
    }

    /// Une relance ne montre rien et garde la dernière taille.
    func testARelaunchShowsNothingButKeepsTheLastSize() {
        let mini = start.applying(.choose(.mini)).relaunched
        XCTAssertEqual(mini.current, .hidden)
        XCTAssertEqual(mini.lastVisible, .mini)
    }

    /// Les valeurs retenues par la version précédente se traduisent.
    func testModesStoredBeforeTheUpdateAreTranslated() {
        XCTAssertEqual(DisplayMode.stored("small"), .mini)
        XCTAssertEqual(DisplayMode.stored("medium"), .reader)
        XCTAssertEqual(DisplayMode.stored("large"), .reader)
        XCTAssertEqual(DisplayMode.stored("background"), .hidden)
        XCTAssertEqual(DisplayMode.stored("mini"), .mini)
        XCTAssertNil(DisplayMode.stored("géant"))
    }
}
