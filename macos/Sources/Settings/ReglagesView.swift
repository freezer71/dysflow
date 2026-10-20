import SwiftUI

/// La fenêtre des réglages, à la manière des Réglages Système : la liste
/// des sections à gauche (Voix, Affichage, Dictée sous macOS 26,
/// Raccourcis, Autorisations, Démarrage), la section choisie à droite.
/// Seul le volet de droite défile, si la section dépasse la fenêtre. Le
/// bandeau « config.toml illisible » et la ligne d'état de la voix restent
/// visibles quelle que soit la section. Le nom `SettingsView` est pris par
/// l'enregistrement du moteur.
struct ReglagesView: View {
    let model: ReaderModel
    /// La section affichée, gardée d'une ouverture à l'autre ;
    /// `SettingsOpener.open(_:)` peut en demander une.
    let navigation: SettingsNavigation
    @State private var store: SettingsStore
    /// Panneau masqué, l'état de la voix (aperçu, téléchargement) n'est plus
    /// relu : la fenêtre le relit à 4 Hz, tant qu'elle est visible seulement.
    @State private var refresh: RefreshLoop
    /// La table des modèles, la place qu'ils occupent et celle qui reste.
    @State private var rows: [VoiceModel] = []
    @State private var used: UInt64 = 0
    @State private var free: UInt64?
    /// L'erreur et la suppression en attente de chaque table, gardées ici :
    /// changer de section ne les perd pas.
    @State private var speechTable = ModelsTableState()
    @State private var dictationTable = ModelsTableState()
    /// La qualité des voix de macOS, lue quand la liste des voix change.
    @State private var macQuality: [String: Int] = [:]
    /// La liste prend le clavier à l'ouverture : ↑ et ↓ changent de section.
    @FocusState private var listFocused: Bool

    /// La largeur de la liste, et la place laissée en haut aux boutons de la fenêtre.
    static let sidebarWidth: CGFloat = 190
    static let titlebarInset: CGFloat = 44
    /// La fenêtre : à 860 pt au moins, le volet garde la largeur de
    /// l'ancienne page (cartes de 597 pt, 588 avant) ; rien ne s'y coupe.
    static let minWidth: CGFloat = 860
    static let idealWidth: CGFloat = 880
    static let minHeight: CGFloat = 520
    static let idealHeight: CGFloat = 640

    init(model: ReaderModel, navigation: SettingsNavigation = .shared) {
        self.model = model
        self.navigation = navigation
        _store = State(initialValue: SettingsStore(backend: model))
        _refresh = State(initialValue: RefreshLoop { model.refresh() })
    }

