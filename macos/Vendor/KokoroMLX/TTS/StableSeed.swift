import Foundation

/// Une graine par phrase, la même d'un lancement à l'autre : une phrase
/// resynthétisée (saut, cache vidé) sonne pareil. FNV-1a 64 bits sur
/// l'UTF-8 de la forme NFC (`hashValue` change à chaque processus).
/// Ajout de Sillon.
public enum StableSeed {
    public static func of(_ text: String) -> UInt64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in text.precomposedStringWithCanonicalMapping.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return hash
    }
}
