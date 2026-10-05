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

    private(set) var mode: PanelMode = .hidden
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture : un changement dans les réglages s'applique au suivant.
    var reading: SettingsView
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    var helpVisible = false

    init(model: ReaderModel) {
        self.model = model
        reading = model.settings()
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
        // Toute fermeture (⌘W, close(), performClose:) passe par le masquage.
        let guardian = PanelCloseGuard { [weak self] in self?.hide() }
        closeGuard = guardian
        window.delegate = guardian
    }

    private var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    func show() {
        reading = model.settings()
        if mode == .hidden {
            window.setFrame(PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame),
                            display: false)
            mode = .panel
            updateIdentifier()
        }
        window.makeKeyAndOrderFront(nil)
        // Le modèle ne suit l'état à chaque image que si le panneau est visible.
        model.isDisplayed = true
        installKeyMonitor()
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
        case .toggleSidebar:
            if mode == .expanded { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode == .panel { expand() }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp: helpVisible.toggle()
        case .openSettings:
            NSApp.activate()
            NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
        }
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

    private func updateIdentifier() {
        window.contentView?.setAccessibilityIdentifier(mode == .expanded ? "lecteur-agrandi" : "panneau-lecture")
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
            guard let action = KeyMap.action(for: KeyPress(event)) else { return event }
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