    var body: some View {
        let bannerShown = ConfigUnreadableBanner.line(unreadable: model.snapshot.configUnreadable) != nil
        VStack(spacing: 0) {
            ConfigUnreadableBanner(model: model, store: store)
            HStack(spacing: 0) {
                sidebar(topInset: bannerShown ? 12 : Self.titlebarInset)
                Rectangle().fill(Palette.divider).frame(width: 1).accessibilityHidden(true)
                VStack(spacing: 0) {
                    pane(navigation.selection)
                    // En bas, toujours là : l'erreur d'enregistrement, puis
                    // l'état de la voix si elle n'est pas prête.
                    if store.error != nil {
                        ErrorText(store.error, identifier: "erreur-reglages")
                            .padding(.horizontal, 36)
                            .padding(.top, 8)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    VoiceStatusLine(model: model)
                        .padding(.horizontal, 36)
                        .padding(.vertical, 10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .background(Palette.page)
            }
        }
        .background(Palette.page)
        // La barre de titre est transparente : la liste monte jusqu'en haut.
        .ignoresSafeArea()
        .modifier(HiddenWindowToolbar())
        .navigationTitle("Settings")
        .frame(minWidth: Self.minWidth, idealWidth: Self.idealWidth, maxWidth: .infinity,
               minHeight: Self.minHeight, idealHeight: Self.idealHeight, maxHeight: .infinity)
        .font(Typo.body)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("page-reglages")
        .defaultFocus($listFocused, true)
        .onAppear { listFocused = true }
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
        // VoiceOver entend la progression d'un téléchargement tous les 25 %,
        // et l'échec d'un aperçu, quelle que soit la section affichée.
        .onChange(of: model.snapshot.download) { old, new in announceProgress(old: old, new: new) }
        .onChange(of: model.snapshot.preview.error) { _, error in
            if let error { Announcement.post(error) }
        }
        // La qualité des voix de macOS, relue quand la liste change (pas à
        // chaque retour sur Voix).
        .task(id: store.capabilities.voices.map(\.id)) {
            let c = store.capabilities
            macQuality = c.family == "system"
                ? VoiceSection.quality(of: c.voices.map(\.id), voices: VoiceSection.installedMacVoices()) : [:]
        }
        // Un modèle installé (téléchargement fini) : sa ligne et ses voix changent.
        .onChange(of: ReaderModel.isDownloading(model.snapshot.download)) { was, running in
            if was, !running { Task { await reloadModels() } }
        }
    }

    private func sidebar(topInset: CGFloat) -> some View {
        SettingsSidebar(navigation: navigation, focus: $listFocused)
            .padding(.top, topInset)
            .frame(width: Self.sidebarWidth)
            .frame(maxHeight: .infinity, alignment: .top)
            .background(Palette.sidebar)
    }

    /// Le volet d'une section : son nom en titre, ses réglages dessous, à
    /// l'identique de l'ancienne page. Il défile seul ; changer de section
    /// le remet en haut.
    private func pane(_ section: SettingsSection) -> some View {
        SettingsPage(title: section.title) {
            switch section {
            case .voix:
                VoiceSection(model: model, store: store, rows: rows, used: used, free: free,
                             onChange: { await reloadModels() }, table: speechTable, macQuality: macQuality)
            case .affichage:
                ReadingSection(store: store)
            case .dictee:
                if Dictation.isSupported {
                    DictationSection(model: model, rows: rows, onChange: { await reloadModels() },
                                     table: dictationTable)
                }
            case .raccourcis:
                ShortcutsSection()
            case .autorisations:
                PermissionsSection()
            case .demarrage:
                StartupSection()
            }
        }
        .environment(\.settingsGroupTitleHidden, true)
        .id(section)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(section.title)
        .accessibilityIdentifier("volet-\(section.rawValue)")
    }

    /// La progression d'un téléchargement de modèle (lecture ou dictée),
    /// dite tous les 25 %.
    private func announceProgress(old: DownloadView?, new: DownloadView?) {
        guard let new, let row = rows.first(where: { $0.id == new.id }),
              let current = ModelTable.fraction(new) else { return }
        let previous = old?.id == new.id ? ModelTable.fraction(old) : nil
        if let text = ModelTable.progressAnnouncement(name: row.name, previous: previous, current: current) {
            Announcement.post(text)
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
        // Filet : un modèle chargé pour la bibliothèque des voix ne survit
        // pas à la fenêtre (la feuille le relâche déjà en se fermant).
        Task { await model.releaseVoicePreview() }
    }
}

/// La liste des sections, dessinée au style « Felt » : la ligne choisie sur
/// un fond gris arrondi, son icône sur une pastille citron, le texte à
/// l'encre ; les autres, nues. Un clic choisit ; la liste a le clavier, ↑ et
/// ↓ y changent de section. VoiceOver : un groupe « Sections des réglages »,
/// la ligne choisie marquée sélectionnée.
struct SettingsSidebar: View {
    let navigation: SettingsNavigation
    var focus: FocusState<Bool>.Binding

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(navigation.available) { section in
                SettingsSidebarRow(section: section, selected: section == navigation.selection,
                                   focused: focus.wrappedValue) {
                    navigation.select(section)
                    focus.wrappedValue = true
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 10)
        .focusable()
        .focused(focus)
        .focusEffectDisabled()
        .onMoveCommand { direction in
            switch direction {
            case .up: navigation.move(by: -1)
            case .down: navigation.move(by: 1)
            default: break
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Settings sections")
        .accessibilityIdentifier("sections-reglages")
    }
}

/// Une ligne de la liste : une icône et un mot, jamais coupé.
private struct SettingsSidebarRow: View {
    let section: SettingsSection
    let selected: Bool
    /// La liste a le clavier : la ligne choisie porte un liseré.
    let focused: Bool
    let action: () -> Void
    @State private var hovering = false

    /// Le glyphe sur la pastille citron : l'encre sombre, en clair comme en sombre.
    private static let onLime = Color(nsColor: NSColor(hex: 0x141414))

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: section.symbol)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(selected ? Self.onLime : Palette.muted)
                    .frame(width: 26, height: 26)
                    .background(RoundedRectangle(cornerRadius: 7).fill(selected ? Palette.accent : .clear))
                    .accessibilityHidden(true)
                Text(section.title)
                    .font(.system(size: 14, weight: selected ? .semibold : .regular))
                    .foregroundStyle(Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .frame(minHeight: 36)
            .background(RoundedRectangle(cornerRadius: 8)
                .fill(selected ? Palette.selected : (hovering ? Palette.hover : .clear)))
            .overlay {
                if selected && focused {
                    RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.ink.opacity(0.4), lineWidth: 1.5)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .focusable(false)
        .onHover { hovering = $0 }
        .pointingHandCursor()
        .accessibilityLabel(section.title)
        .accessibilityIdentifier(section.identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
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
                Button("Replace config.toml with the current settings…") { confirming = true }
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
            .alert("Replace config.toml?", isPresented: $confirming) {
                // Annuler par défaut : Retour et Échap ne remplacent rien.
                Button("Cancel", role: .cancel) {}
                    .keyboardShortcut(.defaultAction)
                Button("Replace", role: .destructive) {
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
                Text("The current file is kept alongside, named config.toml.illisible. The settings shown will be written in its place.")
            }
        }
    }

    static func line(unreadable: Bool) -> String? {
        guard unreadable else { return nil }
        return String(localized: "Unreadable configuration: config.toml could not be read. Settings still apply, but they are not saved until it is fixed or replaced.")
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
