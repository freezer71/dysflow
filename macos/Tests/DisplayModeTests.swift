import XCTest
@testable import Sillon

final class DisplayModeTests: XCTestCase {
    private let start = DisplayState.initial

    func testAtFirstNothingIsShownAndMediumComesBack() {
        XCTAssertEqual(start.current, .background)
        XCTAssertEqual(start.lastVisible, .medium)
    }

    func testChoosingAVisibleModeRemembersIt() {
        let s = start.applying(.choose(.small))
        XCTAssertEqual(s.current, .small)
        XCTAssertEqual(s.lastVisible, .small)
    }

    /// Échap dans le grand mode : d'abord le moyen, puis l'arrière-plan.
    func testEscapeIsTwoStepsFromLarge() {
        let large = start.applying(.choose(.large))
        let medium = large.applying(.escape)
        XCTAssertEqual(medium.current, .medium)
        XCTAssertEqual(medium.lastVisible, .medium)
        let hidden = medium.applying(.escape)
        XCTAssertEqual(hidden.current, .background)
        XCTAssertEqual(hidden.lastVisible, .medium)
    }

    func testEscapeFromSmallGoesToTheBackground() {
        let s = start.applying(.choose(.small)).applying(.escape)
        XCTAssertEqual(s.current, .background)
        XCTAssertEqual(s.lastVisible, .small)
    }

    func testEscapeInTheBackgroundStaysThere() {
        XCTAssertEqual(start.applying(.escape), start)
    }

    /// ⌘W, fermeture : l'arrière-plan d'un coup, même depuis le grand mode.
    func testHideGoesStraightToTheBackground() {
        let s = start.applying(.choose(.large)).applying(.hide)
        XCTAssertEqual(s.current, .background)
        XCTAssertEqual(s.lastVisible, .large)
    }

    /// ⌃⌥S : le dernier mode visible.
    func testShowLastBringsBackTheLastVisibleMode() {
        let s = start.applying(.choose(.small)).applying(.escape).applying(.showLast)
        XCTAssertEqual(s.current, .small)
    }

    /// Le dernier mode visible n'est jamais l'arrière-plan : ⌃⌥S montre
    /// toujours quelque chose.
    func testLastVisibleIsNeverTheBackground() {
        let s = DisplayState(current: .background, lastVisible: .background)
        XCTAssertEqual(s.lastVisible, .medium)
        XCTAssertEqual(s.applying(.showLast).current, .medium)
        XCTAssertEqual(start.applying(.choose(.background)).lastVisible, .medium)
    }

    /// ⌃⌥Espace garde le mode visible ; depuis l'arrière-plan, le moyen.
    func testReadSelectionKeepsAVisibleModeAndOpensMediumFromTheBackground() {
        let small = start.applying(.choose(.small))
        XCTAssertEqual(small.applying(.readSelection), small)
        let opened = small.applying(.escape).applying(.readSelection)
        XCTAssertEqual(opened.current, .medium)
        XCTAssertEqual(opened.lastVisible, .medium)
    }

    func testOnlyTheBackgroundIsInvisible() {
        XCTAssertEqual(DisplayMode.allCases.filter { !$0.isVisible }, [.background])
    }
}
