import AppKit

/// Point d'entrée AppKit : le lecteur vit aussi longtemps que l'app.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static private(set) var shared: AppDelegate!

    /// Lancée par les tests d'interface : lecteur muet, texte d'exemple.
    let isUITesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    private(set) var reader: SillonReader!

    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.shared = self
        reader = isUITesting ? SillonReader.silent() : SillonReader()
    }
}
