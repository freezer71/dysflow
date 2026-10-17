import AppKit
import SwiftUI

/// Ouvre la fenêtre des réglages.
///
/// Depuis macOS 14, `showSettingsWindow:` n'ouvre plus la scène `Settings`
/// de SwiftUI : seule l'action `openSettings` le fait, et elle ne s'obtient
/// que dans une vue d'une scène. L'icône de la barre des menus, toujours
/// présente, la donne ici ; une demande arrivée avant elle attend. La
/// première action reçue reste : celle d'une vue hors scène (le panneau est
/// un NSHostingView dans un NSPanel) pourrait ne rien ouvrir.
///
/// `open(_:)` peut demander une section : la liste y passe avant que la
/// fenêtre ne s'ouvre (ou ne revienne devant). Sans section, la dernière
/// choisie reste.
@MainActor
final class SettingsOpener {
    static let shared = SettingsOpener()

    private let activate: () -> Void
    private let bringToFront: @MainActor () -> Void
    private let navigation: SettingsNavigation
    private var action: (() -> Void)?
    private var pending = false

    init(activate: @escaping () -> Void = { NSApp.activate() },
         bringToFront: @escaping @MainActor () -> Void = { SettingsOpener.bringSettingsWindowToFront() },
         navigation: SettingsNavigation = .shared) {
        self.activate = activate
        self.bringToFront = bringToFront
        self.navigation = navigation
    }

    /// L'action `openSettings` d'une vue ; la première seule est gardée.
    func install(_ action: @escaping () -> Void) {
        guard self.action == nil else { return }
        self.action = action
        if pending {
            pending = false
            action()
        }
    }

    /// Une app de barre des menus n'est pas au premier plan : on l'y met
    /// d'abord, sinon la fenêtre s'ouvrirait derrière l'application active.
    func open(_ section: SettingsSection? = nil) {
        if let section { navigation.select(section) }
        activate()
        if let action { action() } else { pending = true }
        // macOS peut refuser l'activation (elle n'est qu'une demande) : la
        // fenêtre, une fois ouverte, passe quand même devant et prend le clavier.
        // Une première ouverture crée la fenêtre un peu après : un second essai.
        Task { @MainActor [bringToFront] in
            bringToFront()
            try? await Task.sleep(for: .milliseconds(150))
            bringToFront()
        }
    }

    static func bringSettingsWindowToFront() {
        guard let window = NSApp.windows.first(where: { PanelLevel.isSettingsWindow($0) }) else { return }
        window.orderFrontRegardless()
        window.makeKey()
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
