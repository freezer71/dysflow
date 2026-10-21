// `SpeechAnalyzer` n'existe qu'à partir du SDK de macOS 26 (Swift 6.2) :
// compilée avec un Xcode plus ancien, l'app n'a pas de dictée.
#if compiler(>=6.2)
import AVFoundation
import os
import Speech

/// La dictée de macOS 26, sur l'appareil : le micro (`AVAudioEngine`) passe
/// au transcripteur (`SpeechTranscriber`, sinon `DictationTranscriber` sur
/// un Mac qui n'a pas le premier), qui rend ses résultats au fil de l'eau.
///
/// Le micro s'ouvre avant que l'analyse soit prête : les premiers mots
/// attendent dans la file, aucun n'est perdu. Il arrive par un nœud
/// récepteur (`AVAudioSinkNode`), à chaque cycle audio (~10 ms) : un
/// robinet (`installTap`) le livrerait par paquets de 100 ms au moins, et
/// chaque mot s'afficherait d'autant plus tard.
@available(macOS 26, *)
@MainActor
final class SystemDictationEngine: DictationEngine {
    private var audio: AVAudioEngine?
    private var analyzer: SpeechAnalyzer?
    private var input: AsyncStream<AnalyzerInput>.Continuation?
    private var reader: Task<Void, Never>?
    /// Change à chaque arrêt : un démarrage encore en route s'arrête là, sans
    /// ouvrir le micro d'une dictée annulée.
    private var generation = 0
    /// Ce qui a été résolu pour une langue (le transcripteur, sa langue, son
    /// format, la langue installée) : les dictées suivantes ne le redemandent
    /// pas à macOS.
    private var prepared: [String: Prepared] = [:]

    private struct Prepared {
        let locale: Locale
        let speech: Bool
        let format: AVAudioFormat
    }

    nonisolated static let log = Logger(subsystem: "fr.sillage.app", category: "dictee")

    /// Au-delà, la fin de l'analyse est abandonnée : la saisie n'attend pas.
    static let finishDeadline: Duration = .seconds(3)

    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        await cancel()
        let id = generation
        func stillWanted() throws { if generation != id { throw CancellationError() } }
        let began = ContinuousClock.now
        try await MicrophoneCapture.requirePermission()
        try stillWanted()
        let ready: Prepared
        if let known = prepared[locale.identifier] {
            ready = known
        } else {
            ready = try await Self.prepare(locale, events: events)
            prepared[locale.identifier] = ready
        }
        try stillWanted()
        let module = Self.module(ready)
        let format = ready.format
        let (stream, continuation) = AsyncStream.makeStream(of: AnalyzerInput.self)
        let audio = AVAudioEngine()
        // Avant le format : des écouteurs Bluetooth cèdent au micro du Mac.
        MicrophoneChoice.apply(to: audio)
        let node = audio.inputNode
        let natural = node.outputFormat(forBus: 0)
        guard natural.sampleRate > 0, natural.channelCount > 0 else { throw DictationError.noMicrophone }
        guard let tap = AudioTap(from: natural, to: format, input: continuation,
                                 spectrum: { columns in Task { @MainActor in events(.spectrum(columns)) } })
        else { throw DictationError.failed(String(localized: "unsupported audio format")) }
        let sink = AVAudioSinkNode(receiverBlock: tap.receiver())
        audio.attach(sink)
        audio.connect(node, to: sink, format: natural)
        audio.prepare()
        do {
            try audio.start()
        } catch {
            throw DictationError.failed(error.localizedDescription)
        }
        Self.log.info("micro ouvert en \(Int((ContinuousClock.now - began) / .milliseconds(1)), privacy: .public) ms")
        self.audio = audio
        input = continuation
        events(.listening)

