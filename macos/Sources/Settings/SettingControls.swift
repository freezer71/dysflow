import Foundation
import SwiftUI

/// La langue de l'interface : les nombres et les unités s'écrivent à la
/// française en français (virgule, « Mo », « 60 % »), à l'anglaise sinon.
enum UILanguage {  // un seul endroit pour la langue des nombres
    static var isFrench: Bool { Bundle.main.preferredLocalizations.first == "fr" }
    static var numberLocale: Locale { Locale(identifier: isFrench ? "fr_FR" : "en_US") }
}

/// Nombres dans la langue de l'interface, à l'écran comme pour VoiceOver.
enum SettingFormat {
    /// Deux décimales au plus : « 1,25 » (« 1.25 » en anglais), « 1 ».
    static func decimal(_ v: Float) -> String {
        let f = NumberFormatter()
        f.locale = UILanguage.numberLocale
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = 2
        return f.string(from: NSNumber(value: v)) ?? "\(v)"
    }

    /// Deux décimales : pas de 0,35000002 dans config.toml.
    static func rounded(_ v: Float) -> Float { (v * 100).rounded() / 100 }

    static func percent(_ v: Float) -> String { "\(Int((v * 100).rounded()))" + (UILanguage.isFrench ? " %" : "%") }
}

/// Une taille, écrite de la même façon dans toute l'app : base 1000 (comme
/// le Finder), unités françaises en français (octets, Ko, Mo, Go, To),
/// anglaises sinon (bytes, KB, MB, GB, TB), un chiffre après la virgule
/// sous 100 et aucun au-delà (« 342 Mo », « 92,4 Mo » ; « 92.4 MB »).
/// `ByteCountFormatter` écrirait « 342,3 MB » : l'app fixe ses unités.
enum SizeFormat {
    private static var units: [String] { UILanguage.isFrench ? ["Ko", "Mo", "Go", "To"] : ["KB", "MB", "GB", "TB"] }

    static func bytes(_ n: UInt64) -> String {
        guard n >= 1000 else {
            return UILanguage.isFrench ? (n <= 1 ? "\(n) octet" : "\(n) octets")
                : (n == 1 ? "\(n) byte" : "\(n) bytes")
        }
        var value = Double(n) / 1000
        var unit = 0
        // 999,7 Ko s'écrit « 1 Mo », jamais « 1 000 Ko ».
        while value >= 999.5, unit < units.count - 1 {
            value /= 1000
            unit += 1
        }
        let f = NumberFormatter()
        f.locale = UILanguage.numberLocale
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = value >= 99.95 ? 0 : 1
        f.roundingMode = .halfUp
        return "\(f.string(from: NSNumber(value: value)) ?? "\(value)") \(units[unit])"   // xcstrings: garder (un nombre et son unité)
    }
}
