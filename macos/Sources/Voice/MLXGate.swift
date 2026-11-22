import os

/// Le passage unique vers MLX : tout calcul MLX de l'app (la voix Kokoro, les
/// modèles de dictée) passe ici, un à la fois. Deux modèles ne se partagent
/// pas le GPU ni le cache de MLX : la dictée attend la fin d'une synthèse, et
/// l'inverse.
///
/// `shared` est le verrou du cœur Rust (`compute::gpu`, par `gpuLock`) : la
/// copie Rust de MLX et celle de Swift ne calculent jamais en même temps. Un
/// `body` n'appelle donc jamais un calcul Rust : le fil MLX l'attendrait (le
/// cœur le refuse par une erreur). `MLXGate()`, pour les tests, a son verrou
/// propre.
///
/// Un `os_unfair_lock` dans les deux cas : les appels arrivent de fils
/// d'arrière-plan, de façon synchrone (pris et rendu sur le même fil), et la
/// priorité se transmet (le lecteur ou la dictée, `userInitiated`, font
/// monter une préparation `utility` qui tient le verrou). Jamais rentrant :
/// un `body` ne rappelle pas `run`.
final class MLXGate: @unchecked Sendable {
    static let shared = MLXGate(ownLock: nil)

    /// `nil` : le verrou du cœur Rust.
    private let ownLock: OSAllocatedUnfairLock<Void>?

    convenience init() {
        self.init(ownLock: OSAllocatedUnfairLock())
    }

    private init(ownLock: OSAllocatedUnfairLock<Void>?) {
        self.ownLock = ownLock
    }

    func run<T>(_ body: () throws -> T) rethrows -> T {
        if let ownLock { return try ownLock.withLockUnchecked(body) }
        gpuLock()
        defer { gpuUnlock() }
        return try body()
    }
}
