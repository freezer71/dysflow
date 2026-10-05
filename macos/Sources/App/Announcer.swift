/// Ce que VoiceOver doit dire : les changements d'état, jamais les mots —
/// la voix de sillon et celle de VoiceOver ne doivent pas se chevaucher.
struct Announcer {
    private var playing: Bool?
    private var section: UInt32??

    mutating func announcement(for s: Snapshot, doc: DocView) -> String? {
        defer { playing = s.playing; section = .some(s.section) }
        guard let wasPlaying = playing, let wasSection = section else { return nil }
        if s.playing != wasPlaying { return s.playing ? "Lecture" : "Pause" }
        if s.section != wasSection, let id = s.section,
           let title = doc.sections.first(where: { $0.id == id })?.title {
            return "Section : \(title)"
        }
        return nil
    }
}
