import AppKit
import Carbon.HIToolbox
import KeyboardShortcuts
import SwiftUI

/// Ce qu'un appui donne pendant l'enregistrement d'un raccourci.
enum ShortcutCapture: Equatable {
    /// Une combinaison : au moins ⌘, ⌥ ou ⌃ avec une autre touche (⌥D,
    /// ⌃⌥Espace), ou une touche de fonction seule (F5).
    case combo(KeyboardShortcuts.Shortcut)
    /// Une touche de modification seule (Option droite, Fn…), pour la dictée.
    case modifier(DictationModifier)
    /// Une lettre, un chiffre, Maj + lettre… : ce qu'on tape en écrivant.
    case refused
}

/// Lit les appuis pendant l'enregistrement. Logique pure : les événements
/// viennent de `ShortcutRecorder`.
///
/// Une touche seule n'est acceptée que si c'est une touche d'action : une
/// touche de modification (si `allowsModifierAlone`) ou une touche de
/// fonction. Une lettre ou un chiffre ne vaut qu'avec ⌘, ⌥ ou ⌃.
struct ShortcutCaptureReader {
    let allowsModifierAlone: Bool
    /// La touche de modification enfoncée seule, en attente de son relâché.
    private var pending: DictationModifier?

    init(allowsModifierAlone: Bool) {
        self.allowsModifierAlone = allowsModifierAlone
    }

    static let functionKeys: Set<Int> = [
        kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10,
        kVK_F11, kVK_F12, kVK_F13, kVK_F14, kVK_F15, kVK_F16, kVK_F17, kVK_F18, kVK_F19, kVK_F20,
    ]

    mutating func keyDown(keyCode: UInt16, modifiers: NSEvent.ModifierFlags) -> ShortcutCapture {
        pending = nil
        let held = modifiers.intersection([.command, .option, .control, .shift])
        let key = KeyboardShortcuts.Key(rawValue: Int(keyCode))
        if !held.isDisjoint(with: [.command, .option, .control]) || Self.functionKeys.contains(Int(keyCode)) {
            return .combo(KeyboardShortcuts.Shortcut(key, modifiers: held))
        }
        return .refused
    }

    /// `nil` : rien de décidé encore (une touche enfoncée, ou une autre ajoutée).
    mutating func flagsChanged(keyCode: UInt16, modifiers: NSEvent.ModifierFlags) -> ShortcutCapture? {
        let held = modifiers.intersection([.command, .option, .control, .shift, .function])
        guard let key = DictationModifier(keyCode: keyCode) else { pending = nil; return nil }
        if held == key.flag, pending == nil {
            // Enfoncée, seule.
            pending = key
            return nil
        }
        if held.isEmpty, pending == key {
            // Relâchée sans rien d'autre : une touche seule.
            pending = nil
            return allowsModifierAlone ? .modifier(key) : nil
        }
        // Une deuxième touche de modification : ce sera une combinaison.
        pending = nil
        return nil
    }
}

/// Le déclencheur de la dictée : une touche de modification seule
/// (`DictationModifier`, il faut l'Accessibilité) ou une combinaison
/// (`KeyboardShortcuts` `.dictate`). L'un remplace l'autre ; les réglages
/// d'origine gardent les deux (Option droite et ⌃⌥D).
@MainActor
enum DictationTrigger {
    /// Un raccourci s'enregistre : les touches tapées ne déclenchent rien.
    static var isRecording = false

    static func record(_ capture: ShortcutCapture, defaults: UserDefaults = .standard) {
        switch capture {
        case .modifier(let key):
            defaults.set(key.rawValue, forKey: DictationModifier.defaultsKey)
            KeyboardShortcuts.setShortcut(nil, for: .dictate)
        case .combo(let shortcut):
            KeyboardShortcuts.setShortcut(shortcut, for: .dictate)
            defaults.set(DictationModifier.none.rawValue, forKey: DictationModifier.defaultsKey)
        case .refused:
            break
        }
    }

    static func clear(defaults: UserDefaults = .standard) {
        defaults.set(DictationModifier.none.rawValue, forKey: DictationModifier.defaultsKey)
        KeyboardShortcuts.setShortcut(nil, for: .dictate)
    }

    /// « Option droite », « ⌃⌥D », « Option droite ou ⌃⌥D », ou `nil`.
    static func label(modifier: DictationModifier, combo: String?) -> String? {
        let parts = [modifier == .none ? nil : modifier.label, combo].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " ou ")
    }

    static func currentLabel(defaults: UserDefaults = .standard) -> String? {
        label(modifier: DictationModifier.stored(defaults),
              combo: KeyboardShortcuts.getShortcut(for: .dictate).map(ShortcutRecorder.text))
    }
}

