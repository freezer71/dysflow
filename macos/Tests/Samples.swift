@testable import Sillon

extension VoiceModel {
    /// Un paquet de catalogue pour les tests.
    static func sample(id: String, name: String = "Modèle", bytes: UInt64 = 300_000_000,
                       installed: Bool = true, active: Bool = false) -> VoiceModel {
        VoiceModel(id: id, name: name, bytes: bytes, installed: installed, active: active,
                   recommended: false, summary: "Rapide et léger.", family: "kokoro",
                   precision: "int8", languagesLabel: "français, anglais",
                   languages: ["fr", "en"], pace: "Lecture fluide.", voicesLabel: "",
                   licenses: "", kind: "speech", diskBytes: 0, system: false, hasFiles: installed)
    }
}

extension Snapshot {
    /// Un instantané de lecteur muet, aux valeurs choisies.
    static func sample(playing: Bool = false, sentence: UInt32 = 0, elapsed: Double? = 56,
                       total: Double? = 176, speed: Float = 1, voice: VoiceState = .ready,
                       hasDocument: Bool = true, revision: UInt64 = 1) -> Snapshot {
        var s = SillonReader.silent().snapshot()
        s.hasDocument = hasDocument
        s.playing = playing
        s.sentence = sentence
        s.elapsed = elapsed
        s.total = total
        s.speed = speed
        s.voice = voice
        s.revision = revision
        return s
    }
}
