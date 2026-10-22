import AVFoundation
import os

/// Le micro pour les modèles de dictée : tout ce qui est dit, en 16 kHz
/// mono Float32 (le format de Parakeet, Canary, Voxtral et Whisper), gardé
/// en mémoire depuis `start`. Le spectre part vers l'affichage comme pour la
/// dictée de macOS.
///
/// Comme `SystemDictationEngine`, l'audio arrive par un nœud récepteur
/// (`AVAudioSinkNode`), à chaque cycle audio (~10 ms), sur le fil audio ; le
/// tampon est sous verrou, `samples()` et `drain()` se lisent de n'importe
/// quel fil. `start` et `stop` s'appellent d'un même fil (le principal).
/// L'autorisation du micro se demande avant `start` (`requirePermission`).
final class MicrophoneCapture: @unchecked Sendable {
    static let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000,
                                      channels: 1, interleaved: false)!

    private let spectrum: @Sendable ([[Float]]) -> Void
    private var stream: MicrophoneStream?
    private let buffer = OSAllocatedUnfairLock(initialState: Buffer())

    /// L'audio par morceaux d'une seconde au plus. Le fil audio n'ajoute
    /// qu'au dernier morceau : jamais de copie de tout le tampon sous le
    /// verrou, même quand une lecture vient d'en prendre une image (au pire,
    /// le dernier morceau, une seconde, est recopié).
    private struct Buffer {
        static let chunk = 16_000
        var chunks: [[Float]] = []
        var count = 0
        /// Ce que `drain` a déjà rendu.
        var drained = 0

        mutating func append(_ samples: UnsafeBufferPointer<Float>) {
            var rest = samples[...]
            while !rest.isEmpty {
                if chunks.isEmpty || chunks[chunks.count - 1].count >= Self.chunk {
                    var fresh: [Float] = []
                    fresh.reserveCapacity(Self.chunk)
                    chunks.append(fresh)
                }
                let room = Self.chunk - chunks[chunks.count - 1].count
                let part = rest.prefix(room)
                chunks[chunks.count - 1].append(contentsOf: part)
                rest = rest.dropFirst(part.count)
            }
            count += samples.count
        }

        /// À partir de `start`, à plat. Hors du verrou : une image des
        /// morceaux (des références) suffit.
        static func flatten(_ chunks: [[Float]], from start: Int) -> [Float] {
            var out: [Float] = []
            out.reserveCapacity(max(0, chunks.reduce(0) { $0 + $1.count } - start))
            var skip = start
            for chunk in chunks {
                if skip >= chunk.count { skip -= chunk.count; continue }
                out.append(contentsOf: chunk[skip...])
                skip = 0
            }
            return out
        }
    }

    init(spectrum: @escaping @Sendable ([[Float]]) -> Void) {
        self.spectrum = spectrum
    }

    deinit {
        stream?.stop()
    }

    /// L'autorisation du micro, demandée la première fois ; refusée :
    /// `DictationError.microphoneDenied`. Commune à toutes les dictées.
    static func requirePermission() async throws {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return
        case .notDetermined:
            if await AVCaptureDevice.requestAccess(for: .audio) { return }
            throw DictationError.microphoneDenied
        default:
            throw DictationError.microphoneDenied
        }
    }

    /// Ouvre le micro ; le tampon repart de zéro.
    func start() throws {
        stop()
        buffer.withLock { $0 = Buffer() }
        // Des écouteurs Bluetooth en entrée cèdent au micro du Mac.
        guard let device = MicrophoneChoice.device() else { throw DictationError.noMicrophone }
        let stream = try MicrophoneStream(device: device)
        guard let tap = MicrophoneTap(from: stream.format, spectrum: spectrum, append: { [buffer] converted in
            buffer.withLock { $0.append(converted) }
        }) else { throw DictationError.failed(String(localized: "unsupported audio format")) }
        try stream.start(tap.receiver())
        self.stream = stream
    }

    /// Ferme le micro ; ce qui est arrivé reste dans le tampon.
    func stop() {
        stream?.stop()
        stream = nil
    }

    /// Tout l'audio depuis `start` (une copie).
    func samples() -> [Float] {
        Buffer.flatten(buffer.withLock { $0.chunks }, from: 0)
    }

    /// Ce qui est arrivé depuis le dernier `drain` (depuis `start` la
    /// première fois).
    func drain() -> [Float] {
        let (chunks, start) = buffer.withLock { state in
            defer { state.drained = state.count }
            return (state.chunks, state.drained)
        }
        return Buffer.flatten(chunks, from: start)
    }
}

/// Appelé sur le fil audio : le spectre de chaque tampon, puis sa conversion
/// en 16 kHz mono. La conversion reprend celle de l'`AudioTap` de
/// `SystemDictationEngine`, dupliquée plutôt que partagée : celui-là ne
/// compile qu'avec le SDK de macOS 26 et livre des `AnalyzerInput` au
/// transcripteur ; le généraliser toucherait la dictée de macOS.
private final class MicrophoneTap: @unchecked Sendable {
    private let converter: AVAudioConverter
    private let natural: AVAudioFormat
    private let analyzer: SpectrumAnalyzer
    private let spectrum: @Sendable ([[Float]]) -> Void
    private let append: (UnsafeBufferPointer<Float>) -> Void
    /// La sortie de la conversion, réutilisée d'un cycle à l'autre (son
    /// contenu est copié par `append`) : pas d'allocation sur le fil audio,
    /// sauf si un paquet plus long que tous les précédents arrive.
    private var converted: AVAudioPCMBuffer?

    init?(from natural: AVAudioFormat, spectrum: @escaping @Sendable ([[Float]]) -> Void,
          append: @escaping (UnsafeBufferPointer<Float>) -> Void) {
        guard let converter = AVAudioConverter(from: natural, to: MicrophoneCapture.format) else { return nil }
        // Pas d'amorce : la conversion suit le micro sans retard.
        converter.primeMethod = .none
        self.converter = converter
        self.natural = natural
        self.spectrum = spectrum
        self.append = append
        analyzer = SpectrumAnalyzer(sampleRate: natural.sampleRate)
    }

    func receiver() -> AVAudioSinkNodeReceiverBlock {
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
        if let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 {
            let columns = analyzer.push(UnsafeBufferPointer(start: samples, count: Int(buffer.frameLength)))
            if !columns.isEmpty { spectrum(columns) }
        }
        let output = MicrophoneCapture.format
        let ratio = output.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
        if (self.converted?.frameCapacity ?? 0) < capacity {
            self.converted = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: capacity)
        }
        guard let converted = self.converted else { return }
        converted.frameLength = 0
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
        guard status != .error, converted.frameLength > 0, let channel = converted.floatChannelData?[0] else { return }
        append(UnsafeBufferPointer(start: channel, count: Int(converted.frameLength)))
    }
}
