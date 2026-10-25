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
}
