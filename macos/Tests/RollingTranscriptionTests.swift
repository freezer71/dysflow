import XCTest
@testable import Dysflow

final class RollingTranscriptionTests: XCTestCase {
    func testWaitsForTheIntervalAndNeverOverlaps() {
        let clock = ContinuousClock()
        let t0 = clock.now
        var r = RollingTranscription(interval: .milliseconds(1500))
        XCTAssertTrue(r.shouldRead(now: t0, busy: false), "première lecture tout de suite")
        XCTAssertFalse(r.shouldRead(now: t0.advanced(by: .milliseconds(500)), busy: false))
        XCTAssertFalse(r.shouldRead(now: t0.advanced(by: .milliseconds(2000)), busy: true), "jamais deux à la fois")
        XCTAssertTrue(r.shouldRead(now: t0.advanced(by: .milliseconds(2000)), busy: false))
    }
}

final class RollingRhythmTests: XCTestCase {
    func testTheRhythmFollowsTheModelSpeed() {
        var r = RollingTranscription(interval: RollingTranscription.initial)
        r.adapt(lastRead: .milliseconds(100))
        XCTAssertEqual(r.interval, RollingTranscription.fastest, "un modèle rapide relit toutes les 0,4 s")
        r.adapt(lastRead: .milliseconds(300))
        XCTAssertEqual(r.interval, .milliseconds(900), "trois fois la lecture : un tiers du temps occupé")
        r.adapt(lastRead: .seconds(2))
        XCTAssertEqual(r.interval, .seconds(6), "une lecture lente : toujours trois fois, sans plafond")
    }
}
