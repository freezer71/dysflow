import Foundation

/// L'encodeur BPE de Qwen2 (ByteLevel), écrit pour Dysflow à la place de
/// swift-transformers : `vocab.json`, `merges.txt` et les jetons ajoutés de
/// `tokenizer_config.json`. Qwen3-TTS encode (le texte à dire) ; Qwen3-ASR
/// encode son gabarit et décode le texte reconnu (`decode(tokens:)`). Ne lit
/// que : n'écrit jamais dans le dossier du modèle. Parité :
/// `Qwen2BPETokenizerTests` (ids de `transformers`).
public final class Qwen2BPETokenizer: @unchecked Sendable {
    private let vocab: [String: Int]
    /// « a b » → rang de la fusion (plus petit = plus tôt).
    private let ranks: [String: Int]
    /// Jetons ajoutés, du plus long au plus court (contenu, id).
    private let added: [(String, Int)]
    private let split: NSRegularExpression
    /// Jetons ajoutés par id, pour `decode`.
    private let addedByID: [Int: String]
    /// id → morceau (`reverseVocab`), sous `lock`.
    private var reverse: [Int: String]?
    private var cache: [String: [Int]] = [:]
    private let lock = NSLock()

    /// La pré-découpe de `transformers.Qwen2Tokenizer` (chiffres un à un :
    /// `\p{N}`, pas le `\p{N}{1,3}` du port ; la fixture le confirme).
    static let pattern =
        #"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+"#

    /// La table GPT-2 `bytes_to_unicode` : chaque octet devient un caractère
    /// imprimable.
    static let byteEncoder: [Character] = {
        var bytes = Array(33...126) + Array(161...172) + Array(174...255)
        var chars = bytes
        var n = 0
        for b in 0..<256 where !bytes.contains(b) {
            bytes.append(b)
            chars.append(256 + n)
            n += 1
        }
        var table = [Character](repeating: " ", count: 256)
        for (b, c) in zip(bytes, chars) {
            table[b] = Character(Unicode.Scalar(UInt32(c))!)
        }
        return table
    }()

    public convenience init(directory: URL) throws {
        try self.init(vocab: directory.appendingPathComponent("vocab.json"),
                      merges: directory.appendingPathComponent("merges.txt"),
                      config: directory.appendingPathComponent("tokenizer_config.json"))
    }

    public init(vocab vocabURL: URL, merges mergesURL: URL, config configURL: URL) throws {
        vocab = try JSONDecoder().decode([String: Int].self, from: Data(contentsOf: vocabURL))
        var ranks: [String: Int] = [:]
        let lines = try String(contentsOf: mergesURL, encoding: .utf8)
            .split(separator: "\n", omittingEmptySubsequences: true)
        var rank = 0
        for (index, line) in lines.enumerated() {
            if index == 0, line.hasPrefix("#version") { continue }
            let parts = line.split(separator: " ", omittingEmptySubsequences: false)
            guard parts.count == 2 else { continue }
            ranks["\(parts[0]) \(parts[1])"] = rank
            rank += 1
        }
        self.ranks = ranks
        struct Config: Decodable {
            struct Added: Decodable { let content: String }
            let added_tokens_decoder: [String: Added]?
        }
        let config = try JSONDecoder().decode(Config.self, from: Data(contentsOf: configURL))
        let added = (config.added_tokens_decoder ?? [:])
            .compactMap { key, value in Int(key).map { (value.content, $0) } }
            .sorted { $0.0.count > $1.0.count }
        self.added = added
        addedByID = Dictionary(added.map { ($0.1, $0.0) }, uniquingKeysWith: { first, _ in first })
        split = try NSRegularExpression(pattern: Self.pattern)
    }

