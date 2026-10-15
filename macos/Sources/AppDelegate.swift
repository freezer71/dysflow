import AppKit
import KeyboardShortcuts
import MediaPlayer
import UniformTypeIdentifiers

/// Point d'entrée AppKit : le lecteur vit aussi longtemps que l'app.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// Gardé pour la fenêtre des réglages seulement (`ReglagesView` y
    /// dépose son magasin, `Announcement` y lit `isUnitTesting`). Le reste
    /// de l'app reçoit ce qu'il lui faut : `services` par l'environnement
    /// SwiftUI, le reste en paramètre.
    static private(set) weak var shared: AppDelegate?

    /// Ce que les scènes SwiftUI lisent (`.environment(services)`).
    let services = AppServices()

    /// Lancée par les tests d'interface : lecteur muet, texte d'exemple.
    /// Toujours faux dans l'app livrée (Release).
    let isUITesting = AppDelegate.isUITestLaunch(ProcessInfo.processInfo.arguments)
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
        isUITestLaunch(arguments) || environment["XCTestConfigurationFilePath"] != nil
    }

    /// `--ui-testing` : la mise en scène des tests d'interface n'existe
    /// qu'en Debug ; l'app livrée ignore l'argument.
    nonisolated static func isUITestLaunch(_ arguments: [String]) -> Bool {
        #if DEBUG
        arguments.contains("--ui-testing")
        #else
        false
        #endif
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
    /// Lancé par macOS à l'ouverture de session : rien ne s'impose.
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
                panel.show()
            }
        }
    }

    /// ⌘O, « Ouvrir un fichier… » : le panneau d'ouverture, au-dessus du
    /// lecteur (qui flotte). Une app de barre des menus passe d'abord au
    /// premier plan, sinon le panneau s'ouvrirait derrière l'application active.
    func chooseFile() {
        let chooser = NSOpenPanel()
        chooser.allowedContentTypes = OpenableFiles.types
        chooser.allowsMultipleSelection = false
        chooser.canChooseDirectories = false
        chooser.message = "Choisissez un texte, un PDF ou une image à lire"
        chooser.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)
        NSApp.activate()
        chooser.begin { [weak self] response in
            guard response == .OK, let url = chooser.url else { return }
            MainActor.assumeIsolated { self?.openFile(url) }
        }
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        // L'événement Apple du lancement n'est lisible qu'ici.
        launchedAtLogin = LaunchKind.launchedAtLogin()
        launchedWithFiles = LaunchKind.launchedWithFiles()
    }

    /// sillon déjà lancé, rouvert depuis le Dock, le Finder, Spotlight ou un
    /// lanceur : le lecteur.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        guard let model else { return false }
        model.refresh()
        present(LaunchKind.reopenPresentation(hasDocument: model.snapshot.hasDocument))
        return false
    }

    /// Montre ce que décide `LaunchKind`. Le lecteur vide, toujours en
    /// grand ; la dernière taille, par « Afficher ».
    private func present(_ presentation: LaunchPresentation) {
        guard let panel else { return }
        switch presentation {
        case .nothing: break
        case .emptyReader: panel.apply(.choose(.reader))
        case .lastSize: panel.show()
        }
    }

    /// Le modèle ne suit rien en pause et en arrière-plan : à l'ouverture
    /// d'un menu (celui de la barre des menus compris), l'état est relu une
    /// fois. Aucune minuterie, rien entre deux ouvertures.
    @discardableResult
    static func refreshWhenAMenuOpens(center: NotificationCenter = .default,
                                      refresh: @escaping @MainActor @Sendable () -> Void) -> NSObjectProtocol {
        center.addObserver(forName: NSMenu.didBeginTrackingNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { refresh() }
        }
    }
    /// Le magasin de la fenêtre des réglages tant qu'elle existe.
    weak var settingsStore: SettingsStore?

    /// Un réglage qui attend sa pause part avant la fin : macOS attend
    /// (`.terminateLater`) que son écriture finisse, 1,5 s au plus, sans que
    /// la boucle principale tourne à la main. Pendant l'attente, AppKit fait
    /// tourner la boucle dans un mode où l'acteur principal est servi.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let store = settingsStore else { return .terminateNow }
        terminationPending = true
        Task { @MainActor [weak self] in
            await store.flush()
            self?.allowTermination()
        }
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.terminationDeadline)
            self?.allowTermination()
        }
        return .terminateLater
    }

    static let terminationDeadline: Duration = .milliseconds(1500)
    private var terminationPending = false

    /// L'écriture finie ou le délai passé, le premier des deux : la fin reprend.
    private func allowTermination() {
        guard terminationPending else { return }
        terminationPending = false
        NSApp.reply(toApplicationShouldTerminate: true)
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
        #if DEBUG || SILLON_MESURE
        // Mode mesure : seul le lecteur et les mesures tournent. Retour avant le
        // modèle, le panneau, les raccourcis, l'ouverture à la connexion et
        // l'accueil ; sans l'argument, rien ne change. Absent de l'app livrée :
        // `scripts/measure-macos.sh` construit sa propre copie (SILLON_MESURE).
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
        #endif
        #if DEBUG
        // Les tests d'interface posent leur mise en scène sur le lecteur : le
        // modèle n'en sait rien.
        if isUITesting { reader = UITestStaging.reader(reader, arguments: ProcessInfo.processInfo.arguments) }
        #endif
        model = ReaderModel(reader: reader)
        model.refresh()
        let settings = SettingsOpener.shared
        panel = PanelController(model: model, openSettings: { settings.open() },
                                readClipboard: { [weak self] in Task { @MainActor in await self?.read() } },
                                chooseFile: { [weak self] in self?.chooseFile() },
                                openFile: { [weak self] in self?.openFile($0) })
        transport = Transport(model: model)
        services.model = model
        services.panel = panel
        services.readSelection = { [weak self] in await self?.readSelection() }
        services.chooseFile = { [weak self] in self?.chooseFile() }
        menuObserver = Self.refreshWhenAMenuOpens { [model] in model?.refresh() }
        outputWatcher = AudioDeviceWatcher { [weak model] in model?.refreshOutputs() }
        Task { [model] in await model?.loadOutput() }
        if !readerIsSilent { installNowPlaying() }
        if isUITesting {
            let arguments = ProcessInfo.processInfo.arguments
            // `--ui-testing-vide` : rien d'ouvert, comme un premier lancement.
            if !arguments.contains("--ui-testing-vide") { model.open(text: Self.sample, source: "exemple") }
            if arguments.contains("--show-panel") { panel.apply(.choose(.reader)) }
            if arguments.contains("--show-settings") {
                // L'icône de la barre des menus donne l'action dès qu'elle paraît.
                settings.open()
            }
            // Le seul ⌃⌥S, pour son test d'interface (quitter d'abord le
            // sillon installé, qui tiendrait le même raccourci).
            if arguments.contains("--ui-testing-raccourcis") { installToggleShortcut() }
            // Le chemin d'un lancement fait à la main, sous les tests.
            if arguments.contains("--ui-testing-lancement-manuel") {
                model.refresh()
                present(LaunchKind.presentation(atLogin: false, withFiles: false,
                                                hasDocument: model.snapshot.hasDocument))
            }
        }
        guard !isUITesting, !isUnitTesting else { return }
        // Police et taille : des préférences vers config.toml, une fois.
        Task { @MainActor [model = model!, panel = panel!] in
            model.refresh()
            if await ReadingPreferencesMigration.run(defaults: .standard, backend: model,
                                                     configUnreadable: model.snapshot.configUnreadable) {
                panel.reloadReading()
            }
        }
        // Les raccourcis d'abord : l'alerte d'accessibilité bloque (runModal).
        installShortcuts()
        LoginItem.enableOnFirstLaunch()
        Onboarding.runIfNeeded()
        startKokoro()
        // Lancé avec un fichier : le lecteur sur lui. Lancé à la main : le
        // lecteur. À la connexion : rien.
        if let file = pendingFiles.last {
            pendingFiles = []
            openFile(file)
        } else {
            present(LaunchKind.presentation(atLogin: launchedAtLogin, withFiles: launchedWithFiles,
                                            hasDocument: model.snapshot.hasDocument))
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

    /// Le passage à Kokoro (une fois, avec un avis), l'installation
    /// automatique de la voix, puis la préparation des noyaux Metal, maintenant et à chaque fois que la voix active a pu
    /// changer.
    private func startKokoro() {
        model.onVoiceMayHaveChanged = { [weak self] in self?.kokoroBecameActive() }
        // Le passage écrit `config.toml` : hors du fil principal, puis la
        // préparation, sur la voix qui en sort.
        migration = Task { @MainActor [weak self] in
            guard let self, let model = self.model else { return }
            // L'avis du passage (s'il y en a un) se dit une fois dans le lecteur.
            let engine = await model.migrateVoice()
            if let notice = Self.switchNotice(engine: engine, offer: model.snapshot.offer) {
                model.message = notice
                self.panel.post(notice)
            }
            // Puis la voix s'installe seule s'il le faut (rapide : le
            // téléchargement, lui, court à part).
            await model.installDefaultVoice()
            self.kokoroBecameActive()
        }
    }

    /// L'avis du lancement : seulement un passage fait. Une proposition
    /// (`offer`) n'a plus de bouton pour y répondre : sa phrase se tait, la
    /// voix par défaut l'accepte d'elle-même.
    static func switchNotice(engine: String?, offer: VoiceOfferView?) -> String? {
        offer == nil ? engine : nil
    }

    /// Le passage au lancement, gardé : une lecture l'attend après la
    /// capture (le ⌘C simulé ne patiente pas), pour partir sur la bonne voix.
    private var migration: Task<Void, Never>?

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
    func read(capture: (@MainActor () async -> CaptureOutcome)? = nil) async {
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
                // La forme en cours ; masqué, la dernière taille.
                show: { panel.show() }
            ).run()
        }
    }

    /// « Lire la sélection » du menu : comme ⌃⌥Espace ; sans la chaîne de
    /// capture (sous les tests), le presse-papiers seul.
    func readSelection() async {
        if let chain = captureChain {
            await read(capture: { await chain.capture() })
        } else {
            await read()
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

    /// La sélection, sinon la copie simulée : ce que le raccourci « Lire » capture.
    private var captureChain: CaptureChain?

    private func installShortcuts() {
        captureChain = CaptureChain(sources: [
            SelectionSource(),
            CopySource(pasteboard: SystemPasteboard(), keys: SystemKeystrokes()),
        ])
        KeyboardShortcuts.onKeyUp(for: .readSelection) { [unowned self] in
            Task { @MainActor in await self.readSelection() }
        }
        installToggleShortcut()
    }

    /// ⌃⌥S : afficher le lecteur dans sa dernière taille, ou le masquer.
    private func installToggleShortcut() {
        KeyboardShortcuts.onKeyUp(for: .showPanel) { [unowned self] in self.panel.toggleVisibility() }
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
