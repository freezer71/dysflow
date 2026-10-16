import Foundation
import KokoroMLX

/// La dictée par un modèle qui lit tout d'un bloc (Parakeet, Whisper,
/// Canary) : le micro garde tout ce qui est dit, et le modèle relit le tout
/// toutes les 1,5 s (`RollingTranscription`), un texte provisoire qui
/// remplace le précédent ; à la fin, une dernière lecture donne le texte
/// définitif.
///
/// Le micro s'ouvre et `.listening` part tout de suite ; le modèle se charge
/// pendant qu'on parle (rien n'est perdu : l'audio attend dans le tampon).
/// Le premier texte vient donc une fois le modèle chargé. Les lectures
/// passent par `SpeechModelRunner` (sous `MLXGate`, hors du fil principal).
@MainActor
final class MLXDictationEngine: DictationEngine {
    /// Entre deux relectures.
    static let interval: Duration = .milliseconds(1500)
    /// La boucle regarde l'heure à ce pas.
    static let tick: Duration = .milliseconds(100)
    /// Moins d'audio que cela (0,1 s) : rien à lire.
    static let minimumSamples = 1_600

    private let runner: SpeechModelRunner
    private let directory: URL
    private var capture: MicrophoneCapture?
    private var load: Task<Void, Error>?
    private var loop: Task<Void, Never>?
    private var events: (@MainActor @Sendable (DictationEvent) -> Void)?
    private var language = "fr"
    /// Change à chaque arrêt : une lecture d'une dictée finie ou annulée
    /// n'émet rien.
    private var generation = 0

    init(runner: SpeechModelRunner, directory: URL) {
        self.runner = runner
        self.directory = directory
    }

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        await cancel()
        let id = generation
        try await MicrophoneCapture.requirePermission()
        guard generation == id else { throw CancellationError() }
        let capture = try MicrophoneCapture.open(events: events)
        self.capture = capture
        self.events = events
        language = Self.languageCode(locale)
        events(.listening)

        let runner = runner, directory = directory
        let load = Task { try await runner.prepare(directory: directory) }
        self.load = load
        do {
            try await load.value
        } catch {
            if generation == id { await cancel() }
            throw DictationError.failed("le modèle ne se charge pas : \(error.localizedDescription)")
        }
        // Fini ou annulé pendant le chargement : `finish` fait la lecture.
        guard generation == id else { return }
        loop = rollingLoop(capture: capture, events: events, generation: id)
    }

    // L'état est vidé avant chaque attente : une dictée suivante, lancée
    // pendant cette attente, garde le sien.
    func finish() async {
        guard let capture, let events else { return }
        generation &+= 1
        capture.stop()
        let loop = loop, load = load, language = language
        clear()
        // La lecture en cours va au bout (son texte n'est plus montré).
        loop?.cancel()
        await loop?.value
        guard (try? await load?.value) != nil else { return }
        let samples = capture.samples()
        guard samples.count >= Self.minimumSamples else {
            events(.text("", isFinal: true))
            return
        }
        guard let text = try? await runner.transcribe(samples, language: language) else { return }
        events(.text(text, isFinal: true))
    }

    /// Rien n'est émis. Une lecture déjà partie sur le GPU va au bout (elle
    /// ne s'interrompt pas), sans effet.
    func cancel() async {
        generation &+= 1
        capture?.stop()
        loop?.cancel()
        clear()
    }

    private func clear() {
        capture = nil
        loop = nil
        load = nil
        events = nil
    }

    /// Toutes les 100 ms, l'heure ; toutes les 1,5 s, une relecture de tout
    /// l'audio. La lecture est attendue dans la boucle : jamais deux à la fois.
    private func rollingLoop(capture: MicrophoneCapture,
                             events: @escaping @MainActor @Sendable (DictationEvent) -> Void,
                             generation id: Int) -> Task<Void, Never> {
        let runner = runner, language = language
        return Task { @MainActor [weak self] in
            var rolling = RollingTranscription(interval: Self.interval)
            while !Task.isCancelled, self?.generation == id {
                if rolling.shouldRead(now: .now, busy: false) {
                    let samples = capture.samples()
                    if samples.count >= Self.minimumSamples,
                       let text = try? await runner.transcribe(samples, language: language) {
                        guard !Task.isCancelled, self?.generation == id else { return }
                        events(.text(text, isFinal: false))
                    }
                }
                try? await Task.sleep(for: Self.tick)
            }
        }
    }

    /// Le code ISO à deux lettres (« fr », « en ») : celui que Whisper et
    /// Canary attendent, jamais « fr-FR ».
    static func languageCode(_ locale: Locale) -> String {
        locale.language.languageCode?.identifier ?? "fr"
    }
}

