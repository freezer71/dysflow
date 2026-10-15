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

    func testAFreshStoreStartsHidden() {
        let store = DisplayModeStore(defaults: freshDefaults())
        XCTAssertEqual(store.state, .initial)
        XCTAssertNil(store.miniOrigin)
        XCTAssertNil(store.readerFrame)
    }

    func testTheLastSizeSurvivesARelaunch() {
        let defaults = freshDefaults()
        let state = DisplayState.initial.applying(.choose(.mini)).applying(.close)
        DisplayModeStore(defaults: defaults).state = state
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, state)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.lastVisible, .mini)
    }

    /// Préférences de la version précédente : traduites, pas perdues.
    func testModesStoredBeforeTheUpdateAreTranslated() {
        let defaults = freshDefaults()
        defaults.set("background", forKey: DisplayModeStore.currentKey)
        defaults.set("small", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state,
                       DisplayState(current: .hidden, lastVisible: .mini))
        defaults.set("large", forKey: DisplayModeStore.currentKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.current, .reader)
    }

    func testUnreadableValuesFallBackToTheDefaults() {
        let defaults = freshDefaults()
        defaults.set("géant", forKey: DisplayModeStore.currentKey)
        defaults.set("hidden", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, .initial)
    }

    func testTheMiniPlaceSurvivesARelaunch() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).miniOrigin = CGPoint(x: 120, y: 640)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).miniOrigin, CGPoint(x: 120, y: 640))
        defaults.set(["a"], forKey: DisplayModeStore.miniOriginKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).miniOrigin, "valeur illisible : pas de place")
    }

    func testTheReaderFrameSurvivesARelaunch() {
        let defaults = freshDefaults()
        let frame = CGRect(x: 100, y: 200, width: 700, height: 480)
        DisplayModeStore(defaults: defaults).readerFrame = frame
        XCTAssertEqual(DisplayModeStore(defaults: defaults).readerFrame, frame)
        defaults.set([1, 2, Double.nan, 4], forKey: DisplayModeStore.readerFrameKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).readerFrame, "valeur illisible : pas de cadre")
        let store = DisplayModeStore(defaults: defaults)
        store.readerFrame = nil
        XCTAssertNil(DisplayModeStore(defaults: defaults).readerFrame)
    }

    /// Sous les tests, la forme ne va jamais dans les préférences réelles.
    func testUnderTestsTheModeIsNotStoredInTheUserDefaults() {
        let defaults = DisplayModeStore.processDefaults()
        XCTAssertFalse(defaults === UserDefaults.standard)
        let before = UserDefaults.standard.string(forKey: DisplayModeStore.currentKey)
        DisplayModeStore(defaults: defaults).state = DisplayState(current: .reader, lastVisible: .reader)
        XCTAssertEqual(UserDefaults.standard.string(forKey: DisplayModeStore.currentKey), before)
    }
}
