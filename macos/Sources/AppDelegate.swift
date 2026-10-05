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
            if ProcessInfo.processInfo.arguments.contains("--show-settings") {
                NSApp.activate()
                NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
            }
        }
        guard !isUITesting, !isUnitTesting else { return }
        // Les raccourcis d'abord : l'alerte d'accessibilité bloque (runModal).
        installShortcuts()
        LoginItem.enableOnFirstLaunch()
        Onboarding.runIfNeeded()
    }

    /// Une seule lecture à la fois, raccourci ou menu.
    let readGate = ReadGate()

    /// « Lire » : la capture d'abord (l'application au premier plan garde le
    /// clavier pour le ⌘C simulé), puis le panneau, sans activer sillon.
    /// `capture` nil : le presse-papiers seul (depuis le menu).
    func read(capture: (() async -> CaptureOutcome)? = nil) async {
        let pasteboard = SystemPasteboard()
        let model = model!, panel = panel!
        await readGate.run {
            await ReadFlow(
                prepare: { model.prepareVoice() },
                capture: { await capture?() ?? .unsupported },
                open: { model.open(text: $0, source: $1) },
                readClipboard: { ClipboardSnapshot(pasteboard: pasteboard).read() },
                openImage: { try await model.openClipboard() },
                say: { model.message = $0 },
                show: { panel.show() }
            ).run()
        }
    }

    private func installShortcuts() {
        let chain = CaptureChain(sources: [
            SelectionSource(),
            CopySource(pasteboard: SystemPasteboard(), keys: SystemKeystrokes()),
        ])
        KeyboardShortcuts.onKeyUp(for: .readSelection) { [unowned self] in
            Task { @MainActor in await self.read(capture: { await chain.capture() }) }
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
