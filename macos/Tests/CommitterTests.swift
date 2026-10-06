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

    /// La valeur que le moteur a réellement appliquée devient la référence,
    /// et revient à l'appelant avec celle qui avait été envoyée.
    func testTheAppliedValueBecomesTheSavedOne() async {
        var reported: [(sent: Int, applied: Int)] = []
        let c = Committer<Int>(delay: .zero, saved: 0,
                               writeApplied: { new, _ in min(new, 3) },
                               onApplied: { reported.append((sent: $0, applied: $1)) })
        c.change(9)
        let ok = await eventually { !reported.isEmpty }
        XCTAssertTrue(ok)
        XCTAssertEqual(reported.first?.sent, 9)
        XCTAssertEqual(reported.first?.applied, 3)
        XCTAssertEqual(c.saved, 3)
    }

    /// `flush` n'attend pas la pause : ce qui attend part tout de suite.
    func testFlushWritesThePendingValueAtOnce() async {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .seconds(30), saved: 0, write: { writes.append($0) })
        c.change(4)
        await c.flush()
        XCTAssertEqual(writes, [4])
        await c.flush()
        XCTAssertEqual(writes, [4], "rien de plus à écrire")
    }

    /// Une tâche annulée avant d'avoir tourné (changement immédiat aussitôt
    /// remplacé) ne remet pas à zéro la pause longue de celle qui la remplace.
    func testACancelledImmediateChangeKeepsTheLongPause() async throws {
        var writes: [Int] = []
        let c = Committer<Int>(delay: .milliseconds(20), saved: 0, write: { writes.append($0) })
        c.change(1, immediate: true)
        c.change(2, after: .milliseconds(600))
        try await Task.sleep(for: .milliseconds(30))   // la première tâche, annulée, tourne
        c.change(3)                                       // modification ordinaire
        try await Task.sleep(for: .milliseconds(250))
        XCTAssertTrue(writes.isEmpty, "la pause longue tient encore : \(writes)")
        let ok = await eventually { writes == [3] }
        XCTAssertTrue(ok)
    }

    /// `flush` qui renonce à l'attente (l'écriture en vol porte déjà la
    /// valeur) oublie aussi sa pause longue : la modification suivante
    /// attend la pause ordinaire.
    func testFlushThatDropsThePendingWriteAlsoForgetsItsPause() async throws {
        var writes: [Int] = []
        var gate: CheckedContinuation<Void, Never>?
        let c = Committer<Int>(delay: .milliseconds(20), saved: 0, write: { v in
            if v == 1 { await withCheckedContinuation { gate = $0 } }
            writes.append(v)
        })
        c.change(1, immediate: true)
        _ = await eventually { gate != nil }              // 1 est en vol
        c.change(2, after: .seconds(30))                  // pause longue
        c.change(1)                                       // retour à la valeur en vol
        let flushed = Task { await c.flush() }
        try await Task.sleep(for: .milliseconds(20))
        gate?.resume()
        await flushed.value
        XCTAssertEqual(writes, [1])
        c.change(3)
        let ok = await eventually { writes == [1, 3] }
        // `eventually` attend 5 s au plus : la pause longue de 30 s le ferait échouer.
        XCTAssertTrue(ok, "la pause ordinaire, pas celle de 30 s : \(writes)")
    }
}
