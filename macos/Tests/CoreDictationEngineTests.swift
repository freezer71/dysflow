import XCTest
@testable import Dysflow

/// La dictée par le cœur Rust, sur une session simulée (le cœur ne charge
/// rien, le test dit ce qu'il dit) et une source d'audio simulée : jamais le
/// vrai micro, jamais un vrai modèle.
@MainActor
final class CoreDictationEngineTests: XCTestCase {
    /// Une source d'audio simulée : le test y dépose l'audio et le spectre.
    final class Audio: DictationAudioSource, @unchecked Sendable {
        private let lock = NSLock()
        private var waiting: [Float] = []
        private var stopped = false
        var spectrum: (@Sendable ([[Float]]) -> Void)?

        func say(_ samples: [Float]) { lock.withLock { if !stopped { waiting += samples } } }
        func stop() { lock.withLock { stopped = true } }
        var isStopped: Bool { lock.withLock { stopped } }
        func drain() -> [Float] { lock.withLock { defer { waiting = [] }; return waiting } }
    }

    /// Une session du cœur simulée : elle garde l'audio reçu et l'écouteur ;
    /// `finish` dit ce que `ending` lui donne, puis rend le texte, comme le
    /// cœur (toutes les nouvelles remises avant le retour).
    final class Session: DictationSessionProtocol, @unchecked Sendable {
        private let lock = NSLock()
        private var samples: [Float] = []
        private var cancels = 0
        let listener: DictationListener
        var ending: [DictationUpdate] = []

        init(listener: DictationListener) { self.listener = listener }

        func push(samples more: [Float]) { lock.withLock { samples += more } }
        func finish() throws -> String {
            let ending = lock.withLock { self.ending }
            for update in ending { listener.onUpdate(update: update) }
            if case .ended(let text)? = ending.last { return text }
            throw DysflowError.Failed(message: "dictée déjà finie")
        }
        func cancel() { lock.withLock { cancels += 1 } }

        var pushed: [Float] { lock.withLock { samples } }
        var cancelled: Int { lock.withLock { cancels } }
        func say(_ update: DictationUpdate) { listener.onUpdate(update: update) }
    }

    /// Le cœur simulé : les sessions ouvertes, les libérations demandées.
    final class Core: @unchecked Sendable {
        private let lock = NSLock()
        private var opened: [Session] = []
        private var releases = 0
        var refuse: String?

        var sessions: [Session] { lock.withLock { opened } }
        var released: Int { lock.withLock { releases } }

        var dictation: CoreDictation {
            CoreDictation(
                start: { [self] id, _, listener in
                    if let refuse = lock.withLock({ self.refuse }) { throw DysflowError.Failed(message: refuse) }
                    let session = Session(listener: listener)
                    lock.withLock { opened.append(session) }
                    return session
                },
                release: { [self] in lock.withLock { releases += 1 } },
                releaseIfLoaded: { _ in },
                setKeepVoiceMinutes: { _ in },
                computeSeconds: { 0 },
                readOnce: { _, _, _ in "" })
        }
    }

    /// Les événements reçus, sauf le spectre (compté à part).
    final class Events {
        var seen: [DictationEvent] = []
        var spectra = 0
        func receive(_ event: DictationEvent) {
            if case .spectrum = event { spectra += 1 } else { seen.append(event) }
        }
    }

    private let locale = Locale(identifier: "fr-FR")
    private var audio: Audio!
    private var core: Core!

    override func setUp() async throws {
        audio = Audio()
        core = Core()
    }

    override func tearDown() async throws {
        CoreDictation.testRoute = nil
    }

    private var microphone: DictationMicrophone {
        let audio = audio!
        return DictationMicrophone(permission: {}, open: { spectrum in
            audio.spectrum = spectrum
            return audio
        })
    }

    private func engine() -> CoreDictationEngine {
        CoreDictationEngine(id: "parakeet_tdt_v3", core: core.dictation, microphone: microphone)
    }

    private func session() async -> Session {
        _ = await eventuallyTrue { !core.sessions.isEmpty }
        return core.sessions[0]
    }

    private static let loudColumns: [[Float]] = [[Float](repeating: 0.5, count: 12)]

