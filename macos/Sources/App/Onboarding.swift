import AppKit
import ApplicationServices

/// Au premier lancement, dire pourquoi sillon demande l'accessibilité, et
/// mener au bon réglage. Sans elle, le presse-papiers seul.
@MainActor
enum Onboarding {
    private static let doneKey = "accessibilityExplained"
    private static let remindedKey = "accessibilityReminded"

    static func runIfNeeded() {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: doneKey) else { return }
        UserDefaults.standard.set(true, forKey: doneKey)
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = "Lire ce que vous sélectionnez"
        alert.informativeText = """
        Pour lire le texte sélectionné dans n'importe quelle application, \
        sillon a besoin de l'autorisation Accessibilité. Sans elle, il lit \
        seulement ce que vous copiez.
        """
        alert.addButton(withTitle: "Ouvrir les réglages")
        alert.addButton(withTitle: "Plus tard")
        if alert.runModal() == .alertFirstButtonReturn {
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
        }
    }

    /// Le menu le rappelle une fois, discrètement.
    static var needsReminder: Bool {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: remindedKey) else { return false }
        return true
    }

    static func reminded() { UserDefaults.standard.set(true, forKey: remindedKey) }
}
