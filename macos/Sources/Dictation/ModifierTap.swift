import AppKit
import Carbon.HIToolbox

/// Une touche de modification seule, pour lancer la dictée d'un appui.
enum DictationModifier: String, CaseIterable {
    case none
    case rightOption
    case rightCommand

    static let defaultsKey = "dictationModifier"

    var label: String {
        switch self {
        case .none: "Aucune"
        case .rightOption: "Option droite"
        case .rightCommand: "Commande droite"
        }
    }

    var keyCode: UInt16? {
        switch self {
        case .none: nil
        case .rightOption: UInt16(kVK_RightOption)
        case .rightCommand: UInt16(kVK_RightCommand)
        }
    }

    var flag: NSEvent.ModifierFlags {
        switch self {
        case .none: []
        case .rightOption: .option
        case .rightCommand: .command
        }
    }

    /// Celle des réglages ; Option droite par défaut.
    static func stored(_ defaults: UserDefaults = .standard) -> DictationModifier {
        defaults.string(forKey: defaultsKey).flatMap(DictationModifier.init(rawValue:)) ?? .rightOption
    }
}

/// Reconnaît un appui bref sur la touche choisie, seule : enfoncée sans
/// autre modificateur, relâchée vite, sans touche frappée entre-temps.
/// Option sert aussi à écrire (@, |, {…) : ⌥ tenue avec une lettre ne
/// déclenche rien.
struct ModifierTapDetector {
    var key: DictationModifier
    /// Au-delà, c'est un modificateur tenu, pas un appui.
    var longest: TimeInterval = 0.4
    private var pressedAt: TimeInterval?

    init(key: DictationModifier) { self.key = key }

    /// Un changement de modificateurs. Vrai : l'appui est reconnu.
    mutating func flagsChanged(keyCode: UInt16, flags: NSEvent.ModifierFlags, at time: TimeInterval) -> Bool {
        guard let code = key.keyCode else { pressedAt = nil; return false }
        let held = flags.intersection([.command, .option, .control, .shift, .function])
        if keyCode == code {
            if held == key.flag {
                // Enfoncée, seule.
                pressedAt = time
                return false
            }
            if held.isEmpty, let start = pressedAt {
                // Relâchée, sans rien d'autre entre-temps.
                pressedAt = nil
                return time - start <= longest
            }
        }
        // Un autre modificateur, ou la même touche avec d'autres : pas un appui.
        pressedAt = nil
        return false
    }

    /// Une touche frappée pendant que le modificateur est tenu : c'était une
    /// combinaison (⌥ + lettre), pas un appui.
    mutating func keyDown() { pressedAt = nil }
}

/// Écoute les modificateurs dans toutes les applications (il faut
/// l'Accessibilité) et dans sillon, et appelle `onTap` à chaque appui
/// reconnu. La touche est relue à chaque fois : un réglage changé
/// s'applique aussitôt. Vit autant que l'app.
@MainActor
final class ModifierTapMonitor {
    private var detector = ModifierTapDetector(key: .none)
    private var monitors: [Any] = []
    private let onTap: @MainActor () -> Void
    private let key: () -> DictationModifier

    init(key: @escaping () -> DictationModifier = { DictationModifier.stored() },
         onTap: @escaping @MainActor () -> Void) {
        self.key = key
        self.onTap = onTap
        let mask: NSEvent.EventTypeMask = [.flagsChanged, .keyDown]
        if let global = NSEvent.addGlobalMonitorForEvents(matching: mask, handler: { [weak self] event in
            MainActor.assumeIsolated { self?.handle(event) }
        }) {
            monitors.append(global)
        }
        if let local = NSEvent.addLocalMonitorForEvents(matching: mask, handler: { [weak self] event in
            MainActor.assumeIsolated { self?.handle(event) }
            return event
        }) {
            monitors.append(local)
        }
    }

    private func handle(_ event: NSEvent) {
        let current = key()
        if detector.key != current { detector = ModifierTapDetector(key: current) }
        switch event.type {
        case .keyDown:
            detector.keyDown()
        case .flagsChanged:
            if detector.flagsChanged(keyCode: event.keyCode, flags: event.modifierFlags, at: event.timestamp) {
                onTap()
            }
        default:
            break
        }
    }

}
