import XCTest
@testable import Sillon

@MainActor
final class RoutingDictationEngineTests: XCTestCase {
    final class Fake: DictationEngine {
        var started = 0
        func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws { started += 1 }
        func finish() async {}
        func cancel() async {}
    }

    func testSystemByDefault() async throws {
        let system = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "system" }, directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(system.started, 1)
        XCTAssertNil(router.fallbackNotice)
    }

    func testAMissingModelFallsBackToTheSystemAndSaysSo() async throws {
        let system = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" }, directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(system.started, 1)
        XCTAssertNotNil(router.fallbackNotice)
    }

    func testTheChosenModelIsUsed() async throws {
        let system = Fake(), parakeet = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                            directory: { _ in URL(fileURLWithPath: "/m") }, make: { _, _ in parakeet })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(parakeet.started, 1)
        XCTAssertEqual(system.started, 0)
    }
}