    /// Une dictée : la suite d'événements de `MLXDictationEngine` aujourd'hui
    /// (micro ouvert, chargement, textes provisoires avec leur délai et la
    /// mémoire, texte définitif), et tout l'audio arrive au cœur.
    func testADictationGivesTodaysEventsAndLosesNoAudio() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        audio.say([0.1, 0.2])
        session.say(.loading)
        session.say(.loaded(seconds: 0.5))
        session.say(.ready)
        try await start.value
        audio.say([0.3])
        // L'audio part au cœur pendant la dictée, pas seulement à la fin.
        let fed = await eventuallyTrue { session.pushed == [0.1, 0.2, 0.3] }
        XCTAssertTrue(fed, "l'audio part au fil de la dictée : \(session.pushed)")
        session.say(.partial(committed: "", pending: "bonjour"))
        session.say(.committed(text: "bonjour", delaySeconds: 0.25))
        session.say(.partial(committed: "bonjour", pending: ""))
        session.say(.memory(bytes: 800 << 20))
        session.say(.partial(committed: "bonjour", pending: "tout le"))
        audio.say([0.4, 0.5])
        session.ending = [.committed(text: "tout le monde", delaySeconds: 0.5),
                          .partial(committed: "bonjour tout le monde", pending: ""),
                          .ended(text: "bonjour tout le monde")]
        await engine.finish()
        XCTAssertEqual(events.seen, [
            .listening, .loading, .loaded(.seconds(0.5)),
            .text("bonjour", isFinal: false),
            .latency(.milliseconds(250)), .text("bonjour", isFinal: false),
            .memory(800 << 20),
            .text("bonjour tout le", isFinal: false),
            .latency(.milliseconds(500)), .text("bonjour tout le monde", isFinal: false),
            .text("bonjour tout le monde", isFinal: true),
        ])
        XCTAssertEqual(session.pushed, [0.1, 0.2, 0.3, 0.4, 0.5], "tout l'audio, dans l'ordre")
        XCTAssertTrue(audio.isStopped)
        XCTAssertEqual(session.cancelled, 0)
    }

    /// Le spectre du micro continue pendant le chargement : `start` attend
    /// encore le modèle, la fenêtre voit déjà la voix.
    func testTheSpectrumGoesOnWhileTheModelLoads() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        session.say(.loading)
        for _ in 0..<5 { audio.spectrum?(Self.loudColumns) }
        let flowed = await eventuallyTrue { events.spectra == 5 }
        XCTAssertTrue(flowed, "le spectre pendant le chargement")
        XCTAssertEqual(events.seen, [.listening, .loading], "le modèle se charge encore")
        session.say(.ready)
        try await start.value
        await engine.cancel()
    }

    /// Annulée : plus rien n'est émis, la session du cœur est arrêtée, le
    /// micro fermé.
    func testCancelEmitsNothingMore() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        session.say(.ready)
        try await start.value
        await engine.cancel()
        session.say(.partial(committed: "", pending: "trop tard"))
        session.say(.ended(text: "trop tard"))
        _ = await eventuallyTrue(.milliseconds(300)) { events.seen.count > 1 }
        XCTAssertEqual(events.seen, [.listening])
        let stopped = await eventuallyTrue { session.cancelled == 1 }
        XCTAssertTrue(stopped)
        XCTAssertTrue(audio.isStopped)
    }

    /// Annulée pendant le chargement : `start` rend la main sans erreur (pas
    /// de repli), rien n'est émis ensuite.
    func testCancelDuringLoadingReturnsQuietly() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        session.say(.loading)
        _ = await eventuallyTrue { events.seen.count == 2 }
        await engine.cancel()
        try await start.value
        session.say(.ready)
        _ = await eventuallyTrue(.milliseconds(300)) { events.seen.count > 2 }
        XCTAssertEqual(events.seen, [.listening, .loading])
        let stopped = await eventuallyTrue { session.cancelled == 1 }
        XCTAssertTrue(stopped)
    }

    /// La touche relâchée pendant le chargement : `finish` attend le modèle,
    /// tout l'audio dit est lu, et le texte définitif arrive ; `start` rend
    /// la main sans erreur.
    func testFinishDuringLoadingReadsEverythingSaid() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        session.say(.loading)
        audio.say([0.1, 0.2, 0.3])
        session.ending = [.loaded(seconds: 1), .ready,
                          .committed(text: "bonjour", delaySeconds: 1.5),
                          .partial(committed: "bonjour", pending: ""),
                          .ended(text: "bonjour")]
        await engine.finish()
        try await start.value
        XCTAssertEqual(events.seen, [
            .listening, .loading, .loaded(.seconds(1)),
            .latency(.milliseconds(1500)), .text("bonjour", isFinal: false),
            .text("bonjour", isFinal: true),
        ])
        XCTAssertEqual(session.pushed, [0.1, 0.2, 0.3])
    }

    /// Un énoncé illisible après le chargement : la dictée continue, sans
    /// rien montrer de plus.
    func testAFailureAfterTheModelIsReadyIsNotAFallback() async throws {
        let engine = engine(), events = Events()
        let start = Task { try await engine.start(locale: locale) { events.receive($0) } }
        let session = await session()
        session.say(.ready)
        try await start.value
        session.say(.failed(message: "une phrase n'a pas pu être lue"))
        session.say(.partial(committed: "", pending: "suite"))
        _ = await eventuallyTrue { events.seen.count == 2 }
        XCTAssertEqual(events.seen, [.listening, .text("suite", isFinal: false)])
        await engine.cancel()
    }

    /// Le modèle ne se charge pas (`Failed` avant `Ready`), ou le cœur refuse
    /// la session (paquet absent, binaire sans MLX) : la dictée de macOS
    /// prend le relais, comme aujourd'hui, et le micro du cœur se ferme.
    func testALoadFailureFallsBackToMacOSDictation() async throws {
        for refuse in [nil, "ce binaire ne sait pas dicter avec Parakeet v3"] {
            audio = Audio()
            core = Core()
            core.refuse = refuse
            let system = RoutingDictationEngineTests.Fake()
            let engine = engine()
            let router = RoutingDictationEngine(system: system, chosen: { "parakeet_tdt_v3" },
                                                directory: { _ in URL(fileURLWithPath: "/m") },
                                                make: { _, _ in engine })
            let start = Task { try await router.start(locale: locale) { _ in } }
            if refuse == nil {
                let session = await session()
                session.say(.loading)
                session.say(.failed(message: "le modèle ne se charge pas : poids tronqués"))
                try await start.value
                let stopped = await eventuallyTrue { session.cancelled == 1 }
                XCTAssertTrue(stopped)
            } else {
                try await start.value
            }
            XCTAssertEqual(system.started, 1, "\(refuse ?? "Failed")")
            XCTAssertNotNil(router.fallbackNotice)
            XCTAssertEqual(router.running?.fallback, true)
            XCTAssertTrue(audio.isStopped, "le micro du cœur est fermé")
        }
    }

    func testTheShownTextJoinsCommittedAndPendingWithoutExtraSpaces() {
        XCTAssertEqual(CoreDictationEngine.shown(committed: "", pending: "a"), "a")
        XCTAssertEqual(CoreDictationEngine.shown(committed: "a", pending: ""), "a")
        XCTAssertEqual(CoreDictationEngine.shown(committed: "a b", pending: "c"), "a b c")
        XCTAssertEqual(CoreDictationEngine.shown(committed: "", pending: ""), "")
    }
}

