// Modifié par Sillon (Apache-2.0 §4(b)) : français en NFC, langues sans dictionnaire lues lettre à lettre, chinois et japonais retirés — voir ORIGINE.md.
import Foundation

/// Per-language tokenizer config dumped from NeMo's MagpieTTS multilingual
/// vocabulary. Each language's JSON ships the SAME shared 2360-entry vocab
/// (so the cross-lingual decoder sees a single token-ID space); the
/// per-language difference is *how* input text is converted to that
/// vocab's symbols.
public struct MagpieTokenizerConfig: Codable, Sendable {
    public let language: String
    public let tokenizerName: String
    public let type: String
    public let vocabSize: Int
    public let tokens: [String]
    public let bosId: Int?
    public let eosId: Int?
    public let padId: Int?
    public let config: MagpieTokenizerSubConfig?

    enum CodingKeys: String, CodingKey {
        case language
        case tokenizerName = "tokenizer_name"
        case type
        case vocabSize = "vocab_size"
        case tokens
        case bosId = "bos_id"
        case eosId = "eos_id"
        case padId = "pad_id"
        case config
    }

    public struct MagpieTokenizerSubConfig: Codable, Sendable {
        public let punct: Bool?
        public let apostrophe: Bool?
        public let padWithSpace: Bool?
        public let pretrainedModel: String?
        enum CodingKeys: String, CodingKey {
            case punct, apostrophe
            case padWithSpace = "pad_with_space"
            case pretrainedModel = "pretrained_model"
        }
    }
}

/// Maps language-specific text to a sequence of vocab IDs. Loads the
/// shared vocab JSON at construction time.
///
/// **Current scope.** The initial port implements:
///
/// * Symbol-level char tokenization against the bundle's vocab list. Tokens
///   not present in the vocab map to the `<oov>` slot (or are silently
///   dropped if `<oov>` is absent — older bundles).
/// * `pad_with_space` handling per the per-language NeMo config (Mandarin and
///   Hindi prepend / append a space).
/// * Sillon : le japonais, le chinois, l'espagnol et l'allemand (G2P et
///   dictionnaires non copiés) sont lus caractère par caractère ; Sillon
///   ne lit Magpie qu'en français et en anglais.
///
/// **What is intentionally simplified.** Each language ships a NeMo G2P /
/// transliteration model on the Python side (eSpeak-style rule sets for
/// en/es/de, pypinyin for zh, BPMF for hi, byT5 byte tokenization for
/// fr/it/vi). Replicating each of those in pure Swift is a separate, large
/// effort. For now we feed the user's raw text through with light
/// normalisation. The TTS still produces audio — the diction is approximate
/// when the input is not pre-phonemised. Callers who already have IPA can
/// pass it as-is (or use the `--magpie-prephonemized` CLI flag).
public final class MagpieTokenizer {
    public let language: MagpieLanguage
    public let config: MagpieTokenizerConfig
    public let tokenToId: [String: Int]
    public let oovId: Int?
    public let padId: Int?

    public init(language: MagpieLanguage, config: MagpieTokenizerConfig) {
        self.language = language
        self.config = config
        // Build a **per-language** char → aggregated-id map by slicing the
        // bundle's full 2360-entry vocab to this language's sub-tokenizer
        // section `[offset, offset + size)`. NeMo's BaseTokenizer builds the
        // lookup via `{l: i for i, l in enumerate(tokens)}` — a Python dict
        // comprehension where **later occurrences overwrite earlier ones**.
        // Some tokenizers (notably `HindiCharsTokenizer`) emit duplicate
        // entries within their own vocab (the IPA punct list overlaps with
        // the Devanagari character set), and the model was trained using the
        // last-occurrence id. We mirror that here.
        let offset = MagpieSubVocab.offset(for: language)
        let size = MagpieSubVocab.size(for: language)
        let end = min(offset + size, config.tokens.count)
        var map: [String: Int] = [:]
        for i in offset..<end {
            map[config.tokens[i]] = i  // overwrite; last wins
        }
        self.tokenToId = map
        self.oovId = map["<oov>"]
        self.padId = map["<pad>"] ?? config.padId
    }

