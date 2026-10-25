import Foundation

// Réécrit par Dysflow : même API pour WhisperModel que l'amont, sans
// swift-transformers. Le vocabulaire vient de `multilingual.tiktoken`
// (TiktokenDecoder), les ids spéciaux de WhisperSpecialTokens.

public final class WhisperTokenizer {
    let inner: TiktokenDecoder
    let isMultilingual: Bool

    let startOfTranscriptId: Int
    let endOfTextId: Int
    let noTimestampsId: Int
    let prevSotId: Int?
    let transcribeId: Int?
    let translateId: Int?
    let timestampBeginId: Int

    let languageToId: [String: Int]
    let specialTokenIds: Set<Int>

    init(tiktokenURL: URL, special: WhisperSpecialTokens) throws {
        inner = try TiktokenDecoder(contentsOf: tiktokenURL)
        isMultilingual = true
        startOfTranscriptId = special.startOfTranscript
        endOfTextId = special.endOfText
        noTimestampsId = special.noTimestamps
        prevSotId = special.startOfPrev
        transcribeId = special.transcribe
        translateId = special.translate
        timestampBeginId = special.timestampBegin
        languageToId = special.languageToId
        specialTokenIds = Set(special.endOfText..<special.timestampBegin)
    }

    /// Build the decoder prefix for a transcription request. `language` may be
    /// an ISO code ("en") or a full name ("English"); pass `nil` to let
    /// multilingual variants auto-detect.
    public func buildPromptTokens(language: String?, task: String = "transcribe") -> [Int] {
        var tokens: [Int] = [startOfTranscriptId]
        if isMultilingual {
            if let resolved = resolveLanguage(language) {
                tokens.append(resolved)
            }
            switch task.lowercased() {
            case "translate":
                if let id = translateId { tokens.append(id) }
            default:
                if let id = transcribeId { tokens.append(id) }
            }
        }
        tokens.append(noTimestampsId)
        return tokens
    }

    public func resolveLanguage(_ language: String?) -> Int? {
        guard let raw = language?.trimmingCharacters(in: .whitespacesAndNewlines),
              !raw.isEmpty else { return nil }
        let normalized = Self.normalizeLanguageKey(raw)
        if let direct = languageToId[normalized] {
            return direct
        }
        if let mapped = Self.fullNameToCode[normalized],
           let id = languageToId[mapped] {
            return id
        }
        return nil
    }

    /// Decode token IDs to text, stripping every Whisper special / timestamp token.
    /// `dropIncompleteTail` : voir `TiktokenDecoder.decode`.
    public func decode(tokens: [Int], dropIncompleteTail: Bool = false) -> String {
        let filtered = tokens.filter { id in
            id >= 0 && id < timestampBeginId && !specialTokenIds.contains(id)
        }
        return inner.decode(filtered, skipSpecial: true, dropIncompleteTail: dropIncompleteTail)
    }

    private static func normalizeLanguageKey(_ key: String) -> String {
        var k = key.lowercased()
        if k.hasPrefix("<|"), k.hasSuffix("|>") {
            k = String(k.dropFirst(2).dropLast(2))
        }
        return k
    }

    private static let fullNameToCode: [String: String] = [
        "english": "en", "chinese": "zh", "mandarin": "zh", "cantonese": "yue",
        "japanese": "ja", "korean": "ko", "french": "fr", "german": "de",
        "spanish": "es", "italian": "it", "portuguese": "pt", "russian": "ru",
        "polish": "pl", "turkish": "tr", "dutch": "nl", "arabic": "ar",
        "hindi": "hi", "indonesian": "id", "vietnamese": "vi", "thai": "th",
        "ukrainian": "uk", "swedish": "sv", "czech": "cs", "romanian": "ro",
        "hungarian": "hu", "danish": "da", "finnish": "fi", "norwegian": "no",
        "hebrew": "he", "greek": "el", "tagalog": "tl", "filipino": "tl",
        "malay": "ms", "tamil": "ta", "telugu": "te", "bengali": "bn",
        "urdu": "ur",
    ]
}
