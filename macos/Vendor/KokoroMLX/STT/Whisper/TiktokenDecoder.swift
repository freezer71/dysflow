import Foundation

// Ajout de Sillage (pas dans mlx-audio-swift) : remplace swift-transformers,
// dont Whisper ne se servait que pour ids → texte.

/// Décodeur tiktoken minimal pour Whisper : lit `multilingual.tiktoken` (une
/// ligne « jeton_base64 rang » par jeton, rangs 0, 1, 2… contigus ; 0…50256
/// dans les dépôts mlx-community) et ne fait que ids → texte. Pas d'encodage.
///
/// Les jetons spéciaux de Whisper (`<|endoftext|>` = 50257, la tâche, les
/// langues, les horodatages…) ne sont **pas** dans ce fichier : leurs ids
/// suivent le dernier rang. Est donc tenu pour spécial tout id hors de la
/// table (`id < 0` ou `id >= count`), quelle que soit la taille du
/// vocabulaire du modèle.
public final class TiktokenDecoder {
    public enum Failure: Error, CustomStringConvertible {
        case malformedLine(Int)
        case nonContiguousRank(line: Int, rank: Int)

        public var description: String {
            switch self {
            case .malformedLine(let n): return "fichier tiktoken : ligne \(n) illisible"
            case .nonContiguousRank(let n, let r): return "fichier tiktoken : rang \(r) inattendu ligne \(n)"
            }
        }
    }

    /// Octets de chaque jeton, indexés par rang.
    private let bytesByRank: [[UInt8]]

    /// Nombre de jetons ordinaires ; le premier id spécial.
    public var count: Int { bytesByRank.count }

    public init(contentsOf url: URL) throws {
        let text = try String(contentsOf: url, encoding: .utf8)
        var table: [[UInt8]] = []
        table.reserveCapacity(50_257)
        var lineNumber = 0
        for line in text.split(whereSeparator: \.isNewline) {
            lineNumber += 1
            let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
            guard parts.count == 2, let rank = Int(parts[1]) else {
                throw Failure.malformedLine(lineNumber)
            }
            guard rank == table.count else {
                throw Failure.nonContiguousRank(line: lineNumber, rank: rank)
            }
            // Dernière ligne du fichier de Whisper : « = 50256 ». Python le lit
            // comme b'' ; Data(base64Encoded: "=") rend nil : jeton vide.
            if parts[0] == "=" {
                table.append([])
            } else if let data = Data(base64Encoded: String(parts[0])) {
                table.append([UInt8](data))
            } else {
                throw Failure.malformedLine(lineNumber)
            }
        }
        bytesByRank = table
    }

    /// Concatène les octets des jetons puis décode en UTF-8 : un caractère
    /// peut être réparti sur plusieurs jetons. Une séquence invalide donne
    /// U+FFFD, comme `errors="replace"` de tiktoken.
    ///
    /// Ids hors de la table (jetons spéciaux, voir plus haut) : omis si
    /// `skipSpecial`, sinon écrits `<|id|>` (le fichier ne porte pas leur nom).
    ///
    /// `dropIncompleteTail` coupe une séquence UTF-8 inachevée en fin de
    /// texte : en flux, un caractère à cheval sur deux jetons donnerait sinon
    /// un U+FFFD transitoire, et le texte déjà émis ne serait plus un préfixe.
    public func decode(_ ids: [Int], skipSpecial: Bool, dropIncompleteTail: Bool = false) -> String {
        var out: [UInt8] = []
        out.reserveCapacity(ids.count * 4)
        for id in ids {
            if id >= 0 && id < bytesByRank.count {
                out.append(contentsOf: bytesByRank[id])
            } else if !skipSpecial {
                out.append(contentsOf: Array("<|\(id)|>".utf8))
            }
        }
        if dropIncompleteTail {
            out.removeLast(out.count - Self.completeUTF8Length(out))
        }
        return String(decoding: out, as: UTF8.self)
    }

    /// Longueur du plus long préfixe qui ne s'arrête pas au milieu d'un caractère.
    static func completeUTF8Length(_ b: [UInt8]) -> Int {
        var i = b.count - 1
        var continuation = 0
        while i >= 0, continuation < 3, b[i] & 0xC0 == 0x80 {
            i -= 1
            continuation += 1
        }
        guard i >= 0 else { return b.count }
        let lead = b[i]
        let needed: Int
        if lead < 0x80 { needed = 1 }
        else if lead & 0xE0 == 0xC0 { needed = 2 }
        else if lead & 0xF0 == 0xE0 { needed = 3 }
        else if lead & 0xF8 == 0xF0 { needed = 4 }
        else { return b.count } // octet invalide : laisser U+FFFD
        return b.count - i < needed ? i : b.count
    }
}

