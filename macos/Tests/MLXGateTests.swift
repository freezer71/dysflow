import os
import XCTest
@testable import Dysflow

final class MLXGateTests: XCTestCase {
    /// Kokoro et la dictée passent par le même verrou : jamais deux calculs
    /// MLX à la fois.
    func testOneBodyAtATime() async {
        let gate = MLXGate()
        let inside = OSAllocatedUnfairLock(initialState: 0)
        let maxInside = OSAllocatedUnfairLock(initialState: 0)
        await withTaskGroup(of: Void.self) { group in
            for _ in 0..<8 {
                group.addTask {
                    gate.run {
                        let now = inside.withLock { $0 += 1; return $0 }
                        maxInside.withLock { $0 = max($0, now) }
                        usleep(2_000)
                        inside.withLock { $0 -= 1 }
                    }
                }
            }
        }
        XCTAssertEqual(maxInside.withLock { $0 }, 1)
    }

    /// Le verrou de l'app est celui du cœur Rust : pendant `MLXGate.shared.run`,
    /// un calcul Rust lancé d'un autre fil ne commence pas ; il passe ensuite.
    func testTheSharedGateExcludesRust() throws {
        // Une première fois hors du verrou : l'attente mesurée plus bas est
        // celle du verrou, pas celle de la préparation de MLX.
        try rustCompute()
        let outcome = OSAllocatedUnfairLock<Result<Void, Error>?>(initialState: nil)
        let started = DispatchSemaphore(value: 0), finished = DispatchSemaphore(value: 0)
        MLXGate.shared.run {
            Thread.detachNewThread {
                started.signal()
                let result = Result { try rustCompute() }
                outcome.withLock { $0 = result }
                finished.signal()
            }
            XCTAssertEqual(started.wait(timeout: .now() + 10), .success)
            XCTAssertEqual(finished.wait(timeout: .now() + 0.3), .timedOut,
                           "le calcul Rust a commencé pendant MLXGate.shared.run")
        }
        // Le signal peut avoir été pris plus haut : le résultat seul compte.
        let deadline = Date(timeIntervalSinceNow: 10)
        while outcome.withLock({ $0 == nil }), Date() < deadline { usleep(10_000) }
        try XCTUnwrap(outcome.withLock { $0 }, "le verrou n'a pas été rendu").get()
    }

    /// Une erreur dans `body` rend le verrou partagé.
    func testAThrowingBodyReleasesTheSharedGate() throws {
        XCTAssertThrowsError(try MLXGate.shared.run { throw CancellationError() })
        let finished = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            MLXGate.shared.run {}
            finished.signal()
        }
        XCTAssertEqual(finished.wait(timeout: .now() + 10), .success)
    }

    #if DYSFLOW_MLX_RUST
    /// Un calcul Rust demandé en tenant le verrou est refusé : le fil MLX
    /// l'attendrait sans fin.
    func testARustComputeUnderTheSharedGateIsAnError() {
        let result = MLXGate.shared.run { Result { try mlxRustSmoke() } }
        XCTAssertThrowsError(try result.get())
    }
    #endif
}

/// Un calcul du cœur Rust ; sans la feature `mlx`, le verrou que prendrait ce
/// calcul.
@Sendable private func rustCompute() throws {
    #if DYSFLOW_MLX_RUST
    XCTAssertEqual(try mlxRustSmoke(), 6)
    #else
    gpuLock()
    gpuUnlock()
    #endif
}
