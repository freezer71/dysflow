import AppKit
import CoreGraphics

final class SystemPasteboard: PasteboardAccess {
    private let board = NSPasteboard.general

    var changeCount: Int { board.changeCount }

    /// Tous les éléments, sous tous leurs types : une image copiée revient
    /// intacte, pas seulement le texte.
    func save() -> [[String: Data]] {
        (board.pasteboardItems ?? []).map { item in
            var out: [String: Data] = [:]
            for type in item.types {
                if let data = item.data(forType: type) { out[type.rawValue] = data }
            }
            return out
        }
    }

    func restore(_ items: [[String: Data]]) {
        board.clearContents()
        let restored = items.map { saved -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in saved { item.setData(data, forType: NSPasteboard.PasteboardType(type)) }
            return item
        }
        if !restored.isEmpty { board.writeObjects(restored) }
    }

    func string() -> String? { board.string(forType: .string) }
}

/// ⌘C envoyé à l'application au premier plan. Les modificateurs du raccourci
/// encore enfoncés (⌃⌥) sont écrasés : l'événement porte ⌘ seul.
struct SystemKeystrokes: KeystrokeSender {
    func sendCopy() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let c: CGKeyCode = 8
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: c, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cghidEventTap)
        }
    }
}
