import SwiftUI

@main
struct SillonApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent()
        } label: {
            // Toujours présente : c'est elle qui donne l'action d'ouverture
            // des réglages, panneau masqué compris.
            Image(systemName: "text.line.first.and.arrowtriangle.forward")
                .accessibilityLabel("sillon")
                .providesSettingsOpener()
        }
        Settings {
            SettingsRoot()
        }
    }
}

/// Le contenu de la scène `Settings`. Le corps de la scène est évalué au
/// lancement, avant `applicationDidFinishLaunching` : le modèle n'existe pas
/// encore. Lu ici, il l'est à l'ouverture de la fenêtre.
private struct SettingsRoot: View {
    var body: some View {
        if let model = AppDelegate.shared?.model {
            ReglagesView(model: model)
                .background(SettingsWindowName())
        }
    }
}

/// Le titre de la fenêtre suit l'onglet (« Lecture », « Voix »…), comme le
/// veut macOS ; l'identifiant d'accessibilité, lui, la nomme « Réglages »,
/// et son contenu porte ce nom pour VoiceOver. `NSWindow.identifier` reste
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
            window.setAccessibilityIdentifier("Réglages")
            window.contentView?.setAccessibilityLabel("Réglages")
            for button in [NSWindow.ButtonType.miniaturizeButton, .zoomButton] {
                window.standardWindowButton(button)?.isHidden = true
            }
        }
    }
}
