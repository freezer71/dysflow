import Foundation
import KokoroMLX
import os

/// Les plafonds de MLX (`Memory.memoryLimit`, `cacheLimit`) sont globaux au
/// processus. Chaque modèle chargé (Kokoro, la dictée, un modèle de lecture
/// à texte) inscrit ici son besoin ; les plafonds valent **la somme** des
/// besoins inscrits, reposée à chaque changement. Plus de plafonds remis
/// « en dur » : une dictée libérée pendant que Qwen 1.7B lit ne lui impose
/// pas 1 Go. Appelé sous `MLXGate` (les plafonds ne changent pas pendant un
/// calcul).
///
/// Clés : `"kokoro"`, `"dictee"`, `"lecture:<dossier>"`.
final class MLXBudget: @unchecked Sendable {
    struct Need: Equatable {
        var memory: Int
        var cache: Int
    }

    static let shared = MLXBudget()

    private let lock = OSAllocatedUnfairLock()
    /// Gardés par `lock`.
    private var needs: [String: Need] = [:]
    private let apply: (Int, Int) -> Void

    init(apply: @escaping (Int, Int) -> Void = { DictationMLX.setLimits(memory: $0, cache: $1) }) {
        self.apply = apply
    }

    /// La somme des besoins inscrits.
    var total: Need {
        lock.withLockUnchecked {
            needs.values.reduce(Need(memory: 0, cache: 0)) {
                Need(memory: $0.memory + $1.memory, cache: $0.cache + $1.cache)
            }
        }
    }

    /// Inscrit (ou remplace) le besoin de `key`, puis repose les plafonds.
    func set(_ key: String, _ need: Need) {
        lock.withLockUnchecked { needs[key] = need }
        applyNow()
    }

    /// Retire le besoin de `key` (inconnue : sans effet), puis repose les
    /// plafonds.
    func remove(_ key: String) {
        lock.withLockUnchecked { needs[key] = nil }
        applyNow()
    }

    /// Repose la somme ; rien de chargé : rien à poser, les plafonds restent
    /// tels quels (MLX n'a alors rien à plafonner).
    func applyNow() {
        let t = total
        guard t.memory > 0 else { return }
        apply(t.memory, t.cache)
    }

    /// Octets des poids (`*.safetensors`) du dossier et de ses sous-dossiers
    /// (le codec de Qwen3 vit dans `speech_tokenizer/`).
    static func weightBytes(_ directory: URL) -> Int {
        guard let files = FileManager.default.enumerator(at: directory, includingPropertiesForKeys: [.fileSizeKey])
        else { return 0 }
        var total = 0
        for case let url as URL in files where url.pathExtension == "safetensors" {
            total += (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        }
        return total
    }
}
