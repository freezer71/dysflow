import CoreGraphics

/// Ce que le lecteur lit des réglages du moteur (config.toml), relu à
/// chaque ouverture et à chaque réglage écrit.
struct ReadingDisplay: Equatable {
    var font: ReadingFont = .literata
    var size: CGFloat = 22
    var lineSpacing: UInt32 = 1
    var readingRuler = true

    init() {}

    init(_ s: SettingsView) {
        font = ReadingFont(rawValue: s.readingFont) ?? .literata
        size = s.readingSize.isFinite ? CGFloat(min(max(s.readingSize, 14), 40)) : 22
        lineSpacing = s.lineSpacing
        readingRuler = s.readingRuler
    }
}
