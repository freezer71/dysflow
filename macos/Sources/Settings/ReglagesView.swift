import SwiftUI

/// La fenêtre des réglages : six onglets et un magasin de réglages commun.
/// Le nom `SettingsView` est pris par l'enregistrement du moteur (UniFFI).
struct ReglagesView: View {
    let model: ReaderModel
    @State private var store: SettingsStore
    @State private var models: ModelsStore
    /// Panneau masqué, l'état de la voix (chargement, repli, aperçu) n'est
    /// plus relu : la fenêtre le relit à 4 Hz, tant qu'elle est visible
    /// seulement. Un instantané se lit sous verrou, sans le fil du lecteur.
    @State private var refresh: RefreshLoop

    init(model: ReaderModel) {
        self.model = model
        let store = SettingsStore(backend: model)
        _store = State(initialValue: store)
        // Le modèle actif change (« Utiliser », téléchargement qui l'active) :
        // les réglages relisent voix, langues et filtres.
        _models = State(initialValue: ModelsStore(backend: model,
                                                  onActiveModelChanged: { await store.reload() },
                                                  isDownloading: { ReaderModel.isDownloading(model.snapshot.download) }))
        _refresh = State(initialValue: RefreshLoop { model.refresh() })
    }

    var body: some View {
        TabView(selection: SettingsNavigation.shared.binding) {
            VoiceTab(model: model, store: store).settingsTab(.voice)
            ReadingTab(store: store).settingsTab(.reading)
            ModelsTab(model: model, store: models, settings: store).settingsTab(.models)
            ShortcutsTab().settingsTab(.shortcuts)
            AdvancedTab(model: model, store: store).settingsTab(.advanced)
            GeneralTab(model: model, store: store).settingsTab(.general)
        }
        .navigationTitle("Réglages")
        .frame(width: 600, height: 540)
        .font(.system(size: 13))
        .task {
            AppDelegate.shared?.settingsStore = store
            await store.load()
        }
        // La relecture suit la fenêtre réelle : fermée, réduite ou couverte,
        // elle s'arrête ; fermée, ce qui attend sa pause part.
        .background(WindowVisibility(onVisible: { refresh.setVisible($0) },
                                     onClose: { closeWindow() }))
        .onDisappear { refresh.setVisible(false); closeWindow() }
        // Vitesse et volume changés depuis le panneau, resté visible à côté.
        .onChange(of: model.snapshot.speed) { _, _ in followReader() }
        .onChange(of: model.snapshot.volume) { _, _ in followReader() }
        // La fin d'un téléchargement, quel que soit l'onglet affiché.
        .onChange(of: ReaderModel.isDownloading(model.snapshot.download), initial: true) { _, running in
            Task { await models.downloadChanged(downloading: running) }
        }
        .onChange(of: model.snapshot.download?.failed) { _, failed in
            if let failed { Announcement.post("Téléchargement interrompu : \(failed)") }
        }
    }

    private func followReader() {
        store.followReader(speed: model.snapshot.speed, volume: model.snapshot.volume)
    }

    /// Fermer la fenêtre n'abandonne pas un changement qui attend sa pause.
    private func closeWindow() {
        store.endDragging()
        Task { await store.flush() }
    }
}
