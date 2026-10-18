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
/// `MLXGate` (un modèle MLX à la fois, voix comprise), sur une file à part
/// (`queue`) : le fil principal n'attend jamais le GPU, et l'attente du
/// verrou, qui peut durer des secondes, n'occupe pas un fil du pool
/// coopératif de Swift. Le modèle et son dossier sont gardés par ce même
/// verrou.
final class SpeechModelRunner: @unchecked Sendable {
    typealias Loader = (URL) throws -> LoadedSpeechModel

    /// Sans usage pendant ce temps, le modèle est libéré (poids et cache) ;
    /// `nil` : jamais de lui-même. Suit `keep_voice_minutes`
    /// (`idleDelay(keepVoiceMinutes:)`), relu à chaque dictée.
    var releaseAfter: Duration? {
        get { idle.withLock { $0.delay } }
        set {
            idle.withLock { state in
                guard state.delay != newValue else { return }
                state.delay = newValue
                // Une tâche qui dort a pris l'ancien délai : elle est
                // remplacée, pour qu'un délai raccourci libère à temps.
                guard let task = state.task else { return }
                task.cancel()
                state.task = nil
                state.armed &+= 1
                armLocked(&state)
            }
        }
    }

    private let load: Loader
    private let gate: MLXGate
    private let clearCache: () -> Void
    /// Le travail sous `gate` attend ici, pas sur un fil du pool coopératif.
    private let queue = DispatchQueue(label: "fr.sillage.dictee.mlx", qos: .userInitiated)
    /// Gardés par `gate`.
    private var model: LoadedSpeechModel?
    private var directory: URL?
    /// La libération différée : l'heure du dernier usage, et une seule tâche
    /// qui dort jusqu'à l'échéance et se réarme si un usage est venu
    /// entre-temps. Son propre verrou : noter un usage n'attend jamais une
    /// transcription en cours.
    private let idle = OSAllocatedUnfairLock(initialState: Idle())
    /// Le temps passé à calculer sous le verrou (`withModel`, donc
    /// `transcribe`), chargements exclus : le débit de l'essai du micro.
    private let compute = OSAllocatedUnfairLock(initialState: Duration.zero)

    private struct Idle {
        var delay: Duration?
        var lastUse = ContinuousClock.now
        var task: Task<Void, Never>?
        /// Le numéro de la tâche en cours : une tâche remplacée ne fait plus
        /// rien.
        var armed = 0
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

    /// Le plus court délai de libération. Une dictée en cours ne compte pas
    /// sur lui : sa boucle appelle `keepAlive` entre deux relectures, si
    /// espacées soient-elles.
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

    /// `body` sous `gate`, sur `queue` ; l'appelant attend sans tenir de fil.
    private func onQueue<T: Sendable>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<T, Error>) in
            queue.async { [gate] in
                continuation.resume(with: Result { try gate.run(body) })
            }
        }
    }

    /// Charge le modèle de `directory`, s'il ne l'est pas déjà. Un autre
    /// dossier remplace l'ancien modèle (libéré avant, cache vidé).
    func prepare(directory: URL) async throws {
        touch()
        try await onQueue { [self] in
            // Sous le verrou : une libération qui l'attendait voit cet usage.
            defer { touch() }
            if model != nil, self.directory == directory { return }
            if model != nil { unloadLocked() }
            model = try load(directory)
            self.directory = directory
        }
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
    /// Annuler la tâche appelante n'interrompt pas un travail déjà parti : il
    /// va au bout, et le verrou avec.
    func withModel<T: Sendable>(_ body: @escaping @Sendable (LoadedSpeechModel) throws -> T) async throws -> T {
        touch()
        return try await onQueue { [self] in
            defer { touch() }
            guard let model else { throw DictationError.failed("aucun modèle de dictée chargé") }
            let begin = ContinuousClock.now
            defer { compute.withLock { $0 += ContinuousClock.now - begin } }
            return try body(model)
        }
    }

    /// `body` sous `gate`, sur la file du coureur, sans modèle exigé ni
    /// usage noté : les compteurs de MLX, le vidage de son cache.
    func underGate<T: Sendable>(_ body: @escaping @Sendable () -> T) async -> T {
        // `body` ne lève pas : le `try` ne peut pas échouer.
        try! await onQueue(body)
    }

    /// Note un usage sans rien calculer : une dictée en cours le fait entre
    /// deux relectures, pour que le modèle ne parte jamais au milieu.
    func keepAlive() {
        touch()
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

    /// Note l'usage : la libération attend `releaseAfter` à partir de
    /// maintenant (rien si `nil`). Une seule tâche de libération : lancée si
    /// aucune ne dort, sinon elle verra cette heure à son réveil.
    private func touch() {
        idle.withLock { state in
            state.lastUse = .now
            armLocked(&state)
        }
    }

    /// Sous `idle` : lance la tâche de libération s'il n'y en a pas.
    private func armLocked(_ state: inout Idle) {
        guard state.delay != nil, state.task == nil else { return }
        state.armed &+= 1
        let id = state.armed
        let weak = WeakRunner(runner: self)
        state.task = Task.detached(priority: .utility) {
            await Self.idleLoop(weak, id: id)
        }
    }

    private struct WeakRunner: Sendable {
        weak var runner: SpeechModelRunner?
    }

    /// Dort jusqu'à l'échéance (dernier usage + délai), se réarme si un usage
    /// l'a repoussée, puis libère. Le délai passé à `nil` l'arrête. Ne garde
    /// pas le coureur pendant qu'elle dort.
    private static func idleLoop(_ weak: WeakRunner, id: Int) async {
        while !Task.isCancelled {
            guard let wait = weak.runner?.remaining(id: id) else { return }
            if wait > .zero {
                try? await Task.sleep(for: wait)
                continue
            }
            guard let runner = weak.runner else { return }
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                runner.queue.async(qos: .utility) {
                    runner.releaseIfIdle(id: id)
                    continuation.resume()
                }
            }
            return
        }
    }

    /// Le temps jusqu'à l'échéance ; `nil` si la tâche `id` doit s'arrêter
    /// (remplacée, ou plus de délai).
    private func remaining(id: Int) -> Duration? {
        idle.withLock { state in
            guard state.armed == id else { return nil }
            guard let delay = state.delay else {
                state.task = nil
                return nil
            }
            return state.lastUse + delay - .now
        }
    }

    /// Sur `queue`. Sous `gate` : un usage venu pendant l'attente du verrou
    /// (noté sous ce même verrou, en fin de calcul) repousse la libération ;
    /// une tâche neuve dort alors jusqu'à la nouvelle échéance.
    private func releaseIfIdle(id: Int) {
        gate.run {
            let due = idle.withLock { state -> Bool in
                guard state.armed == id else { return false }
                state.task = nil
                guard let delay = state.delay, ContinuousClock.now - state.lastUse >= delay else {
                    armLocked(&state)
                    return false
                }
                return true
            }
            if due { unloadLocked() }
        }
    }
}
