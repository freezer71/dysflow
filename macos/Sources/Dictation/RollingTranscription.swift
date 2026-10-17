import Foundation

/// Le rythme de la relecture glissante : tout l'audio depuis le début est
/// relu par le modèle, une fois par `interval` au plus, jamais deux fois en
/// même temps. Logique pure : l'horloge et l'état « en cours » viennent de
/// l'appelant.
struct RollingTranscription {
    /// Entre le début d'une lecture et celui de la suivante.
    let interval: Duration
    private var lastStart: ContinuousClock.Instant?

    init(interval: Duration) {
        self.interval = interval
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
