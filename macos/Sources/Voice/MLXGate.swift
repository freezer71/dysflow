import os

/// Le passage unique vers MLX : tout calcul MLX de l'app (la voix Kokoro, les
/// modèles de dictée) passe ici, un à la fois. Deux modèles ne se partagent
/// pas le GPU ni le cache de MLX : la dictée attend la fin d'une synthèse, et
/// l'inverse.
///
/// Un `os_unfair_lock`, comme celui qu'avait Kokoro : les appels arrivent de
/// fils d'arrière-plan, de façon synchrone, et la priorité se transmet (le
/// lecteur ou la dictée, `userInitiated`, font monter une préparation
/// `utility` qui tient le verrou). Jamais rentrant : un `body` ne rappelle
/// pas `run`.
final class MLXGate: @unchecked Sendable {
    static let shared = MLXGate()

    private let lock = OSAllocatedUnfairLock()

    init() {}

    func run<T>(_ body: () throws -> T) rethrows -> T {
        try lock.withLockUnchecked(body)
    }
}
