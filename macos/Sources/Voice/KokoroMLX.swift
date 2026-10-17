import Foundation
import KokoroMLX
import os

/// Un passage avant de Kokoro : `KokoroRunner` (MLX) dans l'app, un faux
/// dans les tests. `shouldStop` est consulté aux points d'arrêt du modèle
/// (après les durées, puis dans le décodeur) ; s'il dit vrai, le passage
/// s'arrête et rend `nil`. `nil` : aucun point d'arrêt (préparation).
protocol KokoroForward: AnyObject {
    func run(ids: [Int32], style: [Float],
             shouldStop: (() -> Bool)?) -> (samples: [Float], durations: [Int32])?
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
/// jamais le fil principal. Ce verrou est `MLXGate.shared`, celui de tout
/// MLX dans l'app (la dictée aussi). C'est un `os_unfair_lock`, qui transmet
/// sa priorité : le lecteur (`userInitiated`) qui attend derrière la
/// préparation (`utility`) la fait monter. Le modèle est partagé
/// (préparation, lecteur) : il n'est libéré, poids et cache, qu'au dernier
/// `release()`.
///
/// Abandon : le pool dit, par `SynthesisCancel`, qu'une synthèse n'a plus
/// d'usage (curseur parti, génération changée). Elle est consultée avant le
/// passage (sous le verrou, après l'attente), après les durées, puis dans le
/// décodeur (après `encode`, chaque bloc de `decode`, chaque étage du
/// générateur) ; abandonnée, la synthèse rend `SillonError.Cancelled`, jamais
/// `Failed`. Un passage abandonné en route compte dans `calls` et
/// `forwardSeconds` (il a eu lieu, pour le temps réellement passé) ; un
/// abandon avant le passage n'y compte pas.
final class KokoroMLXCompute: KokoroCompute, @unchecked Sendable {
    typealias Loader = (URL) throws -> KokoroForward

    private let loadForward: Loader
    private let clearCache: () -> Void
    /// Chargement, passages avant et libération : un à la fois, et jamais
    /// pendant un autre calcul MLX de l'app.
    private let gate: MLXGate
    /// Les plafonds de MLX : Kokoro y inscrit son besoin au chargement et le
    /// retire à la libération.
    private let budget: MLXBudget
    /// Gardés par `gate`.
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
    private var forwardNanos: UInt64 = 0

    init(loadForward: @escaping Loader = KokoroMLXCompute.loadRunner,
         clearCache: @escaping () -> Void = KokoroRunner.clearCache,
         gate: MLXGate = .shared,
         budget: MLXBudget = .shared) {
        self.loadForward = loadForward
        self.clearCache = clearCache
        self.gate = gate
        self.budget = budget
    }

    /// Le besoin de Kokoro, mesuré au spike : 1 Go, cache réduit à 64 Mo.
    /// C'est son besoin dans `MLXBudget` (clé `"kokoro"`) : les plafonds de
    /// MLX, globaux au processus, valent la somme des modèles chargés.
    static let memoryLimit = 1 << 30
    static let cacheLimit = 64 << 20

    /// `KokoroRunner.load` pose lui-même les plafonds : il reçoit la somme
    /// des besoins inscrits (dont celui de Kokoro, inscrit juste avant), pour
    /// ne pas étouffer un autre modèle chargé.
    static func loadRunner(_ directory: URL) throws -> KokoroForward {
        let t = MLXBudget.shared.total
        return try KokoroRunner.load(directory: directory,
                                     memoryLimit: max(t.memory, memoryLimit),
                                     cacheLimit: max(t.cache, cacheLimit))
    }

    /// Synthèses faites depuis le lancement (la mesure vérifie qu'il n'y en
    /// a aucune en pause).
    var calls: Int { counter.withLockUnchecked { synthesized } }

    /// Temps cumulé passé dans les passages avant (secondes) : la mesure de
    /// charge en tire la durée moyenne d'une synthèse.
    var forwardSeconds: Double { counter.withLockUnchecked { Double(forwardNanos) / 1e9 } }

