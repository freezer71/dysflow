import KeyboardShortcuts
import SwiftUI

/// Le menu de la barre des menus : cinq lignes, plus l'autorisation
/// d'accessibilité tant qu'elle manque.
struct MenuContent: View {
    /// Avant la fin du lancement et en mode mesure, ni modèle ni panneau :
    /// chaque action passe par `?` et ne fait rien.
    @Environment(AppServices.self) private var app

    var body: some View {
        Button("Lire la sélection") {
            Task { @MainActor in await app.readSelection() }
        }
        .globalKeyboardShortcut(.readSelection)
        Button("Ouvrir un fichier…") { app.chooseFile() }
            .keyboardShortcut("o")
        // Toujours là : de quoi revenir au lecteur.
        Button("Afficher le lecteur") { app.panel?.show() }
            .globalKeyboardShortcut(.showPanel)
        if Onboarding.needsReminder {
            Button("Autoriser la lecture de la sélection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Divider()
        SettingsLink { Text("Réglages…") }
            .keyboardShortcut(",")
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
