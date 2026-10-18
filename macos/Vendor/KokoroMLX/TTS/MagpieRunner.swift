import Foundation
@preconcurrency import MLX

/// L'entrée de Sillage pour MagpieTTS (speech-swift, Apache-2.0). Ajout de
/// Sillage, hors des fichiers repris. Sous `MLXGate` dans l'app : le cache
/// des vocabulaires et la graine de MLX sont partagés.
///
/// Mémoire : les poids 8 bits sont déquantifiés en FP32 au chargement
/// (`MagpieWeightLoader`), ≈ 1,6 Go pour 357 M de paramètres.
public final class MagpieRunner: @unchecked Sendable {
    private let model: MagpieTTS
    public var sampleRate: Int { MagpieTTS.sampleRate }

    /// Lit aussi le dictionnaire anglais livré dans le framework : absent,
    /// `MagpieTTSError.missingFile` ici, plutôt que de l'anglais lu lettre à
    /// lettre ; et ses quelques centaines de ms ne tombent pas sur la
    /// première phrase.
    public static func load(directory: URL) throws -> MagpieRunner {
        try MagpieDictG2P.english.ensureLoaded()
        return MagpieRunner(model: try MagpieTTS.load(directory: directory))
    }

    private init(model: MagpieTTS) {
        self.model = model
    }

    /// NFC : en byT5, un « é » décomposé ferait deux octets inattendus.
    public static func prepare(_ text: String) -> String {
        text.precomposedStringWithCanonicalMapping
    }

    /// Trames permises : 4 par octet de texte, plus 50, sans dépasser le
    /// contexte du décodeur (`max_len` moins les trames du locuteur et BOS).
    /// L'amont coupait à 500 trames (≈ 23 s).
    public static func maxSteps(textBytes: Int, maxLen: Int, bakedT: Int) -> Int {
        min(maxLen - bakedT - 1, 4 * textBytes + 50)
    }

    /// Octets de texte par morceau : la borne de `maxSteps` (4 trames par
    /// octet + 50) reste sous le contexte du décodeur, donc un morceau n'est
    /// jamais coupé par le plafond de trames ; et bien sous les 2 048
    /// positions de l'encodeur de texte (lues hors table au-delà). 471 pour
    /// le paquet 357M (≈ 20 s de parole en français).
    public static func maxChunkBytes(maxLen: Int, bakedT: Int) -> Int {
        max(1, (maxLen - bakedT - 1 - 50) / 4)
    }

    /// Découpe un texte trop long en morceaux d'au plus `maxBytes` octets
    /// UTF-8 : après la dernière ponctuation forte (`. … ; ! ?`) suivie d'une
    /// espace, sinon après une ponctuation faible (`, : — – ) »`), sinon à
    /// la dernière espace ; en dernier recours (un « mot » plus long que la
    /// borne), entre deux caractères. Jamais au milieu d'un caractère.
    public static func chunks(_ text: String, maxBytes: Int) -> [String] {
        var rest = Substring(text).drop(while: \.isWhitespace)
        var out: [String] = []
        while !rest.isEmpty {
            if rest.utf8.count <= maxBytes {
                out.append(String(rest).trimmingCharacters(in: .whitespacesAndNewlines))
                break
            }
            var end = rest.startIndex
            var bytes = 0
            while end < rest.endIndex, bytes + rest[end].utf8.count <= maxBytes {
                bytes += rest[end].utf8.count
                end = rest.index(after: end)
            }
            if end == rest.startIndex { end = rest.index(after: end) }
            let cut = lastBreak(in: rest, before: end, after: ".…;!?")
                ?? lastBreak(in: rest, before: end, after: ",:—–)»")
                ?? lastSpace(in: rest, before: end)
                ?? end
            let piece = String(rest[..<cut]).trimmingCharacters(in: .whitespacesAndNewlines)
            if !piece.isEmpty { out.append(piece) }
            rest = rest[cut...].drop(while: \.isWhitespace)
        }
        return out.filter { !$0.isEmpty }
    }

    /// L'indice juste après le dernier signe de `marks` suivi d'une espace,
    /// avant `end`.
    private static func lastBreak(in s: Substring, before end: Substring.Index,
                                  after marks: String) -> Substring.Index? {
        var i = end
        while i > s.startIndex {
            let prev = s.index(before: i)
            if marks.contains(s[prev]), i < s.endIndex, s[i].isWhitespace, prev > s.startIndex {
                return i
            }
            i = prev
        }
        return nil
    }

    private static func lastSpace(in s: Substring, before end: Substring.Index) -> Substring.Index? {
        var i = end
        while i > s.startIndex {
            let prev = s.index(before: i)
            if s[prev].isWhitespace, prev > s.startIndex { return prev }
            i = prev
        }
        return nil
    }

    /// Une phrase entière, 22 050 Hz mono. `voice` vide : sofia. `nil` :
    /// arrêtée en route. Échantillonnage d'origine (température 0,6,
    /// top-k 80), graine tirée de chaque morceau. Une phrase plus longue que
    /// `maxChunkBytes` est lue en morceaux (`chunks`) mis bout à bout ;
    /// `shouldStop` est aussi consulté entre deux morceaux.
    public func synthesize(text: String, language: String, voice: String,
                           shouldStop: @escaping () -> Bool) throws -> [Float]? {
        let speaker: MagpieSpeaker
        if voice.isEmpty {
            speaker = .sofia
        } else if let named = MagpieSpeaker(named: voice) {
            speaker = named
        } else {
            throw MagpieTTSError.invalidConfig("voix inconnue « \(voice) »")
        }
        let lang: MagpieLanguage = language.prefix(2).lowercased() == "en" ? .english : .french
        let config = model.decoderConfig
        let limit = Self.maxChunkBytes(maxLen: config.maxLen, bakedT: config.bakedT)
        var samples: [Float] = []
        let parts = Self.chunks(Self.prepare(text), maxBytes: limit)
        if parts.isEmpty { throw MagpieTTSError.textEncodingFailed("texte vide") }
        do {
            for chunk in parts {
                if shouldStop() { return nil }
                var params = MagpieTTSParams()
                params.seed = StableSeed.of(chunk)
                params.maxSteps = Self.maxSteps(textBytes: chunk.utf8.count,
                                                maxLen: config.maxLen, bakedT: config.bakedT)
                samples += try model.synthesize(text: chunk, speaker: speaker, language: lang,
                                                params: params, shouldStop: shouldStop)
            }
        } catch MagpieTTSError.stopped {
            return nil
        }
        return samples
    }
}