    /// Chaque `load` est une part. Un autre dossier remplace le modèle (le
    /// moteur rend sa part sur l'ancien avant, voir `SharedCompute`) ; les
    /// autres détenteurs, la préparation, se servent alors du nouveau.
    func load(modelDir: String) throws {
        try gate.run {
            if forward == nil || directory != modelDir {
                if forward != nil {
                    forward = nil
                    directory = nil
                    clearCache()
                }
                budget.set("kokoro", .init(memory: Self.memoryLimit, cache: Self.cacheLimit))
                do {
                    forward = try loadForward(URL(fileURLWithPath: modelDir))
                    directory = modelDir
                } catch {
                    budget.remove("kokoro")
                    throw SillonError.Failed(message: "Calcul Metal indisponible : \(error.localizedDescription)")
                }
            }
            users += 1
        }
    }

    /// La phrase de préparation, en phonèmes : courte mais réaliste (36
    /// symboles, espaces, une virgule), pour que les noyaux préparés soient
    /// ceux d'une lecture. Lettres ASCII seules (a → 43 … z → 68, sans g,
    /// absent du vocabulaire de Kokoro), espace 16, virgule 3, point 4.
    static let warmUpPhonemes = "la vwa se prepar, avek yn fraz kurt."

    static func warmUpIds() -> [Int64] {
        let ids = warmUpPhonemes.unicodeScalars.map { c -> Int64 in
            switch c {
            case " ": return 16
            case ",": return 3
            case ".": return 4
            default: return 43 + Int64(c.value - UnicodeScalar("a").value)
            }
        }
        return [0] + ids + [0]
    }

    /// Une courte synthèse réaliste, style nul, jamais abandonnée et sans
    /// point d'arrêt. La première d'un processus prépare les noyaux Metal
    /// (≈ 9 s après une installation, encore > 0,3 s aux lancements
    /// suivants) ; si un passage complet a déjà eu lieu dans ce processus
    /// (la lecture est partie avant), elle ne fait rien et réussit.
    func warmUp() throws {
        try gate.run {
            guard forward != nil else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
            if kernelsCompiled { return }
            _ = try runLocked(ids: Self.warmUpIds(), style: [Float](repeating: 0, count: 256), isCancelled: nil)
        }
    }

    /// Swift ne garde jamais `cancel` au-delà de l'appel.
    func synthesize(ids: [Int64], style: [Float], cancel: SynthesisCancel) throws -> KokoroOutput {
        try synthesize(ids: ids, style: style, isCancelled: { cancel.isCancelled() })
    }

    /// `isCancelled` : le test d'abandon (le pool, ou un essai).
    func synthesize(ids: [Int64], style: [Float], isCancelled: @escaping () -> Bool) throws -> KokoroOutput {
        try gate.run { try runLocked(ids: ids, style: style, isCancelled: isCancelled) }
    }

    /// Sous `gate`. `isCancelled` nil : jamais abandonnée, sans point d'arrêt.
    private func runLocked(ids: [Int64], style: [Float], isCancelled: (() -> Bool)?) throws -> KokoroOutput {
        // L'abandon d'abord : une synthèse inutile pendant une brève
        // libération des poids est un abandon, pas un échec.
        if isCancelled?() == true { throw SillonError.Cancelled }
        guard let forward else { throw SillonError.Failed(message: "modèle Kokoro non chargé") }
        counter.withLockUnchecked { synthesized += 1 }
        let debut = DispatchTime.now().uptimeNanoseconds
        let sortie = forward.run(ids: ids.map { Int32(clamping: $0) }, style: style, shouldStop: isCancelled)
        let fait = DispatchTime.now().uptimeNanoseconds - debut
        counter.withLockUnchecked { forwardNanos += fait }
        // Abandonné en route : les noyaux de la fin ne sont peut-être pas
        // compilés, la préparation garde son travail.
        guard let sortie else { throw SillonError.Cancelled }
        kernelsCompiled = true
        return KokoroOutput(samples: sortie.samples, durations: sortie.durations.map { UInt32(max(0, $0)) })
    }

    func release() {
        gate.run {
            guard users > 0 else { return }
            users -= 1
            guard users == 0 else { return }
            forward = nil
            directory = nil
            clearCache()
            budget.remove("kokoro")
        }
    }
}
