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
            if let model = AppDelegate.shared?.model {
                ReglagesView(model: model)
            }
        }
    }
}
