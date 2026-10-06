import Foundation
import SwiftUI

/// Un réglage que le modèle actif ignore, montré sur demande : il le dit.
struct IgnorableNote<Content: View>: View {
    let ignored: Bool
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            content
            if ignored {
                Text("sans effet sur ce modèle")
            }
        }
    }
}

/// Nombres à la française, à l'écran comme pour VoiceOver.
enum SettingFormat {
    /// Deux décimales au plus, virgule : « 1,25 », « 1 ».
    static func decimal(_ v: Float) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = 2
        return f.string(from: NSNumber(value: v)) ?? "\(v)"
    }

    static func percent(_ v: Float) -> String { "\(Int((v * 100).rounded())) %" }
}

/// Le nom français d'une langue : « fr » → « Français ».
enum LanguageName {
    private static let french = Locale(identifier: "fr_FR")

    static func display(_ code: String) -> String {
        guard let name = french.localizedString(forLanguageCode: code), name != code else { return code }
        return name.prefix(1).uppercased() + name.dropFirst()
    }
}
