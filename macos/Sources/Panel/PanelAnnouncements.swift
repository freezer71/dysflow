import AppKit

/// Ce que le panneau dit à VoiceOver : les changements d'état (lecture,
/// pause, section) que l'`Announcer` décide, et les avis passagers. Sans
/// VoiceOver, rien ne part.
@MainActor
final class PanelAnnouncements {
    private var announcer = Announcer()
    private weak var window: NSWindow?

    /// Où part une annonce : VoiceOver, s'il est allumé. Remplaçable, pour
    /// qu'un test compte ce que le vrai chemin dit.
    var post: (String) -> Void = { _ in }

    init(window: NSWindow) {
        self.window = window
        post = { [weak self] text in self?.postToVoiceOver(text) }
    }

    /// Repart de l'état courant : rien ne sera dit de ce qui précède (la
    /// fenêtre était retirée).
    func reseed(with s: Snapshot) {
        announcer.reseed(with: s)
    }

    /// La phrase à dire, ou rien : jamais fenêtre masquée.
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func text(for s: Snapshot, doc: DocView, hidden: Bool) -> String? {
        let text = announcer.announcement(for: s, doc: doc)
        return hidden ? nil : text
    }

    private func postToVoiceOver(_ text: String) {
        guard NSWorkspace.shared.isVoiceOverEnabled, let window else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }
}
