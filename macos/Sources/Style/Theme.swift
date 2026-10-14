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

/// Les couleurs du lecteur, celles de Notion (14 oct. 2026) : une encre
/// chaude sur blanc le jour, sur gris très sombre le soir, le bleu de Notion
/// pour le mot lu ; le contraste élevé dès que macOS le demande.
struct Theme: Equatable {
    var background: NSColor
    var text: NSColor
    var past: NSColor
    /// Teinte de la bande de lecture (posée à 25 %).
    var wordFill: NSColor
    /// L'encre du mot lu : plus sombre que le texte (sur fond clair), sans
    /// fond ni changement de graisse.
    var wordText: NSColor
    /// Le trait sous la ligne lue.
    var line: NSColor
    /// Texte des contrôles.
    var chrome: NSColor
    var border: NSColor

    static let paper = Theme(
        background: NSColor(hex: 0xFFFFFF), text: NSColor(hex: 0x37352F),
        past: NSColor(hex: 0x9B9A97), wordFill: NSColor(hex: 0x6AA7EC),
        wordText: NSColor(hex: 0x183347), line: NSColor(hex: 0x2383E2),
        chrome: NSColor(hex: 0x55534E), border: NSColor(hex: 0xE9E9E7))

    static let night = Theme(
        background: NSColor(hex: 0x191919), text: NSColor(hex: 0xD4D4D4),
        past: NSColor(hex: 0x7F7F7F), wordFill: NSColor(hex: 0x2383E2),
        wordText: NSColor(hex: 0xE6F1FF), line: NSColor(hex: 0x529CCA),
        chrome: NSColor(hex: 0xA6A6A6), border: NSColor(hex: 0x2F2F2F))

    static let contrast = Theme(
        background: .black, text: .white, past: NSColor(hex: 0xB0B0B0),
        wordFill: NSColor(hex: 0xFFE600), wordText: .white,
        line: NSColor(hex: 0xFFE600), chrome: .white, border: .white)

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
