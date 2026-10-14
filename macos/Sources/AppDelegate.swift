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
    /// Le calcul MLX de Kokoro ; jamais avec le lecteur muet des tests.
    private(set) var kokoroCompute: KokoroMLXCompute?
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
    private var menuObserver: NSObjectProtocol?
    /// Les sorties audio branchées ou retirées : la liste se relit.
    private var outputWatcher: AudioDeviceWatcher?
    /// Les pages de notes ; leur fenêtre s'ouvre quand on lance sillon soi-même.
    private(set) var notes: NotesWindowController?
    /// Lancé par macOS à l'ouverture de session : la fenêtre des notes ne
    /// s'impose pas.
    private var launchedAtLogin = false

    /// Fichiers reçus avant que le lecteur soit prêt (au lancement, macOS
    /// les donne avant `applicationDidFinishLaunching`).
    private var pendingFiles: [URL] = []
    private var launchedWithFiles = false

    /// `sillon fichier.pdf`, « Ouvrir avec », un fichier posé sur l'icône :
    /// le panneau s'ouvre sur lui. Un seul document à la fois : le dernier.
    func application(_ application: NSApplication, open urls: [URL]) {
        let files = urls.filter(\.isFileURL)
        guard !files.isEmpty else { return }
        guard model != nil, panel != nil else {
            pendingFiles = files
            return
        }
        openFile(files.last!)
    }

    /// La fenêtre des notes fermée : rien à lire, le panneau se retire aussi
    /// (sillon reste dans la barre des menus) ; en pleine lecture, il reste
    /// pour garder la main sur le son.
    private func notesClosed() {
        guard !isUITesting, !isUnitTesting, let model, let panel else { return }
        model.refresh()
        if Self.hidesPanelWhenNotesClose(playing: model.snapshot.playing) { panel.apply(.hide) }
    }

    nonisolated static func hidesPanelWhenNotesClose(playing: Bool) -> Bool { !playing }

    private func openFile(_ url: URL) {
        guard let model, let panel else { return }
        Task { @MainActor in
            await readGate.run {
                model.prepareVoice()
                do {
                    try await model.openFile(url)
                } catch {
                    model.message = "Impossible d'ouvrir « \(url.lastPathComponent) »"
                }
                panel.apply(.readSelection)
            }
        }
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        // L'événement Apple du lancement n'est lisible qu'ici.
        launchedAtLogin = LaunchKind.launchedAtLogin()
        launchedWithFiles = LaunchKind.launchedWithFiles()
    }

    /// sillon déjà lancé, rouvert depuis le Dock, le Finder, Spotlight ou un
    /// lanceur : la page de notes.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        notes?.show()
        return false
    }

    /// Le modèle ne suit rien en pause et en arrière-plan : à l'ouverture
    /// d'un menu (celui de la barre des menus compris), l'état est relu une
    /// fois. Aucune minuterie, rien entre deux ouvertures.
    @discardableResult
    static func refreshWhenAMenuOpens(center: NotificationCenter = .default,
                                      refresh: @escaping @MainActor () -> Void) -> NSObjectProtocol {
        center.addObserver(forName: NSMenu.didBeginTrackingNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { refresh() }
        }
    }
    /// Le magasin de la fenêtre des réglages tant qu'elle existe.
    weak var settingsStore: SettingsStore?

    /// Un réglage qui attend sa pause part avant la fin, attente bornée.
    func applicationWillTerminate(_ notification: Notification) {
        notes?.store.flush()
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
        if readerIsSilent {
            reader = SillonReader.silent()
        } else {
            Self.exportEspeakLocation()
            let compute = KokoroMLXCompute()
            kokoroCompute = compute
            reader = SillonReader.withKokoro(compute: compute)
        }
        // Mode mesure : seul le lecteur et les mesures tournent. Retour avant le
        // modèle, le panneau, les raccourcis, l'ouverture à la connexion et
        // l'accueil ; sans l'argument, rien ne change.
        if ProcessInfo.processInfo.arguments.contains("--measure-charge"), !isUnitTesting {
            Task { @MainActor [reader = reader!, compute = kokoroCompute] in
                await Measure.charge(reader: reader, compute: compute)
                NSApp.terminate(nil)
            }
            return
        }
        if ProcessInfo.processInfo.arguments.contains("--measure"), !isUnitTesting {
            Task { @MainActor [reader = reader!] in
                await Measure.run(reader: reader)
                NSApp.terminate(nil)
            }
            return
        }
        model = ReaderModel(reader: reader)
        if isUITesting {
            model.staged = UITestStaging.staging(for: ProcessInfo.processInfo.arguments)
            model.stagedCapabilities = UITestStaging.capabilities(for: ProcessInfo.processInfo.arguments)
        }
        model.refresh()
        panel = PanelController(model: model)
        transport = Transport(model: model)
        // Sous les tests, un dossier jetable : jamais les notes de l'utilisateur.
        let notesDirectory = readerIsSilent
            ? FileManager.default.temporaryDirectory.appendingPathComponent("sillon-notes-\(UUID().uuidString)")
            : NotesStore.userDirectory
        notes = NotesWindowController(store: NotesStore(directory: notesDirectory), read: { [model, panel] page in
            guard let model, let panel else { return }
            model.open(text: page.markdown, source: "note")
            panel.apply(.readSelection)
        }, closed: { [weak self] in self?.notesClosed() })
        menuObserver = Self.refreshWhenAMenuOpens { [model] in model?.refresh() }
        outputWatcher = AudioDeviceWatcher { [weak model] in model?.refreshOutputs() }
        Task { [model] in await model?.loadOutput() }
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
            if ProcessInfo.processInfo.arguments.contains("--show-notes") { notes?.show() }
        }
        guard !isUITesting, !isUnitTesting else { return }
        // Les raccourcis d'abord : l'alerte d'accessibilité bloque (runModal).
        installShortcuts()
        LoginItem.enableOnFirstLaunch()
        Onboarding.runIfNeeded()
        startKokoro()
        // Lancé avec un fichier : le panneau sur lui. Lancé à la main : la
        // page de notes, comme une app ordinaire.
        if let file = pendingFiles.last {
            pendingFiles = []
            openFile(file)
        } else if !launchedAtLogin, !launchedWithFiles {
            notes?.show()
        }
    }

    /// sillon-espeak (la phonétisation de Kokoro) livré dans l'app : le
    /// moteur le trouve par ces variables, posées avant la première voix.
    nonisolated static func exportEspeakLocation(bundle: Bundle = .main) {
        let helper = bundle.bundleURL.appendingPathComponent("Contents/Helpers/sillon-espeak")
        setenv("SILLON_ESPEAK", helper.path, 1)
        if let resources = bundle.resourceURL {
            setenv("SILLON_ESPEAK_DATA", resources.path, 1)
        }
    }

    /// Le passage à Kokoro (une fois, avec un avis), puis la préparation des
    /// noyaux Metal, maintenant et à chaque fois que la voix active a pu
    /// changer.
    private func startKokoro() {
        model.onVoiceMayHaveChanged = { [weak self] in self?.kokoroBecameActive() }
        // Le passage écrit `config.toml` : hors du fil principal, puis la
        // préparation, sur la voix qui en sort.
        migration = Task { @MainActor [weak self] in
            guard let self, let model = self.model else { return }
            if let notice = Self.launchNotice(engine: await model.migrateVoice(), offer: model.snapshot.offer) {
                model.message = notice
                self.panel.post(notice)
            }
            self.kokoroBecameActive()
        }
    }

    /// Le passage au lancement, gardé : une lecture l'attend après la
    /// capture (le ⌘C simulé ne patiente pas), pour partir sur la bonne voix.
    private var migration: Task<Void, Never>?

    /// L'avis du lancement : une offre en attente se dit à la façon de
    /// l'app (taille comprise, où y répondre) ; sinon l'avis du moteur
    /// (passage fait).
    static func launchNotice(engine: String?, offer: VoiceOfferView?) -> String? {
        if let offer { return VoiceTab.launchNotice(offer) }
        return engine
    }

    /// Kokoro est peut-être devenu la voix active (lancement, voix choisie,
    /// proposition acceptée, téléchargement fini) : les noyaux Metal se
    /// préparent hors du fil principal, à chaque activation (voir
    /// `KokoroWarmUp`), pour que leur préparation ne tombe jamais sur le
    /// premier mot lu.
    func kokoroBecameActive() {
        guard let compute = kokoroCompute, let model else { return }
        let version = KokoroWarmUp.version(of: .main)
        guard !model.voicePreparation.working else { return }
        Task { @MainActor in
            let settings = await model.loadSettings()
            KokoroWarmUp.runIfNeeded(compute: compute,
                                     modelDirectory: KokoroWarmUp.modelDirectory(settings: settings),
                                     version: version, preparation: model.voicePreparation)
        }
    }

    /// Une seule lecture à la fois, raccourci ou menu.
    let readGate = ReadGate()

    /// « Lire » : la capture d'abord (l'application au premier plan garde le
    /// clavier pour le ⌘C simulé), puis le panneau, sans activer sillon.
    /// `capture` nil : le presse-papiers seul (depuis le menu). En mode
    /// mesure, ni modèle ni panneau : rien à faire (le menu plantait).
    func read(capture: (() async -> CaptureOutcome)? = nil) async {
        guard let model, let panel else { return }
        let pasteboard = SystemPasteboard()
        await readGate.run {
            await ReadFlow(
                prepare: { model.prepareVoice() },
                capture: { [migration] in
                    let outcome = await capture?() ?? .unsupported
                    await migration?.value
                    return outcome
                },
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
        // Sans document, rien ne part (Transport).
        KeyboardShortcuts.onKeyUp(for: .playPause) { [unowned self] in _ = self.transport.perform(.toggle) }
        KeyboardShortcuts.onKeyUp(for: .nextSentence) { [unowned self] in _ = self.transport.perform(.next) }
        KeyboardShortcuts.onKeyUp(for: .previousSentence) { [unowned self] in _ = self.transport.perform(.previous) }
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
