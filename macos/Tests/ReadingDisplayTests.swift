import XCTest
@testable import Sillage

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

    /// Police et taille viennent de config.toml, comme l'interlignage.
    func testFontAndSizeComeFromTheSettings() {
        var s = SettingsView.defaults
        XCTAssertEqual(ReadingDisplay(s).font, .literata)
        XCTAssertEqual(ReadingDisplay(s).size, 22)
        s.readingFont = "atkinson"; s.readingSize = 31
        XCTAssertEqual(ReadingDisplay(s).font, .atkinson)
        XCTAssertEqual(ReadingDisplay(s).size, 31)
        s.readingFont = "inconnue"; s.readingSize = .nan
        XCTAssertEqual(ReadingDisplay(s).font, .literata, "police inconnue : la police par défaut")
        XCTAssertEqual(ReadingDisplay(s).size, 22, "taille illisible : la taille par défaut")
    }
}
