import SwiftUI

/// La fenêtre des réglages : une seule page — Voix, Lecture, Raccourcis,
/// Démarrage. Le nom `SettingsView` est pris par l'enregistrement du moteur.
struct ReglagesView: View {
    let model: ReaderModel
    @State private var store: SettingsStore
    /// Panneau masqué, l'état de la voix (aperçu, téléchargement) n'est plus
    /// relu : la fenêtre le relit à 4 Hz, tant qu'elle est visible seulement.
    @State private var refresh: RefreshLoop
    /// La table des modèles, la place qu'ils occupent et celle qui reste.
    @State private var rows: [VoiceModel] = []
    @State private var used: UInt64 = 0
    @State private var free: UInt64?

    init(model: ReaderModel) {
        self.model = model
        _store = State(initialValue: SettingsStore(backend: model))
        _refresh = State(initialValue: RefreshLoop { model.refresh() })
    }

    var body: some View {
        VStack(spacing: 0) {
            ConfigUnreadableBanner(model: model, store: store)
            SettingsPage(title: "Réglages") {
                ErrorText(store.error, identifier: "erreur-reglages")
                VoiceSection(model: model, store: store, rows: rows, used: used, free: free,
                             onChange: { await reloadModels() })
                ReadingSection(store: store)
                ShortcutsSection()
                StartupSection()
            }
            // En bas, discret : l'état de la voix si elle n'est pas prête.
            VoiceStatusLine(model: model)
                .padding(.horizontal, 36)
                .padding(.vertical, 10)
        }
        .background(Palette.page)
        // La barre de titre est transparente : la page monte jusqu'en haut.
        .ignoresSafeArea()
        .modifier(HiddenWindowToolbar())
        .navigationTitle("Réglages")
        .frame(width: 620, height: 680)
        .font(Typo.body)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("page-reglages")
        .task {
            AppDelegate.shared?.settingsStore = store
            await store.load()
            await loadModels()
        }
        // La relecture suit la fenêtre réelle ; fermée, ce qui attend sa pause part.
        .background(WindowVisibility(onVisible: { refresh.setVisible($0) },
                                     onClose: { closeWindow() }))
        .onDisappear { refresh.setVisible(false); closeWindow() }
        // La vitesse changée depuis le lecteur, resté visible à côté.
        .onChange(of: model.snapshot.speed) { _, _ in followReader() }
        // Un modèle installé (téléchargement fini) : sa ligne et ses voix changent.
        .onChange(of: ReaderModel.isDownloading(model.snapshot.download)) { was, running in
            if was, !running { Task { await reloadModels() } }
        }
    }

    /// Les lignes de la table et la place, lues hors du fil principal.
    private func loadModels() async {
        rows = await model.loadModels()
        used = await model.modelsDiskUsage()
        free = await model.freeSpace()
    }

    /// Après une action de la table ou un téléchargement : les modèles, puis
    /// réglages et capacités (le modèle utilisé, donc ses voix, a pu changer).
    private func reloadModels() async {
        await loadModels()
        await store.reload()
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
                    .buttonStyle(.notion)
                    .accessibilityIdentifier("remplacer-configuration")
                ErrorText(error.text, identifier: "erreur-remplacement")
            }
            .padding(.horizontal, 40)
            .padding(.top, 40)
            .padding(.bottom, 14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .adaptive(light: 0xFBF3DB, dark: 0x3A3220)))
            .accessibilityElement(children: .contain)
            .onAppear { AnnounceOnce.post(line) }
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

/// La barre de titre de la fenêtre, effacée (macOS 15 et plus ; avant,
/// `SettingsWindowName` la rend transparente).
private struct HiddenWindowToolbar: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 15, *) {
            content
                .toolbar(removing: .title)
                .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
        } else {
            content
        }
    }
}
