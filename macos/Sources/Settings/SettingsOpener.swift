import AppKit
import SwiftUI

/// Ouvre la fenêtre des réglages.
///
/// Depuis macOS 14, `showSettingsWindow:` n'ouvre plus la scène `Settings`
/// de SwiftUI : seule l'action `openSettings` le fait, et elle ne s'obtient
/// que dans une vue. Les vues toujours présentes (icône de la barre des
/// menus, panneau) la donnent ici ; une demande arrivée avant elle attend.
@MainActor
final class SettingsOpener {
    static let shared = SettingsOpener()

    private let activate: () -> Void
    private var action: (() -> Void)?
    private var pending = false

    init(activate: @escaping () -> Void = { NSApp.activate() }) {
        self.activate = activate
    }

    /// L'action `openSettings` d'une vue.
    func install(_ action: @escaping () -> Void) {
        self.action = action
        if pending {
            pending = false
            action()
        }
    }

    /// Une app de barre des menus n'est pas au premier plan : on l'y met
    /// d'abord, sinon la fenêtre s'ouvrirait derrière l'application active.
    func open() {
        activate()
        if let action { action() } else { pending = true }
    }
}

/// Donne l'action `openSettings` de la vue à `SettingsOpener`.
private struct SettingsOpenerInstaller: ViewModifier {
    @Environment(\.openSettings) private var openSettings

    func body(content: Content) -> some View {
        content.onAppear {
            let openSettings = openSettings
            SettingsOpener.shared.install { openSettings() }
        }
    }
}

extension View {
    func providesSettingsOpener() -> some View {
        modifier(SettingsOpenerInstaller())
    }
}
