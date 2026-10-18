import Foundation
import KokoroMLX
import os

/// Un modèle de lecture qui prend le texte : `Qwen3TTSRunner` ou
/// `MagpieRunner` (framework MLX) dans l'app, un faux dans les tests.
/// `shouldStop` est consulté aux points d'arrêt du modèle (toutes les
/// ~12 trames Qwen, ~20 trames Magpie, et entre deux morceaux Magpie) ;
/// `nil` : il a dit vrai en route.
protocol TextSpeechModel: AnyObject {
    var sampleRate: Int { get }
    func synthesize(text: String, language: String, voice: String,
                    shouldStop: @escaping () -> Bool) throws -> [Float]?
}

extension Qwen3TTSRunner: TextSpeechModel {}
extension MagpieRunner: TextSpeechModel {}

/// Le calcul des modèles de lecture à texte (Qwen3-TTS, MagpieTTS), appelé
/// par le moteur Rust depuis ses fils (voir `TextSpeechCompute` dans
/// ffi/src/lib.rs).
///
/// Un modèle **par dossier**, compté par ses détenteurs : la lecture et
/// l'aperçu de la bibliothèque peuvent tenir chacun le leur, ou tenir le
/// même dossier à la fois (deux `load` du même dossier : un seul
/// chargement, libéré au dernier `release`). Chargement, synthèse et
/// libération passent sous `MLXGate` (une inférence MLX à la fois dans
/// l'app, avec Kokoro et la dictée). Chaque modèle chargé inscrit son besoin
/// dans `MLXBudget` (clé `lecture:<dossier>`) **avant** de charger, le
/// retire si le chargement échoue et à la dernière libération ; les
/// plafonds de MLX ne sont jamais posés ailleurs.
///
/// Abandon : `SynthesisCancel` est consulté avant le calcul (sous le
/// verrou, après l'attente) puis aux points d'arrêt du modèle ; une
/// synthèse abandonnée rend `SillageError.Cancelled`, jamais `Failed`, même
/// si le modèle a levé une erreur après l'abandon.
final class TextSpeechMLXCompute: TextSpeechCompute, @unchecked Sendable {
    typealias Loader = (URL, String) throws -> TextSpeechModel

    static let qwenRuntime = "qwen3_tts_mlx"
    static let magpieRuntime = "magpie_mlx"
    /// La phrase de préparation : compile `compile(shapeless:)` et les
    /// noyaux Metal.
    static let warmUpText = "Bonjour."

    private struct Entry {
        let model: TextSpeechModel
        var users: Int
        /// Vrai après la première synthèse réussie de ce dossier : les
        /// noyaux sont compilés, une nouvelle préparation n'apprend rien.
        /// Repart à faux avec l'entrée, à la dernière libération.
        var warmed = false
    }

    private let loader: Loader
    private let clearCache: () -> Void
    private let gate: MLXGate
    private let budget: MLXBudget
    /// Gardés par `gate`.
    private var loaded: [String: Entry] = [:]

    init(loader: @escaping Loader = TextSpeechMLXCompute.loadModel,
         clearCache: @escaping () -> Void = KokoroRunner.clearCache,
         gate: MLXGate = .shared, budget: MLXBudget = .shared) {
        self.loader = loader
        self.clearCache = clearCache
        self.gate = gate
        self.budget = budget
    }

    /// Qwen3 : les poids (le codec compris) plus 1 Go de marge (pic mesuré
    /// du 1.7B : 3,5–3,7 Go). Magpie : ses poids 8 bits sont déquantifiés
    /// en FP32 au chargement (×4), plus 512 Mo.
    static func need(runtime: String, weightBytes: Int) -> MLXBudget.Need {
        switch runtime {
        case magpieRuntime: return .init(memory: 4 * weightBytes + (512 << 20), cache: 128 << 20)
        default: return .init(memory: weightBytes + (1 << 30), cache: 256 << 20)
        }
    }

