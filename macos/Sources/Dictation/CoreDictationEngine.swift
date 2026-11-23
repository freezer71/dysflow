import Foundation

/// Ce que le cœur Rust fournit à la dictée de l'app (`ffi/src/dictation.rs`),
/// injectable pour les tests. Tous ces appels peuvent attendre (une lecture
/// en cours, la libération d'un modèle) : jamais sur le fil principal, jamais
/// en tenant `MLXGate` (le cœur prend lui-même le verrou du GPU).
struct CoreDictation: Sendable {
    /// Une session : rend la main tout de suite, le modèle se charge ensuite.
    var start: @Sendable (_ id: String, _ language: String, _ listener: DictationListener) throws -> DictationSessionProtocol
    /// Libère le modèle du cœur et sa part du budget de MLX (`"dictee"`).
    var release: @Sendable () -> Void
    /// Libère le modèle du cœur s'il vient de ce paquet.
    var releaseIfLoaded: @Sendable (_ id: String) -> Void
    var setKeepVoiceMinutes: @Sendable (Int64) -> Void
    /// Le temps de lecture cumulé du modèle du cœur, en secondes.
    var computeSeconds: @Sendable () -> Double
    /// Une lecture d'un bloc ; des échantillons vides ne font que charger.
    var readOnce: @Sendable (_ id: String, _ samples: [Float], _ language: String) throws -> String

    static let live = CoreDictation(
        start: { try DictationSession.start(id: $0, language: $1, listener: $2) },
        release: { coreDictationRelease() },
        releaseIfLoaded: { coreDictationReleaseIfLoaded(id: $0) },
        setKeepVoiceMinutes: { coreDictationSetKeepVoiceMinutes(minutes: $0) },
        computeSeconds: { coreDictationComputeSeconds() },
        readOnce: { try transcribeOnce(id: $0, samples: $1, language: $2) })

    /// LA bascule : les modèles que le cœur Rust dicte, au lieu du Swift.
    /// Flip after Task 14 P5 passes, see the plan
    /// (`docs/superpowers/plans/2026-11-22-moteurs-modeles.md`, tâches 14 et
    /// 15) : `["parakeet_tdt_v3", "parakeet_tdt_v2"]`. L'app doit alors être
    /// construite avec la feature `mlx` du moteur.
    /// An id here must be accepted by `stt::recognizer_for` (la seule liste
    /// des moteurs du cœur) : `coreDictationSupports(id)`, que
    /// `testEveryPortedModelIsOneTheCoreCanDictate` vérifie.
    static let ported: Set<String> = []

    /// Pour les tests seulement : remplace `ported`.
    @MainActor static var testRoute: Set<String>?

    /// Le cœur dicte ce modèle (sinon, son moteur Swift).
    @MainActor static func dictates(_ id: String) -> Bool {
        (testRoute ?? ported).contains(id)
    }
}

/// L'audio d'une dictée, en 16 kHz mono : le micro (`MicrophoneCapture`), ou
/// une source simulée dans les tests. `drain` se lit de n'importe quel fil.
protocol DictationAudioSource: AnyObject, Sendable {
    func stop()
    /// Ce qui est arrivé depuis le dernier appel.
    func drain() -> [Float]
}

extension MicrophoneCapture: DictationAudioSource {}

/// Le micro d'une dictée : l'autorisation, puis l'ouverture (le spectre part
/// du fil audio). Injectable : les tests passent une source simulée, jamais
/// le vrai micro.
struct DictationMicrophone: Sendable {
    var permission: @Sendable () async throws -> Void
    var open: @MainActor @Sendable (_ spectrum: @escaping @Sendable ([[Float]]) -> Void) throws -> DictationAudioSource

    static let live = DictationMicrophone(
        permission: { try await MicrophoneCapture.requirePermission() },
        open: { spectrum in
            let capture = MicrophoneCapture(spectrum: spectrum)
            try capture.start()
            return capture
        })
}

