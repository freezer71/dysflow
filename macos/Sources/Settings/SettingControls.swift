import Foundation
import SwiftUI

/// Un réglage que le modèle actif ignore, montré sur demande : il le dit.
/// VoiceOver l'entend en indice de chaque contrôle, pas comme un texte à
/// part ; les enfants ne sont pas fusionnés, un curseur reste réglable.
struct IgnorableNote<Content: View>: View {
    let ignored: Bool
    @ViewBuilder let content: Content

    static var hint: String { "sans effet sur ce modèle" }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if ignored {
                // Un `Group` porte l'indice jusqu'à chacun de ses enfants.
                Group { content }.accessibilityHint(Self.hint)
                Text(Self.hint).accessibilityHidden(true)
            } else {
                content
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

/// Un réglage que le modèle peut fixer seul. « Laisser le modèle décider »
/// est un état à part, distinct de toute valeur ; sinon, une valeur posée au
/// curseur. `onEditingChanged` reçoit le début et la fin du geste : rien ne
/// part avant le relâchement.
struct OptionalSlider: View {
    let title: String
    let ignored: Bool
    @Binding var value: Float?
    let range: ClosedRange<Float>
    let step: Float
    /// La valeur proposée quand on reprend la main.
    let fallback: Float
    var onEditingChanged: (Bool) -> Void = { _ in }

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("\(title) : laisser le modèle décider",
                   isOn: Binding(get: { value == nil }, set: { value = $0 ? nil : fallback }))
            if let current = value {
                Slider(value: Binding(get: { current }, set: { value = SettingFormat.rounded($0) }), in: range, step: step) {
                    Text("\(title) : \(SettingFormat.decimal(current))")
                } onEditingChanged: { onEditingChanged($0) }
                .accessibilityLabel(title)
                .accessibilityValue(SettingFormat.decimal(current))
            }
        }
    }
}

/// La même chose pour un entier.
struct OptionalStepper: View {
    let title: String
    let ignored: Bool
    @Binding var value: UInt32?
    let range: ClosedRange<UInt32>
    let fallback: UInt32

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("\(title) : laisser le modèle décider",
                   isOn: Binding(get: { value == nil }, set: { value = $0 ? nil : fallback }))
            if let current = value {
                Stepper("\(title) : \(current)",
                        value: Binding(get: { current }, set: { value = $0 }), in: range)
                    .accessibilityLabel(title)
                    .accessibilityValue("\(current)")
            }
        }
    }
}

/// La graine : un tirage au hasard, ou une valeur fixe pour une synthèse
/// reproductible.
struct SeedField: View {
    let ignored: Bool
    @Binding var seed: UInt64?

    var body: some View {
        IgnorableNote(ignored: ignored) {
            Toggle("Graine : aléatoire",
                   isOn: Binding(get: { seed == nil }, set: { seed = $0 ? nil : 0 }))
            if seed != nil {
                TextField("Graine", value: Binding(get: { seed ?? 0 }, set: { seed = $0 }),
                          format: IntegerFormatStyle<UInt64>().grouping(.never))
                    .accessibilityLabel("Graine")
                    .accessibilityHint("Entrée pour valider")
                Text("Entrée pour valider")
            }
        }
    }
}