    static func loadModel(_ directory: URL, runtime: String) throws -> TextSpeechModel {
        switch runtime {
        case qwenRuntime: return try Qwen3TTSRunner.load(directory: directory)
        case magpieRuntime: return try MagpieRunner.load(directory: directory)
        default: throw SillageError.Failed(message: "moteur de lecture inconnu : \(runtime)")
        }
    }

    /// Le message d'une erreur, sans l'habit de `SillageError.Failed`.
    static func message(of error: Error) -> String {
        if case let SillageError.Failed(message) = error { return message }
        return error.localizedDescription
    }

    /// Une erreur levée pendant la synthèse, vue par le moteur : abandonnée
    /// (le test le dit, ou le modèle a rendu `Cancelled`), c'est un abandon ;
    /// sinon un échec, avec le message du modèle.
    static func synthesisError(_ error: Error, cancelled: Bool) -> SillageError {
        if cancelled { return .Cancelled }
        if case SillageError.Cancelled = error { return .Cancelled }
        return .Failed(message: message(of: error))
    }

    private func key(_ modelDir: String) -> String { "lecture:" + modelDir }

    func load(modelDir: String, runtime: String) throws {
        try gate.run {
            if loaded[modelDir] != nil {
                loaded[modelDir]!.users += 1
                return
            }
            let url = URL(fileURLWithPath: modelDir)
            // Le besoin d'abord : le chargement se fait déjà sous les
            // plafonds qui le comptent.
            budget.set(key(modelDir), Self.need(runtime: runtime, weightBytes: MLXBudget.weightBytes(url)))
            do {
                loaded[modelDir] = Entry(model: try loader(url, runtime), users: 1)
            } catch {
                budget.remove(key(modelDir))
                clearCache()
                throw SillageError.Failed(message: "Le modèle de lecture ne se charge pas : \(Self.message(of: error))")
            }
        }
    }

    /// Une fois par dossier chargé : un deuxième détenteur du même modèle
    /// (lecture et aperçu) ne relance pas une synthèse complète.
    func warmUp(modelDir: String) throws {
        if gate.run({ loaded[modelDir]?.warmed == true }) { return }
        _ = try synthesize(modelDir: modelDir, text: Self.warmUpText, language: "fr", voice: "",
                           isCancelled: { false })
    }

    /// Swift ne garde jamais `cancel` au-delà de l'appel.
    func synthesize(modelDir: String, text: String, language: String, voice: String,
                    cancel: SynthesisCancel) throws -> SpeechOutput {
        try synthesize(modelDir: modelDir, text: text, language: language, voice: voice,
                       isCancelled: { cancel.isCancelled() })
    }

    /// `isCancelled` : le test d'abandon (le pool, ou la mesure).
    func synthesize(modelDir: String, text: String, language: String, voice: String,
                    isCancelled: @escaping () -> Bool) throws -> SpeechOutput {
        try gate.run {
            // L'abandon d'abord, après l'attente du verrou.
            if isCancelled() { throw SillageError.Cancelled }
            guard let model = loaded[modelDir]?.model else {
                throw SillageError.Failed(message: "modèle de lecture non chargé")
            }
            let samples: [Float]?
            do {
                samples = try model.synthesize(text: text, language: language, voice: voice,
                                               shouldStop: isCancelled)
            } catch {
                throw Self.synthesisError(error, cancelled: isCancelled())
            }
            guard let samples else { throw SillageError.Cancelled }
            loaded[modelDir]?.warmed = true
            return SpeechOutput(samples: samples, sampleRate: UInt32(model.sampleRate))
        }
    }

    func release(modelDir: String) {
        gate.run {
            guard let users = loaded[modelDir]?.users else { return }
            guard users <= 1 else {
                loaded[modelDir]!.users = users - 1
                return
            }
            // Aucune référence au modèle ne reste ici : ses tableaux sont
            // rendus au cache de MLX avant qu'on le vide.
            loaded[modelDir] = nil
            clearCache()
            budget.remove(key(modelDir))
        }
    }
}
