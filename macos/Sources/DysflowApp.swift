import SwiftUI

@main
struct DysflowApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent()
                .environment(delegate.services)
        } label: {
            // Toujours présente : c'est elle qui donne l'action d'ouverture
            // des réglages, panneau masqué compris.
            Image(nsImage: LogoMark.templateImage())
                .accessibilityLabel("dysflow")
                .providesSettingsOpener()
        }
        Settings {
            SettingsRoot()
                .environment(delegate.services)
        }
        // La liste à gauche, la section à droite : 880 × 640 à l'ouverture,
        // jamais moins que la taille minimale du contenu (860 × 520).
        .defaultSize(width: 880, height: 640)
        .windowResizability(.contentMinSize)
    }
}

/// Le contenu de la scène `Settings`. Le corps de la scène est évalué au
/// lancement, avant `applicationDidFinishLaunching` : le modèle n'existe pas
/// encore. Lu ici (et observé), il l'est à l'ouverture de la fenêtre.
private struct SettingsRoot: View {
    @Environment(AppServices.self) private var services

    var body: some View {
        if let model = services.model {
            ReglagesView(model: model)
                .background(SettingsWindowName())
        }
    }
}

/// Le titre de la fenêtre (masqué, lu par Mission Control) est « Réglages » ;
/// l'identifiant d'accessibilité la nomme de même, et son contenu porte ce nom pour VoiceOver. `NSWindow.identifier` reste
/// à SwiftUI, qui retrouve sa fenêtre par lui. Les boutons réduire et
/// agrandir, toujours grisés ici, sont retirés : macOS les expose mal
/// (enfant que son parent ne déclare pas).
private struct SettingsWindowName: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { Probe() }
    func updateNSView(_ nsView: NSView, context: Context) {}

    private final class Probe: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window else { return }
            window.setAccessibilityIdentifier("Réglages") // xcstrings: garder
            Self.hideTitlebar(window)
            // SwiftUI règle la fenêtre après l'avoir posée : on repasse au
            // tour suivant.
            DispatchQueue.main.async { Self.hideTitlebar(window) }
            window.contentView?.setAccessibilityLabel(String(localized: "Settings"))
            for button in [NSWindow.ButtonType.miniaturizeButton, .zoomButton] {
                window.standardWindowButton(button)?.isHidden = true
            }
        }

        /// À la manière de Notion : pas de barre de titre visible, la barre
        /// latérale monte jusqu'au bouton de fermeture.
        static func hideTitlebar(_ window: NSWindow) {
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.titlebarSeparatorStyle = .none
        }
    }
}
