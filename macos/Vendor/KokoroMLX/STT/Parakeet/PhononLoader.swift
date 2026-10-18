// Ajout de Sillon, hors des fichiers repris de mlx-audio-swift : charge
// Phonon-2 (Fermion Research, poids sous CC-BY-4.0, dérivés de Parakeet TDT
// 0.6B v3 de NVIDIA) dans le graphe Parakeet copié. Voir ORIGINE.md.

import Foundation
import MLX
import MLXNN

/// Une couche linéaire dont chaque ligne ne prend que cinq valeurs
/// {−hi, −lo, 0, lo, hi} : deux plans ternaires en 2 bits affines de MLX
/// (code 0, 1 ou 2, échelle s et biais −s par groupe de 128), sommés. Exact,
/// et la mémoire d'un 4 bits.
final class FiveValueLinear: Linear {
    static let groupSize = 128

    /// Plan des valeurs ±lo : `weight` (hérité), `scales`, `biases`.
    let scales: MLXArray
    let biases: MLXArray
    /// Plan des valeurs ±hi.
    let hiWeight: MLXArray
    let hiScales: MLXArray
    let hiBiases: MLXArray

    init(_ planes: PhononContainer.Planes) {
        let groups = planes.lo.dim(1) * 16 / Self.groupSize
        let perGroup = { (scale: MLXArray) -> MLXArray in
            repeated(scale.expandedDimensions(axis: 1), count: groups, axis: 1)
        }
        let loScales = perGroup(planes.loScale), hiScales = perGroup(planes.hiScale)
        self.scales = loScales
        self.biases = -loScales
        self.hiWeight = planes.hi
        self.hiScales = hiScales
        self.hiBiases = -hiScales
        super.init(weight: planes.lo, bias: nil)
        freeze()
    }

    override var shape: (Int, Int) {
        (weight.dim(0), weight.dim(1) * 16)
    }

    override func callAsFunction(_ x: MLXArray) -> MLXArray {
        let g = Self.groupSize
        return quantizedMM(x, weight, scales: scales, biases: biases, transpose: true, groupSize: g, bits: 2)
            + quantizedMM(x, hiWeight, scales: hiScales, biases: hiBiases, transpose: true, groupSize: g, bits: 2)
    }

    /// La matrice dense [rows, columns] (exacte en float16).
    func dense() -> MLXArray {
        let g = Self.groupSize
        return dequantized(weight, scales: scales, biases: biases, groupSize: g, bits: 2)
            + dequantized(hiWeight, scales: hiScales, biases: hiBiases, groupSize: g, bits: 2)
    }
}

/// Le conteneur `fermion-five-value-parakeet-v1` de Phonon-2 : un en-tête
/// JSON (taille sur 8 octets petit-boutistes, puis l'index), puis les
/// enregistrements bout à bout, nommés comme `ParakeetForTDT` de
/// transformers. Trois sortes :
/// - `five_value` [o, i] : un trit par poids (5 par octet, chiffre 1 = zéro,
///   chaque ligne complétée à l'octet), puis un bit par poids non nul (1 :
///   magnitude hi), puis lo et hi par ligne en float16 ;
/// - `int6` : entiers de 6 bits (4 dans 3 octets, décalés de 32), une échelle
///   float16 par ligne en fin d'enregistrement ;
/// - `fp16` : tel quel.
/// Référence : `fermion_container.py` du dépôt FermionResearch/Phonon-2.
///
/// Le décodage se fait en opérations MLX (comme `repack_ternary` de
/// mlx-audio) : 600 millions de poids, trop pour une boucle Swift.
public enum PhononContainer {
    static let format = "fermion-five-value-parakeet-v1"
    public static let archiveName = "phonon-2.bps.tar.zst"

    struct Record: Decodable {
        let n: String
        let k: String
        let shape: [Int]
        let b: Int
    }

    private struct Header: Decodable {
        let format: String
        let index: [Record]
    }

