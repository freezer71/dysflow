import Foundation
import SwiftUI

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

    /// Deux décimales : pas de 0,35000002 dans config.toml.
    static func rounded(_ v: Float) -> Float { (v * 100).rounded() / 100 }

    static func percent(_ v: Float) -> String { "\(Int((v * 100).rounded())) %" }
}

/// Une taille, écrite de la même façon dans toute l'app : base 1000 (comme
/// le Finder), unités françaises (octets, Ko, Mo, Go, To), virgule
/// décimale, un chiffre après la virgule sous 100 et aucun au-delà
/// (« 342 Mo », « 92,4 Mo »). L'app n'a que la localisation `en` :
/// `ByteCountFormatter` y écrirait « 342,3 MB ».
enum SizeFormat {
    private static let units = ["Ko", "Mo", "Go", "To"]

    static func bytes(_ n: UInt64) -> String {
        guard n >= 1000 else { return n <= 1 ? "\(n) octet" : "\(n) octets" }
        var value = Double(n) / 1000
        var unit = 0
        // 999,7 Ko s'écrit « 1 Mo », jamais « 1 000 Ko ».
        while value >= 999.5, unit < units.count - 1 {
            value /= 1000
            unit += 1
        }
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = value >= 99.95 ? 0 : 1
        f.roundingMode = .halfUp
        return "\(f.string(from: NSNumber(value: value)) ?? "\(value)") \(units[unit])"
    }
}

/// Le nom français d'une langue : « fr » → « Français ».
enum LanguageName {
    private static let french = Locale(identifier: "fr_FR")

    static func display(_ code: String) -> String {
        guard let name = french.localizedString(forLanguageCode: code), name != code else { return code }
        return name.prefix(1).uppercased() + name.dropFirst()
    }
}
