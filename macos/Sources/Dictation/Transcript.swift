import Foundation

/// Ce que la dictée a entendu : le texte arrêté, puis la part encore
/// provisoire, que le moteur réécrit à chaque résultat jusqu'à l'arrêter.
struct Transcript: Equatable {
    private(set) var settled = ""
    private(set) var pending = ""

    /// Un résultat du moteur : un provisoire remplace le précédent, un
    /// définitif s'ajoute au texte arrêté et efface le provisoire.
    mutating func receive(_ text: String, isFinal: Bool) {
        if isFinal {
            settled = Self.join(settled, text)
            pending = ""
        } else {
            pending = Self.clean(text)
        }
    }

    /// Le texte entier, tel qu'il sera saisi.
    var text: String { Self.join(settled, pending) }
    var isEmpty: Bool { text.isEmpty }

    private static func clean(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Deux morceaux, séparés d'une espace.
    static func join(_ a: String, _ b: String) -> String {
        let (a, b) = (clean(a), clean(b))
        if a.isEmpty { return b }
        if b.isEmpty { return a }
        return a + " " + b
    }
}
