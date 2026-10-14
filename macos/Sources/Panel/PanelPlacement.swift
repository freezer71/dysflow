import AppKit

/// Où se pose la fenêtre du panneau, mode par mode : le cadre de chaque
/// mode, les bornes de taille, et la place du mode petit retenue quand
/// l'utilisateur la déplace.
@MainActor
final class PanelPlacement {
    private let window: NSWindow
    private let store: DisplayModeStore
    /// Le mode affiché, lu à chaque déplacement.
    private let mode: () -> DisplayMode
    /// Vrai pendant que le code place la fenêtre : seul un déplacement de
    /// l'utilisateur fixe la place du mode petit.
    private var placing = false
    private var moveObserver: NSObjectProtocol?

    init(window: NSWindow, store: DisplayModeStore, mode: @escaping () -> DisplayMode) {
        self.window = window
        self.store = store
        self.mode = mode
        observeMoves()
    }

    var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    /// L'écran principal (celui de la barre des menus).
    private var primaryScreenFrame: CGRect {
        NSScreen.screens.first?.visibleFrame ?? screenFrame
    }

    func frame(for mode: DisplayMode, contentHeight: CGFloat) -> CGRect {
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
    func applySizeLimits(for mode: DisplayMode) {
        if mode == .small {
            window.styleMask.remove(.resizable)
        } else {
            window.styleMask.insert(.resizable)
        }
    }

    func place(_ frame: CGRect, animated: Bool) {
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
    /// Un redimensionnement différé de la vue hôte (le haut fixe, l'origine
    /// bouge) n'est pas un déplacement : seule la barre à sa taille compte.
    func windowMoved() {
        guard mode() == .small, !placing, window.frame.size == SmallPlacement.size else { return }
        store.smallOrigin = window.frame.origin
    }
}
