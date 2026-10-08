import AppKit
import SwiftUI

/// Ouvre, agrandit, réduit et masque le panneau ; route le clavier.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var keyMonitor: Any?
    @ObservationIgnored private var contentHeight: CGFloat = 0
    @ObservationIgnored private var resizePending = false
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var announcer = Announcer()
    /// ↑ ↓ : la vue de lecture affichée dit où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()

    private(set) var mode: PanelMode = .hidden
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture et à chaque réglage écrit.
    var reading = ReadingDisplay()
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false

    init(model: ReaderModel) {
        self.model = model
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
        host.setAccessibilityLabel("Panneau de lecture")
        // Toute fermeture (⌘W, close(), performClose:) passe par le masquage.
        let guardian = PanelCloseGuard { [weak self] in self?.hide() }
        closeGuard = guardian
        window.delegate = guardian
        observeSettingsWindow()
    }

    private var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    func show() {
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        if mode == .hidden {
            window.setFrame(PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame),
                            display: false)
            mode = .panel
            updateIdentifier()
        }
        window.makeKeyAndOrderFront(nil)
        // Le modèle ne suit l'état à chaque image que si le panneau est visible.
        model.isDisplayed = true
        // L'annonceur ne voit rien panneau masqué : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcer.reseed(with: model.snapshot)
        installKeyMonitor()
    }

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
    /// ne la recouvre plus.
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

    /// L'action d'une touche, avec le pas d'avance rapide des réglages.
    func action(for press: KeyPress) -> ReaderAction? {
        KeyMap.action(for: press, seekStep: Int32(reading.seekStep))
    }

    func expand() {
        guard mode == .panel else { return }
        mode = .expanded
        animate(to: PanelGeometry.expandedFrame(screen: screenFrame))
        updateIdentifier()
    }

    func collapseOrHide() {
        switch mode {
        case .expanded:
            mode = .panel
            animate(to: PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame))
            updateIdentifier()
        case .panel:
            hide()
        case .hidden:
            break
        }
    }

    /// Le seul chemin qui masque : pause, fenêtre retirée, suivi et clavier
    /// arrêtés. Sans effet si le panneau est déjà masqué.
    private func hide() {
        guard mode != .hidden else { return }
        model.send(.pause)
        mode = .hidden
        if window.isVisible { window.orderOut(nil) }
        model.isDisplayed = false
        removeKeyMonitor()
    }

    /// La phrase à dire, ou rien : jamais panneau masqué (fenêtre retirée).
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func announcementText(for s: Snapshot) -> String? {
        let text = announcer.announcement(for: s, doc: model.document)
        return mode == .hidden ? nil : text
    }

    /// Dit à VoiceOver un changement d'état (lecture, pause, section), jamais
    /// un mot. Sans effet quand VoiceOver est éteint.
    func announce(_ s: Snapshot) {
        guard let text = announcementText(for: s), NSWorkspace.shared.isVoiceOverEnabled else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }

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
            guard self.mode == .panel else { return }
            self.animate(to: PanelGeometry.panelFrame(contentHeight: self.contentHeight,
                                                      screen: self.screenFrame))
        }
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .expand: expand()
        case .collapseOrHide:
            if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else { collapseOrHide() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            flash("Phrase copiée")
        case .openSearch:
            if mode == .panel { expand() }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode == .expanded { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode == .panel { expand() }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp: helpVisible.toggle()
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

    private func animate(to frame: CGRect) {
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true)
        } else {
            window.setFrame(frame, display: true, animate: true)
        }
    }

    /// L'identifiant pour les tests, le nom pour VoiceOver.
    private func updateIdentifier() {
        let expanded = mode == .expanded
        window.contentView?.setAccessibilityIdentifier(expanded ? "lecteur-agrandi" : "panneau-lecture")
        window.contentView?.setAccessibilityLabel(expanded ? "Lecteur agrandi" : "Panneau de lecture")
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

/// Le délégué de la fenêtre : une fermeture demandée devient un masquage.
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