extension MicrophoneCapture {
    /// Ouvre un micro dont le spectre part vers la fenêtre.
    @MainActor
    static func open(events: @escaping @MainActor @Sendable (DictationEvent) -> Void) throws -> MicrophoneCapture {
        let capture = MicrophoneCapture(spectrum: { columns in
            Task { @MainActor in events(.spectrum(columns)) }
        })
        try capture.start()
        return capture
    }
}

// MARK: - Les modèles

/// Charge un modèle de dictée d'après son dossier (`models/<id>`), sous
/// `MLXGate` (le chargeur de `SpeechModelRunner`).
///
/// Plafonds de MLX : globaux au processus. Kokoro pose les siens (1 Go,
/// cache 64 Mo) à son chargement seulement ; ils étoufferaient un modèle de
/// dictée (Voxtral pèse 3 Go). Un modèle de dictée pose donc les siens à son
/// chargement et à chaque lecture (Kokoro a pu se charger entre-temps) :
/// mémoire = poids + 2 Go de marge, cache 256 Mo. Libéré, il remet ceux de
/// Kokoro (`release`). Entre les deux, une synthèse de Kokoro tourne sous
/// les plafonds de la dictée, plus larges : sans effet sur elle.
enum SpeechModels {
    static let cacheLimit = 256 << 20
    static let memoryMargin = 2 << 30

    static func load(_ directory: URL) throws -> LoadedSpeechModel {
        let memory = weightBytes(directory) + memoryMargin
        DictationMLX.setLimits(memory: memory, cache: cacheLimit)
        let id = directory.lastPathComponent
        switch id {
        case "parakeet_tdt_v3":
            return ParakeetSpeech(try ParakeetModel.fromDirectory(directory), memoryLimit: memory)
        case "canary_1b_v2_q8":
            return CanarySpeech(try CanaryModel.fromModelDirectory(directory), memoryLimit: memory)
        case "voxtral_realtime_4bit":
            return VoxtralSpeech(try VoxtralRealtimeModel.fromDirectory(directory), memoryLimit: memory)
        case _ where id.hasPrefix("whisper_"):
            return WhisperSpeech(try WhisperModel.fromDirectory(directory), memoryLimit: memory)
        default:
            throw DictationError.failed("modèle de dictée inconnu : \(id)")
        }
    }

    /// Après la libération du modèle (sous `MLXGate`) : cache vidé, plafonds
    /// de Kokoro remis.
    static func release() {
        KokoroRunner.clearCache()
        DictationMLX.setLimits(memory: KokoroMLXCompute.memoryLimit, cache: KokoroMLXCompute.cacheLimit)
    }

    /// Avant chaque lecture (sous `MLXGate`).
    static func applyLimits(memory: Int) {
        DictationMLX.setLimits(memory: memory, cache: cacheLimit)
    }

    /// Octets des poids (`*.safetensors`) du dossier.
    static func weightBytes(_ directory: URL) -> Int {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.fileSizeKey])) ?? []
        return files.filter { $0.pathExtension == "safetensors" }
            .reduce(0) { $0 + ((try? $1.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0) }
    }
}

