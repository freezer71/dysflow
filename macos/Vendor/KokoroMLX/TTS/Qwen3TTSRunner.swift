import Foundation
@preconcurrency import MLX

/// L'entrée de Dysflow pour Qwen3-TTS, comme `KokoroRunner` pour Kokoro :
/// l'app ne lie pas MLX. Ajout de Dysflow, hors des fichiers repris. Tout
/// s'appelle sous `MLXGate` (dans l'app) : une synthèse à la fois.
public final class Qwen3TTSRunner: @unchecked Sendable {
    public static let sampleRate = 24_000
    public var sampleRate: Int { Self.sampleRate }

    private let model: Qwen3TTSModel
    /// Base : le x-vector figé de la voix « siwis ». CustomVoice : `nil`.
    private let speakerEmbedding: MLXArray?
    /// Les voix acceptées : `siwis` (Base), ou les locuteurs de `spk_id`.
    public let voices: [String]

    /// Les fichiers que le chargement lit (relevé §1.e). Refusé avant toute
    /// lecture s'il en manque un : rien n'est créé, modifié ni effacé dans
    /// `directory` (le catalogue Rust en est seul maître).
    public static func checkFiles(_ directory: URL) throws {
        let names = ["config.json", "model.safetensors", "vocab.json", "merges.txt", "tokenizer_config.json",
                     "speech_tokenizer/config.json", "speech_tokenizer/model.safetensors"]
        for name in names {
            var isDirectory: ObjCBool = false
            let path = directory.appendingPathComponent(name).path
            guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), !isDirectory.boolValue else {
                throw Qwen3TTSError.missingFile(name)
            }
        }
    }

    public static func load(directory: URL) throws -> Qwen3TTSRunner {
        try checkFiles(directory)
        let model = try Qwen3TTSModel.fromModelDirectory(directory)
        guard let talker = model.config.talkerConfig else {
            throw Qwen3TTSError.modelNotInitialized("talker_config")
        }
        if model.config.ttsModelType == "base" {
            guard let url = Bundle(for: Qwen3TTSRunner.self).url(forResource: "qwen3-06b-siwis",
                                                                 withExtension: "safetensors") else {
                throw Qwen3TTSError.missingFile("qwen3-06b-siwis.safetensors")
            }
            guard let vector = try MLX.loadArrays(url: url)["speaker_embedding"],
                  vector.dim(-1) == talker.hiddenSize else {
                throw Qwen3TTSError.unknownVoice("siwis")
            }
            return Qwen3TTSRunner(model: model, speakerEmbedding: vector.reshaped([1, -1]), voices: ["siwis"])
        }
        let speakers = (talker.spkId ?? [:]).keys.sorted()
        guard !speakers.isEmpty else { throw Qwen3TTSError.modelNotInitialized("spk_id vide") }
        return Qwen3TTSRunner(model: model, speakerEmbedding: nil, voices: speakers)
    }

    private init(model: Qwen3TTSModel, speakerEmbedding: MLXArray?, voices: [String]) {
        self.model = model
        self.speakerEmbedding = speakerEmbedding
        self.voices = voices
    }

    /// `fr` → `french`, `en` → `english` (clés de `codec_language_id`) ;
    /// une autre langue : `auto`.
    public static func languageName(_ code: String) -> String {
        switch code.prefix(2).lowercased() {
        case "fr": return "french"
        case "en": return "english"
        default: return "auto"
        }
    }

    /// Une phrase entière, 24 kHz mono. `voice` vide : la première voix.
    /// `nil` : `shouldStop` a dit vrai en route. Réglages de
    /// `generation_config.json` : température 0,9, top-k 50, top-p 1,
    /// pénalité 1,05 ; graine tirée de la phrase.
    public func synthesize(text: String, language: String, voice: String,
                           shouldStop: @escaping () -> Bool) throws -> [Float]? {
        var speaker: String?
        if speakerEmbedding == nil {
            let name = voice.isEmpty ? voices[0] : voice.lowercased()
            guard voices.contains(name) else { throw Qwen3TTSError.unknownVoice(voice) }
            speaker = name
        }
        MLXRandom.seed(StableSeed.of(text))
        do {
            let audio = try model.generateVoiceDesign(
                text: text, instruct: speaker, language: Self.languageName(language),
                temperature: 0.9, topK: 50, topP: 1.0, repetitionPenalty: 1.05, minP: 0.0,
                maxTokens: 4096, speakerEmbedding: speakerEmbedding, shouldStop: shouldStop)
            eval(audio)
            return audio.reshaped([-1]).asType(.float32).asArray(Float.self)
        } catch Qwen3TTSError.stopped {
            return nil
        }
    }
}
