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

    /// Le plus court écart entre deux lectures (un modèle rapide ne relit
    /// pas plus souvent), et celui d'avant la première mesure.
    static let fastest: Duration = .milliseconds(400)
    static let initial: Duration = .milliseconds(1500)

    /// Règle le rythme sur la dernière lecture : trois fois sa durée, jamais
    /// moins de `fastest`, sans plafond. Un modèle rapide relit souvent (le
    /// texte suit la voix) ; un lent, ou une longue dictée (tout est relu
    /// depuis le début), espace ses lectures : la puce n'est jamais occupée
    /// plus d'un tiers du temps (un plafond la ferait tourner sans relâche
    /// dès qu'une lecture dépasse le tiers de ce plafond).
    mutating func adapt(lastRead: Duration) {
        interval = max(Self.fastest, lastRead * 3)
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
