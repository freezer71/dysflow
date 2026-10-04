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