/// L'enregistreur de raccourci des réglages, en français : un clic, puis
/// la touche ou la combinaison. Échap annule ; la croix efface.
///
/// Pour la dictée, une touche de modification seule est acceptée ; pour les
/// autres raccourcis, seulement une combinaison (ou une touche de fonction).
/// Une lettre ou un chiffre seul est refusé : c'est ce qu'on tape en écrivant.
struct ShortcutRecorder: View {
    enum Target: Equatable {
        case shortcut(KeyboardShortcuts.Name)
        case dictation
    }

    let target: Target
    let title: String
    let identifier: String

    @State private var recording = false
    @State private var refused = false
    @State private var monitor: Any?
    @State private var reader = ShortcutCaptureReader(allowsModifierAlone: false)
    /// Change à chaque enregistrement : l'affichage relit les réglages.
    @State private var revision = 0

    static let placeholder = "Tapez le raccourci…"
    static let refusal = "Ajoutez ⌘, ⌥ ou ⌃"

    var body: some View {
        let current = currentLabel
        HStack(spacing: 6) {
            Button {
                recording ? stop() : start()
            } label: {
                Text(recording ? (refused ? Self.refusal : Self.placeholder) : (current ?? "Aucun"))
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(recording ? Palette.muted : (current == nil ? Palette.muted : Palette.ink))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(minWidth: 150)
                    .padding(.horizontal, 14)
                    .frame(height: 32)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Palette.control))
                    .overlay {
                        if recording {
                            RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.accent, lineWidth: 2)
                        }
                    }
                    .contentShape(RoundedRectangle(cornerRadius: 10))
            }
            .buttonStyle(.plain)
            .pointingHandCursor()
            .help(help)
            .accessibilityLabel(title)
            .accessibilityValue(recording ? Self.placeholder : (current ?? "aucun"))
            .accessibilityHint(help)
            .accessibilityIdentifier(identifier)
            if current != nil, !recording {
                Button {
                    clear()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 14))
                        .foregroundStyle(Palette.muted)
                        .frame(width: 22, height: 22)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .pointingHandCursor()
                .help("Effacer le raccourci")
                .accessibilityLabel("Effacer : \(title)")
                .accessibilityIdentifier("\(identifier)-effacer")
            }
        }
        .id(revision)
        .onDisappear { stop() }
    }

    private var help: String {
        switch target {
        case .dictation:
            "Cliquez, puis tapez une touche de modification seule (Option, Commande, Contrôle, Maj, Fn) "
                + "ou une combinaison (⌥D). Échap annule."
        case .shortcut:
            "Cliquez, puis tapez une combinaison (⌃⌥S) ou une touche de fonction. Échap annule."
        }
    }

    private var currentLabel: String? {
        _ = revision
        switch target {
        case .dictation: return DictationTrigger.currentLabel()
        case .shortcut(let name): return KeyboardShortcuts.getShortcut(for: name).map(Self.text)
        }
    }

    /// Une combinaison, écrite comme sur le clavier, avec les noms français.
    static func text(_ shortcut: KeyboardShortcuts.Shortcut) -> String {
        shortcut.description.replacingOccurrences(of: "Space", with: "Espace")
    }

    private func start() {
        reader = ShortcutCaptureReader(allowsModifierAlone: target == .dictation)
        refused = false
        recording = true
        // Les raccourcis actuels ne se déclenchent pas pendant la saisie.
        KeyboardShortcuts.isEnabled = false
        DictationTrigger.isRecording = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .flagsChanged]) { event in
            MainActor.assumeIsolated { handle(event) }
            return nil
        }
    }

    private func stop() {
        guard recording else { return }
        recording = false
        refused = false
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        KeyboardShortcuts.isEnabled = true
        DictationTrigger.isRecording = false
    }

    private func handle(_ event: NSEvent) {
        let capture: ShortcutCapture?
        switch event.type {
        case .keyDown:
            if Int(event.keyCode) == kVK_Escape, event.modifierFlags.intersection(.deviceIndependentFlagsMask)
                .intersection([.command, .option, .control, .shift]).isEmpty {
                stop()
                return
            }
            capture = reader.keyDown(keyCode: event.keyCode, modifiers: event.modifierFlags)
        case .flagsChanged:
            capture = reader.flagsChanged(keyCode: event.keyCode, modifiers: event.modifierFlags)
        default:
            capture = nil
        }
        guard let capture else { return }
        if capture == .refused {
            refused = true
            NSSound.beep()
            return
        }
        switch target {
        case .dictation:
            DictationTrigger.record(capture)
        case .shortcut(let name):
            if case .combo(let shortcut) = capture { KeyboardShortcuts.setShortcut(shortcut, for: name) }
        }
        stop()
        revision += 1
    }

    private func clear() {
        switch target {
        case .dictation: DictationTrigger.clear()
        case .shortcut(let name): KeyboardShortcuts.setShortcut(nil, for: name)
        }
        revision += 1
    }
}
