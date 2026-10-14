import AppKit
import SwiftUI

/// La fenêtre des notes : ouverte quand on lance sillon soi-même (Dock,
/// Spotlight, un lanceur), ou depuis le menu. Tant qu'elle est ouverte,
/// sillon a son icône dans le Dock et passe au ⌘Tab. Fermée, il redevient
/// discret dans la barre des menus (le panneau se retire s'il ne lit rien).
@MainActor
final class NotesWindowController: NSObject, NSWindowDelegate {
    let store: NotesStore
    private let read: (NotePage) -> Void
    /// Après la fermeture de la fenêtre (l'app décide du panneau).
    private let closed: () -> Void
    private var window: NSWindow?

    init(store: NotesStore, read: @escaping (NotePage) -> Void, closed: @escaping () -> Void = {}) {
        self.store = store
        self.read = read
        self.closed = closed
    }

    var isVisible: Bool { window?.isVisible ?? false }

    func show() {
        let window = self.window ?? makeWindow()
        self.window = window
        NSApp.setActivationPolicy(.regular)
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    private func makeWindow() -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1040, height: 720),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = "Notes"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        window.isReleasedWhenClosed = false
        window.setAccessibilityIdentifier("Notes")
        window.contentView = NSHostingView(rootView: NotesView(store: store, read: read))
        window.center()
        // La place et la taille d'une ouverture à l'autre.
        window.setFrameAutosaveName("notes")
        window.delegate = self
        return window
    }

    func windowWillClose(_ notification: Notification) {
        store.flush()
        // Plus de fenêtre à montrer : l'icône quitte le Dock. Les réglages,
        // s'ils sont ouverts, restent accessibles par la barre des menus.
        NSApp.setActivationPolicy(.accessory)
        closed()
    }
}

/// Ouvert par l'utilisateur, ou par macOS à l'ouverture de session ?
enum LaunchKind {
    /// Lu dans l'événement Apple du lancement (à appeler pendant
    /// `applicationWillFinishLaunching`). Sans lui, un démarrage dans les
    /// deux premières minutes de la machine passe pour une ouverture de
    /// session : la fenêtre ne s'impose pas au démarrage.
    @MainActor
    static func launchedAtLogin(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent,
                                uptime: TimeInterval = ProcessInfo.processInfo.systemUptime) -> Bool {
        if let event, event.eventID == kAEOpenApplication,
           event.paramDescriptor(forKeyword: keyAEPropData)?.enumCodeValue == keyAELaunchedAsLogInItem {
            return true
        }
        return uptime < 120
    }

    /// Lancé pour ouvrir des fichiers (`sillon fichier.pdf`, « Ouvrir
    /// avec ») : le panneau les montre, la page de notes ne s'impose pas.
    /// Les fichiers eux-mêmes arrivent par `application(_:open:)`, parfois
    /// après la fin du lancement.
    @MainActor
    static func launchedWithFiles(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent) -> Bool {
        event?.eventID == kAEOpenDocuments
    }
}