/// Ids spéciaux de Whisper multilingue (`tokenizer.py` d'OpenAI), déduits de
/// `n_vocab` : les dépôts mlx-community n'ont ni `generation_config.json` ni
/// `tokenizer_config.json`. Vérifiés contre `generation_config.json`
/// d'openai/whisper-tiny (51865) et openai/whisper-large-v3-turbo (51866).
///
/// Turbo (51866) a une langue de plus (`yue` = 50358) : tous les jetons
/// placés après les langues (tâche, `notimestamps`, horodatages) sont décalés
/// de +1. `endoftext`, `startoftranscript` et les langues ne bougent pas.
public struct WhisperSpecialTokens: Sendable {
    /// Tailles gérées. Les modèles anglais seuls (`.en`, 51864) ont une autre
    /// table : refusés par `WhisperModel.fromDirectory`.
    public static let supportedVocabularySizes: Set<Int> = [51865, 51866]

    /// Ordre officiel ; les 99 premières pour 51865, les 100 pour 51866.
    static let languageCodes = [
        "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi",
        "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk",
        "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
        "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
        "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su", "yue",
    ]

    /// Partie non spéciale de `suppress_tokens` (identique tiny / large-v3-turbo).
    static let suppressBase = [
        1, 2, 7, 8, 9, 10, 14, 25, 26, 27, 28, 29, 31, 58, 59, 60, 61, 62, 63, 90, 91, 92, 93, 359, 503, 522,
        542, 873, 893, 902, 918, 922, 931, 1350, 1853, 1982, 2460, 2627, 3246, 3253, 3268, 3536, 3846, 3961,
        4183, 4667, 6585, 6647, 7273, 9061, 9383, 10428, 10929, 11938, 12033, 12331, 12562, 13793, 14157,
        14635, 15265, 15618, 16553, 16604, 18362, 18956, 20075, 21675, 22520, 26130, 26161, 26435, 28279,
        29464, 31650, 32302, 32470, 36865, 42863, 47425, 49870, 50254,
    ]

    public let endOfText = 50257
    public let startOfTranscript = 50258
    public let languageToId: [String: Int]
    public let translate: Int
    public let transcribe: Int
    public let startOfLM: Int
    public let startOfPrev: Int
    public let noSpeech: Int
    public let noTimestamps: Int
    /// Premier horodatage `<|0.00|>` ; tout ce qui suit est horodatage.
    public let timestampBegin: Int

    /// 100 langues si `vocabularySize >= 51866`, 99 sinon. N'a de sens que
    /// pour `supportedVocabularySizes`.
    public init(vocabularySize: Int) {
        let languageCount = vocabularySize >= 51866 ? 100 : 99 // 51866 − 51765 − 1
        var map: [String: Int] = [:]
        for (i, code) in Self.languageCodes.prefix(languageCount).enumerated() {
            map[code] = 50259 + i
        }
        languageToId = map
        translate = 50259 + languageCount
        transcribe = translate + 1
        startOfLM = translate + 2
        startOfPrev = translate + 3
        noSpeech = translate + 4
        noTimestamps = translate + 5
        timestampBegin = translate + 6
    }

    public var english: Int { languageToId["en"]! }
    public var french: Int { languageToId["fr"]! }

    var suppressTokens: [Int] {
        Self.suppressBase + [startOfTranscript, translate, transcribe, startOfLM, startOfPrev, noSpeech]
    }

    var beginSuppressTokens: [Int] { [220, endOfText] }

    /// Remplit `WhisperModel.generationConfig` quand le dossier n'a pas de
    /// `generation_config.json` (cas des dépôts mlx-community).
    var generationConfig: WhisperGenerationConfig {
        var g = WhisperGenerationConfig()
        g.isMultilingual = true
        g.decoderStartTokenId = startOfTranscript
        g.eosTokenId = endOfText
        g.noTimestampsTokenId = noTimestamps
        g.prevSotTokenId = startOfPrev
        g.suppressTokens = suppressTokens
        g.beginSuppressTokens = beginSuppressTokens
        g.langToId = languageToId.reduce(into: [:]) { $0["<|\($1.key)|>"] = $1.value }
        g.taskToId = ["transcribe": transcribe, "translate": translate]
        return g
    }
}
