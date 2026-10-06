@testable import Sillon

extension VoiceModel {
    /// Un paquet de catalogue pour les tests.
    static func sample(id: String, name: String = "Modèle", bytes: UInt64 = 300_000_000,
                       installed: Bool = true, active: Bool = false) -> VoiceModel {
        VoiceModel(id: id, name: name, bytes: bytes, installed: installed, active: active,
                   recommended: false, summary: "Rapide et léger.", family: "supertonic",
                   precision: "f16", languagesLabel: "31 langues, dont le français",
                   languages: ["fr", "en"], pace: "Lecture fluide.")
    }
}
