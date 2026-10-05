import AppKit
import ApplicationServices
import Carbon.HIToolbox

/// Ce qu'une source a répondu. La chaîne s'arrête sur tout sauf `.unsupported`.
enum CaptureOutcome: Equatable {
    /// Du texte, à lire.
    case text(String)
    /// L'application a répondu : rien n'est sélectionné. Pas de repli sur ⌘C
    /// (VS Code, JetBrains copient alors la ligne entière).
    case noSelection
    /// L'application ne donne pas sa sélection : on peut essayer la source suivante.
    case unsupported
    /// Interdit : champ de mot de passe, saisie sécurisée, ou sillon lui-même
    /// au premier plan. Toute la capture s'arrête, rien n'est lu ni copié.
    case veto
}

/// Une façon d'obtenir le texte que l'utilisateur veut entendre.
protocol TextSource {
    func outcome() async -> CaptureOutcome
}

private func nonBlank(_ s: String?) -> String? {
    guard let s, !s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
    return s
}

/// Ce que l'accessibilité dit de l'élément qui a le focus.
enum SelectionReading: Equatable {
    case selected(String)
    case empty
    case unsupported
    case secureField
    case ownProcess
}

/// Lecture réelle de l'élément ayant le focus, avec garde du processus et du sous-rôle.
func readFocusedSelection() -> SelectionReading {
    let system = AXUIElementCreateSystemWide()
    var focused: CFTypeRef?
    guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString,
                                        &focused) == .success,
          let element = focused, CFGetTypeID(element) == AXUIElementGetTypeID()
    else { return .unsupported }
    let ax = element as! AXUIElement
    var pid: pid_t = 0
    if AXUIElementGetPid(ax, &pid) == .success, pid == getpid() { return .ownProcess }
    var subrole: CFTypeRef?
    if AXUIElementCopyAttributeValue(ax, kAXSubroleAttribute as CFString, &subrole) == .success,
       (subrole as? String) == (kAXSecureTextFieldSubrole as String) { return .secureField }
    var selected: CFTypeRef?
    guard AXUIElementCopyAttributeValue(ax, kAXSelectedTextAttribute as CFString,
                                        &selected) == .success,
          let string = selected as? String
    else { return .unsupported }
    return nonBlank(string) == nil ? .empty : .selected(string)
}

/// La sélection, lue directement dans l'application au premier plan.
/// Instantané, et le presse-papiers n'est pas touché.
struct SelectionSource: TextSource {
    var trusted: () -> Bool = { AXIsProcessTrusted() }
    var secureInput: () -> Bool = { IsSecureEventInputEnabled() }
    var read: () -> SelectionReading = readFocusedSelection

    func outcome() async -> CaptureOutcome {
        guard trusted() else { return .unsupported }
        if secureInput() { return .veto }
        switch read() {
        case .selected(let s): return .text(s)
        case .empty: return .noSelection
        case .unsupported: return .unsupported
        case .secureField, .ownProcess: return .veto
        }
    }
}

/// Un type du presse-papiers et ses octets, dans l'ordre d'origine.
struct SavedType: Equatable {
    let type: String
    let data: Data
}

/// Un élément du presse-papiers : ses types dans l'ordre où l'application les a posés.
struct SavedItem: Equatable {
    var types: [SavedType]
}

let transientType = "org.nspasteboard.TransientType"

/// Types « promesse » : leurs octets n'existent pas tant que l'application source ne
/// les fournit pas. Les matérialiser serait lent, voire impossible ; ils sont donc
/// ignorés. LIMITE : une promesse de fichier (Finder, Photos) ne revient pas.
let promisedTypes: Set<String> = [
    "com.apple.NSFilePromiseItemMetaData",
    "com.apple.pasteboard.promised-file-url",
    "com.apple.pasteboard.promised-file-content-type",
]

func isPromised(_ type: String) -> Bool {
    promisedTypes.contains(type) || type.hasPrefix("dyn.")
}

/// Les éléments à réécrire : même ordre, et le marqueur « transitoire » sur le premier
/// pour que les historiques de presse-papiers n'enregistrent pas la restauration
/// comme une nouvelle copie. (`ConcealedType`, s'il y était, reste : c'est un type comme un autre.)
func itemsForRestore(_ items: [SavedItem]) -> [SavedItem] {
    guard var first = items.first else { return [] }
    if !first.types.contains(where: { $0.type == transientType }) {
        first.types.append(SavedType(type: transientType, data: Data()))
    }
    return [first] + items.dropFirst()
}

protocol PasteboardAccess: AnyObject {
    var changeCount: Int { get }
    func save() -> [SavedItem]
    func restore(_ items: [SavedItem])
    func string() -> String?
}

protocol KeystrokeSender {
    func sendCopy()
}

/// Pour les applications qui ne donnent pas leur sélection (Chrome,
/// Electron…) : un ⌘C simulé, puis le presse-papiers remis comme avant.
struct CopySource: TextSource {
    let pasteboard: PasteboardAccess
    let keys: KeystrokeSender
    var trusted: () -> Bool = { AXIsProcessTrusted() }
    var timeout: Duration = .milliseconds(250)
    var secureInput: () -> Bool = { IsSecureEventInputEnabled() }
    /// Vrai si sillon lui-même est l'application au premier plan : jamais de ⌘C vers soi.
    var ownAppFrontmost: () -> Bool = {
        NSWorkspace.shared.frontmostApplication?.processIdentifier == getpid()
    }

    func outcome() async -> CaptureOutcome {
        guard trusted() else { return .unsupported }
        if secureInput() || ownAppFrontmost() { return .veto }
        var saved: [SavedItem]? = pasteboard.save()
        defer { saved = nil }
        let before = pasteboard.changeCount
        keys.sendCopy()
        let deadline = ContinuousClock.now + timeout
        while pasteboard.changeCount == before {
            if ContinuousClock.now >= deadline { break }
            try? await Task.sleep(for: .milliseconds(10))
        }
        // Dernier coup d'œil : une copie tardive ne doit pas écraser le presse-papiers
        // de l'utilisateur. Rien n'a changé : on n'y touche pas.
        guard pasteboard.changeCount != before else { return .unsupported }
        // Écriture en deux temps (effacer, puis poser le texte) : on laisse le texte arriver.
        var seen = pasteboard.changeCount
        let settle = ContinuousClock.now + .milliseconds(100)
        while ContinuousClock.now < settle {
            try? await Task.sleep(for: .milliseconds(20))
            let now = pasteboard.changeCount
            if now == seen && pasteboard.string() != nil { break }
            seen = now
        }
        let copied = pasteboard.string()
        pasteboard.restore(saved ?? [])
        saved = nil
        if let text = nonBlank(copied) { return .text(text) }
        return .noSelection
    }
}

/// Essaie chaque source dans l'ordre ; la première qui répond l'emporte.
/// Un veto, ou « rien de sélectionné » répondu par l'application, arrête tout.
struct CaptureChain {
    let sources: [TextSource]

    func capture() async -> String? {
        for source in sources {
            switch await source.outcome() {
            case .text(let text): return text
            case .noSelection, .veto: return nil
            case .unsupported: continue
            }
        }
        return nil
    }
}
