import AppKit
import CoreGraphics

final class SystemPasteboard: PasteboardAccess {
    private let board = NSPasteboard.general

    var changeCount: Int { board.changeCount }

    /// Tous les éléments, sous tous leurs types et dans leur ordre : une image copiée
    /// revient intacte. Les promesses (fichiers, fournisseurs paresseux) sont ignorées.
    func save() -> [SavedItem] {
        (board.pasteboardItems ?? []).map { item in
            SavedItem(types: item.types.compactMap { type in
                guard !isPromised(type.rawValue), let data = item.data(forType: type) else { return nil }
                return SavedType(type: type.rawValue, data: data)
            })
        }
    }

    func restore(_ items: [SavedItem]) {
        board.clearContents()
        let restored = itemsForRestore(items).map { saved -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for t in saved.types { item.setData(t.data, forType: NSPasteboard.PasteboardType(t.type)) }
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