/// La dictée par le cœur Rust (`DictationSession`) : le cœur découpe aux
/// silences, relit l'énoncé en cours et garde le modèle. Ici, le micro et
/// la traduction des nouvelles du cœur en `DictationEvent`.
///
/// Comme `MLXDictationEngine` : le micro s'ouvre et `.listening` part tout de
/// suite, le spectre suit pendant le chargement, l'audio attend dans le cœur
/// (rien n'est perdu), et `start` rend la main une fois le modèle prêt — ou
/// lève l'erreur du chargement, et l'aiguilleur se replie sur la dictée de
/// macOS.
///
/// Les fils :
/// - le spectre vient du fil audio et passe au fil principal ;
/// - l'audio part au cœur (`push`, qui n'attend jamais) depuis une tâche
///   hors du fil principal, toutes les `feedTick` ;
/// - les nouvelles arrivent sur un fil du cœur (`DictationListener`), sont
///   mises en file (`AsyncStream`), et lues sur le fil principal, dans
///   l'ordre ;
/// - `finish` attend la dernière lecture du cœur hors du fil principal.
///
/// Correspondance : `Partial` → `.text(committed + " " + pending, isFinal:
/// false)` ; `Committed.delay` → `.latency` ; `Memory` → `.memory` ; `Ended` →
/// `.text(text, isFinal: true)` ; `Loading` et `Loaded` → `.loading` et
/// `.loaded` ; un `Failed` avant `Ready` : `start` lève l'erreur (le repli),
/// après : journalisé (un énoncé perdu, la dictée continue).
@MainActor
final class CoreDictationEngine: DictationEngine {
    /// L'audio part au cœur à ce pas.
    static let feedTick: Duration = .milliseconds(40)

    private let id: String
    private let core: CoreDictation
    private let microphone: DictationMicrophone
    /// Avant la session, hors du fil principal, micro ouvert : le coureur
    /// Swift libère son modèle (et sa part du budget).
    private let before: @Sendable () -> Void
    private var run: Run?
    /// Change à chaque arrêt : un démarrage dépassé ne touche plus à rien.
    private var generation = 0

    init(id: String, core: CoreDictation = .live, before: @escaping @Sendable () -> Void = {},
         microphone: DictationMicrophone = .live) {
        self.id = id
        self.core = core
        self.before = before
        self.microphone = microphone
    }

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        await cancel()
        let mine = generation
        try await microphone.permission()
        guard generation == mine else { throw CancellationError() }
        let audio = try microphone.open { columns in
            Task { @MainActor in events(.spectrum(columns)) }
        }
        events(.listening)

        let relay = Relay()
        let core = core, id = id, before = before, language = MLXDictationEngine.languageCode(locale)
        let opening = Task.detached(priority: .userInitiated) { () throws -> DictationSessionProtocol in
            before()
            return try core.start(id, language, relay)
        }
        let run = Run(events: events, audio: audio, relay: relay, opening: opening)
        self.run = run
        run.consumer = consume(run)

        let session: DictationSessionProtocol
        do {
            session = try await opening.value
        } catch {
            if self.run === run { await cancel() }
            throw DictationError.failed(TextSpeechMLXCompute.message(of: error))
        }
        // Annulée : `cancel` arrête la session. Finie : `finish` fait le reste.
        guard !run.cancelled else { return }
        if self.run === run { run.feed = Self.feed(audio: audio, session: session) }
        do {
            try await run.waitUntilReady()
        } catch {
            if self.run === run { await cancel() }
            throw error
        }
    }

    // L'état est vidé avant chaque attente : une dictée suivante, lancée
    // pendant cette attente, garde le sien.
    func finish() async {
        generation &+= 1
        guard let run else {
            DictationLog.log.error("fin : micro pas encore ouvert, rien à lire")
            return
        }
        self.run = nil
        run.audio.stop()
        run.feed?.cancel()
        await run.feed?.value
        guard let session = try? await run.opening.value else {
            run.relay.end()
            DictationLog.log.error("fin : la session du cœur n'a pas démarré")
            return
        }
        let rest = run.audio.drain()
        let ended = await Task.detached(priority: .userInitiated) { () -> Result<String, Error> in
            if !rest.isEmpty { session.push(samples: rest) }
            return Result { try session.finish() }
        }.value
        // Au retour de `finish`, le cœur a tout remis : la file se ferme
        // derrière `Ended`, qui part vers la fenêtre avant ce retour-ci.
        run.relay.end()
        await run.consumer?.value
        switch ended {
        case .success(let text):
            DictationLog.log.info("fin : \(text.count, privacy: .public) caractères (cœur)")
        case .failure(let error):
            DictationLog.log.error("fin : \(TextSpeechMLXCompute.message(of: error), privacy: .public)")
        }
    }

    /// Rien n'est émis. Une lecture déjà partie va au bout, sans effet.
    func cancel() async {
        generation &+= 1
        guard let run else { return }
        self.run = nil
        run.cancelled = true
        run.audio.stop()
        run.feed?.cancel()
        run.relay.end()
        run.resumeReady(nil)
        let opening = run.opening
        Task.detached(priority: .utility) { (try? await opening.value)?.cancel() }
    }

    /// L'audio du micro vers le cœur, toutes les `feedTick`, hors du fil
    /// principal ; `push` n'attend jamais (ni le modèle, ni une lecture).
    private nonisolated static func feed(audio: DictationAudioSource,
                                         session: DictationSessionProtocol) -> Task<Void, Never> {
        Task.detached(priority: .userInitiated) {
            while !Task.isCancelled {
                let samples = audio.drain()
                if !samples.isEmpty { session.push(samples: samples) }
                try? await Task.sleep(for: feedTick)
            }
        }
    }

    /// Les nouvelles du cœur, dans l'ordre, sur le fil principal.
    private func consume(_ run: Run) -> Task<Void, Never> {
        Task { @MainActor in
            for await update in run.relay.updates {
                guard !run.cancelled else { continue }
                run.receive(update)
            }
            run.resumeReady(DictationError.failed(String(localized: "the model won’t load: no answer")))
        }
    }

    /// Le texte montré : le définitif, puis la relecture en cours, sans
    /// espace de trop quand l'un des deux est vide.
    nonisolated static func shown(committed: String, pending: String) -> String {
        [committed, pending].filter { !$0.isEmpty }.joined(separator: " ")
    }
}