    /// Les deux plans d'une matrice à cinq valeurs : codes 2 bits rangés par
    /// 16 dans des `uint32` [o, i / 16], le premier dans les bits de poids
    /// faible (le rangement de MLX), et les magnitudes par ligne (float16).
    struct Planes {
        let lo: MLXArray
        let hi: MLXArray
        let loScale: MLXArray
        let hiScale: MLXArray
    }

    /// Ce que le conteneur donne au modèle : les tableaux, sous les noms du
    /// graphe Swift, et les couches à cinq valeurs qui remplacent ses
    /// `Linear`.
    struct Weights {
        var arrays: [String: MLXArray] = [:]
        var linears: [String: FiveValueLinear] = [:]
    }

    enum Failure: LocalizedError {
        case archive(String)
        case malformed(String)

        var errorDescription: String? {
            switch self {
            case .archive(let detail): "archive Phonon illisible : \(detail)"
            case .malformed(let detail): "conteneur Phonon invalide : \(detail)"
            }
        }
    }

    // MARK: - L'archive

    /// Un fichier de l'archive `.tar.zst`, lu par le `tar` de macOS
    /// (libarchive lit le zstd), sans rien écrire sur le disque.
    static func extract(_ member: String, from archive: URL) throws -> Data {
        let tar = Process()
        tar.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        tar.arguments = ["-xOf", archive.path, member]
        let out = Pipe(), err = Pipe()
        tar.standardOutput = out
        tar.standardError = err
        do {
            try tar.run()
        } catch {
            throw Failure.archive(error.localizedDescription)
        }
        // Lu avant d'attendre : un tube plein bloquerait `tar`.
        let data = out.fileHandleForReading.readDataToEndOfFile()
        let message = err.fileHandleForReading.readDataToEndOfFile()
        tar.waitUntilExit()
        guard tar.terminationStatus == 0, !data.isEmpty else {
            let detail = String(data: message, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
            throw Failure.archive(detail?.isEmpty == false ? detail! : "\(member) absent")
        }
        return data
    }

    // MARK: - Le conteneur

    /// Lit tout le conteneur.
    static func decode(_ data: Data) throws -> Weights {
        guard data.count >= 8 else { throw Failure.malformed("trop court") }
        let headerSize = data.withUnsafeBytes { Int($0.loadUnaligned(fromByteOffset: 0, as: UInt64.self).littleEndian) }
        guard headerSize > 0, 8 + headerSize <= data.count else { throw Failure.malformed("en-tête") }
        let header = try JSONDecoder().decode(Header.self, from: data.subdata(in: 8 ..< 8 + headerSize))
        guard header.format == format else { throw Failure.malformed("format \(header.format)") }
        guard 8 + headerSize + header.index.reduce(0, { $0 + $1.b }) == data.count else {
            throw Failure.malformed("taille \(data.count)")
        }

        var weights = Weights()
        var lstmBiases: [String: MLXArray] = [:]
        var offset = 8 + headerSize
        for record in header.index {
            let blob = data.subdata(in: offset ..< offset + record.b)
            offset += record.b
            switch record.k {
            case "five_value":
                guard record.shape.count == 2 else { throw Failure.malformed(record.n) }
                let linear = FiveValueLinear(try planes(blob, rows: record.shape[0], columns: record.shape[1],
                                                        name: record.n))
                // Un enregistrement à la fois : les intermédiaires ne s'accumulent pas.
                eval(linear)
                let key = graphKey(record.n)
                if key.contains(".pointwise_conv") {
                    // Les convolutions de noyau 1 restent des `Conv1d` [o, 1, i] dans le graphe.
                    weights.arrays["\(key).weight"] = linear.dense().expandedDimensions(axis: 1)
                } else {
                    weights.linears[key] = linear
                }
            case "int6":
                try store(record, intSix(blob, shape: record.shape), in: &weights, lstmBiases: &lstmBiases)
            case "fp16":
                guard blob.count == 2 * record.shape.reduce(1, *) else { throw Failure.malformed(record.n) }
                try store(record, MLXArray(blob, record.shape, type: Float16.self), in: &weights,
                          lstmBiases: &lstmBiases)
            default:
                throw Failure.malformed("sorte \(record.k) (\(record.n))")
            }
        }

        // LSTM : un seul biais dans le graphe, la somme des deux de PyTorch.
        for layer in Set(lstmBiases.keys.map { $0.dropLast(3) }) {
            guard let ih = lstmBiases["\(layer).ih"], let hh = lstmBiases["\(layer).hh"] else {
                throw Failure.malformed("biais du LSTM \(layer)")
            }
            weights.arrays["\(layer).bias"] = ih + hh
        }
        return weights
    }

    /// Range un tableau dense sous son nom du graphe, à la disposition de MLX
    /// (canaux en dernier pour les convolutions).
    private static func store(_ record: Record, _ value: MLXArray, in weights: inout Weights,
                              lstmBiases: inout [String: MLXArray]) throws {
        let name = record.n
        // Compteur de BatchNorm, jamais lu en inférence.
        if name.hasSuffix(".num_batches_tracked") { return }
        if let range = name.range(of: #"^decoder\.lstm\.bias_(ih|hh)_l\d+$"#, options: .regularExpression) {
            let parts = name[range].split(separator: "_")
            let layer = parts.last!.dropFirst()
            lstmBiases["decoder.prediction.dec_rnn.lstm.\(layer).\(parts[1])"] = value
            return
        }
        var array = value
        switch value.ndim {
        case 3: array = value.transposed(0, 2, 1)
        case 4: array = value.transposed(0, 2, 3, 1)
        default: break
        }
        eval(array)
        weights.arrays[graphKey(name)] = array
    }

    /// Le nom d'un tenseur de transformers dans le graphe Swift : les noms
    /// de NeMo (ceux des safetensors de mlx-community), puis les renommages
    /// de `ParakeetModel.remapKey` (`joint_net.2` → `joint_net`, `posBiasU`,
    /// liste du sous-échantillonnage → `conv0`, `depthwise_layers.n`,
    /// `pointwise_layers.n`).
    static func graphKey(_ name: String) -> String {
        var key = name
        for (old, new) in [
            ("encoder.subsampling.linear", "encoder.pre_encode.out"),
            (".conv.norm.", ".conv.batch_norm."),
            (".self_attn.q_proj", ".self_attn.linear_q"),
            (".self_attn.k_proj", ".self_attn.linear_k"),
            (".self_attn.v_proj", ".self_attn.linear_v"),
            (".self_attn.o_proj", ".self_attn.linear_out"),
            (".self_attn.relative_k_proj", ".self_attn.linear_pos"),
            (".self_attn.bias_u", ".self_attn.posBiasU"),
            (".self_attn.bias_v", ".self_attn.posBiasV"),
            ("decoder.embedding.", "decoder.prediction.embed."),
            ("decoder.decoder_projector.", "joint.pred."),
            ("encoder_projector.", "joint.enc."),
            ("joint.head.", "joint.joint_net."),
        ] {
            key = key.replacingOccurrences(of: old, with: new)
        }
        let prefix = "encoder.subsampling.layers."
        if key.hasPrefix(prefix) {
            // conv.0, puis par bloc : profondeur (2 + 3n), point (3 + 3n), ReLU (4 + 3n).
            let rest = key.dropFirst(prefix.count)
            if let dot = rest.firstIndex(of: "."), let index = Int(rest[..<dot]) {
                let suffix = rest[rest.index(after: dot)...]
                let layer = index == 0 ? "conv0"
                    : (index - 2) % 3 == 0 ? "depthwise_layers.\((index - 2) / 3)"
                    : "pointwise_layers.\((index - 2) / 3)"
                key = "encoder.pre_encode.\(layer).\(suffix)"
            }
        }
        if let range = key.range(of: #"^decoder\.lstm\.weight_(ih|hh)_l\d+$"#, options: .regularExpression) {
            let parts = key[range].split(separator: "_")
            let layer = parts.last!.dropFirst()
            key = "decoder.prediction.dec_rnn.lstm.\(layer).\(parts[1] == "ih" ? "Wx" : "Wh")"
        }
        return key
    }

    /// `int6` : q × échelle de sa ligne, en float32.
    static func intSix(_ blob: Data, shape: [Int]) throws -> MLXArray {
        guard let rows = shape.first, rows > 0 else { throw Failure.malformed("int6 sans lignes") }
        let total = shape.reduce(1, *)
        let bodySize = blob.count - 2 * rows
        guard bodySize >= 0, bodySize % 3 == 0, bodySize / 3 * 4 >= total else {
            throw Failure.malformed("int6 de \(blob.count) octets pour \(shape)")
        }
        let body = MLXArray(blob.prefix(bodySize), [bodySize / 3, 3], type: UInt8.self).asType(.uint32)
        let packed = body[0..., 0] + body[0..., 1] * 256 + body[0..., 2] * 65_536
        let shifts = MLXArray([0, 6, 12, 18].map(UInt32.init))
        let q = bitwiseAnd(packed.expandedDimensions(axis: 1) >> shifts, MLXArray(UInt32(0x3F)))
            .flattened()[0 ..< total].asType(.int32) - 32
        let scales = MLXArray(blob.suffix(2 * rows), [rows, 1], type: Float16.self).asType(.float32)
        return (q.reshaped(rows, total / rows).asType(.float32) * scales).reshaped(shape)
    }

    /// Les trits de chaque octet (0…242), du chiffre de poids faible au fort.
    private static var trits: MLXArray {
        MLXArray((0 ..< 243).flatMap { byte in [1, 3, 9, 27, 81].map { UInt32(byte / $0 % 3) } }, [243, 5])
    }

    /// `five_value` [rows, columns] → deux plans 2 bits. Code 1 + signe sur
    /// le plan de sa magnitude, 1 (zéro) sur l'autre.
    static func planes(_ blob: Data, rows: Int, columns: Int, name: String) throws -> Planes {
        guard rows > 0, columns > 0, columns % FiveValueLinear.groupSize == 0 else {
            throw Failure.malformed("forme [\(rows), \(columns)] de \(name)")
        }
        let rowBytes = (columns + 4) / 5
        let tritBytes = rows * rowBytes
        guard blob.count >= tritBytes else { throw Failure.malformed(name) }

        let bytes = MLXArray(blob.prefix(tritBytes), [rows, rowBytes], type: UInt8.self).asType(.int32)
        // [rows, rowBytes, 5] → [rows, columns] : le remplissage de fin de ligne tombe.
        let codes = trits.take(bytes, axis: 0).reshaped(rows, rowBytes * 5)[0..., 0 ..< columns]
        let nonZero = codes .!= MLXArray(UInt32(1))
        let count = nonZero.asType(.int32).sum().item(Int.self)

        let bitsStart = tritBytes
        let scalesStart = bitsStart + (count + 7) / 8
        guard scalesStart + 4 * rows == blob.count else {
            throw Failure.malformed("\(name) : \(blob.count) octets, \(scalesStart + 4 * rows) attendus")
        }
        // Le bit du k-ième non-nul (dans l'ordre des lignes) : 1 = magnitude hi.
        var isHi = MLXArray.zeros([rows, columns], type: Bool.self)
        if count > 0 {
            let packedBits = MLXArray(blob[blob.startIndex + bitsStart ..< blob.startIndex + scalesStart],
                                      type: UInt8.self).asType(.uint32)
            let bits = bitwiseAnd(packedBits.expandedDimensions(axis: 1) >> MLXArray((0 ..< 8).map(UInt32.init)),
                                  MLXArray(UInt32(1))).flattened()
            let rank = clip(nonZero.asType(.int32).flattened().cumsum() - 1, min: 0)
            isHi = nonZero .&& (bits.take(rank).reshaped(rows, columns) .== MLXArray(UInt32(1)))
        }
        let one = MLXArray(UInt32(1))
        let loCodes = which(nonZero .&& .!isHi, codes, one)
        let hiCodes = which(isHi, codes, one)
        // Seize codes par mot : code × 4^k, sans chevauchement (somme = ou).
        let place = MLXArray((0 ..< 16).map { UInt32(1) << UInt32(2 * $0) })
        let pack = { (c: MLXArray) -> MLXArray in
            (c.reshaped(rows, columns / 16, 16) * place).sum(axis: -1).asType(.uint32)
        }
        let scales = MLXArray(blob.suffix(4 * rows), [2, rows], type: Float16.self)
        return Planes(lo: pack(loCodes), hi: pack(hiCodes), loScale: scales[0], hiScale: scales[1])
    }
}

public extension ParakeetModel {
    /// Phonon-2 : le dossier du paquet tient l'archive telle que téléchargée
    /// (`config.json` de NeMo et `model.fermion`), lue sans être décompressée
    /// sur le disque. Le graphe est celui de Parakeet v3 (TDT) ; ses
    /// `Linear` d'encodeur deviennent des `FiveValueLinear`.
    static func fromPhonon(_ directory: URL, computeDType: DType = .bfloat16) throws -> ParakeetModel {
        let archive = directory.appendingPathComponent(PhononContainer.archiveName)

        var configText = String(decoding: try PhononContainer.extract("config.json", from: archive), as: UTF8.self)
        // Comme `fromDirectory` : les flottants hors JSON de NeMo.
        for token in ["-Infinity", "Infinity", "NaN"] {
            configText = configText.replacingOccurrences(of: token, with: "null")
        }
        let rawConfig = try JSONDecoder().decode(ParakeetRawConfig.self, from: Data(configText.utf8))
        guard try ParakeetVariantResolver.resolve(rawConfig) == .tdt else {
            throw PhononContainer.Failure.malformed("Phonon-2 attend un Parakeet TDT")
        }
        let cfg = try ParakeetConfigParser.parseTDT(rawConfig)
        let model = ParakeetModel(
            variant: .tdt,
            preprocessConfig: cfg.preprocessor,
            encoderConfig: cfg.encoder,
            vocabulary: cfg.joint.vocabulary,
            durations: cfg.decoding.durations,
            maxSymbols: cfg.decoding.greedy?.maxSymbols,
            decoderConfig: cfg.decoder,
            jointConfig: cfg.joint,
            ctcConfig: nil
        )

        let weights = try PhononContainer.decode(try PhononContainer.extract("model.fermion", from: archive))
        model.update(modules: ModuleChildren.unflattened(weights.linears.mapValues { $0 as Module }))
        // Chaque paramètre du graphe reçoit sa valeur, à la bonne forme.
        var parameters = weights.arrays
        for (path, linear) in weights.linears {
            for (name, value) in linear.parameters().flattened() {
                parameters["\(path).\(name)"] = value
            }
        }
        try model.update(parameters: ModuleParameters.unflattened(parameters), verify: .all)

        model.computeDType = computeDType
        // Comme `fromDirectory` : les flottants au type de calcul, les mots
        // des plans (`uint32`) tels quels.
        let casted = Dictionary(
            uniqueKeysWithValues: model.parameters().flattened().map { key, value -> (String, MLXArray) in
                guard value.dtype.isFloatingPoint, value.dtype != computeDType else { return (key, value) }
                return (key, value.asType(computeDType))
            }
        )
        try model.update(parameters: ModuleParameters.unflattened(casted), verify: .noUnusedKeys)
        model.train(false)
        eval(model)
        return model
    }
}
