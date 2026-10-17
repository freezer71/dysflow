import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon pour les modèles de dictée, comme `KokoroRunner` pour
/// la voix : l'app ne lie pas MLX, elle passe par ici pour l'audio en
/// tableau, les plafonds et le cache. Ajouté par Sillon, hors des fichiers
/// repris. Tout s'appelle sous `MLXGate` (dans l'app).
public enum DictationMLX {
    /// Les plafonds de MLX : globaux au processus. Posés par `MLXBudget`
    /// (l'app) : la somme des besoins des modèles chargés.
    public static func setLimits(memory: Int, cache: Int) {
        Memory.memoryLimit = memory
        Memory.cacheLimit = cache
    }

    /// Vide le cache de MLX (après une session Voxtral, par exemple).
    public static func clearCache() {
        Memory.clearCache()
    }

    /// Octets que MLX tient : tableaux vivants (`active`) et tampons gardés
    /// dans son cache (`cache`). Pour les mesures.
    public static func counters() -> (active: Int, cache: Int) {
        (Memory.activeMemory, Memory.cacheMemory)
    }
}

public extension STTGenerationModel {
    /// `samples` : 16 kHz mono, en tableau ; l'app n'a pas à construire de
    /// `MLXArray`.
    func generate(samples: [Float], generationParameters: STTGenerateParameters) -> STTOutput {
        generate(audio: MLXArray(samples), generationParameters: generationParameters)
    }
}
