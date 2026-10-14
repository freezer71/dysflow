import AppKit
import CoreText

enum DisplayDefaults {
    static let fontKey = "readingFont"
    static let sizeKey = "readingSize"
    static let defaultSize: Double = 22
}

/// Les polices de lecture, embarquées dans l'app.
enum ReadingFont: String, CaseIterable, Identifiable {
    case literata, atkinson, openDyslexic

    var id: String { rawValue }

    var label: String {
        switch self {
        case .literata: "Literata"
        case .atkinson: "Atkinson Hyperlegible"
        case .openDyslexic: "OpenDyslexic"
        }
    }

    var familyName: String { label }

    func font(size: CGFloat, bold: Bool = false, italic: Bool = false) -> NSFont {
        var traits: NSFontTraitMask = []
        if bold { traits.insert(.boldFontMask) }
        if italic { traits.insert(.italicFontMask) }
        return NSFontManager.shared.font(withFamily: familyName, traits: traits,
                                         weight: bold ? 9 : 5, size: size)
            ?? .systemFont(ofSize: size, weight: bold ? .semibold : .regular)
    }

    /// Rend les polices du paquet utilisables par leur nom de famille.
    static func registerBundled() {
        guard let urls = Bundle.main.urls(forResourcesWithExtension: nil, subdirectory: "Fonts")
        else { return }
        let fonts = urls.filter { ["ttf", "otf"].contains($0.pathExtension.lowercased()) }
        CTFontManagerRegisterFontURLs(fonts as CFArray, .process, true, nil)
    }
}
