import AppKit
import SwiftUI

/// Montre la fenêtre dans l'un des quatre modes — petit, moyen, grand,
/// arrière-plan — et route le clavier. Une seule fenêtre : chaque mode change
/// son contenu et sa taille ; l'arrière-plan la retire et la lecture continue.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Mode courant, dernier mode visible et place du mode petit, retenus
    /// d'un lancement à l'autre.
    @ObservationIgnored let store: DisplayModeStore
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var keyMonitor: Any?
    @ObservationIgnored private var contentHeight: CGFloat = 0
    @ObservationIgnored private var resizePending = false
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var announcer = Announcer()
    /// ↑ ↓ : la vue de lecture affichée (ou sa copie cachée du mode petit)
    /// dit où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()
    /// Vrai pendant que le code place la fenêtre : seul un déplacement de
    /// l'utilisateur fixe la place du mode petit.
    @ObservationIgnored private var placing = false
    @ObservationIgnored private var moveObserver: NSObjectProtocol?

    private(set) var mode: DisplayMode = .background
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture et à chaque réglage écrit.
    var reading = ReadingDisplay()
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false

    init(model: ReaderModel, store: DisplayModeStore? = nil) {
        self.model = model
        self.store = store ?? DisplayModeStore(defaults: Self.processDefaults)
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
        host.setAccessibilityLabel("Panneau de lecture")
        // Toute fermeture (⌘W, close(), performClose:) passe à l'arrière-plan.
        let guardian = PanelCloseGuard { [weak self] in self?.apply(.hide) }
        closeGuard = guardian
        window.delegate = guardian
        observeSettingsWindow()
        observeMoves()
    }

    /// Les préférences du processus, lues une fois : sous les tests, chaque
    /// appel à `processDefaults()` vide la suite.
    private static let processDefaults = DisplayModeStore.processDefaults()

    private var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    /// L'écran principal (celui de la barre des menus).
    private var primaryScreenFrame: CGRect {
        NSScreen.screens.first?.visibleFrame ?? screenFrame
    }

    /// La largeur du texte du mode moyen : la copie cachée du mode petit la
    /// garde, pour que ↑ ↓ visent les mêmes lignes.
    var shadowTextWidth: CGFloat {
        PanelGeometry.panelFrame(contentHeight: 0, screen: screenFrame).width
            - 2 * PanelGeometry.horizontalPadding
    }

    // MARK: - Modes

    /// Une transition (⌘1/2/3, Échap, ⌃⌥S, ⌃⌥Espace, fermeture), retenue
    /// puis appliquée.
    func apply(_ event: DisplayEvent) {
        let next = store.state.applying(event)
        store.state = next
        present(next.current)
    }

    /// ⌃⌥S, « Afficher sillon » : le dernier mode visible.
    func show() {
        apply(.showLast)
    }

    /// Au lancement : le mode retenu reparaît, sans prendre le clavier à
    /// l'application au premier plan. Arrière-plan : rien à montrer.
    func restore() {
        let current = store.state.current
        guard current.isVisible else { return }
        present(current, takeKey: false)
    }

    /// ⏎ : le grand mode, depuis le moyen comme depuis la barre.
    func expand() {
        guard mode == .small || mode == .medium else { return }
        apply(.choose(.large))
    }

    /// Échap : du grand au moyen, sinon l'arrière-plan.
    func collapseOrHide() {
        apply(.escape)
    }

    private func present(_ target: DisplayMode, takeKey: Bool = true) {
        guard target.isVisible else { hideWindow(); return }
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        let wasHidden = mode == .background || !window.isVisible
        if wasHidden || target != mode {
            mode = target
            applySizeLimits(for: target)
            place(frame(for: target), animated: !wasHidden)
            updateIdentifier()
        }
        if takeKey { window.makeKeyAndOrderFront(nil) } else { window.orderFrontRegardless() }
        // Le modèle ne suit l'état en pause (coup d'œil lent, 4 Hz) qu'au
        // moyen et au grand mode. La barre du mode petit suit la cadence
        // « lit sans être affiché » : en pause, elle ne coûte rien.
        model.isDisplayed = target != .small
        // L'annonceur ne voit rien en arrière-plan : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcer.reseed(with: model.snapshot)
        installKeyMonitor()
    }

    /// L'arrière-plan : la fenêtre se retire, la lecture continue. Le suivi à
    /// chaque image ne tourne plus que si la voix lit (cadence du modèle).
    private func hideWindow() {
        guard mode != .background else { return }
        mode = .background
        if window.isVisible { window.orderOut(nil) }
        model.isDisplayed = false
        removeKeyMonitor()
    }

    private func frame(for mode: DisplayMode) -> CGRect {
        switch mode {
        case .small:
            return SmallPlacement.frame(savedOrigin: store.smallOrigin,
                                        screens: NSScreen.screens.map(\.visibleFrame),
                                        main: primaryScreenFrame)
        case .large:
            return PanelGeometry.expandedFrame(screen: screenFrame)
        case .medium, .background:
            return PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame)
        }
    }

    /// Le mode petit ne se redimensionne pas ; les autres, si. Des bornes de
    /// taille ne tiendraient pas : la vue hôte réécrit celles de la fenêtre
    /// à chaque changement de contenu.
    private func applySizeLimits(for mode: DisplayMode) {
        if mode == .small {
            window.styleMask.remove(.resizable)
        } else {
            window.styleMask.insert(.resizable)
        }
    }

    private func place(_ frame: CGRect, animated: Bool) {
        placing = true
        defer { placing = false }
        if animated, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true, animate: true)
        } else {
            window.setFrame(frame, display: animated)
        }
    }

    private func observeMoves() {
        moveObserver = NotificationCenter.default.addObserver(
            // Livré tout de suite (`queue: nil`), pendant que `placing` est
            // vrai : une file différée verrait chaque placement du code comme
            // un déplacement de l'utilisateur.
            forName: NSWindow.didMoveNotification, object: window, queue: nil
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.windowMoved() }
        }
    }

    /// La barre déplacée par l'utilisateur : sa place est retenue.
    func windowMoved() {
        guard mode == .small, !placing else { return }
        store.smallOrigin = window.frame.origin
    }

    // MARK: - Réglages

    /// Relit ce que le panneau affiche des réglages, hors du fil principal :
    /// un changement dans la fenêtre des réglages s'y voit aussitôt.
    /// Des relectures qui se chevauchent : seule la dernière demandée compte.
    func reloadReading() {
        readingGeneration &+= 1
        let generation = readingGeneration
        Task {
            let loaded = ReadingDisplay(await model.loadSettings())
            guard generation == readingGeneration else { return }
            reading = loaded
        }
    }
    @ObservationIgnored private var readingGeneration = 0

    /// Fenêtre des réglages ouverte : le panneau reste affiché à côté, mais
    /// ne la recouvre plus, quel que soit le mode.
    private(set) var settingsVisible = false

    func settingsVisibilityChanged(_ visible: Bool) {
        guard visible != settingsVisible else { return }
        settingsVisible = visible
        window.level = PanelLevel.level(settingsVisible: visible)
    }

    private func observeSettingsWindow() {
        let center = NotificationCenter.default
        settingsObservers = [
            center.addObserver(forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] note in
                MainActor.assumeIsolated {
                    guard let self, let w = note.object as? NSWindow, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(true)
                    // Devant le panneau, sans passer au-dessus des autres apps.
                    w.orderFront(nil)
                }
            },
            center.addObserver(forName: NSWindow.willCloseNotification, object: nil, queue: .main) { [weak self] note in
                MainActor.assumeIsolated {
                    guard let self, let w = note.object as? NSWindow, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(false)
                }
            },
        ]
    }
    @ObservationIgnored private var settingsObservers: [NSObjectProtocol] = []

    // MARK: - Clavier

    /// L'action d'une touche, avec le pas d'avance rapide des réglages.
    func action(for press: KeyPress) -> ReaderAction? {
        KeyMap.action(for: press, seekStep: Int32(reading.seekStep))
    }

    /// La phrase à dire, ou rien : jamais en arrière-plan (fenêtre retirée).
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func announcementText(for s: Snapshot) -> String? {
        let text = announcer.announcement(for: s, doc: model.document)
        return mode == .background ? nil : text
    }

    /// Dit à VoiceOver un changement d'état (lecture, pause, section), jamais
    /// un mot. Sans effet quand VoiceOver est éteint.
    func announce(_ s: Snapshot) {
        guard let text = announcementText(for: s) else { return }
        lastAnnouncement = text
        announcementCount &+= 1
        guard NSWorkspace.shared.isVoiceOverEnabled else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }

    /// Ce que `announce` a dit en dernier (ou aurait dit, VoiceOver éteint)
    /// et combien de fois : un état de lecture, jamais un mot.
    @ObservationIgnored private(set) var lastAnnouncement: String?
    @ObservationIgnored private(set) var announcementCount = 0

    func contentHeightChanged(_ height: CGFloat) {
        guard height != contentHeight else { return }
        contentHeight = height
        // Appelé pendant la mise à jour SwiftUI : redimensionner la fenêtre
        // au milieu de cette passe est annulé. On le fait au tour suivant,
        // une seule fois pour toutes les hauteurs reçues entre-temps.
        guard !resizePending else { return }
        resizePending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.resizePending = false
            guard self.mode == .medium else { return }
            self.place(PanelGeometry.panelFrame(contentHeight: self.contentHeight,
                                                screen: self.screenFrame), animated: true)
        }
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .expand: expand()
        case .display(let target): apply(.choose(target))
        case .collapseOrHide:
            if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else { collapseOrHide() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            flash("Phrase copiée")
        case .openSearch:
            if mode != .large { apply(.choose(.large)) }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode == .large { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode != .large { apply(.choose(.large)) }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp:
            // La barre du mode petit n'a pas la place : l'aide s'ouvre au moyen.
            if mode == .small {
                apply(.choose(.medium))
                helpVisible = true
            } else {
                helpVisible.toggle()
            }
        case .openSettings:
            SettingsOpener.shared.open()
        case .openVoiceSettings:
            SettingsOpener.shared.open(tab: .voice)
        }
    }

    /// La voix reprend au premier mot de la ligne affichée voisine ; rien
    /// au-delà de la première ou de la dernière.
    private func moveLine(_ delta: Int) {
        let s = model.snapshot
        guard s.hasDocument else { return }
        guard let target = lines.target(word: s.word, sentence: s.sentence, delta: delta) else {
            flash("Aucune ligne de ce côté")
            return
        }
        model.send(.seekToWord(sentence: target.sentence, word: target.word))
    }

    /// Un message qui s'efface seul, sauf si un autre l'a remplacé.
    private func flash(_ text: String) {
        model.message = text
        messageToken &+= 1
        let token = messageToken
        DispatchQueue.main.asyncAfter(deadline: .now() + messageDuration) { [weak self] in
            guard let self, self.messageToken == token, self.model.message == text else { return }
            self.model.message = nil
        }
    }
    @ObservationIgnored private var messageToken = 0

    /// L'identifiant pour les tests, le nom pour VoiceOver.
    private func updateIdentifier() {
        let identifier: String
        let label: String
        switch mode {
        case .small:
            identifier = "lecteur-reduit"; label = "Lecteur réduit"
        case .large:
            identifier = "lecteur-agrandi"; label = "Lecteur agrandi"
        case .medium, .background:
            identifier = "panneau-lecture"; label = "Panneau de lecture"
        }
        window.contentView?.setAccessibilityIdentifier(identifier)
        window.contentView?.setAccessibilityLabel(label)
    }

    private func installKeyMonitor() {
        guard keyMonitor == nil else { return }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            // Un champ de saisie garde ses touches, sauf Échap.
            if self.window.firstResponder is NSTextView,
               !(self.window.firstResponder is ReadingTextView),
               event.keyCode != KeyCode.escape {
                return event
            }
            // Une liste (sommaire, signets) garde ses flèches verticales et
            // Entrée : on la parcourt, puis on y va.
            if self.window.firstResponder is NSTableView,
               [KeyCode.up, KeyCode.down, KeyCode.enter].contains(event.keyCode),
               !event.modifierFlags.contains(.option) {
                return event
            }
            guard let action = self.action(for: KeyPress(event)) else { return event }
            self.perform(action)
            return nil
        }
    }

    private func removeKeyMonitor() {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
    }
}

/// Le délégué de la fenêtre : une fermeture demandée devient l'arrière-plan.
@MainActor
private final class PanelCloseGuard: NSObject, NSWindowDelegate {
    let hide: () -> Void
    init(hide: @escaping () -> Void) { self.hide = hide }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        hide()
        return false
    }

    /// `close()` ne demande pas l'avis du délégué : on remet l'état d'accord.
    func windowWillClose(_ notification: Notification) {
        hide()
    }
}
