import AppKit
import KeyboardShortcuts
import MediaPlayer

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
    /// Le lecteur est muet et hors disque (tests d'interface, tests unitaires).
    private(set) var readerIsSilent = false

    /// Sous XCTest (tests unitaires) comme sous les tests d'interface, le
    /// lecteur est muet et n'écrit rien : jamais ~/.config/sillon ni
    /// ~/Library/Application Support/sillon.
    nonisolated static func usesSilentReader(arguments: [String], environment: [String: String]) -> Bool {
        arguments.contains("--ui-testing") || environment["XCTestConfigurationFilePath"] != nil
    }
    private(set) var model: ReaderModel!
    private(set) var panel: PanelController!
    /// Lecture, pause, phrases et timeline venues d'ailleurs que du panneau.
    private(set) var transport: Transport!
    /// « À l'écoute » et les touches média ; jamais avec le lecteur muet des
    /// tests (il prendrait les touches média de l'utilisateur).
    private(set) var nowPlaying: NowPlaying?
    /// Le magasin de la fenêtre des réglages tant qu'elle existe.
    weak var settingsStore: SettingsStore?

    /// Un réglage qui attend sa pause part avant la fin, attente bornée.
    func applicationWillTerminate(_ notification: Notification) {
        guard let store = settingsStore else { return }
        var done = false
        Task { @MainActor in await store.flush(); done = true }
        let deadline = Date().addingTimeInterval(1.5)
        while !done, Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        ReadingFont.registerBundled()
        Self.shared = self
        readerIsSilent = Self.usesSilentReader(arguments: ProcessInfo.processInfo.arguments,
                                               environment: ProcessInfo.processInfo.environment)
        reader = readerIsSilent ? SillonReader.silent() : SillonReader()
        // Mode mesure : seul le lecteur et les mesures tournent. Retour avant le
        // modèle, le panneau, les raccourcis, l'ouverture à la connexion et
        // l'accueil ; sans l'argument, rien ne change.
        if ProcessInfo.processInfo.arguments.contains("--measure"), !isUnitTesting {
            Task { @MainActor [reader = reader!] in
                await Measure.run(reader: reader)
                NSApp.terminate(nil)
            }
            return
        }
        model = ReaderModel(reader: reader)
        model.refresh()
        panel = PanelController(model: model)
        transport = Transport(model: model)
        if !readerIsSilent { installNowPlaying() }
        // Le mode laissé à la dernière fermeture (sous les tests : suite
        // vidée, donc l'arrière-plan).
        panel.restore()
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.apply(.choose(.medium)) }
            if ProcessInfo.processInfo.arguments.contains("--show-settings") {
                // L'icône de la barre des menus donne l'action dès qu'elle paraît.
                SettingsOpener.shared.open()
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
                openImage: { try await model.openImage($0) },
                say: { model.message = $0 },
                // Le mode en cours ; depuis l'arrière-plan, le moyen.
                show: { panel.apply(.readSelection) }
            ).run()
        }
    }

    private func installNowPlaying() {
        let model = model!
        let nowPlaying = NowPlaying(
            center: MPNowPlayingInfoCenter.default(),
            remote: SystemRemoteCommands(),
            transport: transport,
            restDelay: { NowPlaying.restDelay(keepVoiceMinutes: await model.loadSettings().keepVoiceMinutes) },
            refresh: { model.refresh() })
        model.onChange = { snapshot, document in nowPlaying.update(snapshot, document: document) }
        self.nowPlaying = nowPlaying
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
