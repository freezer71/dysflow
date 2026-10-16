import AppKit
import ApplicationServices
import Carbon.HIToolbox

/// Le presse-papiers, quand la dictée y pose son texte.
protocol PasteboardWriting: PasteboardAccess {
    /// `transient` : marqué pour que les historiques de presse-papiers ne le
    /// gardent pas (il n'y est que le temps du ⌘V).
    func write(_ text: String, transient: Bool)
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
    /// Le clavier est chez sillon (le lecteur) : ⌘V y lirait le texte.
    case ownWindow

    var message: String {
        switch self {
        case .noAccessibility: "Texte copié : ⌘V pour le coller (autorisez l'Accessibilité pour la saisie directe)"
        case .secureInput, .ownWindow: "Texte copié : ⌘V pour le coller"
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
    /// L'Accessibilité, et le droit d'envoyer des touches, sans rien
    /// demander : sans eux, macOS ouvrirait sa demande au moment du ⌘V.
    var trusted: () -> Bool = { AXIsProcessTrusted() && CGPreflightPostEventAccess() }
    var secureInput: () -> Bool = { IsSecureEventInputEnabled() }
    /// Une fenêtre de sillon a le clavier, ou sillon est au premier plan.
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
        if let reason = copyReason() {
            pasteboard.write(text, transient: false)
            return .copied(reason)
        }
        let saved = pasteboard.save()
        pasteboard.write(text, transient: true)
        let ours = pasteboard.changeCount
        keys.sendPaste()
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
    func write(_ text: String, transient: Bool) {
        let board = NSPasteboard.general
        board.clearContents()
        let item = NSPasteboardItem()
        item.setString(text, forType: .string)
        if transient { item.setData(Data(), forType: NSPasteboard.PasteboardType(transientType)) }
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
