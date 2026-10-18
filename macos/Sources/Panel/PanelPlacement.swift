import AppKit

/// Où se pose la fenêtre, forme par forme, et ce qui en est retenu : la
/// place du mini quand l'utilisateur le déplace, le cadre du lecteur quand
/// il le déplace ou le redimensionne.
@MainActor
final class PanelPlacement {
    private let window: NSWindow
    private let store: DisplayModeStore
    /// La forme affichée, lue à chaque changement.
    private let mode: () -> DisplayMode
    /// Vrai pendant que le code place la fenêtre : seul un geste de
    /// l'utilisateur est retenu.
    private var placing = false
    private var observers: [NSObjectProtocol] = []
    /// Le dernier geste à retenir, écrit une fois le geste fini : glisser ou
    /// redimensionner envoie une notification par image.
    private var pendingSave: (() -> Void)?
    private var saveTimer: Timer?
    static let saveDelay: TimeInterval = 0.3

    init(window: NSWindow, store: DisplayModeStore, mode: @escaping () -> DisplayMode) {
        self.window = window
        self.store = store
        self.mode = mode
        observeChanges()
    }

    private var screens: [CGRect] { NSScreen.screens.map(\.visibleFrame) }

    /// L'écran principal (celui de la barre des menus).
    private var primaryScreenFrame: CGRect {
        NSScreen.screens.first?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
    }

    func frame(for mode: DisplayMode) -> CGRect {
        switch mode {
        case .mini:
            return MiniPlacement.frame(savedOrigin: store.miniOrigin, screens: screens, main: primaryScreenFrame)
        case .reader, .hidden:
            return ReaderPlacement.frame(saved: store.readerFrame, screens: screens, main: primaryScreenFrame)
        }
    }

    /// Le mini ne se redimensionne pas ; le lecteur, si, jamais sous sa
    /// taille minimale (l'en-tête et la barre se chevaucheraient).
    func applySizeLimits(for mode: DisplayMode) {
        if mode == .mini {
            window.styleMask.remove(.resizable)
            window.minSize = MiniPlacement.size
        } else {
            window.styleMask.insert(.resizable)
            window.minSize = ReaderPlacement.minimumSize
        }
    }

    /// Animé, le cadre glisse d'une forme à l'autre (sauf « Réduire les
    /// animations »).
    func place(_ frame: CGRect, animated: Bool) {
        // Un geste encore en attente est retenu avant que le code ne bouge
        // la fenêtre.
        flushSave()
        placing = true
        defer { placing = false }
        if animated, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true, animate: true)
        } else {
            window.setFrame(frame, display: animated)
        }
    }

    private func observeChanges() {
        for name in [NSWindow.didMoveNotification, NSWindow.didResizeNotification] {
            // Livré tout de suite (`queue: nil`), pendant que `placing` est
            // vrai : une file différée verrait chaque placement du code comme
            // un geste de l'utilisateur.
            observers.append(NotificationCenter.default.addObserver(
                forName: name, object: window, queue: nil
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.windowChanged() }
            })
        }
        observers.append(NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: nil
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.flushSave() }
        })
    }

    /// Un geste de l'utilisateur : la place du mini (à sa taille seulement,
    /// un redimensionnement différé de la vue hôte ne compte pas), ou le
    /// cadre du lecteur.
    func windowChanged() {
        guard !placing else { return }
        switch mode() {
        case .mini:
            guard window.frame.size == MiniPlacement.size else { return }
            let origin = window.frame.origin
            scheduleSave { [store] in store.miniOrigin = origin }
        case .reader:
            let frame = window.frame
            scheduleSave { [store] in store.readerFrame = frame }
        case .hidden:
            break
        }
    }

    /// Retient la valeur du geste tout de suite, l'écrit après un temps
    /// calme : une seule écriture par geste.
    private func scheduleSave(_ save: @escaping () -> Void) {
        pendingSave = save
        saveTimer?.invalidate()
        let timer = Timer(timeInterval: Self.saveDelay, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.flushSave() }
        }
        timer.tolerance = 0.1
        RunLoop.main.add(timer, forMode: .common)
        saveTimer = timer
    }

    /// Écrit le geste en attente, s'il y en a un.
    func flushSave() {
        saveTimer?.invalidate()
        saveTimer = nil
        let save = pendingSave
        pendingSave = nil
        save?()
    }
}
