import Foundation

/// Le rythme de la relecture glissante : tout l'audio depuis le début est
/// relu par le modèle, une fois par `interval` au plus, jamais deux fois en
/// même temps. Logique pure : l'horloge et l'état « en cours » viennent de
/// l'appelant.
struct RollingTranscription {
    /// Entre le début d'une lecture et celui de la suivante.
    private(set) var interval: Duration
    private var lastStart: ContinuousClock.Instant?

    init(interval: Duration) {
        self.interval = interval
    }

    /// Le plus court et le plus long écart entre deux lectures.
    static let fastest: Duration = .milliseconds(400)
    static let slowest: Duration = .milliseconds(1500)

    /// Règle le rythme sur la dernière lecture : trois fois sa durée, entre
    /// `fastest` et `slowest`. Un modèle rapide relit souvent (le texte suit
    /// la voix), sans jamais occuper la puce plus d'un tiers du temps ; un
    /// lent, ou une longue dictée (tout est relu depuis le début), espace
    /// ses lectures.
    mutating func adapt(lastRead: Duration) {
        interval = min(Self.slowest, max(Self.fastest, lastRead * 3))
    }

    /// Vrai s'il faut lancer une lecture maintenant : aucune n'est en cours,
    /// et `interval` s'est écoulé depuis le début de la précédente (la
    /// première, tout de suite). Vrai compte comme un début de lecture.
    mutating func shouldRead(now: ContinuousClock.Instant, busy: Bool) -> Bool {
        guard !busy else { return false }
        if let lastStart, now - lastStart < interval { return false }
        lastStart = now
        return true
    }
}