/// L'aiguillage vers le cœur : une seule bascule (`CoreDictation.ported`),
/// fermée tant que la P5 de la tâche 14 n'est pas passée ; et chaque côté
/// libère son modèle, avec sa part du budget, quand l'autre dicte.
@MainActor
final class CoreDictationRoutingTests: XCTestCase {
    final class Echo: LoadedSpeechModel {
        func transcribe(_ samples: [Float], language: String) throws -> String { "" }
    }

    override func tearDown() async throws {
        CoreDictation.testRoute = nil
    }

    private let folder = URL(fileURLWithPath: "/m/parakeet_tdt_v3")

    /// Parakeet reste sur Swift : la bascule est fermée.
    /// La bascule ne nomme que des modèles que le cœur sait dicter dans ce
    /// binaire (`stt::recognizer_for`) : sinon chaque dictée de ce modèle
    /// tomberait sur le repli.
    func testEveryPortedModelIsOneTheCoreCanDictate() {
        for id in CoreDictation.ported {
            XCTAssertTrue(coreDictationSupports(id: id), "\(id) : le cœur ne le dicte pas (build sans mlx ?)")
        }
        XCTAssertFalse(coreDictationSupports(id: "inconnu"))
    }

    func testParakeetStaysOnSwiftUntilTheSwitchIsFlipped() {
        XCTAssertTrue(CoreDictation.ported.isEmpty, "à ouvrir après la P5 de la tâche 14")
        let runner = SpeechModelRunner(load: { _ in Echo() }, gate: MLXGate(), clearCache: {})
        for id in ["parakeet_tdt_v3", "parakeet_tdt_v2"] {
            XCTAssertFalse(CoreDictation.dictates(id))
            XCTAssertTrue(Dictation.makeModelEngine(id: id, directory: folder, runner: runner,
                                                    core: CoreDictationEngineTests.Core().dictation) is MLXDictationEngine)
        }
    }