extension STTGenerateParameters {
    /// Les réglages par défaut d'un modèle, la langue et le plafond de jetons
    /// changés (les champs sont des `let`).
    func with(language: String?, maxTokens: Int? = nil) -> STTGenerateParameters {
        STTGenerateParameters(maxTokens: maxTokens ?? self.maxTokens, temperature: temperature, topP: topP,
                              topK: topK, verbose: verbose, language: language,
                              chunkDuration: chunkDuration, minChunkDuration: minChunkDuration,
                              repetitionPenalty: repetitionPenalty, repetitionContextSize: repetitionContextSize,
                              kvBits: kvBits, kvGroupSize: kvGroupSize, quantizedKVStart: quantizedKVStart)
    }
}

/// Parakeet v3 reconnaît la langue tout seul : `language` est ignoré.
final class ParakeetSpeech: LoadedSpeechModel {
    private let model: ParakeetModel
    private let memoryLimit: Int

    init(_ model: ParakeetModel, memoryLimit: Int) {
        self.model = model
        self.memoryLimit = memoryLimit
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits(memory: memoryLimit)
        return model.generate(samples: samples, generationParameters: model.defaultGenerationParameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Whisper : la langue est donnée (« fr », « en »), pas devinée.
final class WhisperSpeech: LoadedSpeechModel {
    private let model: WhisperModel
    private let memoryLimit: Int

    init(_ model: WhisperModel, memoryLimit: Int) {
        self.model = model
        self.memoryLimit = memoryLimit
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits(memory: memoryLimit)
        let parameters = model.defaultGenerationParameters.with(language: language)
        return model.generate(samples: samples, generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Canary : la langue est obligatoire (source et cible). Son vocabulaire
/// garde des jetons spéciaux (`<|…|>`) que le décodage laisse passer : ils
/// sont retirés.
final class CanarySpeech: LoadedSpeechModel {
    /// Le décodeur a 1 024 positions au plus.
    static let maxTokens = 1024
    private static let special = try! NSRegularExpression(pattern: #"<\|[^|]*\|>"#)

    private let model: CanaryModel
    private let memoryLimit: Int

    init(_ model: CanaryModel, memoryLimit: Int) {
        self.model = model
        self.memoryLimit = memoryLimit
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits(memory: memoryLimit)
        let parameters = model.defaultGenerationParameters.with(language: language, maxTokens: Self.maxTokens)
        return Self.clean(model.generate(samples: samples, generationParameters: parameters).text)
    }

    static func clean(_ text: String) -> String {
        special.stringByReplacingMatches(in: text, range: NSRange(text.startIndex..., in: text), withTemplate: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Voxtral Realtime : la dictée passe par une session de flux
/// (`VoxtralDictationEngine`) ; `transcribe` lit tout d'un bloc. Multilingue
/// sans invite : `language` est ignoré.
final class VoxtralSpeech: LoadedSpeechModel {
    private let model: VoxtralRealtimeModel
    private let memoryLimit: Int

    init(_ model: VoxtralRealtimeModel, memoryLimit: Int) {
        self.model = model
        self.memoryLimit = memoryLimit
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits(memory: memoryLimit)
        return model.generate(samples: samples, generationParameters: model.defaultGenerationParameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Une session neuve, retard `delayMs` (sous `MLXGate`).
    func makeSession(delayMs: Int) -> VoxtralSession {
        SpeechModels.applyLimits(memory: memoryLimit)
        return VoxtralSession(model.makeStreamSession(transcriptionDelayMs: delayMs))
    }
}

/// Une session Voxtral, gardée par le moteur de dictée ; elle ne se touche
/// que sous `MLXGate` (dans `SpeechModelRunner.withModel`), un appel à la
/// fois.
final class VoxtralSession: @unchecked Sendable {
    private let session: VoxtralRealtimeStreamSession

    init(_ session: VoxtralRealtimeStreamSession) {
        self.session = session
    }

    /// Le texte nouveau depuis le dernier appel.
    func step(_ samples: [Float]) -> String {
        session.step(samples).text
    }

    /// Ferme le flux ; le texte entier.
    func finish() -> String {
        session.finish()
        return session.text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
