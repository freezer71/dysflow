import AppKit
import ApplicationServices

/// Une façon d'obtenir le texte que l'utilisateur veut entendre.
protocol TextSource {
    func text() async -> String?
}

private func nonBlank(_ s: String?) -> String? {
    guard let s, !s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
    return s
}

/// La sélection, lue directement dans l'application au premier plan.
/// Instantané, et le presse-papiers n'est pas touché.
struct SelectionSource: TextSource {
    var trusted: () -> Bool = { AXIsProcessTrusted() }

    func text() async -> String? {
        guard trusted() else { return nil }
        let system = AXUIElementCreateSystemWide()
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString,
                                            &focused) == .success,
              let element = focused, CFGetTypeID(element) == AXUIElementGetTypeID()
        else { return nil }
        var selected: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element as! AXUIElement,
                                            kAXSelectedTextAttribute as CFString,
                                            &selected) == .success
        else { return nil }
        return nonBlank(selected as? String)
    }
}

protocol PasteboardAccess: AnyObject {
    var changeCount: Int { get }
    func save() -> [[String: Data]]
    func restore(_ items: [[String: Data]])
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

    func text() async -> String? {
        guard trusted() else { return nil }
        let saved = pasteboard.save()
        let before = pasteboard.changeCount
        keys.sendCopy()
        let deadline = ContinuousClock.now + timeout
        while pasteboard.changeCount == before {
            if ContinuousClock.now >= deadline { return nil }
            try? await Task.sleep(for: .milliseconds(10))
        }
        let copied = pasteboard.string()
        pasteboard.restore(saved)
        return nonBlank(copied)
    }
}

/// Essaie chaque source dans l'ordre ; la première qui répond l'emporte.
struct CaptureChain {
    let sources: [TextSource]

    func capture() async -> String? {
        for source in sources {
            if let text = await source.text() { return text }
        }
        return nil
    }
}
