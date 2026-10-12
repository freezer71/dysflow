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
        VStack(spacing: 0) {
            ConfigUnreadableBanner(model: model, store: store)
            tabs
        }
        .navigationTitle("Réglages")
        .frame(width: 600, height: 540)
        .font(.system(size: 13))
    }

    private var tabs: some View {
        TabView(selection: SettingsNavigation.shared.binding) {
            VoiceTab(model: model, store: store).settingsTab(.voice)
            ReadingTab(store: store).settingsTab(.reading)
            ModelsTab(model: model, store: models, settings: store).settingsTab(.models)
            ShortcutsTab().settingsTab(.shortcuts)
            AdvancedTab(model: model, store: store).settingsTab(.advanced)
            GeneralTab(model: model, store: store).settingsTab(.general)
        }
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

/// `config.toml` n'a pu être lu : les réglages s'appliquent sans s'écrire
/// (le moteur ne le réécrit jamais avec ses défauts). « Remplacer… » écrit
/// les réglages en cours, après confirmation, en gardant une copie du
/// fichier à côté (`config.toml.illisible`).
struct ConfigUnreadableBanner: View {
    let model: ReaderModel
    let store: SettingsStore
    @State private var confirming = false
    @State private var error = ErrorNote()

    var body: some View {
        if let line = Self.line(unreadable: model.snapshot.configUnreadable) {
            VStack(alignment: .leading, spacing: 6) {
                Text(line)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("configuration-illisible")
                Button("Remplacer config.toml par les réglages actuels…") { confirming = true }
                    .accessibilityIdentifier("remplacer-configuration")
                ErrorText(error.text, identifier: "erreur-remplacement")
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary)
            .accessibilityElement(children: .contain)
            .onAppear { Announcement.post(line) }
            .alert("Remplacer config.toml ?", isPresented: $confirming) {
                // Annuler par défaut : Retour et Échap ne remplacent rien.
                Button("Annuler", role: .cancel) {}
                    .keyboardShortcut(.defaultAction)
                Button("Remplacer", role: .destructive) {
                    Task {
                        await store.flush()
                        do {
                            try await model.replaceUnreadableConfig()
                            error.clear()
                        } catch {
                            self.error.fail(SettingsStore.message(of: error))
                        }
                    }
                }
            } message: {
                Text("Le fichier actuel est gardé à côté, sous le nom config.toml.illisible. "
                     + "Les réglages affichés seront écrits à sa place.")
            }
        }
    }

    static func line(unreadable: Bool) -> String? {
        guard unreadable else { return nil }
        return "Configuration illisible : config.toml n'a pas pu être lu. Les réglages s'appliquent, "
            + "mais ne sont pas enregistrés tant qu'il n'est pas corrigé ou remplacé."
    }
}
