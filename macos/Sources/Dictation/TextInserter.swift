import AppKit
import ApplicationServices
import Carbon.HIToolbox

/// Ce que le texte dicté laisse voir une fois dans le presse-papiers.
enum PasteboardPrivacy: Equatable {
    /// Une copie ordinaire, que l'utilisateur reprend avec ⌘V.
    case normal
    /// Le temps du ⌘V : ni gardé par les historiques de presse-papiers, ni
    /// envoyé aux autres appareils (presse-papiers universel).
    case transient
    /// Pour un champ de mot de passe : comme `transient`, et marqué
    /// confidentiel (les gestionnaires de presse-papiers ne l'affichent pas).
    case concealed
}

/// Le presse-papiers, quand la dictée y pose son texte.
protocol PasteboardWriting: PasteboardAccess {
    func write(_ text: String, privacy: PasteboardPrivacy)
}

protocol PasteKeystroke {
    func sendPaste()
}

/// Ce qu'est devenu le texte dicté.
enum InsertOutcome: Equatable {
    /// Collé dans le champ qui a le focus ; le presse-papiers est revenu.
    case pasted
    /// Laissé dans le presse-papiers : rien ne pouvait le saisir.
    case copied(CopyReason)
    /// Rien à saisir.
    case nothing
}

enum CopyReason: Equatable {
    /// Sans l'autorisation Accessibilité, pas de ⌘V simulé.
    case noAccessibility
    /// Saisie sécurisée (mot de passe) : macOS ne laisse rien passer.
    case secureInput
    /// Le clavier est chez Dysflow (le lecteur) : ⌘V y lirait le texte.
    case ownWindow

    var message: String {
        switch self {
        case .noAccessibility: String(localized: "Text copied: press ⌘V to paste (allow Accessibility for direct typing)")
        case .secureInput, .ownWindow: String(localized: "Text copied: press ⌘V to paste")
        }
    }
}

/// Saisit le texte dicté dans le champ qui a le focus, quelle que soit
/// l'application : le texte passe par le presse-papiers le temps d'un ⌘V
/// simulé, puis le presse-papiers revient comme avant.
@MainActor
struct TextInserter {
    let pasteboard: PasteboardWriting
    let keys: PasteKeystroke
    /// L'Accessibilité, sans rien demander : elle vaut aussi pour le ⌘V,
    /// et se relit dès qu'elle est accordée (pas besoin de relancer).
    var trusted: () -> Bool = { AXIsProcessTrusted() }
    var secureInput: () -> Bool = { IsSecureEventInputEnabled() }
    /// Une fenêtre de Dysflow a le clavier, ou Dysflow est au premier plan.
    var ownAppHasKeyboard: () -> Bool = {
        NSApp.keyWindow != nil || NSWorkspace.shared.frontmostApplication?.processIdentifier == getpid()
    }
    /// Le temps laissé à l'application pour lire le presse-papiers.
    var restoreDelay: Duration = .milliseconds(500)

    static func system() -> TextInserter {
        TextInserter(pasteboard: SystemPasteboard(), keys: SystemKeystrokes())
    }

    func insert(_ text: String) async -> InsertOutcome {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return .nothing }
        let front = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "?"
        if let reason = copyReason() {
            DictationLog.log.info("saisie : copiée (\(String(describing: reason), privacy: .public)), app au premier plan : \(front, privacy: .public)")
            // Un champ de mot de passe : le texte peut en être un.
            pasteboard.write(text, privacy: reason == .secureInput ? .concealed : .normal)
            return .copied(reason)
        }
        let saved = pasteboard.save()
        pasteboard.write(text, privacy: .transient)
        let ours = pasteboard.changeCount
        keys.sendPaste()
        DictationLog.log.info("saisie : ⌘V envoyé à \(front, privacy: .public)")
        try? await Task.sleep(for: restoreDelay)
        // Une copie faite entre-temps par l'utilisateur reste.
        if pasteboard.changeCount == ours { pasteboard.restore(saved) }
        return .pasted
    }

    private func copyReason() -> CopyReason? {
        if !trusted() { return .noAccessibility }
        if secureInput() { return .secureInput }
        if ownAppHasKeyboard() { return .ownWindow }
        return nil
    }
}

extension SystemPasteboard: PasteboardWriting {
    func write(_ text: String, privacy: PasteboardPrivacy) {
        let board = NSPasteboard.general
        // Hors du cas ordinaire, le texte reste sur ce Mac.
        if privacy == .normal {
            board.clearContents()
        } else {
            board.prepareForNewContents(with: .currentHostOnly)
        }
        let item = NSPasteboardItem()
        item.setString(text, forType: .string)
        if privacy != .normal { item.setData(Data(), forType: NSPasteboard.PasteboardType(transientType)) }
        if privacy == .concealed { item.setData(Data(), forType: NSPasteboard.PasteboardType(concealedType)) }
        board.writeObjects([item])
    }
}

/// ⌘V envoyé à l'application au premier plan ; comme pour ⌘C, les
/// modificateurs du raccourci encore enfoncés sont écrasés.
extension SystemKeystrokes: PasteKeystroke {
    func sendPaste() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let v = CGKeyCode(kVK_ANSI_V)
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: v, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cghidEventTap)
        }
    }
}
