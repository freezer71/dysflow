import Foundation
import KokoroMLX
import os

/// Un modèle de dictée chargé (Parakeet, Canary, Voxtral, Whisper) : de
/// l'audio en 16 kHz mono vers du texte. Appelé sous `MLXGate`, hors du fil
/// principal.
protocol LoadedSpeechModel: AnyObject {
    func transcribe(_ samples: [Float], language: String) throws -> String
}

/// Charge un modèle de dictée, le garde, et le libère après un temps sans
/// usage (comme la voix, `keep_voice_minutes`).
///
/// Le chargement, la transcription et la libération passent tous sous
/// `MLXGate` (un modèle MLX à la fois, voix comprise), sur un fil
/// d'arrière-plan : le fil principal n'attend jamais le GPU. Le modèle et son
/// dossier sont gardés par ce même verrou.
final class SpeechModelRunner: @unchecked Sendable {
    typealias Loader = (URL) throws -> LoadedSpeechModel

    /// Sans usage pendant ce temps, le modèle est libéré (poids et cache) ;
    /// `nil` : jamais de lui-même. Suit `keep_voice_minutes`
    /// (`idleDelay(keepVoiceMinutes:)`), relu à chaque dictée.
    var releaseAfter: Duration? {
        get { idle.withLock { $0.delay } }
        set { idle.withLock { $0.delay = newValue } }
    }

    private let load: Loader
    private let gate: MLXGate
    private let clearCache: () -> Void
    /// Gardés par `gate`.
    private var model: LoadedSpeechModel?
    private var directory: URL?
    /// La libération différée, relancée à chaque usage. Son propre verrou :
    /// la relancer n'attend jamais une transcription en cours.
    private let idle = OSAllocatedUnfairLock(initialState: Idle())
    /// Le temps passé à calculer sous le verrou (`withModel`, donc
    /// `transcribe`), chargements exclus : le débit de l'essai du micro.
    private let compute = OSAllocatedUnfairLock(initialState: Duration.zero)

    private struct Idle {
        var delay: Duration?
        var generation = 0
        var task: Task<Void, Never>?
    }

    init(load: @escaping Loader,
         gate: MLXGate = .shared,
         releaseAfter: Duration? = .seconds(10 * 60),
         clearCache: @escaping () -> Void = KokoroRunner.clearCache) {
        self.load = load
        self.gate = gate
        self.clearCache = clearCache
        idle.withLock { $0.delay = releaseAfter }
    }

    /// Le plus court délai de libération : jamais entre deux relectures
    /// d'une même dictée (1,5 s d'écart au plus, hors lecture).
    static let shortestDelay: Duration = .seconds(5)

    /// Le délai de libération d'après `keep_voice_minutes`, comme la voix :
    /// négatif, jamais de lui-même ; 0, dès la dictée finie (après
    /// `shortestDelay`) ; sinon ce nombre de minutes sans usage.
    static func idleDelay(keepVoiceMinutes minutes: Int64) -> Duration? {
        guard minutes >= 0 else { return nil }
        return max(.seconds(min(minutes, 1_000_000) * 60), shortestDelay)
    }

    deinit {
        idle.withLock { $0.task?.cancel() }
    }

    /// Charge le modèle de `directory`, s'il ne l'est pas déjà. Un autre
    /// dossier remplace l'ancien modèle (libéré avant, cache vidé).
    func prepare(directory: URL) async throws {
        touch()
        defer { touch() }
        try await Task.detached(priority: .userInitiated) { [self] in
            try gate.run {
                if model != nil, self.directory == directory { return }
                if model != nil { unloadLocked() }
                model = try load(directory)
                self.directory = directory
            }
        }.value
    }

    /// `samples` : 16 kHz mono. Échoue si aucun modèle n'est chargé
    /// (`prepare` d'abord, ou libéré entre-temps).
    func transcribe(_ samples: [Float], language: String) async throws -> String {
        try await withModel { try $0.transcribe(samples, language: language) }
    }

    /// `body` sur le modèle chargé, sous `gate`, hors du fil principal : pour
    /// ce que `transcribe` ne dit pas (un modèle qui transcrit au fil de
    /// l'eau). Échoue si aucun modèle n'est chargé ; repousse la libération
    /// comme `transcribe`. `body` ne garde pas le modèle au-delà de l'appel.
    ///
    /// Annuler la tâche appelante n'interrompt pas un travail déjà parti
    /// (`Task.detached`) : il va au bout, et le verrou avec.
    func withModel<T: Sendable>(_ body: @escaping @Sendable (LoadedSpeechModel) throws -> T) async throws -> T {
        touch()
        defer { touch() }
        return try await Task.detached(priority: .userInitiated) { [self] in
            try gate.run {
                guard let model else { throw DictationError.failed("aucun modèle de dictée chargé") }
                let begin = ContinuousClock.now
                defer { compute.withLock { $0 += ContinuousClock.now - begin } }
                return try body(model)
            }
        }.value
    }

    /// Le temps de calcul cumulé depuis la création du coureur ; l'essai du
    /// micro en fait la différence avant et après.
    var computeTime: Duration { compute.withLock { $0 } }

    /// Le dossier du modèle chargé, `nil` s'il n'y en a pas. Attend une
    /// transcription en cours (sous `gate`) : pas sur le fil principal.
    var loadedDirectory: URL? {
        gate.run { model == nil ? nil : directory }
    }

    /// Libère le modèle et vide le cache de MLX ; attend une transcription
    /// en cours.
    func release() {
        idle.withLock {
            $0.generation &+= 1
            $0.task?.cancel()
            $0.task = nil
        }
        gate.run { unloadLocked() }
    }

    /// Libère le modèle s'il vient de `directory` (la fin d'un essai du
    /// micro) ; un autre, chargé entre-temps par une dictée, reste.
    func release(ifLoaded directory: URL) {
        gate.run {
            guard model != nil, self.directory == directory else { return }
            unloadLocked()
        }
    }

    /// Sous `gate`.
    private func unloadLocked() {
        guard model != nil else { return }
        model = nil
        directory = nil
        clearCache()
    }

    /// Repousse la libération : `releaseAfter` à partir de maintenant (rien
    /// si `nil`). Une
    /// libération déjà réveillée mais dépassée (un usage est venu entre-temps)
    /// ne fait rien.
    private func touch() {
        idle.withLock { state in
            state.generation &+= 1
            state.task?.cancel()
            state.task = nil
            guard let delay = state.delay else { return }
            let generation = state.generation
            state.task = Task.detached(priority: .utility) { [weak self] in
                try? await Task.sleep(for: delay)
                guard !Task.isCancelled else { return }
                self?.releaseIfIdle(generation)
            }
        }
    }

    private func releaseIfIdle(_ generation: Int) {
        gate.run {
            guard idle.withLock({ $0.generation == generation }) else { return }
            unloadLocked()
        }
    }
}
