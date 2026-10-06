import SwiftUI

/// La fenêtre des réglages : six onglets et un magasin de réglages commun.
/// Le nom `SettingsView` est pris par l'enregistrement du moteur (UniFFI).
struct ReglagesView: View {
    let model: ReaderModel
    @State private var store: SettingsStore

    init(model: ReaderModel) {
        self.model = model
        _store = State(initialValue: SettingsStore(backend: model))
    }

    var body: some View {
        TabView(selection: SettingsNavigation.shared.binding) {
            VoiceTab(model: model, store: store).settingsTab(.voice)
            ReadingTab(store: store).settingsTab(.reading)
            VoiceSettings(model: model).settingsTab(.models)
            ShortcutSettings().settingsTab(.shortcuts)
            GeneralSettings().settingsTab(.general)
        }
        .navigationTitle("Réglages")
        .frame(width: 600, height: 540)
        .font(.system(size: 13))
        .task {
            AppDelegate.shared?.settingsStore = store
            await store.load()
        }
        // Fermer la fenêtre n'abandonne pas un changement qui attend sa pause.
        .onDisappear { Task { await store.flush() } }
        // Panneau masqué, l'état de la voix (chargement, repli, aperçu) n'est
        // plus relu : la fenêtre le relit à 4 Hz tant qu'elle est ouverte. Un
        // instantané se lit sous verrou, sans passer par le fil du lecteur.
        .task {
            while !Task.isCancelled {
                model.refresh()
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }
}
