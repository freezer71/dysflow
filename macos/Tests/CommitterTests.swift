import XCTest
@testable import Sillon

/// Attend, en laissant tourner les tâches Swift, que `condition` soit vraie.
@MainActor
private func eventually(_ condition: () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

@MainActor
final class CommitterTests: XCTestCase {
    func testRapidChangesWriteOnlyTheLast() async throws {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .milliseconds(40), saved: 0, write: { writes.append($0) })
        for v in 1...20 { c.change(v) }
        let ok = await eventually { writes.count == 1 }
        XCTAssertTrue(ok)
        try await Task.sleep(for: .milliseconds(150))
        XCTAssertEqual(writes, [20])
        XCTAssertEqual(c.saved, 20)
    }

    func testUnchangedValueIsNotWritten() async throws {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .milliseconds(10), saved: 5, write: { writes.append($0) })
        c.change(5)
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertTrue(writes.isEmpty)
    }

    func testNothingWrittenBeforePrime() async throws {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .milliseconds(10), write: { writes.append($0) })
        c.change(3)
        try await Task.sleep(for: .milliseconds(80))
        XCTAssertTrue(writes.isEmpty)
        c.prime(1)
        c.change(3)
        let ok = await eventually { writes == [3] }
        XCTAssertTrue(ok)
    }

    func testImmediateSkipsTheDelay() async {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .seconds(30), saved: 0, write: { writes.append($0) })
        c.change(2)
        c.change(7, immediate: true)
        let ok = await eventually { writes == [7] }
        XCTAssertTrue(ok)
    }

    func testWritesAreSerialised() async throws {
        var active = 0, peak = 0, writes: [Int] = []
        let c = Committer<Int>(delay: .zero, saved: 0, write: { v in
            active += 1; peak = max(peak, active)
            try await Task.sleep(for: .milliseconds(60))
            writes.append(v); active -= 1
        })
        c.change(1, immediate: true)
        try await Task.sleep(for: .milliseconds(20))
        c.change(2, immediate: true)
        let ok = await eventually { writes == [1, 2] }
        XCTAssertTrue(ok)
        XCTAssertEqual(peak, 1)
    }

    func testFailureIsReported() async throws {
        struct Boom: Error {}
        var reported: Error?
        let c = Committer<Int>(delay: .zero, saved: 0, write: { _ in throw Boom() },
                               onResult: { reported = $0 })
        c.change(1, immediate: true)
        let ok = await eventually { reported != nil }
        XCTAssertTrue(ok)
        XCTAssertEqual(c.saved, 0)
    }
}
