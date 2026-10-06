import Foundation

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
