import AVFoundation
import CoreAudio

/// Le micro de la dictée. Ouvrir le micro d'écouteurs Bluetooth (AirPods)
/// les fait passer en mode « mains libres » : la musique se coupe un
/// instant, revient étouffée et mono, puis se recoupe à la fermeture. Par
/// défaut, des écouteurs Bluetooth en entrée cèdent donc la place au micro
/// du Mac ; la musique continue d'y sortir telle quelle.
enum MicrophoneChoice: String, CaseIterable {
    /// Le micro du Mac quand l'entrée du système est en Bluetooth.
    case builtInOverBluetooth
    /// L'entrée du système, quelle qu'elle soit.
    case system

    static let defaultsKey = "dictationMicrophone"

    static func stored(_ defaults: UserDefaults = .standard) -> MicrophoneChoice {
        defaults.string(forKey: defaultsKey).flatMap(MicrophoneChoice.init(rawValue:)) ?? .builtInOverBluetooth
    }

    var label: String {
        switch self {
        case .builtInOverBluetooth: String(localized: "Mac microphone with Bluetooth headphones")
        case .system: String(localized: "System input")
        }
    }

    /// Le micro à ouvrir à la place de l'entrée du système ; `nil` : l'entrée
    /// du système.
    func replacement(defaultInput: AudioInput?, inputs: [AudioInput]) -> AudioInput? {
        guard self == .builtInOverBluetooth, let defaultInput, defaultInput.bluetooth else { return nil }
        return inputs.first(where: \.builtIn)
    }

    /// Le micro à ouvrir : le remplaçant, sinon l'entrée du système ; `nil` :
    /// aucun micro.
    static func device(choice: MicrophoneChoice = .stored()) -> AudioDeviceID? {
        let defaultInput = AudioInput.systemDefault()
        return (choice.replacement(defaultInput: defaultInput, inputs: AudioInput.all()) ?? defaultInput)?.id
    }
}

/// Une entrée audio de Core Audio, vue par la dictée.
struct AudioInput: Equatable {
    var id: AudioDeviceID
    var transport: UInt32

    var bluetooth: Bool {
        transport == kAudioDeviceTransportTypeBluetooth || transport == kAudioDeviceTransportTypeBluetoothLE
    }

    var builtIn: Bool { transport == kAudioDeviceTransportTypeBuiltIn }

    static func systemDefault() -> AudioInput? {
        guard let id = read(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice),
              id != 0 else { return nil }
        return input(id)
    }

    /// Les appareils qui ont au moins un flux d'entrée.
    static func all() -> [AudioInput] {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDevices,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        let system = AudioObjectID(kAudioObjectSystemObject)
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr else { return [] }
        var ids = [AudioDeviceID](repeating: 0, count: Int(size) / MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &ids) == noErr else { return [] }
        return ids.compactMap { hasInput($0) ? input($0) : nil }
    }

    private static func input(_ id: AudioDeviceID) -> AudioInput {
        AudioInput(id: id, transport: read(id, kAudioDevicePropertyTransportType) ?? 0)
    }

    private static func hasInput(_ id: AudioDeviceID) -> Bool {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreams,
                                                 mScope: kAudioObjectPropertyScopeInput,
                                                 mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        return AudioObjectGetPropertyDataSize(id, &address, 0, nil, &size) == noErr && size > 0
    }

    /// Une propriété de 32 bits (un appareil, un type de transport).
    private static func read(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> UInt32? {
        var address = AudioObjectPropertyAddress(mSelector: selector,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var value: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        return AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr ? value : nil
    }
}
