import XCTest
@testable import Sillon

final class DisplayModeStoreTests: XCTestCase {
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.affichage.magasin"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testAFreshStoreStartsInTheBackground() {
        XCTAssertEqual(DisplayModeStore(defaults: freshDefaults()).state, .initial)
        XCTAssertNil(DisplayModeStore(defaults: freshDefaults()).smallOrigin)
    }

    func testTheModeSurvivesARelaunch() {
        let defaults = freshDefaults()
        let state = DisplayState.initial.applying(.choose(.small)).applying(.escape)
        DisplayModeStore(defaults: defaults).state = state
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, state)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.lastVisible, .small)
    }

    func testUnreadableValuesFallBackToTheDefaults() {
        let defaults = freshDefaults()
        defaults.set("géant", forKey: DisplayModeStore.currentKey)
        defaults.set("background", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, .initial)
    }

    func testTheSmallBarPlaceSurvivesARelaunch() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).smallOrigin = CGPoint(x: 120, y: 640)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).smallOrigin, CGPoint(x: 120, y: 640))
        defaults.set(["a"], forKey: DisplayModeStore.smallOriginKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).smallOrigin, "valeur illisible : pas de place")
        let store = DisplayModeStore(defaults: defaults)
        store.smallOrigin = CGPoint(x: 1, y: 2)
        store.smallOrigin = nil
        XCTAssertNil(DisplayModeStore(defaults: defaults).smallOrigin)
    }

    /// Sous les tests, le mode ne va jamais dans les préférences réelles.
    func testUnderTestsTheModeIsNotStoredInTheUserDefaults() {
        let defaults = DisplayModeStore.processDefaults()
        XCTAssertFalse(defaults === UserDefaults.standard)
        let before = UserDefaults.standard.string(forKey: DisplayModeStore.currentKey)
        DisplayModeStore(defaults: defaults).state = DisplayState(current: .large, lastVisible: .large)
        XCTAssertEqual(UserDefaults.standard.string(forKey: DisplayModeStore.currentKey), before)
    }
}

final class SmallPlacementTests: XCTestCase {
    private let main = CGRect(x: 0, y: 0, width: 1440, height: 875)
    private let side = CGRect(x: 1440, y: 0, width: 1920, height: 1055)

    func testWithoutASavedPlaceTheBarSitsOnTheMainScreen() {
        let frame = SmallPlacement.frame(savedOrigin: nil, screens: [main, side], main: main)
        XCTAssertEqual(frame, SmallPlacement.defaultFrame(on: main))
        XCTAssertEqual(frame.size, SmallPlacement.size)
        XCTAssertTrue(main.contains(frame))
    }

    func testASavedPlaceOnAScreenIsKept() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main, side], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1600, y: 500))
    }

    func testAPlaceOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main], main: main)
        XCTAssertEqual(frame, SmallPlacement.defaultFrame(on: main))
    }

    func testABarHalfOffTheEdgeIsPulledBackOnScreen() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1440 - 340, y: 100))
        XCTAssertTrue(main.contains(frame))
        // Deux écrans : celui qui en montre le plus la reçoit.
        let shared = SmallPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main, side], main: main)
        XCTAssertEqual(shared.origin, CGPoint(x: 1440, y: 100))
    }

    func testWithoutAnyScreenTheMainFrameIsUsed() {
        XCTAssertEqual(SmallPlacement.frame(savedOrigin: CGPoint(x: 5, y: 5), screens: [], main: main),
                       SmallPlacement.defaultFrame(on: main))
    }

    func testASecondScreenWithANegativeOriginKeepsItsPlace() {
        let left = CGRect(x: -1920, y: -100, width: 1920, height: 1080)
        let origin = CGPoint(x: -1000, y: 300)
        let frame = SmallPlacement.frame(savedOrigin: origin, screens: [main, left], main: main)
        XCTAssertEqual(frame, CGRect(origin: origin, size: SmallPlacement.size))
        // À cheval sur x = 0 : l'écran qui en montre le plus la reçoit, entière.
        let straddling = CGPoint(x: -SmallPlacement.size.width * 0.75, y: 300)
        let pulled = SmallPlacement.frame(savedOrigin: straddling, screens: [main, left], main: main)
        XCTAssertEqual(pulled.origin, CGPoint(x: -SmallPlacement.size.width, y: 300))
        XCTAssertTrue(left.contains(pulled))
        let other = CGPoint(x: -SmallPlacement.size.width * 0.25, y: 300)
        let onMain = SmallPlacement.frame(savedOrigin: other, screens: [main, left], main: main)
        XCTAssertEqual(onMain.origin, CGPoint(x: 0, y: 300))
        XCTAssertTrue(main.contains(onMain))
    }

    func testABarNearTheTopEdgeStaysBelowTheMenuBar() {
        // visibleFrame : la barre des menus (24 pt) est déjà retranchée.
        let visible = CGRect(x: 0, y: 0, width: 1440, height: 851)
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 200, y: 851 - 10), screens: [visible], main: visible)
        XCTAssertEqual(frame.maxY, visible.maxY)
        XCTAssertTrue(visible.contains(frame))
        XCTAssertEqual(frame.size, SmallPlacement.size)
    }

    func testAPlaceFarOutsideEveryScreenReturnsToTheDefaultFrame() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 9000, y: -7000), screens: [main, side], main: main)
        XCTAssertEqual(frame, SmallPlacement.defaultFrame(on: main))
    }
}