    /// `allowSpecial` vrai (par défaut, comme `transformers`) : un jeton
    /// ajouté écrit en toutes lettres (`<|im_end|>`, `<think>`…) devient son
    /// id. Faux : tout est du texte ordinaire, pour la phrase à lire (un
    /// livre peut contenir ces chaînes) ; seul le gabarit garde ses jetons.
    public func encode(text: String, allowSpecial: Bool = true) -> [Int] {
        let text = text.precomposedStringWithCanonicalMapping
        guard allowSpecial else { return encodeOrdinary(text) }
        var ids: [Int] = []
        var rest = Substring(text)
        while !rest.isEmpty {
            // Le jeton ajouté le plus proche (le plus long à égalité).
            var next: (range: Range<Substring.Index>, id: Int)?
            for (content, id) in added {
                if let r = rest.range(of: content), r.lowerBound < (next?.range.lowerBound ?? rest.endIndex) {
                    next = (r, id)
                }
            }
            guard let found = next else {
                ids += encodeOrdinary(String(rest))
                break
            }
            ids += encodeOrdinary(String(rest[..<found.range.lowerBound]))
            ids.append(found.id)
            rest = rest[found.range.upperBound...]
        }
        return ids
    }

    /// Ids → texte (Qwen3-ASR) : les jetons ajoutés rendent leur contenu
    /// (`<asr_text>` sépare la langue reconnue du texte), les autres leurs
    /// octets (table GPT-2 inversée), décodés en UTF-8 une fois mis bout à
    /// bout (un caractère peut tenir sur deux jetons). Un id inconnu est
    /// omis. Comme `decode(tokens:)` de swift-transformers sans
    /// `skipSpecialTokens`.
    public func decode(tokens: [Int]) -> String {
        let pieces = reverseVocab()
        var text = ""
        var bytes: [UInt8] = []
        func flush() {
            text += String(decoding: bytes, as: UTF8.self)
            bytes.removeAll(keepingCapacity: true)
        }
        for id in tokens {
            if let special = addedByID[id] {
                flush()
                text += special
            } else if let piece = pieces[id] {
                for scalar in piece.unicodeScalars {
                    if let byte = Self.byteDecoder[scalar] { bytes.append(byte) }
                }
            }
        }
        flush()
        return text
    }

    /// id → morceau, construit au premier décodage seulement (la lecture,
    /// qui n'encode que, ne le paie pas).
    private func reverseVocab() -> [Int: String] {
        lock.lock()
        defer { lock.unlock() }
        if let reverse { return reverse }
        let built = Dictionary(vocab.map { ($0.value, $0.key) }, uniquingKeysWith: { first, _ in first })
        reverse = built
        return built
    }

    /// L'inverse de `byteEncoder`.
    static let byteDecoder: [Unicode.Scalar: UInt8] = {
        var table: [Unicode.Scalar: UInt8] = [:]
        for (byte, character) in byteEncoder.enumerated() {
            table[character.unicodeScalars.first!] = UInt8(byte)
        }
        return table
    }()

    private func encodeOrdinary(_ text: String) -> [Int] {
        guard !text.isEmpty else { return [] }
        let ns = text as NSString
        var ids: [Int] = []
        for match in split.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            ids += encodePiece(ns.substring(with: match.range))
        }
        return ids
    }

    private func encodePiece(_ piece: String) -> [Int] {
        lock.lock()
        let hit = cache[piece]
        lock.unlock()
        if let hit { return hit }
        let symbols = piece.utf8.map { String(Self.byteEncoder[Int($0)]) }
        let ids = bpe(symbols).compactMap { vocab[$0] }
        lock.lock()
        cache[piece] = ids
        lock.unlock()
        return ids
    }

    /// Fusionne la paire de plus petit rang, toutes ses occurrences de gauche
    /// à droite, jusqu'à ce qu'aucune paire ne soit connue (GPT-2).
    private func bpe(_ symbols: [String]) -> [String] {
        var parts = symbols
        while parts.count > 1 {
            var best: (rank: Int, first: String, second: String)?
            for i in 0..<(parts.count - 1) {
                if let r = ranks["\(parts[i]) \(parts[i + 1])"], r < (best?.rank ?? Int.max) {
                    best = (r, parts[i], parts[i + 1])
                }
            }
            guard let best else { break }
            var merged: [String] = []
            merged.reserveCapacity(parts.count)
            var i = 0
            while i < parts.count {
                if i < parts.count - 1, parts[i] == best.first, parts[i + 1] == best.second {
                    merged.append(best.first + best.second)
                    i += 2
                } else {
                    merged.append(parts[i])
                    i += 1
                }
            }
            parts = merged
        }
        return parts
    }
}
