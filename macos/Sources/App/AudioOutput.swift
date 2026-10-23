import CoreAudio
import Foundation

/// Les choix de sortie audio, les mêmes partout : la barre des menus, les
/// réglages et le panneau.
enum AudioOutputChoices {
    struct Choice: Hashable, Identifiable {
        let title: String
        /// Le nom de la sortie ; vide : celle du système.
        let value: String
        var id: String { value }
    }

    /// « Sortie du système » en tête, les sorties branchées ensuite ; une
    /// sortie choisie mais débranchée reste listée, et le dit.
    static func choices(_ outputs: AudioOutputsView, chosen: String) -> [Choice] {
        var list = [Choice(title: systemTitle(outputs), value: "")]
        list += outputs.devices.map { Choice(title: $0, value: $0) }
        if !chosen.isEmpty, !outputs.devices.contains(chosen) {
            list.append(Choice(title: String(localized: "\(chosen) (disconnected)", comment: "Audio output that is chosen but unplugged"), value: chosen))
        }
        return list
    }

    static func systemTitle(_ outputs: AudioOutputsView) -> String {
        guard let system = outputs.system else { return String(localized: "System Output") }
        return String(localized: "System Output (\(system))")
    }

    /// Le nom court montré sur le bouton du panneau : la sortie choisie,
    /// sinon celle que le système utilise.
    static func buttonTitle(_ outputs: AudioOutputsView, chosen: String) -> String {
        guard chosen.isEmpty else { return chosen }
        return outputs.system ?? String(localized: "System")
    }

    /// Le symbole de la sortie choisie, pour le bouton du panneau : des
    /// écouteurs, sinon le symbole AirPlay, jamais un haut-parleur (c'est
    /// celui du volume, son voisin).
    static func symbol(for chosen: String) -> String {
        let name = chosen.lowercased()
        let worn = ["airpods", "casque", "écouteur", "ecouteur", "headphone", "buds", "beats"] // xcstrings: garder
        return worn.contains { name.contains($0) } ? "headphones" : "airplayaudio"
    }
}

/// Prévient quand une sortie audio est branchée ou retirée, ou quand macOS
/// change de sortie par défaut : la liste se relit à ce moment-là, jamais à
/// intervalles.
final class AudioDeviceWatcher {
    private let onChange: () -> Void
    private var listener: AudioObjectPropertyListenerBlock?
    private let selectors: [AudioObjectPropertySelector] = [
        kAudioHardwarePropertyDevices, kAudioHardwarePropertyDefaultOutputDevice,
    ]

    init(onChange: @escaping () -> Void) {
        self.onChange = onChange
        let block: AudioObjectPropertyListenerBlock = { [weak self] _, _ in self?.onChange() }
        listener = block
        for selector in selectors {
            var address = Self.address(selector)
            AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, .main, block)
        }
    }

    deinit {
        guard let listener else { return }
        for selector in selectors {
            var address = Self.address(selector)
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, .main, listener)
        }
    }

    private static func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                   mElement: kAudioObjectPropertyElementMain)
    }
}
