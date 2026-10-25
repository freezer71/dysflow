import XCTest
@testable import Dysflow

/// Un passage avant factice : jamais MLX ni de poids dans les tests. Il
/// mesure le nombre d'appels simultanés (le calcul doit les sérialiser), et
/// imite les points d'arrêt de `KokoroModel` : après les durées, après
/// `encode`, après chacun des 4 blocs de `decode`, après chacun des 2
/// étages du générateur ; puis le générateur finit.
final class FakeKokoroForward: KokoroForward, @unchecked Sendable {
    private let lock = NSLock()
    private var inFlight = 0
    private(set) var maxInFlight = 0
    private(set) var runs = 0
    private var decoded = 0
    private var stopPoints = 0
    private var lastIds: [Int32] = []
    private var lastHadStopTest: Bool?
    /// Si posé, chaque passage attend ce signal (synthèse « longue »).
    var gate: DispatchSemaphore?
    /// Signalé à l'entrée de chaque passage.
    let entered = DispatchSemaphore(value: 0)

    static let stopPointsPerPass = 1 + 1 + 4 + 2

    func run(ids: [Int32], style: [Float],
             shouldStop: (() -> Bool)?) -> (samples: [Float], durations: [Int32])? {
        lock.withLock {
            inFlight += 1
            runs += 1
            maxInFlight = max(maxInFlight, inFlight)
            lastIds = ids
            lastHadStopTest = shouldStop != nil
        }
        defer { lock.withLock { inFlight -= 1 } }
        entered.signal()
        if let gate { gate.wait() } else { usleep(500) }
        if let shouldStop {
            for _ in 0..<Self.stopPointsPerPass {
                lock.withLock { stopPoints += 1 }
                if shouldStop() { return nil }
            }
        }
        // Le générateur va au bout.
        lock.withLock { decoded += 1 }
        return ([Float](repeating: 0, count: ids.count * 600), [Int32](repeating: 2, count: ids.count))
    }

    var observedMax: Int { lock.withLock { maxInFlight } }
    var observedRuns: Int { lock.withLock { runs } }
    /// Passages dont le générateur est allé au bout.
    var decoderRuns: Int { lock.withLock { decoded } }
    /// Points d'arrêt atteints, tous passages confondus.
    var observedStopPoints: Int { lock.withLock { stopPoints } }
    var observedLastIds: [Int32] { lock.withLock { lastIds } }
    /// Le dernier passage avait-il un test d'abandon (des points d'arrêt) ?
    var observedLastHadStopTest: Bool? { lock.withLock { lastHadStopTest } }
}

/// Les essais d'avant l'abandon : une synthèse jamais abandonnée.
extension KokoroMLXCompute {
    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        try synthesize(ids: ids, style: style, cancel: SynthesisCancel.fixed(cancelled: false))
    }
}

/// Compte les chargements et les vidages de cache demandés au calcul.
final class ForwardFactory: @unchecked Sendable {
    private let lock = NSLock()
    private var counts = (loads: 0, clears: 0)
    private var loaded: [String] = []
    let forward = FakeKokoroForward()
    var loads: Int { lock.withLock { counts.loads } }
    var directories: [String] { lock.withLock { loaded } }
    var clears: Int { lock.withLock { counts.clears } }

    /// Un registre à part, qui ne touche pas aux plafonds de MLX du
    /// processus de test (les essais de fumée chargent de vrais modèles).
    let budget = MLXBudget(apply: { _, _ in })

