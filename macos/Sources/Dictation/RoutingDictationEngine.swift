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
    /// Le moteur qui a démarré : `finish` et `cancel` vont à lui.
    @ObservationIgnored private var active: DictationEngine?
    /// `finish` ou `cancel` est arrivé pendant le démarrage : plus de repli.
    @ObservationIgnored private var stopped = false

    init(system: DictationEngine?,
         chosen: @escaping () -> String,
         directory: @escaping (String) -> URL?,
         make: @escaping (String, URL) -> DictationEngine?) {
        self.system = system
        self.chosen = chosen
        self.directory = directory
        self.make = make
    }

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        stopped = false
        let id = chosen()
        guard id != DictationModel.system else {
            fallbackNotice = nil
            try await startSystem(locale: locale, events: events, instead: nil)
            return
        }
        guard let folder = directory(id), let engine = make(id, folder) else {
            try await startSystem(locale: locale, events: events, instead: id)
            return
        }
        active = engine
        do {
            try await engine.start(locale: locale, events: events)
            fallbackNotice = nil
        } catch {
            guard !stopped, Self.mayFallBack(after: error) else { throw error }
            await engine.cancel()
            try await startSystem(locale: locale, events: events, instead: id)
        }
    }

    func finish() async {
        stopped = true
        await active?.finish()
    }

    func cancel() async {
        stopped = true
        await active?.cancel()
    }

    /// `instead` : le modèle qui manque (le repli est dit).
    private func startSystem(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void,
                             instead missing: String?) async throws {
        if let missing { fallbackNotice = "\(DictationModel.label(missing)) indisponible : dictée de macOS" }
        guard let system else {
            active = nil
            throw DictationError.failed("\(DictationModel.label(missing ?? "")) indisponible")
        }
        active = system
        try await system.start(locale: locale, events: events)
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
