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
/// L'autorisation du micro se demande avant `start`.
final class MicrophoneCapture: @unchecked Sendable {
    static let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000,
                                      channels: 1, interleaved: false)!

    private let spectrum: @Sendable ([[Float]]) -> Void
    private var audio: AVAudioEngine?
    private let buffer = OSAllocatedUnfairLock(initialState: Buffer())

    private struct Buffer {
        var samples: [Float] = []
        /// Ce que `drain` a déjà rendu.
        var drained = 0
    }

    init(spectrum: @escaping @Sendable ([[Float]]) -> Void) {
        self.spectrum = spectrum
    }

    deinit {
        audio?.stop()
    }

    /// Ouvre le micro ; le tampon repart de zéro.
    func start() throws {
        stop()
        buffer.withLock { $0 = Buffer() }
        let audio = AVAudioEngine()
        let node = audio.inputNode
        let natural = node.outputFormat(forBus: 0)
        guard natural.sampleRate > 0, natural.channelCount > 0 else { throw DictationError.noMicrophone }
        guard let tap = MicrophoneTap(from: natural, spectrum: spectrum, append: { [buffer] converted in
            buffer.withLock { $0.samples.append(contentsOf: converted) }
        }) else { throw DictationError.failed("format audio non pris en charge") }
        let sink = AVAudioSinkNode(receiverBlock: tap.receiver())
        audio.attach(sink)
        audio.connect(node, to: sink, format: natural)
        audio.prepare()
        do {
            try audio.start()
        } catch {
            throw DictationError.failed(error.localizedDescription)
        }
        self.audio = audio
    }

    /// Ferme le micro ; ce qui est arrivé reste dans le tampon.
    func stop() {
        audio?.stop()
        audio = nil
    }

    /// Tout l'audio depuis `start` (une copie).
    func samples() -> [Float] {
        buffer.withLock { $0.samples }
    }

    /// Ce qui est arrivé depuis le dernier `drain` (depuis `start` la
    /// première fois).
    func drain() -> [Float] {
        buffer.withLock { state in
            let fresh = Array(state.samples[state.drained...])
            state.drained = state.samples.count
            return fresh
        }
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
        guard status != .error, converted.frameLength > 0, let channel = converted.floatChannelData?[0] else { return }
        append(UnsafeBufferPointer(start: channel, count: Int(converted.frameLength)))
    }
}
