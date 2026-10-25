/// Ce que VoiceOver doit dire : les changements d'état, jamais les mots —
/// la voix de dysflow et celle de VoiceOver ne doivent pas se chevaucher.
struct Announcer {
    private var playing: Bool?
    private var section: UInt32??
    private var revision: UInt64?

    /// Repart de l'état courant : rien ne sera dit pour ce qui précède.
    mutating func reseed(with s: Snapshot) {
        playing = s.playing
        section = .some(s.section)
        revision = s.revision
    }

    /// Quand lecture et section changent ensemble, la lecture l'emporte : la
    /// section se dira au changement suivant seulement si elle bouge encore.
    /// Une section qui disparaît (`nil`) ne dit rien.
    mutating func announcement(for s: Snapshot, doc: DocView) -> String? {
        guard let wasPlaying = playing, let wasSection = section, let wasRevision = revision
        else { reseed(with: s); return nil }
        // Nouveau document : les identifiants de section repartent de zéro.
        guard s.revision == wasRevision else { reseed(with: s); return nil }
        defer { playing = s.playing; section = .some(s.section) }
        if s.playing != wasPlaying { return s.playing ? String(localized: "Playing", comment: "VoiceOver") : String(localized: "Paused", comment: "VoiceOver") }
        if s.section != wasSection, let id = s.section,
           let title = doc.sections.first(where: { $0.id == id })?.title {
            return String(localized: "Section: \(title)", comment: "VoiceOver")
        }
        return nil
    }
}
