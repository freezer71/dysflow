import AppKit
import SwiftUI

/// Ouvre, agrandit, réduit et masque le panneau ; route le clavier.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored private let window = ReadingPanel()
    @ObservationIgnored private var keyMonitor: Any?
    @ObservationIgnored private var contentHeight: CGFloat = 0

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
            model.send(.pause)
            window.orderOut(nil)
            mode = .hidden
            model.isDisplayed = false
            removeKeyMonitor()
        case .hidden:
            break
        }
    }

    func contentHeightChanged(_ height: CGFloat) {
        guard height != contentHeight else { return }
        contentHeight = height
        // Appelé pendant la mise à jour SwiftUI : redimensionner la fenêtre
        // au milieu de cette passe est annulé. On le fait au tour suivant.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.mode == .panel else { return }
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
            model.message = "Phrase copiée"
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
