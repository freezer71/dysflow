import AppKit

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

/// Les couleurs du lecteur. « Papier » le jour, sa version de nuit le soir,
/// le contraste élevé dès que macOS le demande.
struct Theme: Equatable {
    var background: NSColor
    var text: NSColor
    var past: NSColor
    var wordFill: NSColor
    var wordText: NSColor
    var line: NSColor
    /// Texte des contrôles.
    var chrome: NSColor
    var border: NSColor

    static let paper = Theme(
        background: NSColor(hex: 0xFBF8F1), text: NSColor(hex: 0x2A2722),
        past: NSColor(hex: 0x9A948A), wordFill: NSColor(hex: 0xCFE3FF),
        wordText: NSColor(hex: 0x1B2A44), line: NSColor(hex: 0x7AA7E8),
        chrome: NSColor(hex: 0x8A847A), border: NSColor(hex: 0xE6E0D3))

    static let night = Theme(
        background: NSColor(hex: 0x1E1B17), text: NSColor(hex: 0xE8E1D4),
        past: NSColor(hex: 0x8A8276), wordFill: NSColor(hex: 0x2C4A75),
        wordText: NSColor(hex: 0xE6EFFF), line: NSColor(hex: 0x7AA7E8),
        chrome: NSColor(hex: 0x9A9286), border: NSColor(hex: 0x3A342C))

    static let contrast = Theme(
        background: .black, text: .white, past: NSColor(hex: 0xB0B0B0),
        wordFill: NSColor(hex: 0xFFE600), wordText: .black,
        line: NSColor(hex: 0xFFE600), chrome: .white, border: .white)

    static func resolve(dark: Bool, increaseContrast: Bool) -> Theme {
        if increaseContrast { return .contrast }
        return dark ? .night : .paper
    }
}
