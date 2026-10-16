import XCTest
@testable import Sillon

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
