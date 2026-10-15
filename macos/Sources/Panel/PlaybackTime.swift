import KeyboardShortcuts

/// Les temps du mini, calculés sans vue.
enum PlaybackTime {
    /// Le temps restant estimé, jamais négatif ; nil tant que la durée
    /// n'est pas connue.
    static func remaining(elapsed: Double?, total: Double?) -> Double? {
        guard let elapsed, let total, elapsed.isFinite, total.isFinite else { return nil }
        return max(0, total - elapsed)
    }

    /// « -2:00 », ou « –:–– » tant que la durée n'est pas connue.
    static func shortRemaining(elapsed: Double?, total: Double?) -> String {
        remaining(elapsed: elapsed, total: total).map { "-" + PlayerBar.time($0) } ?? PlayerBar.time(nil)
    }

    /// Ce que VoiceOver dit du temps restant : « 2:00 ».
    static func spokenRemaining(elapsed: Double?, total: Double?) -> String {
        remaining(elapsed: elapsed, total: total).map { PlayerBar.time($0) } ?? "inconnu"
    }

    /// La valeur de la timeline pour VoiceOver : « 0:56 sur 2:56 », ou le
    /// pourcentage lu tant que la durée n'est pas connue.
    static func timelineValue(elapsed: Double?, total: Double?, progress: Float) -> String {
        if let elapsed, let total, elapsed.isFinite, total.isFinite {
            return "\(PlayerBar.time(elapsed)) sur \(PlayerBar.time(total))"
        }
        return "\(Int(Timeline.clamped(Double(progress)) * 100)) %"
    }

    /// La barre réduite sans document.
    @MainActor
    static func emptyText(shortcut: KeyboardShortcuts.Shortcut?) -> String {
        guard let shortcut else { return "Rien à lire — raccourci : ⌘," }
        return "Rien à lire — " + PanelInvitation.keys(shortcut)
    }
}
