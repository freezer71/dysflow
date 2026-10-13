import KeyboardShortcuts
import SwiftUI

struct MenuContent: View {
    /// En mode mesure (`--measure`, `--measure-charge`), ni modèle, ni
    /// panneau, ni transport : chaque action passe par `?` et ne fait rien.
    private var app: AppDelegate? { AppDelegate.shared }
    private var model: ReaderModel? { AppDelegate.shared?.model }

    var body: some View {
        // Le temps restant vient de l'instantané du modèle : aucune minuterie
        // ajoutée, il se met à jour à la cadence existante.
        let s = model?.snapshot
        let hasDocument = s?.hasDocument ?? false
        // Un clic dans le menu a déjà pris le focus à l'application : seule
        // la copie déjà faite est atteignable d'ici.
        Button("Lire le presse-papiers") {
            Task { @MainActor in await AppDelegate.shared?.read() }
        }
        // Toujours là, même relancée en arrière-plan : de quoi revenir.
        Button("Afficher sillon") { app?.panel?.show() }
        Menu("Taille") {
            Button("Petit") { app?.panel?.apply(.choose(.small)) }
                .keyboardShortcut("1", modifiers: .command)
            Button("Moyen") { app?.panel?.apply(.choose(.medium)) }
                .keyboardShortcut("2", modifiers: .command)
            Button("Grand") { app?.panel?.apply(.choose(.large)) }
                .keyboardShortcut("3", modifiers: .command)
        }
        Divider()
        Button(s?.playing == true ? "Pause" : "Lecture") { _ = app?.transport?.perform(.toggle) }
            .globalKeyboardShortcut(.playPause)
            .disabled(!hasDocument)
        Button("Phrase précédente") { _ = app?.transport?.perform(.previous) }
            .globalKeyboardShortcut(.previousSentence)
            .disabled(!hasDocument)
        Button("Phrase suivante") { _ = app?.transport?.perform(.next) }
            .globalKeyboardShortcut(.nextSentence)
            .disabled(!hasDocument)
        if hasDocument, let line = PlaybackTime.menuLine(elapsed: s?.elapsed, total: s?.total) {
            Text(line)
        }
        Divider()
        Text(voiceLine)
        if Onboarding.needsReminder {
            Button("Autoriser la lecture de la sélection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Button("Mettre la voix au repos") { model?.restVoice() }
        Divider()
        SettingsLink { Text("Réglages…") }
            .keyboardShortcut(",")
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var voiceLine: String {
        switch model?.snapshot.voice {
        case .ready: "Voix prête"
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système (\(reason))"
        case .unavailable(let reason): "Voix indisponible (\(reason))"
        default: "Voix au repos"
        }
    }
}
