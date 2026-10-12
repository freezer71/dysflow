import XCTest
@testable import Sillon

/// Un passage avant factice : jamais MLX ni de poids dans les tests. Il
/// mesure le nombre d'appels simultanés (le calcul doit les sérialiser).
final class FakeKokoroForward: KokoroForward, @unchecked Sendable {
    private let lock = NSLock()
    private var inFlight = 0
    private(set) var maxInFlight = 0
    private(set) var runs = 0
    /// Si posé, chaque passage attend ce signal (synthèse « longue »).
    var gate: DispatchSemaphore?
    /// Signalé à l'entrée de chaque passage.
    let entered = DispatchSemaphore(value: 0)

    func run(ids: [Int32], style: [Float]) -> (samples: [Float], durations: [Int32]) {
        lock.withLock {
            inFlight += 1
            runs += 1
            maxInFlight = max(maxInFlight, inFlight)
        }
        entered.signal()
        if let gate { gate.wait() } else { usleep(500) }
        lock.withLock { inFlight -= 1 }
        return ([Float](repeating: 0, count: ids.count * 600), [Int32](repeating: 2, count: ids.count))
    }

    var observedMax: Int { lock.withLock { maxInFlight } }
    var observedRuns: Int { lock.withLock { runs } }
}

/// Compte les chargements et les vidages de cache demandés au calcul.
final class ForwardFactory: @unchecked Sendable {
    private let lock = NSLock()
    private var counts = (loads: 0, clears: 0)
    let forward = FakeKokoroForward()
    var loads: Int { lock.withLock { counts.loads } }
    var clears: Int { lock.withLock { counts.clears } }

    func compute() -> KokoroMLXCompute {
        KokoroMLXCompute(
            loadForward: { [self] _ in lock.withLock { counts.loads += 1 }; return forward },
            clearCache: { [self] in lock.withLock { counts.clears += 1 } })
    }
}

final class KokoroMLXComputeTests: XCTestCase {
    /// La préparation et le lecteur partagent le modèle : il n'est libéré
    /// (poids et cache) qu'au dernier `release()`.
    func testTheModelIsSharedAndFreedAtTheLastRelease() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        try compute.load(modelDir: "/m/kokoro")
        XCTAssertEqual(factory.loads, 1, "chargé une fois")
        compute.release()
        XCTAssertEqual(factory.clears, 0)
        XCTAssertEqual(try compute.synthesize(ids: [0, 43, 0], style: [Float](repeating: 0, count: 256)).durations,
                       [2, 2, 2])
        compute.release()
        XCTAssertEqual(factory.clears, 1, "dernier relâché : cache vidé")
        XCTAssertThrowsError(try compute.synthesize(ids: [0], style: []))
        compute.release()
        XCTAssertEqual(factory.clears, 1, "un relâché de trop ne fait rien")
        try compute.load(modelDir: "/m/kokoro")
        XCTAssertEqual(factory.loads, 2, "rechargé après libération")
    }

    func testAFailedLoadHoldsNothing() {
        let compute = KokoroMLXCompute(loadForward: { _ in throw CocoaError(.fileNoSuchFile) },
                                       clearCache: {})
        XCTAssertThrowsError(try compute.load(modelDir: "/absent")) { error in
            guard case SillonError.Failed(let message) = error else { return XCTFail("\(error)") }
            XCTAssertTrue(message.hasPrefix("Calcul Metal indisponible"), message)
        }
        XCTAssertThrowsError(try compute.warmUp(), "rien de chargé")
        XCTAssertEqual(compute.calls, 0)
    }

    /// Rust appelle depuis ses fils de synthèse, parfois en même temps (un
    /// ancien moteur finit pendant qu'un nouveau se charge et se prépare) :
    /// jamais deux passages MLX à la fois, et le compte reste juste.
    func testConcurrentCallsFromBackgroundThreadsAreSerialised() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")   // le lecteur garde le modèle
        let threads = 8, perThread = 20
        DispatchQueue.concurrentPerform(iterations: threads) { i in
            do {
                try compute.load(modelDir: "/m/kokoro")
                if i % 2 == 0 { try compute.warmUp() }
                for _ in 0..<perThread {
                    _ = try compute.synthesize(ids: [0, 43, 0], style: [Float](repeating: 0, count: 256))
                }
                compute.release()
            } catch {
                XCTFail("\(error)")
            }
        }
        let expected = threads * perThread + threads / 2
        XCTAssertEqual(factory.forward.observedMax, 1, "un passage MLX à la fois")
        XCTAssertEqual(factory.forward.observedRuns, expected)
        XCTAssertEqual(compute.calls, expected)
        XCTAssertEqual(factory.loads, 1)
        XCTAssertEqual(factory.clears, 0, "le lecteur le tient encore")
        compute.release()
        XCTAssertEqual(factory.clears, 1)
    }

    /// Lire le compte (mesure, fil principal) n'attend jamais une synthèse
    /// en cours.
    @MainActor
    func testReadingTheCountNeverWaitsForASynthesis() throws {
        let factory = ForwardFactory()
        let gate = DispatchSemaphore(value: 0)
        factory.forward.gate = gate
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        let done = expectation(description: "synthèse finie")
        DispatchQueue.global().async {
            _ = try? compute.synthesize(ids: [0], style: [])
            done.fulfill()
        }
        XCTAssertEqual(factory.forward.entered.wait(timeout: .now() + 5), .success)
        let start = Date()
        XCTAssertEqual(compute.calls, 1)
        XCTAssertLessThan(Date().timeIntervalSince(start), 0.5, "le fil principal n'attend pas")
        gate.signal()
        wait(for: [done], timeout: 5)
        compute.release()
    }
}
