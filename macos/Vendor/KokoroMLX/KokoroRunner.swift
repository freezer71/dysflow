import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon dans ce framework : MLX ne se lie qu'ici, une seule
/// copie dans l'app. Ajouté par Sillon, hors des fichiers repris.
public final class KokoroRunner: @unchecked Sendable {
    private let model: KokoroModel

    /// Charge le modèle sous un plafond mémoire et un cache réduit.
    public static func load(directory: URL, memoryLimit: Int, cacheLimit: Int) throws -> KokoroRunner {
        GPU.set(memoryLimit: memoryLimit)
        GPU.set(cacheLimit: cacheLimit)
        return KokoroRunner(model: try KokoroModel.load(directory: directory))
    }

    private init(model: KokoroModel) {
        self.model = model
    }

    /// Un passage avant à vitesse 1 : l'audio à 24 kHz et une durée par jeton.
    /// `nil` si `shouldStop`, consulté aux points d'arrêt du modèle (durées,
    /// puis dans le décodeur), dit vrai : le décodeur n'est pas allé au
    /// bout. `shouldStop` nil : aucun point d'arrêt (préparation).
    public func run(ids: [Int32], style: [Float],
                    shouldStop: (() -> Bool)?) -> (samples: [Float], durations: [Int32])? {
        let input = MLXArray(ids).reshaped([1, -1])
        let refS = MLXArray(style, [1, style.count])
        guard let sortie = model(inputIds: input, refS: refS, speed: 1, shouldStop: shouldStop) else {
            return nil
        }
        let (audio, durations) = sortie
        eval(audio, durations)
        return (audio.reshaped([-1]).asType(.float32).asArray(Float.self),
                durations.reshaped([-1]).asType(.int32).asArray(Int32.self))
    }

    /// Vide le cache de MLX : la voix part au repos.
    public static func clearCache() {
        GPU.clearCache()
    }
}
