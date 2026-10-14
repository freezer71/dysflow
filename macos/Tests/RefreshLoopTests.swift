import XCTest
@testable import Sillon

/// La relecture à 4 Hz de la fenêtre des réglages suit sa visibilité.
@MainActor
final class RefreshLoopTests: XCTestCase {
    func testTheLoopRunsOnlyWhileTheWindowIsVisible() async throws {
        var ticks = 0
        let loop = RefreshLoop(interval: .milliseconds(10)) { ticks += 1 }
        await drainMainActor()
        XCTAssertFalse(loop.isRunning)
        XCTAssertEqual(ticks, 0, "rien avant que la fenêtre paraisse")
        loop.setVisible(true)
        let ran = await eventuallyTrue { ticks >= 3 }
        XCTAssertTrue(ran)
        loop.setVisible(false)
        XCTAssertFalse(loop.isRunning)
        let stopped = ticks
        // La boucle annulée se réveille (sommeil interrompu) et sort : les
        // travaux en file passés, aucune relecture de plus.
        await drainMainActor()
        XCTAssertEqual(ticks, stopped, "fenêtre fermée : plus aucune relecture")
        loop.setVisible(true)
        let again = await eventuallyTrue { ticks > stopped }
        XCTAssertTrue(again, "rouverte, elle reprend")
        loop.setVisible(false)
    }

    func testShowingTwiceRunsASingleLoop() async throws {
        var ticks = 0
        // Un intervalle qu'aucun test n'atteint : chaque boucle relit une
        // fois en partant, puis dort.
        let loop = RefreshLoop(interval: .seconds(3600)) { ticks += 1 }
        loop.setVisible(true)
        loop.setVisible(true)
        loop.setVisible(true)
        await drainMainActor()
        loop.setVisible(false)
        // Une boucle : une relecture ; trois boucles en feraient trois.
        XCTAssertEqual(ticks, 1)
    }
}
