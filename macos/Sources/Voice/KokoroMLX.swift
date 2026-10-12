import Foundation
import KokoroMLX

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
/// jamais le fil principal. Le modèle est partagé (préparation, lecteur) : il
/// n'est libéré, poids et cache, qu'au dernier `release()`.
final class KokoroMLXCompute: KokoroCompute, @unchecked Sendable {
    typealias Loader = (URL) throws -> KokoroForward

    private let loadForward: Loader
    private let clearCache: () -> Void
    /// Chargement, passages avant et libération : un à la fois.
    private let mlx = NSLock()
    /// Gardés par `mlx`.
    private var forward: KokoroForward?
    private var users = 0
    /// Le compte, sous son propre verrou : le lire n'attend jamais une
    /// synthèse en cours.
    private let counter = NSLock()
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
    var calls: Int { counter.withLock { synthesized } }

    func load(modelDir: String) throws {
        try mlx.withLock {
            if forward == nil {
                do {
                    forward = try loadForward(URL(fileURLWithPath: modelDir))
                } catch {
                    throw SillonError.Failed(message: "Calcul Metal indisponible : \(error.localizedDescription)")
                }
            }
            users += 1
        }
    }

    /// Une courte synthèse : « bala. » (b a l a .), style nul. La première
    /// après une installation compile les noyaux Metal.
    func warmUp() throws {
        _ = try synthesize(ids: [0, 44, 43, 54, 43, 4, 0], style: [Float](repeating: 0, count: 256))
    }

    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        try mlx.withLock {
            guard let forward else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
            counter.withLock { synthesized += 1 }
            let (samples, durations) = forward.run(ids: ids.map { Int32(clamping: $0) }, style: style)
            return KokoroOutput(samples: samples, durations: durations.map { UInt32(max(0, $0)) })
        }
    }

    func release() {
        mlx.withLock {
            guard users > 0 else { return }
            users -= 1
            guard users == 0 else { return }
            forward = nil
            clearCache()
        }
    }
}
