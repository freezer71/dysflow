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
        XCTAssertEqual(Theme.paper.background.hex, "#FFFFFF")
        XCTAssertEqual(Theme.paper.wordFill.hex, "#6AA7EC")
        XCTAssertEqual(Theme.night.background.hex, "#191919")
        XCTAssertEqual(Theme.contrast.wordFill.hex, "#FFE600")
    }

    /// « Encre calme » : le mot lu change d'encre, le trait a sa couleur ;
    /// en contraste élevé, le mot reste blanc (il n'a plus de fond jaune) et
    /// le trait est jaune.
    func testCursorInkAndLine() {
        XCTAssertEqual(Theme.paper.wordText.hex, "#183347")
        XCTAssertEqual(Theme.night.wordText.hex, "#E6F1FF")
        XCTAssertEqual(Theme.contrast.wordText.hex, "#FFFFFF")
        XCTAssertEqual(Theme.contrast.line.hex, "#FFE600")
        for (name, theme) in [("papier", Theme.paper), ("nuit", .night), ("contraste", .contrast)] {
            XCTAssertGreaterThanOrEqual(Theme.contrastRatio(theme.wordText, theme.background), 7, name)
        }
    }

    /// Les contrôles (temps, vitesse, aides) se lisent : 4,5:1 au moins sur
    /// le fond, comme le texte courant (WCAG AA).
    func testChromeIsReadableOnItsBackground() {
        for (name, theme) in [("papier", Theme.paper), ("nuit", .night), ("contraste", .contrast)] {
            XCTAssertGreaterThanOrEqual(Theme.contrastRatio(theme.chrome, theme.background), 4.5, name)
        }
        // 7:1 au moins, en clair comme en sombre : à 13 pt, les traits fins
        // du temps (« –:–– ») paraissent plus pâles que leur couleur, et
        // 5,2:1 échouait à l'audit d'accessibilité de macOS.
        for (name, theme) in [("papier", Theme.paper), ("nuit", .night)] {
            XCTAssertGreaterThanOrEqual(Theme.contrastRatio(theme.chrome, theme.background), 7, name)
        }
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
