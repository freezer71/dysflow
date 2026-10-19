import AppKit
import ApplicationServices

/// Au premier lancement, dire pourquoi sillage demande l'accessibilité, et
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
        alert.messageText = String(localized: "Read what you select")
        alert.informativeText = String(localized: """
        To read selected text in any app, sillage needs the Accessibility \
        permission. Without it, it only reads what you copy.
        """)
        alert.addButton(withTitle: String(localized: "Open System Settings"))
        alert.addButton(withTitle: String(localized: "Later"))
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
