import XCTest
@testable import Sillon

final class ThemeTests: XCTestCase {
    func testResolution() {
        XCTAssertEqual(Theme.resolve(dark: false, increaseContrast: false), .paper)
        XCTAssertEqual(Theme.resolve(dark: true, increaseContrast: false), .night)
        XCTAssertEqual(Theme.resolve(dark: false, increaseContrast: true), .contrast)
        XCTAssertEqual(Theme.resolve(dark: true, increaseContrast: true), .contrast)
    }

    func testPaperColorsMatchTheSpec() {
        XCTAssertEqual(Theme.paper.background.hex, "#FBF8F1")
        XCTAssertEqual(Theme.paper.wordFill.hex, "#CFE3FF")
        XCTAssertEqual(Theme.night.background.hex, "#1E1B17")
        XCTAssertEqual(Theme.contrast.wordFill.hex, "#FFE600")
    }

    /// Les contrôles (temps, vitesse, aides) se lisent : 4,5:1 au moins sur
    /// le fond, comme le texte courant (WCAG AA).
    func testChromeIsReadableOnItsBackground() {
        for (name, theme) in [("papier", Theme.paper), ("nuit", .night), ("contraste", .contrast)] {
            XCTAssertGreaterThanOrEqual(Theme.contrastRatio(theme.chrome, theme.background), 4.5, name)
        }
        // 7,3:1 en clair : à 13 pt, les traits fins du temps (« –:–– »)
        // paraissent plus pâles que leur couleur, et 5,2:1 échouait à
        // l'audit d'accessibilité de macOS.
        XCTAssertEqual(Theme.paper.chrome.hex, "#575249")
    }

    func testBundledFontsResolve() {
        ReadingFont.registerBundled()
        for font in ReadingFont.allCases {
            let nsFont = font.font(size: 20)
            XCTAssertEqual(nsFont.familyName, font.familyName, "\(font)")
        }
    }

    /// Chaque famille donne une vraie graisse grasse et une vraie italique.
    func testBoldAndItalicFacesAreReal() {
        ReadingFont.registerBundled()
        for font in ReadingFont.allCases {
            let bold = font.font(size: 20, bold: true)
            let italic = font.font(size: 20, italic: true)
            XCTAssertEqual(bold.familyName, font.familyName, "\(font) gras")
            XCTAssertEqual(italic.familyName, font.familyName, "\(font) italique")
            XCTAssertTrue(NSFontManager.shared.traits(of: bold).contains(.boldFontMask)
                          || bold.fontDescriptor.symbolicTraits.contains(.bold), "\(font) gras")
            XCTAssertTrue(NSFontManager.shared.traits(of: italic).contains(.italicFontMask), "\(font) italique")
        }
    }
}
