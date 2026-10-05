import Foundation

/// Écrit la dernière valeur d'une rafale de changements, une écriture à la
/// fois : un curseur qu'on glisse ne produit pas une écriture par pas.
@MainActor
final class Committer<Value: Equatable> {
    private let delay: Duration
    private let write: (Value) async throws -> Void
    private let onResult: (Error?) -> Void
    private var pending: Task<Void, Never>?
    private var inFlight: Task<Void, Error>?
    /// Ce que le moteur a déjà : une valeur identique n'est pas renvoyée.
    private(set) var saved: Value?

    init(delay: Duration = .milliseconds(150), saved: Value? = nil,
         write: @escaping (Value) async throws -> Void,
         onResult: @escaping (Error?) -> Void = { _ in }) {
        self.delay = delay
        self.saved = saved
        self.write = write
        self.onResult = onResult
    }

    /// Fixe la valeur de départ (chargée du moteur) sans rien écrire.
    func prime(_ value: Value) { saved = value }

    /// `immediate` : sans pause (relâchement d'un curseur).
    func change(_ value: Value, immediate: Bool = false) {
        guard let saved else { return }   // pas encore chargé
        pending?.cancel()
        if value == saved && inFlight == nil { pending = nil; return }
        let wait = immediate ? .zero : delay
        pending = Task {
            if wait > .zero {
                do { try await Task.sleep(for: wait) } catch { return }
            }
            if let inFlight { _ = try? await inFlight.value }
            if Task.isCancelled { return }
            let job = Task { try await write(value) }
            inFlight = job
            do {
                try await job.value
                self.saved = value
                onResult(nil)
            } catch {
                onResult(error)
            }
            inFlight = nil
        }
    }
}