    func compute() -> KokoroMLXCompute {
        KokoroMLXCompute(
            loadForward: { [self] url in
                lock.withLock { counts.loads += 1; loaded.append(url.path) }
                return forward
            },
            clearCache: { [self] in lock.withLock { counts.clears += 1 } },
            budget: budget)
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
        let budget = MLXBudget(apply: { _, _ in })
        let compute = KokoroMLXCompute(loadForward: { _ in throw CocoaError(.fileNoSuchFile) },
                                       clearCache: {}, budget: budget)
        XCTAssertThrowsError(try compute.load(modelDir: "/absent")) { error in
            guard case DysflowError.Failed(let message) = error else { return XCTFail("\(error)") }
            XCTAssertTrue(message.hasPrefix("Calcul Metal indisponible"), message)
        }
        XCTAssertThrowsError(try compute.warmUp(), "rien de chargé")
        XCTAssertEqual(compute.calls, 0)
        XCTAssertEqual(budget.total, .init(memory: 0, cache: 0), "un chargement raté ne garde aucun besoin")
    }

    /// Kokoro inscrit son besoin dans `MLXBudget` au chargement et le retire
    /// au dernier `release()` seulement.
    func testKokoroHoldsItsNeedInTheBudgetWhileLoaded() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        let kokoro = MLXBudget.Need(memory: KokoroMLXCompute.memoryLimit, cache: KokoroMLXCompute.cacheLimit)
        try compute.load(modelDir: "/m/kokoro")
        try compute.load(modelDir: "/m/kokoro")
        XCTAssertEqual(factory.budget.total, kokoro, "inscrit une fois")
        compute.release()
        XCTAssertEqual(factory.budget.total, kokoro, "encore un détenteur")
        compute.release()
        XCTAssertEqual(factory.budget.total, .init(memory: 0, cache: 0))
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
        let syntheses = threads * perThread
        XCTAssertEqual(factory.forward.observedMax, 1, "un passage MLX à la fois")
        // Une préparation venue après une synthèse ne fait rien.
        XCTAssertEqual(compute.calls, factory.forward.observedRuns)
        XCTAssertTrue((syntheses...syntheses + threads / 2).contains(compute.calls), "\(compute.calls)")
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

