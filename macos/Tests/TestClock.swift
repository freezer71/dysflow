import Foundation
@testable import Dysflow

/// Une horloge qu'on avance à la main : les pauses du `Committer` et du
/// `SettingsStore` attendent sur elle au lieu du temps réel, et un test qui
/// vérifie « rien ne part pendant la pause » ne dépend plus de la charge de
/// la machine.
@MainActor
final class TestClock {
    private(set) var now: Duration = .zero
    private var sleepers: [Int: (deadline: Duration, continuation: CheckedContinuation<Void, Error>)] = [:]
    private var nextID = 0

    /// Les pauses en cours.
    var pending: Int { sleepers.count }

    /// À passer en `sleep:`.
    nonisolated var sleep: Committer<Int>.Sleep {
        { [self] duration in try await self.wait(duration) }
    }

    func wait(_ duration: Duration) async throws {
        try Task.checkCancellation()
        if duration <= .zero { return }
        nextID += 1
        let id = nextID
        let deadline = now + duration
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                sleepers[id] = (deadline, continuation)
            }
        } onCancel: {
            Task { @MainActor in self.cancel(id) }
        }
    }

    private func cancel(_ id: Int) {
        sleepers.removeValue(forKey: id)?.continuation.resume(throwing: CancellationError())
    }

    /// Avance le temps : les pauses arrivées à terme reprennent, dans l'ordre
    /// de leur échéance, et les tâches qu'elles relancent tournent.
    func advance(by duration: Duration) async {
        // Les tâches tout juste créées posent d'abord leur pause.
        await settle()
        let target = now + duration
        while let (id, sleeper) = sleepers.filter({ $0.value.deadline <= target })
            .min(by: { $0.value.deadline < $1.value.deadline }) {
            now = sleeper.deadline
            sleepers[id] = nil
            sleeper.continuation.resume()
            await settle()
        }
        now = target
        await settle()
    }

    /// Laisse tourner les tâches prêtes, sans faire avancer le temps.
    func settle() async {
        for _ in 0..<50 { await Task.yield() }
    }
}
