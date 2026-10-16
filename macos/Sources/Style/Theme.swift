import AppKit
import SwiftUI

extension NSColor {
    convenience init(hex: UInt32) {
        self.init(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                  green: CGFloat((hex >> 8) & 0xFF) / 255,
                  blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }

    var hex: String {
        let c = usingColorSpace(.sRGB) ?? self
        return String(format: "#%02X%02X%02X",
                      Int((c.redComponent * 255).rounded()),
                      Int((c.greenComponent * 255).rounded()),
                      Int((c.blueComponent * 255).rounded()))
    }
}

/// Les couleurs du lecteur, au style « Felt » (15 oct. 2026) : des gris
/// neutres, des zones pleines sans bordure, un vert citron pour ce qui est
/// lu. La ligne lue passe sous une bande légère ; le mot lu, sous une
/// pastille citron qui glisse de mot en mot et se remplit avec la voix. Le
/// contraste élevé dès que macOS le demande.
struct Theme: Equatable {
    var background: NSColor
    var text: NSColor
    var past: NSColor
    /// Teinte de la bande de lecture, posée à `bandAlpha`.
    var wordFill: NSColor
    var bandAlpha: CGFloat
    /// La pastille du mot lu, posée à `pillAlpha` ; la part que la voix a
    /// dite s'y remplit, à `wipeAlpha` de plus.
    var pill: NSColor
    var pillAlpha: CGFloat
    var wipeAlpha: CGFloat
    /// L'encre du mot lu : plus marquée que le texte, sans fond ni changement
    /// de graisse.
    var wordText: NSColor
    /// Le trait sous la ligne lue, la part lue de la progression, le disque
    /// de lecture.
    var line: NSColor
    /// Ce qui s'écrit sur `line` (le symbole du disque de lecture).
    var onLine: NSColor
    /// Texte des contrôles.
    var chrome: NSColor
    var border: NSColor
    /// Le fond des zones posées sur la page : sommaire, barre de lecture.
    var surface: NSColor

    /// Le vert citron de l'app.
    static let lime = NSColor(hex: 0xC6F36B)

    static let paper = Theme(
        background: NSColor(hex: 0xFFFFFF), text: NSColor(hex: 0x1A1A1A),
        past: NSColor(hex: 0x9A9A9A), wordFill: lime, bandAlpha: 0.22,
        pill: lime, pillAlpha: 0.45, wipeAlpha: 0.55,
        wordText: NSColor(hex: 0x0A0A0A), line: NSColor(hex: 0x141414),
        onLine: .white, chrome: NSColor(hex: 0x4A4A4A),
        border: NSColor(hex: 0xEBEBEB), surface: NSColor(hex: 0xF3F3F3))

    static let night = Theme(
        background: NSColor(hex: 0x161616), text: NSColor(hex: 0xE6E6E6),
        past: NSColor(hex: 0x7A7A7A), wordFill: .white, bandAlpha: 0.06,
        pill: lime, pillAlpha: 0.2, wipeAlpha: 0.3,
        wordText: NSColor(hex: 0xFFFFFF), line: lime,
        onLine: NSColor(hex: 0x141414), chrome: NSColor(hex: 0xB5B5B5),
        border: NSColor(hex: 0x2A2A2A), surface: NSColor(hex: 0x222222))

    static let contrast = Theme(
        background: .black, text: .white, past: NSColor(hex: 0xB0B0B0),
        wordFill: NSColor(hex: 0xFFE600), bandAlpha: 0.2,
        pill: NSColor(hex: 0xFFE600), pillAlpha: 0.35, wipeAlpha: 0, wordText: .white,
        line: NSColor(hex: 0xFFE600), onLine: .black, chrome: .white, border: .white,
        surface: .black)

    /// Rapport de contraste WCAG entre deux couleurs (1 à 21).
    static func contrastRatio(_ a: NSColor, _ b: NSColor) -> Double {
        func luminance(_ color: NSColor) -> Double {
            let c = color.usingColorSpace(.sRGB) ?? color
            func channel(_ v: CGFloat) -> Double {
                let v = Double(v)
                return v <= 0.03928 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * channel(c.redComponent) + 0.7152 * channel(c.greenComponent)
                + 0.0722 * channel(c.blueComponent)
        }
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    static func resolve(dark: Bool, increaseContrast: Bool) -> Theme {
        if increaseContrast { return .contrast }
        return dark ? .night : .paper
    }
}

extension EnvironmentValues {
    /// Les couleurs du panneau, pour les contrôles qui ne les reçoivent pas
    /// en paramètre (le disque de lecture).
    @Entry var panelTheme: Theme = .paper
}
