import Foundation
import KokoroMLX

/// La dictée par un modèle qui lit tout d'un bloc (Parakeet, Whisper,
/// Canary, Qwen3-ASR, Cohere) : le micro garde tout ce qui est dit, et le modèle relit le tout
/// toutes les 0,4 s au moins, à trois fois sa durée de lecture
/// (`RollingTranscription`), un
/// texte provisoire qui remplace le précédent, avec son délai ; à la fin,
/// une dernière lecture donne le texte définitif.
///
/// Le micro s'ouvre et `.listening` part tout de suite ; le modèle se charge
/// pendant qu'on parle (rien n'est perdu : l'audio attend dans le tampon).
/// Le premier texte vient donc une fois le modèle chargé. Les lectures
/// passent par `SpeechModelRunner` (sous `MLXGate`, hors du fil principal).
@MainActor
final class MLXDictationEngine: DictationEngine {
    /// Entre deux relectures, avant la première mesure.
    static let interval: Duration = RollingTranscription.initial
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
        // Avant tout : un `finish` arrivé avant l'ouverture du micro arrête
        // aussi le démarrage en cours.
        generation &+= 1
        guard let capture, let events else { return }
        capture.stop()
        let loop = loop, load = load, language = language
        clear()
        // La lecture en cours va au bout (son texte n'est plus montré).
        loop?.cancel()
        await loop?.value
        guard (try? await load?.value) != nil else { return }
        // Comme Voxtral : les tampons gardés par MLX pendant la dictée sont
        // rendus, une fois le texte définitif lu.
        defer { releaseCache() }
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
        let hadCapture = capture != nil
        capture?.stop()
        loop?.cancel()
        clear()
        if hadCapture { releaseCache() }
    }

    /// Vide le cache de MLX (sous `MLXGate`, après la lecture en cours) ;
    /// le modèle reste chargé.
    private func releaseCache() {
        let runner = runner
        Task { await runner.underGate { DictationMLX.clearCache() } }
    }

    private func clear() {
        capture = nil
        loop = nil
        load = nil
        events = nil
    }

    /// Toutes les 100 ms, l'heure ; au rythme de `RollingTranscription`, une
    /// relecture de tout l'audio, dont la durée règle le rythme et donne le
    /// délai. La lecture est attendue dans la boucle : jamais deux à la fois.
    private func rollingLoop(capture: MicrophoneCapture,
                             events: @escaping @MainActor @Sendable (DictationEvent) -> Void,
                             generation id: Int) -> Task<Void, Never> {
        let runner = runner, language = language
        return Task { @MainActor [weak self] in
            var rolling = RollingTranscription(interval: Self.interval)
            while !Task.isCancelled, self?.generation == id {
                // Les relectures peuvent s'espacer de plusieurs secondes : le
                // modèle n'est jamais libéré au milieu d'une dictée.
                runner.keepAlive()
                if rolling.shouldRead(now: .now, busy: false) {
                    let samples = capture.samples()
                    let clock = ContinuousClock(), start = clock.now
                    if samples.count >= Self.minimumSamples,
                       let text = try? await runner.transcribe(samples, language: language) {
                        guard !Task.isCancelled, self?.generation == id else { return }
                        let read = clock.now - start
                        rolling.adapt(lastRead: read)
                        events(.latency(read))
                        events(.text(text, isFinal: false))
                        // Sous `MLXGate`, hors du fil principal : jamais pendant un calcul.
                        let active = await runner.underGate { DictationMLX.counters().active }
                        guard !Task.isCancelled, self?.generation == id else { return }
                        events(.memory(active))
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
/// Plafonds de MLX : globaux au processus, ils valent la somme des besoins
/// des modèles chargés (`MLXBudget`). Un modèle de dictée y inscrit le sien
/// à son chargement (clé `"dictee"`) : mémoire = poids + 2 Go de marge,
/// cache 256 Mo. Libéré, il le retire : les plafonds redeviennent la somme
/// de ce qui reste chargé (Kokoro, un modèle de lecture), jamais ceux de
/// Kokoro remis en dur.
enum SpeechModels {
    static let cacheLimit = 256 << 20
    static let memoryMargin = 2 << 30

    static func load(_ directory: URL) throws -> LoadedSpeechModel {
        MLXBudget.shared.set("dictee", .init(memory: weightBytes(directory) + memoryMargin, cache: cacheLimit))
        do {
            return try loadModel(directory)
        } catch {
            MLXBudget.shared.remove("dictee")
            throw error
        }
    }

    private static func loadModel(_ directory: URL) throws -> LoadedSpeechModel {
        let id = directory.lastPathComponent
        switch id {
        case "parakeet_tdt_v3", "parakeet_tdt_v2":
            return ParakeetSpeech(try ParakeetModel.fromDirectory(directory))
        case "phonon_2":
            return ParakeetSpeech(try ParakeetModel.fromPhonon(directory))
        case "canary_1b_v2_q8":
            return CanarySpeech(try CanaryModel.fromModelDirectory(directory))
        case "voxtral_realtime_4bit":
            return VoxtralSpeech(try VoxtralRealtimeModel.fromDirectory(directory))
        case "nemotron_35_asr_streaming_8bit":
            return NemotronSpeech(try NemotronASRModel.fromDirectory(directory))
        case "qwen3_asr_06b_8bit", "qwen3_asr_17b_4bit":
            return Qwen3ASRSpeech(try Qwen3ASRModel.fromModelDirectory(directory))
        case "cohere_transcribe_8bit":
            return CohereSpeech(try CohereTranscribeModel.fromDirectory(directory))
        case _ where id.hasPrefix("whisper_"):
            return WhisperSpeech(try WhisperModel.fromDirectory(directory))
        default:
            throw DictationError.failed("modèle de dictée inconnu : \(id)")
        }
    }

    /// Après la libération du modèle (sous `MLXGate`) : cache vidé, besoin
    /// de la dictée retiré.
    static func release() {
        KokoroRunner.clearCache()
        MLXBudget.shared.remove("dictee")
    }

    /// Avant chaque lecture (sous `MLXGate`) : la somme des besoins reposée
    /// (un modèle a pu se charger ou se libérer entre-temps).
    static func applyLimits() {
        MLXBudget.shared.applyNow()
    }

    /// Octets des poids du dossier (`MLXBudget.weightBytes`).
    static func weightBytes(_ directory: URL) -> Int {
        MLXBudget.weightBytes(directory)
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

/// Parakeet v3 (et Phonon-2, son dérivé) reconnaît la langue tout seul :
/// `language` est ignoré. Parakeet v2, même moteur, ne connaît que
/// l'anglais : la langue n'y change rien non plus.
final class ParakeetSpeech: LoadedSpeechModel {
    private let model: ParakeetModel
    init(_ model: ParakeetModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        return model.generate(samples: samples, generationParameters: model.defaultGenerationParameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Whisper : la langue est donnée (« fr », « en »), pas devinée.
///
/// Whisper lit par fenêtres de 30 s à positions fixes (0, 30 s, 60 s…),
/// chacune seule, et colle leurs textes. Une relecture glissante ne change
/// que la dernière : le texte des fenêtres complètes déjà lues est gardé
/// (avec une empreinte de leur audio) et seule la suite est relue. Même
/// découpage, même collage que `WhisperModel.generate`, décodage glouton :
/// le texte est identique à une lecture de tout.
final class WhisperSpeech: LoadedSpeechModel {
    private let model: WhisperModel
    /// Le texte des fenêtres complètes déjà lues, par rang.
    private var read: [Int: ReadWindow] = [:]

    private struct ReadWindow {
        let fingerprint: Int
        let language: String
        let text: String
    }

    init(_ model: WhisperModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        let window = WhisperAudioConfig.chunkLengthSamples
        guard samples.count > window else {
            read = [:]
            return readWindow(samples, language: language)
        }
        var texts: [String] = []
        var index = 0
        for start in stride(from: 0, to: samples.count, by: window) {
            let slice = samples[start..<min(start + window, samples.count)]
            let text: String
            if slice.count == window {
                let fingerprint = Self.fingerprint(slice)
                if let known = read[index], known.fingerprint == fingerprint, known.language == language {
                    text = known.text
                } else {
                    text = readWindow(Array(slice), language: language)
                    read[index] = ReadWindow(fingerprint: fingerprint, language: language, text: text)
                }
            } else {
                text = readWindow(Array(slice), language: language)
            }
            if !text.isEmpty { texts.append(text) }
            index += 1
        }
        read = read.filter { $0.key < index }
        return texts.joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// 30 s au plus : une seule fenêtre, son texte sans espaces autour.
    private func readWindow(_ samples: [Float], language: String) -> String {
        let parameters = model.defaultGenerationParameters.with(language: language)
        return model.generate(samples: samples, generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// L'empreinte des octets de l'audio (au bit près).
    static func fingerprint(_ samples: ArraySlice<Float>) -> Int {
        var hasher = Hasher()
        samples.withUnsafeBytes { hasher.combine(bytes: $0) }
        return hasher.finalize()
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
    init(_ model: CanaryModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        let parameters = model.defaultGenerationParameters.with(language: language, maxTokens: Self.maxTokens)
        return Self.clean(model.generate(samples: samples, generationParameters: parameters).text)
    }

    static func clean(_ text: String) -> String {
        special.stringByReplacingMatches(in: text, range: NSRange(text.startIndex..., in: text), withTemplate: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Qwen3-ASR (0.6B et 1.7B) : la langue n'est jamais imposée, le modèle la
/// reconnaît seul (il écrit « language French<asr_text>… », dont seul le
/// texte est gardé). `language` est ignoré.
final class Qwen3ASRSpeech: LoadedSpeechModel {
    private let model: Qwen3ASRModel
    init(_ model: Qwen3ASRModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        let parameters = model.defaultGenerationParameters.with(language: nil)
        return model.generate(samples: samples, generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Cohere Transcribe : la langue est imposée (celle du réglage « Langue »
/// de la dictée, « fr » ou « en ») ; il ne la devine pas.
///
/// L'audio est lu par tranches de 30 s au plus, coupées aux silences
/// (`splitAudioIntoChunks`) : le modèle a appris sur 35 s au plus
/// (`max_audio_clip_s`) et son encodeur ne voit pas au-delà de 6,7 min.
/// Le plafond de jetons vaut pour toute la dictée (il est partagé entre
/// les tranches) : il est levé, chaque tranche restant bornée par les
/// 1 024 positions du décodeur.
final class CohereSpeech: LoadedSpeechModel {
    static let chunkSeconds: Float = 30
    static let maxTokens = 1 << 20

    private let model: CohereTranscribeModel
    init(_ model: CohereTranscribeModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        let base = model.defaultGenerationParameters
        let parameters = STTGenerateParameters(
            maxTokens: Self.maxTokens, temperature: base.temperature, topP: base.topP, topK: base.topK,
            verbose: false, language: language, chunkDuration: Self.chunkSeconds,
            minChunkDuration: base.minChunkDuration)
        // Les tranches sont jointes par des sauts de ligne : une seule ligne
        // dans le texte dicté.
        return model.generate(samples: samples, generationParameters: parameters).text
            .split(whereSeparator: \.isNewline).joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Un modèle qui dicte au fil de la parole (`StreamingDictationEngine`) :
/// il ouvre une session de flux, sous `MLXGate`.
protocol StreamingSpeechModel: LoadedSpeechModel {
    /// Une session neuve ; `language` : « fr », « en ».
    func makeSession(language: String) -> SpeechStreamSession
}

/// Une session de flux, gardée par le moteur de dictée ; elle ne se touche
/// que sous `MLXGate` (dans `SpeechModelRunner.withModel`), un appel à la
/// fois.
protocol SpeechStreamSession: AnyObject, Sendable {
    /// Le texte nouveau depuis le dernier appel.
    func step(_ samples: [Float]) -> String
    /// Ferme le flux ; le texte entier.
    func finish() -> String
}

/// Voxtral Realtime : la dictée passe par une session de flux
/// (`StreamingDictationEngine`) ; `transcribe` lit tout d'un bloc.
/// Multilingue sans invite : `language` est ignoré.
final class VoxtralSpeech: StreamingSpeechModel {
    /// Le retard du texte sur la voix (le défaut du modèle).
    static let delayMs = 480

    private let model: VoxtralRealtimeModel
    init(_ model: VoxtralRealtimeModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        return model.generate(samples: samples, generationParameters: model.defaultGenerationParameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Une session neuve, retard `delayMs` (sous `MLXGate`).
    func makeSession(language: String) -> SpeechStreamSession {
        SpeechModels.applyLimits()
        return VoxtralSession(model.makeStreamSession(transcriptionDelayMs: Self.delayMs))
    }
}

final class VoxtralSession: SpeechStreamSession, @unchecked Sendable {
    private let session: VoxtralRealtimeStreamSession

    init(_ session: VoxtralRealtimeStreamSession) {
        self.session = session
    }

    func step(_ samples: [Float]) -> String {
        session.step(samples).text
    }

    func finish() -> String {
        session.finish()
        return session.text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Nemotron 3.5 ASR Streaming : un vrai flux (encodeur à caches et RNN-T
/// glouton, `NemotronASRStreamSession`) ; chaque bout d'audio n'est lu
/// qu'une fois, au lieu de relire tout l'audio. La langue est passée en
/// invite (« fr », « en » : le dictionnaire d'invites du modèle les a) ;
/// `transcribe` lit tout d'un bloc (l'essai du micro peut s'en servir).
final class NemotronSpeech: StreamingSpeechModel {
    /// Tranches de 560 ms (contexte droit de 6 trames de 80 ms) : le texte
    /// suit la voix d'environ 0,6 s, comme Voxtral. Fiche du modèle, FLEURS
    /// français avec l'invite de langue : 9,45 % de mots faux à 560 ms,
    /// 9,03 % à 1,12 s (la tranche native, deux fois plus en retard),
    /// 10,97 % à 80 ms.
    static let chunkMs = 560

    private let model: NemotronASRModel
    init(_ model: NemotronASRModel) {
        self.model = model
    }

    func transcribe(_ samples: [Float], language: String) throws -> String {
        SpeechModels.applyLimits()
        let parameters = model.defaultGenerationParameters.with(language: language)
        return model.generate(samples: samples, generationParameters: parameters)
            .text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func makeSession(language: String) -> SpeechStreamSession {
        SpeechModels.applyLimits()
        return NemotronSession(model.makeStreamSession(language: language, chunkMs: Self.chunkMs))
    }
}

final class NemotronSession: SpeechStreamSession, @unchecked Sendable {
    private let session: NemotronASRStreamSession

    init(_ session: NemotronASRStreamSession) {
        self.session = session
    }

    func step(_ samples: [Float]) -> String {
        session.step(samples).text
    }

    func finish() -> String {
        session.finish()
        return session.text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
