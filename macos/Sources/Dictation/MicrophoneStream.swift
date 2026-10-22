import AudioToolbox
import AVFoundation
import CoreAudio

/// Le flux d'un micro précis, ouvert par une unité HAL d'entrée seule. Pas
/// d'`AVAudioEngine` : son nœud d'entrée s'attache d'abord à l'entrée du
/// système, et des AirPods en entrée se reconfigurent alors, leur sortie
/// coupée un instant (mesuré : 0,1 à 0,8 s à chaque dictée), même quand le
/// micro est ensuite changé. Ici, l'appareil est choisi avant
/// l'initialisation : aucun autre n'est touché.
///
/// Le format rendu est celui d'`AVAudioSinkNode` (Float32 non entrelacé, à
/// la fréquence de l'appareil) : les receveurs d'avant servent tels quels.
final class MicrophoneStream: @unchecked Sendable {
    let format: AVAudioFormat
    private let unit: AudioUnit
    /// Rempli par `AudioUnitRender`, sur le fil audio seulement.
    private let rendered: AVAudioPCMBuffer
    private var receiver: AVAudioSinkNodeReceiverBlock?
    private var started = false

    /// Le plus grand tampon que le fil audio demande d'un coup.
    private static let maxFrames: AVAudioFrameCount = 8192

    init(device: AudioDeviceID) throws {
        var description = AudioComponentDescription(
            componentType: kAudioUnitType_Output, componentSubType: kAudioUnitSubType_HALOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple, componentFlags: 0, componentFlagsMask: 0)
        guard let component = AudioComponentFindNext(nil, &description) else { throw DictationError.noMicrophone }
        var made: AudioUnit?
        try Self.check(AudioComponentInstanceNew(component, &made))
        guard let unit = made else { throw DictationError.noMicrophone }
        self.unit = unit
        do {
            var on: UInt32 = 1, off: UInt32 = 0, device = device
            let size = UInt32(MemoryLayout<UInt32>.size)
            try Self.check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, 1, &on, size))
            try Self.check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0, &off, size))
            try Self.check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                                &device, UInt32(MemoryLayout<AudioDeviceID>.size)))
            // Le format de l'appareil (côté entrée de l'élément 1) donne la
            // fréquence et les canaux ; l'unité rend du Float32 non entrelacé.
            var native = AudioStreamBasicDescription()
            var nativeSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
            try Self.check(AudioUnitGetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 1,
                                                &native, &nativeSize))
            guard native.mSampleRate > 0, native.mChannelsPerFrame > 0,
                  let format = AVAudioFormat(standardFormatWithSampleRate: native.mSampleRate,
                                             channels: native.mChannelsPerFrame),
                  let rendered = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: Self.maxFrames)
            else { throw DictationError.noMicrophone }
            var wanted = format.streamDescription.pointee
            try Self.check(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1,
                                                &wanted, UInt32(MemoryLayout<AudioStreamBasicDescription>.size)))
            var maxFrames = Self.maxFrames
            try Self.check(AudioUnitSetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0,
                                                &maxFrames, UInt32(MemoryLayout<UInt32>.size)))
            self.format = format
            self.rendered = rendered
        } catch {
            AudioComponentInstanceDispose(unit)
            throw error
        }
    }

    deinit {
        stop()
        AudioComponentInstanceDispose(unit)
    }

    /// Ouvre le micro ; `receiver` reçoit chaque tampon, sur le fil audio.
    func start(_ receiver: @escaping AVAudioSinkNodeReceiverBlock) throws {
        self.receiver = receiver
        var callback = AURenderCallbackStruct(inputProc: { refCon, flags, time, bus, frames, _ in
            let stream = Unmanaged<MicrophoneStream>.fromOpaque(refCon).takeUnretainedValue()
            return stream.pull(flags, time, bus, frames)
        }, inputProcRefCon: Unmanaged.passUnretained(self).toOpaque())
        try Self.check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, 0,
                                            &callback, UInt32(MemoryLayout<AURenderCallbackStruct>.size)))
        try Self.check(AudioUnitInitialize(unit))
        do {
            try Self.check(AudioOutputUnitStart(unit))
        } catch {
            AudioUnitUninitialize(unit)
            throw error
        }
        started = true
    }

    /// Ferme le micro ; sans effet s'il l'est déjà.
    func stop() {
        guard started else { return }
        started = false
        AudioOutputUnitStop(unit)
        AudioUnitUninitialize(unit)
    }

    private func pull(_ flags: UnsafeMutablePointer<AudioUnitRenderActionFlags>, _ time: UnsafePointer<AudioTimeStamp>,
                      _ bus: UInt32, _ frames: UInt32) -> OSStatus {
        guard frames <= Self.maxFrames, let receiver else { return noErr }
        // La longueur règle la taille des tampons de la liste : écrite à la
        // main dans `mutableAudioBufferList`, elle n'est pas gardée (rendu
        // refusé, -50).
        rendered.frameLength = frames
        let status = AudioUnitRender(unit, flags, time, bus, frames, rendered.mutableAudioBufferList)
        guard status == noErr else { return status }
        return receiver(time, frames, rendered.audioBufferList)
    }

    private static func check(_ status: OSStatus) throws {
        guard status == noErr else {
            throw DictationError.failed(String(localized: "microphone unavailable (\(status))"))
        }
    }
}