/// Une dictée en cours : son micro, sa session, ses nouvelles.
@MainActor
private final class Run {
    let events: @MainActor @Sendable (DictationEvent) -> Void
    let audio: DictationAudioSource
    let relay: Relay
    let opening: Task<DictationSessionProtocol, Error>
    var feed: Task<Void, Never>?
    var consumer: Task<Void, Never>?
    var cancelled = false
    /// `Ready` est arrivé : le modèle est là.
    private var ready = false
    /// Le chargement a échoué (un `Failed` avant `Ready`).
    private var failure: Error?
    private var waiting: CheckedContinuation<Void, Error>?

    init(events: @escaping @MainActor @Sendable (DictationEvent) -> Void, audio: DictationAudioSource,
         relay: Relay, opening: Task<DictationSessionProtocol, Error>) {
        self.events = events
        self.audio = audio
        self.relay = relay
        self.opening = opening
    }

    func receive(_ update: DictationUpdate) {
        switch update {
        case .loading:
            events(.loading)
        case .loaded(let seconds):
            events(.loaded(.seconds(seconds)))
        case .ready:
            ready = true
            resumeReady(nil)
        case .partial(let committed, let pending):
            events(.text(CoreDictationEngine.shown(committed: committed, pending: pending), isFinal: false))
        case .committed(_, let delaySeconds):
            events(.latency(.seconds(delaySeconds)))
        case .memory(let bytes):
            events(.memory(Int(clamping: bytes)))
        case .ended(let text):
            events(.text(text, isFinal: true))
        case .failed(let message):
            if ready {
                DictationLog.log.error("cœur : \(message, privacy: .public)")
            } else {
                resumeReady(DictationError.failed(message))
            }
        }
    }

    /// Le modèle prêt, l'échec du chargement, ou l'annulation (sans erreur).
    func waitUntilReady() async throws {
        if ready || cancelled { return }
        if let failure { throw failure }
        try await withCheckedThrowingContinuation { waiting = $0 }
    }

    /// `nil` : prêt (ou annulé) ; sinon l'échec du chargement.
    func resumeReady(_ error: Error?) {
        if let error, !ready, failure == nil { failure = error }
        guard let waiting else { return }
        self.waiting = nil
        if let error, !cancelled { waiting.resume(throwing: error) } else { waiting.resume() }
    }
}

/// Le `DictationListener` du cœur : chaque nouvelle part dans une file, sans
/// attendre (`on_update` est appelé sur un fil du cœur).
private final class Relay: DictationListener, @unchecked Sendable {
    let updates: AsyncStream<DictationUpdate>
    private let continuation: AsyncStream<DictationUpdate>.Continuation

    init() {
        (updates, continuation) = AsyncStream.makeStream(bufferingPolicy: .unbounded)
    }

    func onUpdate(update: DictationUpdate) {
        continuation.yield(update)
    }

    func end() {
        continuation.finish()
    }
}
