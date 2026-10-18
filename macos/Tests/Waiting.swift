import Foundation
@testable import Sillage

/// Fait tourner la boucle principale jusqu'à ce que `condition` soit vraie.
@MainActor
@discardableResult
func waitUntil(_ timeout: TimeInterval = 5, _ condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while !condition() {
        if Date() > deadline { return false }
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }
    return true
}

/// Attend, en laissant tourner les tâches Swift, que `condition` soit vraie.
@MainActor
func eventuallyTrue(_ timeout: Duration = .seconds(5), _ condition: () -> Bool) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: timeout)
    while !condition() {
        if clock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return true
}

/// Une source d'images qu'on déclenche à la main : `tick()` rend une image si
/// la source tourne, comme le ferait l'écran.
@MainActor
final class ManualFrames: FrameSource {
    var onFrame: (() -> Void)?
    private(set) var isRunning = false
    func start() { isRunning = true }
    func stop() { isRunning = false }
    func tick() { if isRunning { onFrame?() } }
}

/// Une porte qu'un test ouvre à la main : ce qui l'attend reste suspendu
/// jusque-là, sans dépendre d'une durée.
@MainActor
final class TestGate {
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var isOpen = false
    /// Combien de tâches attendent la porte.
    var waiting: Int { waiters.count }

    func wait() async {
        if isOpen { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func open() {
        isOpen = true
        let resumed = waiters
        waiters = []
        resumed.forEach { $0.resume() }
    }
}

/// Laisse passer ce qui est déjà en file sur l'acteur principal : sa file est
/// servie dans l'ordre, une tâche ajoutée après passe donc après elles.
@MainActor
func drainMainActor() async {
    for _ in 0..<3 { await Task { @MainActor in }.value }
}
