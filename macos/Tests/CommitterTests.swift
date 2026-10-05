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

    /// L'écriture reçoit aussi la valeur déjà écrite : elle peut n'envoyer
    /// que ce qui a changé.
    func testWriteReceivesThePreviouslySavedValue() async throws {
        var pairs: [[Int]] = []
        let c = Committer<Int>(delay: .zero, saved: 0, writeChange: { new, previous in
            pairs.append([previous, new])
        })
        c.change(1, immediate: true)
        _ = await eventually { pairs.count == 1 }
        c.change(2, immediate: true)
        let ok = await eventually { pairs.count == 2 }
        XCTAssertTrue(ok)
        XCTAssertEqual(pairs, [[0, 1], [1, 2]])
    }
}

/// L'onglet Lecture n'écrit que le réglage touché : basculer la règle ne
/// remet pas la vitesse changée entre-temps au clavier.
final class ReadingValuesPatchTests: XCTestCase {
    func testOnlyTheChangedFieldIsWritten() {
        var current = SettingsView(speed: 1.5, lineSpacing: 2, readingRuler: true, keepVoiceMinutes: 10)
        var old = ReadingValues(); old.speed = 1.0; old.lineSpacing = 2; old.ruler = true
        var new = old; new.ruler = false
        ReadingValues.write(new, over: old, into: &current)
        XCTAssertEqual(current.speed, 1.5, "la vitesse prise au clavier reste")
        XCTAssertFalse(current.readingRuler)
        XCTAssertEqual(current.lineSpacing, 2)
    }

    func testEachChangedFieldIsWritten() {
        var current = SettingsView(speed: 1.5, lineSpacing: 2, readingRuler: true, keepVoiceMinutes: 10)
        let old = ReadingValues()
        var new = old; new.speed = 2.0; new.lineSpacing = 3
        ReadingValues.write(new, over: old, into: &current)
        XCTAssertEqual(current.speed, 2.0)
        XCTAssertEqual(current.lineSpacing, 3)
        XCTAssertTrue(current.readingRuler)
        XCTAssertEqual(current.keepVoiceMinutes, 10)
    }
}
