import XCTest
@testable import Sillage

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
        XCTAssertEqual(router.running, .init(name: DictationModel.label(DictationModel.system), fallback: false))
    }

    func testAMissingModelFallsBackToTheSystemAndSaysSo() async throws {
        let system = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" }, directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(system.started, 1)
        XCTAssertNotNil(router.fallbackNotice)
        // La fenêtre de dictée nomme le remplaçant, en repli.
        XCTAssertEqual(router.running, .init(name: DictationModel.label(DictationModel.system), fallback: true))
        XCTAssertTrue(DictationModelName.text(router.running!).contains(DictationModel.label(DictationModel.system)))
    }

    func testTheChosenModelIsUsed() async throws {
        let system = Fake(), parakeet = Fake()
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                            directory: { _ in URL(fileURLWithPath: "/m") }, make: { _, _ in parakeet })
        try await router.start(locale: Locale(identifier: "fr-FR")) { _ in }
        XCTAssertEqual(parakeet.started, 1)
        XCTAssertEqual(system.started, 0)
        XCTAssertEqual(router.running, .init(name: "Parakeet v3", fallback: false))
    }
}

/// Les courses de l'aiguilleur : des moteurs dont le démarrage attend le
/// test (une continuation), pour ordonner à la main chargements, échecs,
/// annulations et dictées suivantes.
@MainActor
final class RoutingDictationEngineRaceTests: XCTestCase {
    final class Held: DictationEngine {
        let name: String
        var started = 0, finished = 0, cancelled = 0
        /// `false` : `start` rend la main tout de suite.
        var holds = true
        private var pending: CheckedContinuation<Void, Error>?

        init(_ name: String, holds: Bool = true) {
            self.name = name
            self.holds = holds
        }

        var isWaiting: Bool { pending != nil }

        func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
            started += 1
            guard holds else { return }
            try await withCheckedThrowingContinuation { pending = $0 }
        }
        func finish() async { finished += 1 }
        func cancel() async { cancelled += 1 }

