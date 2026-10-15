import XCTest
@testable import Sillon

/// Le panneau lit des réglages du moteur l'interlignage et la règle.
final class ReadingDisplayTests: XCTestCase {
    func testTakesWhatThePanelNeedsFromTheSettings() {
        var s = SettingsView.defaults
        s.lineSpacing = 3; s.readingRuler = false
        let d = ReadingDisplay(s)
        XCTAssertEqual(d.lineSpacing, 3)
        XCTAssertFalse(d.readingRuler)
        XCTAssertEqual(ReadingDisplay(), ReadingDisplay(SettingsView.defaults))
    }

    func testAnEmptyPatchTouchesNothing() {
        XCTAssertTrue(SettingsPatch.empty.isEmpty)
        var p = SettingsPatch.empty
        p.speed = 1.2
        XCTAssertFalse(p.isEmpty)
    }
}
