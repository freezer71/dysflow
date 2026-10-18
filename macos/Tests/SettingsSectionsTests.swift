import XCTest
@testable import Sillage

@MainActor
final class SettingsSectionsTests: XCTestCase {
    /// Les trois polices, sous leur nom ; la valeur est celle de config.toml.
    func testTheFontsAreOfferedByName() {
        XCTAssertEqual(ReadingSection.fontOptions.map(\.title), ["Literata", "Atkinson Hyperlegible", "OpenDyslexic"])
        XCTAssertEqual(ReadingSection.fontOptions.map(\.value), ["literata", "atkinson", "openDyslexic"])
    }

    /// Une police écrite à la main et inconnue se montre comme celle qui
    /// s'affiche vraiment : Literata.
    func testAnUnknownFontShowsAsTheDefault() {
        XCTAssertEqual(ReadingSection.shownFont("atkinson"), "atkinson")
        XCTAssertEqual(ReadingSection.shownFont("comic-sans"), "literata")
    }

    /// La page porte les deux raccourcis globaux, pas plus.
    func testTheShortcutsAreTheTwoGlobalOnes() {
        XCTAssertEqual(ShortcutsSection.rows.map(\.identifier), ["raccourci-lecture-selection", "raccourci-afficher"])
        XCTAssertEqual(ShortcutsSection.rows.map(\.title), ["Lire la sélection", "Afficher / masquer le lecteur"])
    }
}
