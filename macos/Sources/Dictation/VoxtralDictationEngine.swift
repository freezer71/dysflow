import Foundation
import KokoroMLX

/// La dictée par Voxtral Realtime, au fil de la parole : toutes les 80 ms
/// (un jeton de Voxtral), ce que le micro a reçu depuis la fois précédente
/// passe à une session de flux, et le texte nouveau s'ajoute au provisoire.
/// Le texte suit la voix d'environ 0,5 s (retard de 480 ms, le réglage du
/// modèle). À la fin, la session se ferme et rend le texte définitif.
///
/// Comme `MLXDictationEngine` : le micro s'ouvre et `.listening` part tout
/// de suite, le modèle se charge pendant qu'on parle, et la première passe
/// reprend tout ce qui a été dit depuis. La session vit avec le moteur ;
/// chaque appel au modèle passe par `SpeechModelRunner.withModel` (sous
/// `MLXGate`, hors du fil principal).
@MainActor
final class VoxtralDictationEngine: DictationEngine {
    /// Un jeton de Voxtral : 1 280 échantillons à 16 kHz.
    static let tick: Duration = .milliseconds(80)
    /// Le retard du texte sur la voix (le défaut du modèle).
    static let delayMs = 480

    private let runner: SpeechModelRunner
    private let directory: URL
    private var capture: MicrophoneCapture?
    private var load: Task<VoxtralSession, Error>?
    private var loop: Task<Void, Never>?
    private var events: (@MainActor @Sendable (DictationEvent) -> Void)?
    /// Le texte provisoire : les morceaux rendus par la session, bout à bout.
    private var partial = ""
    /// Change à chaque arrêt : rien n'est émis pour une dictée finie ou
    /// annulée.
    private var generation = 0

    init(runner: SpeechModelRunner, directory: URL) {
        self.runner = runner
        self.directory = directory
    }

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        await cancel()
        let id = generation
        try await MicrophoneCapture.requirePermission()
        guard generation == id else { throw CancellationError() }
        let capture = try MicrophoneCapture.open(events: events)
        self.capture = capture
        self.events = events
        partial = ""
        events(.listening)

        let runner = runner, directory = directory
        let load = Task {
            try await runner.prepare(directory: directory)
            return try await runner.withModel { model in
                guard let voxtral = model as? VoxtralSpeech else {
                    throw DictationError.failed("ce modèle n'est pas Voxtral")
                }
                return voxtral.makeSession(delayMs: Self.delayMs)
            }
        }
        self.load = load
        let session: VoxtralSession
        do {
            session = try await load.value
        } catch {
            if generation == id { await cancel() }
            throw DictationError.failed("le modèle ne se charge pas : \(error.localizedDescription)")
        }
        // Fini ou annulé pendant le chargement : `finish` fait le reste.
        guard generation == id else { return }
        loop = feedLoop(capture: capture, session: session, events: events, generation: id)
    }

    // L'état est vidé avant chaque attente : une dictée suivante, lancée
    // pendant cette attente, garde le sien.
    func finish() async {
        guard let capture, let events else { return }
        generation &+= 1
        capture.stop()
        let loop = loop, load = load
        clear()
        // Le dernier bloc parti va au bout ; le reste du micro suit ici.
        loop?.cancel()
        await loop?.value
        guard let session = try? await load?.value else { return }
        let rest = capture.drain()
        let text = try? await runner.withModel { _ in
            if !rest.isEmpty { _ = session.step(rest) }
            return session.finish()
        }
        releaseCache()
        guard let text else { return }
        events(.text(text, isFinal: true))
    }

    /// Rien n'est émis. Un bloc déjà parti sur le GPU va au bout, sans effet.
    func cancel() async {
        let hadSession = load != nil
        generation &+= 1
        capture?.stop()
        loop?.cancel()
        clear()
        if hadSession { releaseCache() }
    }

    private func clear() {
        capture = nil
        loop = nil
        load = nil
        events = nil
    }

    /// Après une session, le cache de MLX garde ses tampons : vidé (sous
    /// `MLXGate`, après le dernier bloc).
    private func releaseCache() {
        let runner = runner
        Task { _ = try? await runner.withModel { _ in DictationMLX.clearCache() } }
    }

    /// Toutes les 80 ms, ce qui est arrivé depuis la fois d'avant. Chaque
    /// bloc est attendu : jamais deux à la fois, et dans l'ordre.
    private func feedLoop(capture: MicrophoneCapture, session: VoxtralSession,
                          events: @escaping @MainActor @Sendable (DictationEvent) -> Void,
                          generation id: Int) -> Task<Void, Never> {
        let runner = runner
        return Task { @MainActor [weak self] in
            while !Task.isCancelled, self?.generation == id {
                let samples = capture.drain()
                if !samples.isEmpty,
                   let delta = try? await runner.withModel({ _ in session.step(samples) }) {
                    guard !Task.isCancelled, let self, self.generation == id else { return }
                    if !delta.isEmpty {
                        self.partial += delta
                        events(.text(self.partial, isFinal: false))
                    }
                }
                try? await Task.sleep(for: Self.tick)
            }
        }
    }
}