        reader = Self.forward(module, to: events)
        let analyzer = SpeechAnalyzer(modules: [module])
        self.analyzer = analyzer
        do {
            try await analyzer.start(inputSequence: stream)
        } catch {
            guard generation == id else { throw CancellationError() }
            await cancel()
            throw DictationError.failed(error.localizedDescription)
        }
    }

    // L'état est vidé avant chaque attente : une dictée suivante, lancée
    // pendant cette attente, garde le sien.
    func finish() async {
        stopAudio()
        guard let analyzer else { return }
        let reader = reader
        clear()
        // Passé le délai, l'analyse s'arrête net : ce qui est arrivé reste.
        let deadline = Task { @MainActor in
            try? await Task.sleep(for: Self.finishDeadline)
            if !Task.isCancelled { await analyzer.cancelAndFinishNow() }
        }
        try? await analyzer.finalizeAndFinishThroughEndOfInput()
        await reader?.value
        deadline.cancel()
    }

    func cancel() async {
        stopAudio()
        reader?.cancel()
        let analyzer = analyzer
        clear()
        if let analyzer { await analyzer.cancelAndFinishNow() }
    }

    private func stopAudio() {
        generation &+= 1
        audio?.stop()
        audio = nil
        input?.finish()
        input = nil
    }

    private func clear() {
        reader = nil
        analyzer = nil
    }

    // MARK: - Préparation

    /// Une fois par langue : le meilleur transcripteur que ce Mac sait faire
    /// tourner, la langue installée, le format qu'il attend.
    private static func prepare(_ locale: Locale,
                                events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws -> Prepared {
        let speech: Bool
        let supported: Locale
        if SpeechTranscriber.isAvailable,
           let found = await SpeechTranscriber.supportedLocale(equivalentTo: locale) {
            (speech, supported) = (true, found)
        } else if let found = await DictationTranscriber.supportedLocale(equivalentTo: locale) {
            (speech, supported) = (false, found)
        } else {
            throw DictationError.languageUnavailable
        }
        let probe = module(Prepared(locale: supported, speech: speech, format: AVAudioFormat()))
        try await install(probe, events: events)
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [probe]) else {
            throw DictationError.languageUnavailable
        }
        return Prepared(locale: supported, speech: speech, format: format)
    }

    /// Un transcripteur neuf (un par dictée).
    private static func module(_ ready: Prepared) -> any SpeechModule {
        ready.speech
            ? SpeechTranscriber(locale: ready.locale, preset: .progressiveTranscription)
            : DictationTranscriber(locale: ready.locale, preset: .progressiveLongDictation)
    }

    /// La langue s'installe la première fois (macOS la télécharge et la
    /// garde) ; l'avancement se montre pendant ce temps.
    private static func install(_ module: any SpeechModule,
                                events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws {
        let request: AssetInstallationRequest?
        do {
            request = try await AssetInventory.assetInstallationRequest(supporting: [module])
        } catch {
            throw DictationError.languageUnavailable
        }
        // Déjà installée : rien à montrer (la demande se règle en un instant).
        guard let request else { return }
        if await AssetInventory.status(forModules: [module]) == .installed {
            try? await request.downloadAndInstall()
            return
        }
        events(.downloading(0))
        let progress = request.progress
        let watcher = Task { @MainActor in
            while !Task.isCancelled {
                events(.downloading(progress.fractionCompleted))
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
        defer { watcher.cancel() }
        do {
            try await request.downloadAndInstall()
        } catch {
            throw DictationError.failed(String(localized: "the language couldn’t be installed"))
        }
    }

    /// Les résultats du transcripteur, rendus en texte.
    private static func forward(_ module: any SpeechModule,
                                to events: @escaping @MainActor @Sendable (DictationEvent) -> Void) -> Task<Void, Never> {
        Task { @MainActor in
            do {
                if let transcriber = module as? SpeechTranscriber {
                    for try await result in transcriber.results {
                        events(.text(String(result.text.characters), isFinal: result.isFinal))
                    }
                } else if let transcriber = module as? DictationTranscriber {
                    for try await result in transcriber.results {
                        events(.text(String(result.text.characters), isFinal: result.isFinal))
                    }
                }
            } catch {
                // Annulé, ou l'analyse s'est arrêtée : ce qui est arrivé reste.
            }
        }
    }
}

/// Ce qui reçoit le micro, appelé sur le fil audio : chaque tampon est
/// converti au format du transcripteur et mis dans sa file, et son spectre
/// part vers l'affichage.
@available(macOS 26, *)
private final class AudioTap: @unchecked Sendable {
    private let converter: AVAudioConverter
    private let output: AVAudioFormat
    private let input: AsyncStream<AnalyzerInput>.Continuation
    private let spectrum: @Sendable ([[Float]]) -> Void
    private let analyzer: SpectrumAnalyzer
    private let natural: AVAudioFormat
    /// Pour le journal : la taille réelle des paquets du micro.
    private var calls = 0
    private var frames = 0

    init?(from natural: AVAudioFormat, to output: AVAudioFormat,
          input: AsyncStream<AnalyzerInput>.Continuation, spectrum: @escaping @Sendable ([[Float]]) -> Void) {
        guard let converter = AVAudioConverter(from: natural, to: output) else { return nil }
        // Pas d'amorce : la conversion suit le micro sans retard.
        converter.primeMethod = .none
        self.converter = converter
        self.output = output
        self.input = input
        self.spectrum = spectrum
        self.natural = natural
        analyzer = SpectrumAnalyzer(sampleRate: natural.sampleRate)
    }

    deinit {
        guard calls > 0 else { return }
        let rate = natural.sampleRate
        SystemDictationEngine.log.info(
            "micro : \(self.calls, privacy: .public) paquets de \(Int(Double(self.frames) / Double(self.calls) / rate * 1000), privacy: .public) ms en moyenne")
    }

    /// Construit hors de l'acteur principal : le bloc tourne sur le fil
    /// audio temps réel, à chaque cycle.
    nonisolated func receiver() -> AVAudioSinkNodeReceiverBlock {
        { [self] _, frames, list in
            guard frames > 0,
                  let buffer = AVAudioPCMBuffer(pcmFormat: natural, bufferListNoCopy: list, deallocator: nil)
            else { return noErr }
            buffer.frameLength = frames
            receive(buffer)
            return noErr
        }
    }

    private func receive(_ buffer: AVAudioPCMBuffer) {
        calls += 1
        frames += Int(buffer.frameLength)
        if let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 {
            let columns = analyzer.push(UnsafeBufferPointer(start: samples, count: Int(buffer.frameLength)))
            if !columns.isEmpty { spectrum(columns) }
        }
        let ratio = output.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
        guard let converted = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        let status = converter.convert(to: converted, error: &error) { _, inputStatus in
            if fed {
                inputStatus.pointee = .noDataNow
                return nil
            }
            fed = true
            inputStatus.pointee = .haveData
            return buffer
        }
        guard status != .error, converted.frameLength > 0 else { return }
        input.yield(AnalyzerInput(buffer: converted))
    }
}
#endif
