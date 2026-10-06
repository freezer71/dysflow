import XCTest
@testable import Sillon

/// La relecture à 4 Hz de la fenêtre des réglages suit sa visibilité.
@MainActor
final class RefreshLoopTests: XCTestCase {
    func testTheLoopRunsOnlyWhileTheWindowIsVisible() async throws {
        var ticks = 0
        let loop = RefreshLoop(interval: .milliseconds(10)) { ticks += 1 }
        try await Task.sleep(for: .milliseconds(60))
        XCTAssertEqual(ticks, 0, "rien avant que la fenêtre paraisse")
        loop.setVisible(true)
        let ran = await eventuallyTrue { ticks >= 3 }
        XCTAssertTrue(ran)
        loop.setVisible(false)
        XCTAssertFalse(loop.isRunning)
        let stopped = ticks
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertEqual(ticks, stopped, "fenêtre fermée : plus aucune relecture")
        loop.setVisible(true)
        let again = await eventuallyTrue { ticks > stopped }
        XCTAssertTrue(again, "rouverte, elle reprend")
        loop.setVisible(false)
    }

    func testShowingTwiceRunsASingleLoop() async throws {
        var ticks = 0
        let loop = RefreshLoop(interval: .milliseconds(50)) { ticks += 1 }
        loop.setVisible(true)
        loop.setVisible(true)
        loop.setVisible(true)
        try await Task.sleep(for: .milliseconds(120))
        loop.setVisible(false)
        // Une boucle : 3 relectures environ (0, 50, 100 ms) ; trois en feraient 9.
        XCTAssertLessThanOrEqual(ticks, 4)
    }
}