        func succeed() { pending?.resume(); pending = nil }
        func fail(_ error: Error) { pending?.resume(throwing: error); pending = nil }
    }

    private let locale = Locale(identifier: "fr-FR")

    /// Un routeur sur Parakeet ; `engines` : les moteurs rendus par `make`,
    /// dans l'ordre des dictées.
    private func router(system: Held, engines: [Held]) -> RoutingDictationEngine {
        var queue = engines
        return RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                      directory: { _ in URL(fileURLWithPath: "/m") },
                                      make: { _, _ in queue.isEmpty ? nil : queue.removeFirst() })
    }

    /// Le modèle ne se charge pas : la dictée de macOS prend le relais, et
    /// c'est elle que `finish` arrête.
    func testALoadFailureFallsBackToTheSystem() async throws {
        let system = Held("system", holds: false), a = Held("A")
        let router = router(system: system, engines: [a])
        let start = Task { try await router.start(locale: locale) { _ in } }
        _ = await eventuallyTrue { a.isWaiting }
        a.fail(DictationError.failed("chargement"))
        try await start.value
        XCTAssertEqual(system.started, 1)
        XCTAssertEqual(a.cancelled, 1)
        XCTAssertNotNil(router.fallbackNotice)
        XCTAssertEqual(router.running?.fallback, true, "un chargement raté se voit dans la fenêtre")
        await router.finish()
        XCTAssertEqual(system.finished, 1)
        XCTAssertEqual(a.finished, 0)
    }

    /// Annulée pendant le chargement, puis le chargement échoue : pas de
    /// repli (la dictée de macOS n'ouvre pas le micro).
    func testNoFallbackAfterCancel() async throws {
        let system = Held("system", holds: false), a = Held("A")
        let router = router(system: system, engines: [a])
        let start = Task { try await router.start(locale: locale) { _ in } }
        _ = await eventuallyTrue { a.isWaiting }
        await router.cancel()
        a.fail(DictationError.failed("chargement"))
        do { try await start.value; XCTFail("l'échec remonte") } catch {}
        XCTAssertEqual(system.started, 0)
        XCTAssertNil(router.fallbackNotice)
    }

    func testNoFallbackAfterFinish() async throws {
        let system = Held("system", holds: false), a = Held("A")
        let router = router(system: system, engines: [a])
        let start = Task { try await router.start(locale: locale) { _ in } }
        _ = await eventuallyTrue { a.isWaiting }
        await router.finish()
        a.fail(DictationError.failed("chargement"))
        do { try await start.value; XCTFail("l'échec remonte") } catch {}
        XCTAssertEqual(system.started, 0)
        XCTAssertNil(router.fallbackNotice)
    }

    /// La course : A annulée, B démarre, puis A échoue (tard). B reste la
    /// dictée en cours — c'est elle que `finish` et `cancel` arrêtent ;
    /// l'échec de A n'ouvre pas la dictée de macOS et ne dit rien.
    func testALateFailureNeverReplacesTheNextDictation() async throws {
        let system = Held("system", holds: false), a = Held("A"), b = Held("B", holds: false)
        let router = router(system: system, engines: [a, b])
        let first = Task { try await router.start(locale: locale) { _ in } }
        _ = await eventuallyTrue { a.isWaiting }
        await router.cancel()
        try await router.start(locale: locale) { _ in }
        XCTAssertEqual(b.started, 1)

        a.fail(DictationError.failed("chargement"))
        do {
            try await first.value
            XCTFail("la dictée remplacée ne démarre pas")
        } catch {
            XCTAssertTrue(error is CancellationError, "\(error)")
        }
        XCTAssertEqual(system.started, 0)
        XCTAssertNil(router.fallbackNotice)

        await router.finish()
        XCTAssertEqual(b.finished, 1)
        XCTAssertEqual(system.finished, 0)
        await router.cancel()
        XCTAssertEqual(b.cancelled, 1)
        XCTAssertEqual(system.cancelled, 0)
    }

    /// A démarre tard (son micro s'ouvre) après que B a pris la place : A
    /// est arrêtée, B reste.
    func testALateSuccessIsStopped() async throws {
        let system = Held("system", holds: false), a = Held("A"), b = Held("B", holds: false)
        let router = router(system: system, engines: [a, b])
        let first = Task { try await router.start(locale: locale) { _ in } }
        _ = await eventuallyTrue { a.isWaiting }
        await router.cancel()
        try await router.start(locale: locale) { _ in }
        let cancelledBefore = a.cancelled

        a.succeed()
        do { try await first.value; XCTFail("remplacée") } catch {
            XCTAssertTrue(error is CancellationError, "\(error)")
        }
        XCTAssertEqual(a.cancelled, cancelledBefore + 1)
        await router.finish()
        XCTAssertEqual(b.finished, 1)
        XCTAssertEqual(a.finished, 0)
    }

    /// Une nouvelle dictée arrête l'ancien moteur s'il n'a pas été arrêté.
    func testStartStopsThePreviousEngine() async throws {
        let system = Held("system", holds: false), a = Held("A", holds: false), b = Held("B", holds: false)
        let router = router(system: system, engines: [a, b])
        try await router.start(locale: locale) { _ in }
        try await router.start(locale: locale) { _ in }
        XCTAssertEqual(a.cancelled, 1)
        await router.finish()
        XCTAssertEqual(b.finished, 1)
    }

    /// Libérer le modèle (supprimé, ou un autre choisi) efface aussi le
    /// repli affiché.
    func testReleasingTheModelClearsTheNotice() async throws {
        let system = Held("system", holds: false)
        let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                            directory: { _ in nil }, make: { _, _ in nil })
        try await router.start(locale: locale) { _ in }
        XCTAssertNotNil(router.fallbackNotice)
        router.releaseModel()
        XCTAssertNil(router.fallbackNotice)
    }
}
