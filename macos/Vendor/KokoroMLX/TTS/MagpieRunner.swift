import Foundation
@preconcurrency import MLX

/// L'entrée de Sillon pour MagpieTTS (speech-swift, Apache-2.0). Ajout de
/// Sillon, hors des fichiers repris. Sous `MLXGate` dans l'app : le cache
/// des vocabulaires et la graine de MLX sont partagés.
public final class MagpieRunner: @unchecked Sendable {
    private let model: MagpieTTS
    public var sampleRate: Int { MagpieTTS.sampleRate }

    public static func load(directory: URL) throws -> MagpieRunner {
        MagpieRunner(model: try MagpieTTS.load(directory: directory))
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

    /// Une phrase entière, 22 050 Hz mono. `voice` vide : sofia. `nil` :
    /// arrêtée en route. Échantillonnage d'origine (température 0,6,
    /// top-k 80), graine tirée de la phrase.
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
        let text = Self.prepare(text)
        var params = MagpieTTSParams()
        params.seed = StableSeed.of(text)
        params.maxSteps = Self.maxSteps(textBytes: text.utf8.count,
                                        maxLen: model.decoderConfig.maxLen,
                                        bakedT: model.decoderConfig.bakedT)
        do {
            return try model.synthesize(text: text, speaker: speaker, language: lang,
                                        params: params, shouldStop: shouldStop)
        } catch MagpieTTSError.stopped {
            return nil
        }
    }
}
