import Foundation
import Observation

/// L'aiguilleur de la dictée : à chaque dictée, le modèle choisi dans les
/// réglages (`DictationModel`), ou la dictée de macOS.
///
/// Un modèle choisi mais absent (pas installé), ou qui ne se charge pas,
/// cède la place à la dictée de macOS, et `fallbackNotice` le dit (la
/// section Dictée des réglages le montre). Un micro refusé ou absent, ou une
/// dictée annulée, ne cède jamais : la dictée de macOS n'y ferait pas mieux.
@MainActor
@Observable
final class RoutingDictationEngine: DictationEngine {
    /// « Parakeet v3 indisponible : dictée de macOS » ; `nil` quand le modèle
    /// choisi a démarré (ou que c'est la dictée de macOS).
    private(set) var fallbackNotice: String?

    @ObservationIgnored private let system: DictationEngine?
    @ObservationIgnored private let chosen: () -> String
    @ObservationIgnored private let directory: (String) -> URL?
    @ObservationIgnored private let make: (String, URL) -> DictationEngine?
    /// Le coureur partagé des modèles de dictée : `releaseModel` le libère.
    @ObservationIgnored let runner: SpeechModelRunner?
    /// Au début de chaque dictée (relire le délai de libération du modèle).
    @ObservationIgnored var willStart: @MainActor () -> Void = {}
    /// Le moteur qui a démarré : `finish` et `cancel` vont à lui.
    @ObservationIgnored private var active: DictationEngine?
    /// `finish` ou `cancel` est arrivé pendant le démarrage : plus de repli.
    @ObservationIgnored private var stopped = false
    /// Change à chaque `start` : un démarrage dépassé par un autre (annulé,
    /// puis une nouvelle dictée) ne touche plus à rien — ni au moteur actif,
    /// ni au repli — et arrête le moteur qu'il tient.
    @ObservationIgnored private var token = 0

    init(system: DictationEngine?,
         chosen: @escaping () -> String,
         directory: @escaping (String) -> URL?,
         runner: SpeechModelRunner? = nil,
         make: @escaping (String, URL) -> DictationEngine?) {
        self.system = system
        self.chosen = chosen
        self.directory = directory
        self.runner = runner
        self.make = make
    }

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        token &+= 1
        let mine = token
        stopped = false
        willStart()
        let id = chosen()
        guard id != DictationModel.system else {
            fallbackNotice = nil
            try await startSystem(locale: locale, events: events, instead: nil, token: mine)
            return
        }
        guard let folder = directory(id), let engine = make(id, folder) else {
            try await startSystem(locale: locale, events: events, instead: id, token: mine)
            return
        }
        try await retire(keeping: engine, token: mine)
        active = engine
        do {
            try await engine.start(locale: locale, events: events)
        } catch {
            guard token == mine else { throw await stale(engine) }
            guard !stopped, Self.mayFallBack(after: error) else { throw error }
            await engine.cancel()
            guard token == mine else { throw CancellationError() }
            guard !stopped else { throw error }
            try await startSystem(locale: locale, events: events, instead: id, token: mine)
            return
        }
        guard token == mine else { throw await stale(engine) }
        fallbackNotice = nil
    }

    func finish() async {
        stopped = true
        await active?.finish()
    }

    func cancel() async {
        stopped = true
        await active?.cancel()
    }

    /// Le modèle de dictée choisi est supprimé, ou un autre est choisi : le
    /// coureur le libère (hors du fil principal : la libération attend une
    /// lecture en cours), et le repli affiché, périmé, s'efface.
    func releaseModel() {
        fallbackNotice = nil
        guard let runner else { return }
        Task.detached(priority: .utility) { runner.release() }
    }

    /// `instead` : le modèle qui manque (le repli est dit).
    private func startSystem(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void,
                             instead missing: String?, token mine: Int) async throws {
        if let missing { fallbackNotice = "\(DictationModel.label(missing)) indisponible : dictée de macOS" }
        guard let system else {
            active = nil
            throw DictationError.failed("\(DictationModel.label(missing ?? "")) indisponible")
        }
        try await retire(keeping: system, token: mine)
        active = system
        do {
            try await system.start(locale: locale, events: events)
        } catch {
            guard token == mine else { throw await stale(system) }
            throw error
        }
        guard token == mine else { throw await stale(system) }
    }

    /// Arrête le moteur de la dictée d'avant, s'il n'est pas celui-ci.
    private func retire(keeping engine: DictationEngine, token mine: Int) async throws {
        if let previous = active, previous !== engine {
            active = nil
            await previous.cancel()
        }
        guard token == mine, !stopped else { throw CancellationError() }
    }

    /// Un démarrage dépassé : son moteur s'arrête, sauf s'il sert la dictée
    /// en cours (la dictée de macOS est partagée).
    private func stale(_ engine: DictationEngine) async -> CancellationError {
        if engine !== active { await engine.cancel() }
        return CancellationError()
    }

    /// Le micro (refusé, absent) et l'annulation ne se règlent pas en
    /// changeant de moteur.
    static func mayFallBack(after error: Error) -> Bool {
        if error is CancellationError { return false }
        switch error as? DictationError {
        case .microphoneDenied, .noMicrophone: return false
        default: return true
        }
    }
}
