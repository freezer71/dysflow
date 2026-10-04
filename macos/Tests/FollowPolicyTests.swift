import XCTest
@testable import Sillon

/// Quand la vue suit-elle la lecture ? Pas pendant que l'on relit en faisant
/// défiler le texte.
final class FollowPolicyTests: XCTestCase {
    private let t0 = Date(timeIntervalSinceReferenceDate: 1000)

    func testFollowsByDefault() {
        var p = FollowPolicy()
        XCTAssertTrue(p.shouldFollow(newSentence: false, at: t0))
    }

    func testUserScrollSuspendsFollowing() {
        var p = FollowPolicy()
        p.userScrolled(at: t0)
        XCTAssertFalse(p.shouldFollow(newSentence: false, at: t0.addingTimeInterval(1)))
        XCTAssertFalse(p.shouldFollow(newSentence: true, at: t0.addingTimeInterval(1)))
    }

    func testAPlaybackCommandResumes() {
        var p = FollowPolicy()
        p.userScrolled(at: t0)
        p.resume()
        XCTAssertTrue(p.shouldFollow(newSentence: false, at: t0.addingTimeInterval(0.1)))
    }

    func testANewSentenceAfterQuietDelayResumes() {
        var p = FollowPolicy()
        p.userScrolled(at: t0)
        // Le délai est passé, mais on reste dans la même phrase : on attend.
        XCTAssertFalse(p.shouldFollow(newSentence: false, at: t0.addingTimeInterval(5)))
        XCTAssertTrue(p.shouldFollow(newSentence: true, at: t0.addingTimeInterval(4)))
        XCTAssertTrue(p.shouldFollow(newSentence: false, at: t0.addingTimeInterval(4.1)))
    }

    func testScrollingAgainRestartsTheDelay() {
        var p = FollowPolicy()
        p.userScrolled(at: t0)
        p.userScrolled(at: t0.addingTimeInterval(3))
        XCTAssertFalse(p.shouldFollow(newSentence: true, at: t0.addingTimeInterval(5)))
        XCTAssertTrue(p.shouldFollow(newSentence: true, at: t0.addingTimeInterval(7)))
    }
}
