import AppKit
import KeyboardShortcuts

/// Point d'entrée AppKit : le lecteur vit aussi longtemps que l'app.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static private(set) var shared: AppDelegate!

    /// Lancée par les tests d'interface : lecteur muet, texte d'exemple.
    let isUITesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    /// Hôte des tests unitaires : ni ouverture à la connexion, ni alerte, ni
    /// raccourcis globaux (ils s'inscriraient pour l'app de développement).
    let isUnitTesting = ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
    private(set) var reader: SillonReader!
    private(set) var model: ReaderModel!
    private(set) var panel: PanelController!

    func applicationDidFinishLaunching(_ notification: Notification) {
        ReadingFont.registerBundled()
        Self.shared = self
        reader = isUITesting ? SillonReader.silent() : SillonReader()
        model = ReaderModel(reader: reader)
        model.refresh()
        panel = PanelController(model: model)
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.show() }
        }
        guard !isUITesting, !isUnitTesting else { return }
        LoginItem.enableOnFirstLaunch()
        Onboarding.runIfNeeded()
        installShortcuts()
    }

    /// « Lire » capture d'abord (l'application au premier plan garde le
    /// clavier pour le ⌘C simulé), puis montre le panneau sans activer sillon.
    private func installShortcuts() {
        let pasteboard = SystemPasteboard()
        let chain = CaptureChain(sources: [
            SelectionSource(),
            CopySource(pasteboard: pasteboard, keys: SystemKeystrokes()),
        ])
        KeyboardShortcuts.onKeyUp(for: .readSelection) { [unowned self] in
            Task { @MainActor in
                await ReadFlow(
                    prepare: { self.model.prepareVoice() },
                    capture: { await chain.capture() },
                    open: { self.model.open(text: $0, source: "sélection") },
                    clipboardIsConcealed: { pasteboard.isConcealed },
                    openClipboard: { try await self.model.openClipboard() },
                    say: { self.model.message = $0 },
                    show: { self.panel.show() }
                ).run()
            }
        }
        KeyboardShortcuts.onKeyUp(for: .showPanel) { [unowned self] in self.panel.show() }
    }

    static let sample = """
    # Les monades

    Une monade est une structure qui encapsule une valeur et un contexte. \
    Les monades permettent d'enchaîner des calculs qui portent un contexte, \
    sans le manipuler à la main à chaque étape.

    ## Exemples

    On les retrouve partout : options, listes, promesses.
    """
}
