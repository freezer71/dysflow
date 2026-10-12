import Foundation
import KokoroMLX
import os

/// Un passage avant de Kokoro : `KokoroRunner` (MLX) dans l'app, un faux
/// dans les tests.
protocol KokoroForward: AnyObject {
    func run(ids: [Int32], style: [Float]) -> (samples: [Float], durations: [Int32])
}

extension KokoroRunner: KokoroForward {}

/// Le calcul de Kokoro en MLX, appelé par le moteur Rust depuis ses fils de
/// synthèse.
///
/// Contrat (voir `KokoroCompute` dans ffi/src/lib.rs) : les appels arrivent
/// de fils d'arrière-plan, de façon synchrone, et parfois en même temps (un
/// ancien moteur finit une synthèse pendant qu'un nouveau se charge et se
/// prépare ; la préparation des noyaux au lancement aussi). Tout le travail
/// MLX passe donc sous un seul verrou, un appel à la fois, et rien n'attend
/// jamais le fil principal. Le verrou est un `os_unfair_lock`, qui transmet
/// sa priorité : le lecteur (`userInitiated`) qui attend derrière la
/// préparation (`utility`) la fait monter. Le modèle est partagé
/// (préparation, lecteur) : il n'est libéré, poids et cache, qu'au dernier
/// `release()`.
final class KokoroMLXCompute: KokoroCompute, @unchecked Sendable {
    typealias Loader = (URL) throws -> KokoroForward

    private let loadForward: Loader
    private let clearCache: () -> Void
    /// Chargement, passages avant et libération : un à la fois.
    private let mlx = OSAllocatedUnfairLock()
    /// Gardés par `mlx`.
    private var forward: KokoroForward?
    private var directory: String?
    private var users = 0
    /// Un passage a déjà eu lieu dans ce processus : les noyaux Metal sont
    /// compilés, la préparation n'a plus rien à faire.
    private var kernelsCompiled = false
    /// Le compte, sous son propre verrou : le lire n'attend jamais une
    /// synthèse en cours.
    private let counter = OSAllocatedUnfairLock()
    private var synthesized = 0

    init(loadForward: @escaping Loader = KokoroMLXCompute.loadRunner,
         clearCache: @escaping () -> Void = KokoroRunner.clearCache) {
        self.loadForward = loadForward
        self.clearCache = clearCache
    }

    /// Plafond mesuré au spike : 1 Go, cache réduit à 64 Mo.
    static func loadRunner(_ directory: URL) throws -> KokoroForward {
        try KokoroRunner.load(directory: directory, memoryLimit: 1 << 30, cacheLimit: 64 << 20)
    }

    /// Synthèses faites depuis le lancement (la mesure vérifie qu'il n'y en
    /// a aucune en pause).
    var calls: Int { counter.withLockUnchecked { synthesized } }

    /// Chaque `load` est une part. Un autre dossier remplace le modèle (le
    /// moteur rend sa part sur l'ancien avant, voir `SharedCompute`) ; les
    /// autres détenteurs, la préparation, se servent alors du nouveau.
    func load(modelDir: String) throws {
        try mlx.withLockUnchecked {
            if forward == nil || directory != modelDir {
                if forward != nil {
                    forward = nil
                    directory = nil
                    clearCache()
                }
                do {
                    forward = try loadForward(URL(fileURLWithPath: modelDir))
                    directory = modelDir
                } catch {
                    throw SillonError.Failed(message: "Calcul Metal indisponible : \(error.localizedDescription)")
                }
            }
            users += 1
        }
    }

    /// Une courte synthèse : « bala. » (b a l a .), style nul. La première
    /// après une installation compile les noyaux Metal ; si un passage les a
    /// déjà compilés dans ce processus (la lecture est partie avant), elle ne
    /// fait rien et réussit.
    func warmUp() throws {
        try mlx.withLockUnchecked {
            guard forward != nil else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
            if kernelsCompiled { return }
            _ = try runLocked(ids: [0, 44, 43, 54, 43, 4, 0], style: [Float](repeating: 0, count: 256))
        }
    }

    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        try mlx.withLockUnchecked { try runLocked(ids: ids, style: style) }
    }

    /// Sous `mlx`.
    private func runLocked(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        guard let forward else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
        counter.withLockUnchecked { synthesized += 1 }
        let (samples, durations) = forward.run(ids: ids.map { Int32(clamping: $0) }, style: style)
        kernelsCompiled = true
        return KokoroOutput(samples: samples, durations: durations.map { UInt32(max(0, $0)) })
    }

    func release() {
        mlx.withLockUnchecked {
            guard users > 0 else { return }
            users -= 1
            guard users == 0 else { return }
            forward = nil
            directory = nil
            clearCache()
        }
    }
}