    /// La bascule ouverte (pour l'essai seulement) : l'aiguilleur fait
    /// dicter le vrai `CoreDictationEngine`, par une session simulée ; le
    /// coureur Swift a d'abord rendu son modèle et sa part « dictee » du
    /// budget ; le texte définitif arrive.
    func testTheSwitchSendsParakeetThroughTheCore() async throws {
        CoreDictation.testRoute = ["parakeet_tdt_v3"]
        let core = CoreDictationEngineTests.Core(), audio = CoreDictationEngineTests.Audio()
        let budget = MLXBudget(apply: { _, _ in })
        let runner = SpeechModelRunner(load: { _ in
            budget.set("dictee", .init(memory: 3 << 30, cache: 256 << 20))
            return Echo()
        }, gate: MLXGate(), clearCache: { budget.remove("dictee") })
        try await runner.prepare(directory: URL(fileURLWithPath: "/m/whisper_tiny_4bit"))
        XCTAssertEqual(budget.total.memory, 3 << 30, "Whisper Swift chargé")
        let microphone = DictationMicrophone(permission: {}, open: { _ in audio })
        let router = RoutingDictationEngine(
            system: RoutingDictationEngineTests.Fake(), chosen: { "parakeet_tdt_v3" },
            directory: { [folder] _ in folder }, runner: runner, core: core.dictation,
            make: { id, folder in
                Dictation.makeModelEngine(id: id, directory: folder, runner: runner,
                                          core: core.dictation, microphone: microphone)
            })
        let events = CoreDictationEngineTests.Events()
        let start = Task { try await router.start(locale: Locale(identifier: "fr-FR")) { events.receive($0) } }
        _ = await eventuallyTrue { !core.sessions.isEmpty }
        let session = core.sessions[0]
        XCTAssertEqual(budget.total, .init(memory: 0, cache: 0), "le Swift a rendu sa part avant le cœur")
        XCTAssertNil(runner.loadedDirectory)
        session.say(.ready)
        try await start.value
        XCTAssertNil(router.fallbackNotice)
        audio.say([0.5])
        session.ending = [.ended(text: "bonjour")]
        await router.finish()
        XCTAssertEqual(events.seen, [.listening, .text("bonjour", isFinal: true)])
        XCTAssertEqual(session.pushed, [0.5])
        XCTAssertEqual(core.released, 0, "le cœur garde son modèle pour la dictée suivante")
    }

    /// Un modèle Swift va dicter : le cœur rend le sien (et sa part du
    /// budget de la copie Rust) ; `releaseModel` libère les deux côtés.
    func testASwiftModelReleasesTheCoreModelAndReleaseModelFreesBoth() async throws {
        CoreDictation.testRoute = ["parakeet_tdt_v3"]
        let core = CoreDictationEngineTests.Core()
        let budget = MLXBudget(apply: { _, _ in })
        let runner = SpeechModelRunner(load: { _ in
            budget.set("dictee", .init(memory: 3 << 30, cache: 256 << 20))
            return Echo()
        }, gate: MLXGate(), clearCache: { budget.remove("dictee") })
        let whisper = URL(fileURLWithPath: "/m/whisper_tiny_4bit")
        XCTAssertTrue(Dictation.makeModelEngine(id: "whisper_tiny_4bit", directory: whisper, runner: runner,
                                                core: core.dictation) is MLXDictationEngine)
        let released = await eventuallyTrue { core.released == 1 }
        XCTAssertTrue(released, "le cœur rend son modèle avant un modèle Swift")

        try await runner.prepare(directory: whisper)
        let router = RoutingDictationEngine(system: nil, chosen: { "whisper_tiny_4bit" },
                                            directory: { _ in whisper }, runner: runner, core: core.dictation,
                                            make: { _, _ in nil })
        router.releaseModel()
        let both = await eventuallyTrue { core.released == 2 && budget.total.memory == 0 }
        XCTAssertTrue(both, "les deux côtés libérés, aucune part de budget ne reste")
    }
}