    public static func load(from url: URL, language: MagpieLanguage) throws -> MagpieTokenizer {
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw MagpieTTSError.missingFile(url.lastPathComponent)
        }
        let data = try Data(contentsOf: url)
        let cfg = try JSONDecoder().decode(MagpieTokenizerConfig.self, from: data)
        return MagpieTokenizer(language: language, config: cfg)
    }

    /// Magpie multilingual text-embedding adds BOS + EOS rows past the vocab:
    /// `text_embedding.size = vocab_size + 2`. Round-trip inference requires
    /// the text token sequence to be terminated with `eos_id` so the decoder
    /// can attend to a proper sentence boundary. (See
    /// `magpietts.py`: `self.eos_id = num_tokens - 1` with
    /// `num_tokens = vocab_size + 2`.)
    public var eosId: Int { config.vocabSize + 1 }
    public var bosId: Int { config.vocabSize }

    /// Phonemise + char-tokenise. Returns the integer ID sequence consumed by
    /// the text encoder. The trailing `eos_id = vocab_size + 1` is **always**
    /// appended (mirroring NeMo's round-trip test and the model's training
    /// regime); without it the AR loop often fails to terminate.
    ///
    /// Dispatch per language (matches `model_config.yaml`):
    /// - en / es / de — IPA dict G2P, char-tokenise IPA stream
    /// - fr / it / vi — `byT5` UTF-8 byte encoder (`byte + 1979`)
    /// - hi          — char-level (Devanagari graphemes are in the vocab)
    /// - zh / ja     — char-level fallback **for now** (needs jieba+pypinyin
    ///                 and Apple morpheme analyzer, tracked as follow-up)
    public func tokenize(_ text: String, prephonemized: Bool = false) -> [Int] {
        var content: [Int]
        if prephonemized {
            content = charLookup(text)
        } else {
            content = encodePerLanguage(text)
        }

        // `pad_with_space` per the bundle's tokenizer config, with one
        // override: Japanese reuses the EN tokenizer JSON (no JA file
        // ships), so we hardcode `pad_with_space=true` for JA to match
        // `model_config.yaml`.
        let padWithSpace: Bool
        if language == .japanese {
            padWithSpace = true
        } else {
            padWithSpace = config.config?.padWithSpace ?? false
        }
        let space = tokenToId[" "]
        if padWithSpace, let s = space {
            content.insert(s, at: 0)
            content.append(s)
        }

        var clean = content.filter { $0 >= 0 }
        clean.append(eosId)
        return clean
    }

    /// Map raw text through the per-language preprocessing + phoneme
    /// pipeline. Always returns Magpie shared-vocab IDs.
    private func encodePerLanguage(_ text: String) -> [Int] {
        switch language {
        case .english:
            // NeMo `english_text_preprocessing(text, lower=False)` fait NFD
            // et retire les diacritiques ; ici, NFKC seulement (forme
            // composée de compatibilité : ligatures et pleine chasse
            // ramenées à l'ASCII), diacritiques gardés. Casse gardée : le
            // dictionnaire est cherché en minuscules.
            let normalised = text
                .precomposedStringWithCompatibilityMapping
            return charLookup(MagpieDictG2P.english.phonemize(normalised))

        case .french:
            // byt5_byte tokeniser. NeMo lowercases before encoding.
            // Sillon : NFC d'abord (un « é » décomposé ferait deux octets).
            return MagpieByT5Encoder.encode(MagpieRunner.prepare(text).lowercased(), language: .french)

        case .italian:
            return MagpieByT5Encoder.encode(text.lowercased(), language: .italian)

        case .vietnamese:
            return MagpieByT5Encoder.encode(text.lowercased(), language: .vietnamese)

        case .hindi, .spanish, .german, .chinese, .japanese:
            // hi : char_hindi tokeniser, Devanagari graphemes verbatim.
            // Sillon : es, de (dictionnaires IPA), zh, ja (G2P) non copiés :
            // lecture caractère par caractère, comme `prephonemized`.
            return charLookup(text)
        }
    }

    /// Codepoint-by-codepoint vocab lookup for IPA / Devanagari / Latin
    /// strings. We iterate `unicodeScalars` (not `Character`) so that
    /// Devanagari conjuncts like "स्ते" (4 codepoints clustered into one
    /// grapheme by Swift) decompose into individual vocab entries — Magpie
    /// trained on per-codepoint inputs.
    private func charLookup(_ s: String) -> [Int] {
        var ids: [Int] = []
        ids.reserveCapacity(s.unicodeScalars.count)
        for scalar in s.unicodeScalars {
            ids.append(idForChar(String(scalar)))
        }
        return ids
    }

    private func idForChar(_ s: String) -> Int {
        if let id = tokenToId[s] { return id }
        if let oov = oovId { return oov }
        return -1
    }

}
