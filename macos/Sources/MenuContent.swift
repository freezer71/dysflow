import KeyboardShortcuts
import SwiftUI

/// Le menu de la barre des menus : cinq lignes (sept avec la dictée), plus l'autorisation
/// d'accessibilité tant qu'elle manque.
struct MenuContent: View {
    /// Avant la fin du lancement et en mode mesure, ni modèle ni panneau :
    /// chaque action passe par `?` et ne fait rien.
    @Environment(AppServices.self) private var app

    var body: some View {
        Button("Read Selection") {
            Task { @MainActor in await app.readSelection() }
        }
        .globalKeyboardShortcut(.readSelection)
        if let dictation = app.dictation {
            Button("Dictate") { dictation.toggle() }
                .globalKeyboardShortcut(.dictate)
            // La saisie est partie au mauvais endroit : le texte se reprend ici.
            Button("Copy Last Dictation") { dictation.copyLast() }
                .disabled(dictation.lastText == nil)
        }
        Button("Open File…") { app.chooseFile() }
            .keyboardShortcut("o")
        // Toujours là : de quoi revenir au lecteur.
        Button("Show Reader") { app.panel?.show() }
            .globalKeyboardShortcut(.showPanel)
        if Onboarding.needsReminder {
            Button("Allow Reading the Selection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Divider()
        // Pas de `SettingsLink` : il ouvre la fenêtre sans mettre sillage au
        // premier plan, et elle s'ouvre derrière l'application active.
        Button("Settings…") { SettingsOpener.shared.open() }
            .keyboardShortcut(",")
        Button("Quit sillage") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
