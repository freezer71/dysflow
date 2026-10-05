import SwiftUI

struct MenuContent: View {
    private var model: ReaderModel? { AppDelegate.shared?.model }

    var body: some View {
        // Un clic dans le menu a déjà pris le focus à l'application : seule
        // la copie déjà faite est atteignable d'ici.
        Button("Lire le presse-papiers") {
            Task { @MainActor in
                guard let app = AppDelegate.shared else { return }
                await ReadFlow(
                    prepare: { app.model.prepareVoice() },
                    capture: { .unsupported },
                    open: { _ in },
                    clipboardIsConcealed: { SystemPasteboard().isConcealed },
                    openClipboard: { try await app.model.openClipboard() },
                    say: { app.model.message = $0 },
                    show: { app.panel.show() }
                ).run()
            }
        }
        Button("Afficher sillon") { AppDelegate.shared?.panel.show() }
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
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var voiceLine: String {
        switch model?.snapshot.voice {
        case .ready: "Voix prête"
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système (\(reason))"
        default: "Voix au repos"
        }
    }
}
