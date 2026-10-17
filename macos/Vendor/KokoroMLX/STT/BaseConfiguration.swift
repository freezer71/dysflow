import Foundation
import MLX

/// Ajout de Sillon : sous-ensemble de `MLXLMCommon.BaseConfiguration`
/// (mlx-swift-lm 3.32.3, licence MIT, ml-explore), seul symbole de MLXLMCommon
/// qu'emploient les modèles STT copiés. Seule la quantification est lue ;
/// le nom et les membres sont gardés pour ne toucher aucun site d'appel.
/// Sans `model_type` (configs NeMo de Parakeet), le décodage échoue, comme
/// en amont, et l'appelant se replie sur sa lecture à plat.
/// Public, comme en amont, parce que `CanaryConfig` (public) a des champs de
/// ces types ; seuls les trois types le sont, leurs membres restent internes.
public struct BaseConfiguration: Decodable, Sendable {
    let modelType: String

    public struct Quantization: Codable, Sendable, Equatable {
        init(groupSize: Int, bits: Int) {
            self.groupSize = groupSize
            self.bits = bits
        }

        let groupSize: Int
        let bits: Int
        private var _mode: QuantizationMode? = nil

        var mode: QuantizationMode { _mode ?? .affine }
        var asTuple: (Int, Int, QuantizationMode) { (groupSize, bits, mode) }

        enum CodingKeys: String, CodingKey {
            case groupSize = "group_size"
            case bits
            case _mode = "mode"
        }
    }

    /// Réglage d'une couche : ignorée (`false` dans la config) ou quantifiée
    /// avec ses propres paramètres.
    enum QuantizationOption: Sendable {
        case skip
        case quantize(Quantization)
    }

    public struct PerLayerQuantization: Sendable {
        var quantization: Quantization? = nil
        var perLayerQuantization: [String: QuantizationOption]

        init(quantization: Quantization? = nil, perLayerQuantization: [String: QuantizationOption]) {
            self.quantization = quantization
            self.perLayerQuantization = perLayerQuantization
        }

        /// Les paramètres d'une couche : son réglage propre, sinon le réglage
        /// global.
        func quantization(layer: String) -> Quantization? {
            switch perLayerQuantization[layer] {
            case .skip: return nil
            case .quantize(let q): return q
            case nil: return quantization
            }
        }
    }

    /// La clé `quantization` : réglage global plus, à côté, une entrée par
    /// couche particulière.
    private struct Container: Decodable {
        var perLayer: PerLayerQuantization

        struct Key: CodingKey {
            let stringValue: String
            let intValue: Int? = nil
            init(stringValue: String) { self.stringValue = stringValue }
            init?(intValue: Int) { return nil }
        }

        init(from decoder: Decoder) throws {
            let q = try Quantization(from: decoder)
            var map: [String: QuantizationOption] = [:]
            let c = try decoder.container(keyedBy: Key.self)
            for key in c.allKeys {
                switch key.stringValue {
                case "group_size", "bits", "mode", "quant_method", "linear_class", "quantization_mode":
                    continue
                default:
                    if let f = try? c.decode(Bool.self, forKey: key) {
                        if !f { map[key.stringValue] = .skip }
                    } else {
                        map[key.stringValue] = .quantize(try c.decode(Quantization.self, forKey: key))
                    }
                }
            }
            perLayer = PerLayerQuantization(quantization: q, perLayerQuantization: map)
        }
    }

    private let container: Container?
    var perLayerQuantization: PerLayerQuantization? { container?.perLayer }

    enum CodingKeys: String, CodingKey {
        case modelType = "model_type"
        case container = "quantization"
    }
}
