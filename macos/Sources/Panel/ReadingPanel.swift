import AppKit

/// Tailles du panneau, calculées sans fenêtre pour être éprouvées.
enum PanelGeometry {
    /// Barre de lecture, aides et marges autour du texte.
    static let chrome: CGFloat = 116
    static let minimumHeight: CGFloat = 150
    static let maximumWidth: CGFloat = 760
    /// La marge de chaque côté du contenu, moyen et grand modes.
    static let horizontalPadding: CGFloat = 22

    static func panelFrame(contentHeight: CGFloat, screen: CGRect) -> CGRect {
        let width = min(maximumWidth, screen.width * 0.6)
        let wanted = contentHeight >= .greatestFiniteMagnitude / 2
            ? .greatestFiniteMagnitude : contentHeight + chrome
        let height = min(max(wanted, minimumHeight), screen.height * 0.45)
        // Le haut du panneau à un quart de l'écran, comme Spotlight.
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - width / 2, y: top - height, width: width, height: height)
    }

    static func expandedFrame(screen: CGRect) -> CGRect {
        screen.insetBy(dx: 24, dy: 24)
    }
}

/// Le niveau du panneau : au-dessus des autres fenêtres, sauf quand la
/// fenêtre des réglages est ouverte, qu'il ne doit pas recouvrir.
enum PanelLevel {
    static let settingsWindowIdentifier = "Réglages"

    static func level(settingsVisible: Bool) -> NSWindow.Level {
        settingsVisible ? .normal : .floating
    }

    @MainActor static func isSettingsWindow(_ window: NSWindow) -> Bool {
        window.accessibilityIdentifier() == settingsWindowIdentifier
    }
}

/// Une fenêtre qui prend le clavier sans activer l'app : l'application en
/// cours reste au premier plan derrière.
final class ReadingPanel: NSPanel {
    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 720, height: 200),
                   styleMask: [.nonactivatingPanel, .titled, .closable, .fullSizeContentView, .resizable],
                   backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        // Le titre ne s'affiche pas, mais nomme la fenêtre pour VoiceOver.
        title = "sillon"
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        isMovableByWindowBackground = true
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        // Fermable (⌘W, performClose:) pour que la fermeture passe par le
        // délégué ; les boutons restent cachés.
        for button in [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton] {
            standardWindowButton(button)?.isHidden = true
        }
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