    /// Le moteur passe à un autre dossier (`SharedCompute` rend sa part sur
    /// l'ancien, puis charge le nouveau) pendant que la préparation tient
    /// encore le modèle : le nouveau dossier est chargé, sans part en trop.
    func testAnotherDirectoryReplacesTheModelWithoutAnExtraHolder() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/a")      // la préparation
        try compute.load(modelDir: "/m/a")      // le moteur
        compute.release()                       // le moteur rend /m/a…
        try compute.load(modelDir: "/m/b")      // … et charge /m/b
        XCTAssertEqual(factory.directories, ["/m/a", "/m/b"])
        XCTAssertEqual(factory.clears, 1, "les poids de /m/a partent")
        compute.release()                       // la préparation finit
        XCTAssertNoThrow(try compute.synthesize(ids: [0], style: []), "le moteur tient /m/b")
        compute.release()                       // le moteur rend /m/b
        XCTAssertEqual(factory.clears, 2)
        XCTAssertThrowsError(try compute.synthesize(ids: [0], style: []))
    }

    /// Le pool n'a plus l'usage de la synthèse quand les durées sont là : le
    /// décodeur ne tourne pas, et l'abandon revient comme tel, pas comme un
    /// échec. Le passage compte (`calls`, `forwardSeconds`) : il a eu lieu.
    func testAnAbandonAfterTheDurationsSkipsTheDecoder() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        var asked = 0
        // Utile au départ, inutile une fois les durées calculées.
        XCTAssertThrowsError(try compute.synthesize(ids: [0, 43, 0], style: [],
                                                    isCancelled: { asked += 1; return asked > 1 })) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(asked, 2, "avant le passage, puis après les durées")
        XCTAssertEqual(factory.forward.observedStopPoints, 1)
        XCTAssertEqual(factory.forward.observedRuns, 1)
        XCTAssertEqual(factory.forward.decoderRuns, 0, "aucune exécution du décodeur")
        XCTAssertEqual(compute.calls, 1, "le passage a eu lieu")
        XCTAssertGreaterThan(compute.forwardSeconds, 0, "son temps compte")

        // Une synthèse utile va au bout.
        let out = try compute.synthesize(ids: [0, 43, 0], style: [], cancel: SynthesisCancel.fixed(cancelled: false))
        XCTAssertEqual(out.durations, [2, 2, 2])
        XCTAssertEqual(factory.forward.decoderRuns, 1)
        compute.release()
    }

    /// Le saut arrive quand les durées sont déjà là, pendant le décodeur
    /// (le cas mesuré : la suite de la phrase précédente) : le passage
    /// s'arrête au point suivant, le générateur ne finit pas.
    func testAStopRequestedInTheDecoderStopsIt() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        var asked = 0
        // Avant le passage, durées, encode : utile ; au 1er bloc de decode : plus.
        XCTAssertThrowsError(try compute.synthesize(ids: [0, 43, 0], style: [],
                                                    isCancelled: { asked += 1; return asked > 3 })) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(asked, 4)
        XCTAssertEqual(factory.forward.observedStopPoints, 3, "durées, encode, 1er bloc")
        XCTAssertEqual(factory.forward.decoderRuns, 0, "le générateur ne finit pas")
        // Toujours utile : tous les points consultés, le générateur finit.
        _ = try compute.synthesize(ids: [0, 43, 0], style: [], isCancelled: { false })
        XCTAssertEqual(factory.forward.observedStopPoints, 3 + FakeKokoroForward.stopPointsPerPass)
        XCTAssertEqual(factory.forward.decoderRuns, 1)
        compute.release()
    }

    /// Une synthèse inutile pendant une brève libération des poids : un
    /// abandon, pas un échec.
    func testAnAbandonWithoutWeightsIsStillAnAbandon() {
        let compute = ForwardFactory().compute()
        XCTAssertThrowsError(try compute.synthesize(ids: [0], style: [],
                                                    cancel: SynthesisCancel.fixed(cancelled: true))) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
    }

    /// La préparation : une vraie phrase courte (noyaux des tailles d'une
    /// lecture), jamais abandonnée, sans point d'arrêt ni synchronisation
    /// de plus.
    func testTheWarmUpSpeaksARealisticSentenceWithoutStopPoints() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        try compute.warmUp()
        let ids = factory.forward.observedLastIds
        XCTAssertGreaterThanOrEqual(ids.count, 30, "\(ids.count) jetons")
        XCTAssertEqual(ids.first, 0)
        XCTAssertEqual(ids.last, 0)
        XCTAssertTrue(ids.contains(16) && ids.contains(3), "espaces et virgule")
        XCTAssertEqual(factory.forward.observedLastHadStopTest, false)
        XCTAssertEqual(factory.forward.observedStopPoints, 0)
        compute.release()
    }

    /// Déjà inutile avant le passage (par exemple en attente derrière la
    /// préparation) : MLX n'est pas appelé du tout.
    func testAnAbandonBeforeThePassSkipsMLX() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        XCTAssertThrowsError(try compute.synthesize(ids: [0, 43, 0], style: [],
                                                    cancel: SynthesisCancel.fixed(cancelled: true))) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(factory.forward.observedRuns, 0)
        XCTAssertEqual(compute.calls, 0)
        compute.release()
    }

    /// Une synthèse a déjà compilé les noyaux dans ce processus : la
    /// préparation n'ajoute pas un passage derrière la lecture.
    func testTheWarmUpIsSkippedOnceASynthesisCompiledTheKernels() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        _ = try compute.synthesize(ids: [0, 43, 0], style: [])
        try compute.warmUp()
        XCTAssertEqual(factory.forward.observedRuns, 1, "aucun passage de plus")
        XCTAssertEqual(compute.calls, 1)
        compute.release()

        let fresh = ForwardFactory()
        let cold = fresh.compute()
        try cold.load(modelDir: "/m/kokoro")
        try cold.warmUp()
        try cold.warmUp()
        XCTAssertEqual(fresh.forward.observedRuns, 1, "une préparation, la seconde est inutile")
        cold.release()
    }
}
