import SwiftUI

/// La feuille « Bibliothèque des voix », ouverte depuis la section Voix :
/// toutes les voix des modèles de lecture installés, à écouter sur un texte
/// à soi et à une vitesse d'essai, puis à choisir. Clavier : ↑ ↓ choisissent
/// une ligne, Espace l'écoute, Retour l'utilise, Échap ferme sans rien changer.
struct VoiceLibrarySheet: View {
    let model: ReaderModel
    /// Utilise la voix (en passant à son modèle s'il le faut).
    let onUse: (VoiceLibraryEntry) async throws -> Void
    @State private var library: VoiceLibraryStore
    @State private var useError = ErrorNote()
    @State private var using = false
    /// Où est le clavier : Retour n'« utilise » que depuis la liste, jamais
    /// en écrivant (recherche, texte à lire).
    enum Field: Hashable { case list, search, text }
    @FocusState private var focus: Field?
    @Environment(\.dismiss) private var dismiss

    static let width: CGFloat = 760
    static let height: CGFloat = 680

    init(model: ReaderModel, sampleText: String, speed: Float,
         onUse: @escaping (VoiceLibraryEntry) async throws -> Void) {
        self.model = model
        self.onUse = onUse
        _library = State(initialValue: VoiceLibraryStore(backend: model, sampleText: sampleText, speed: speed))
    }

    var body: some View {
        let preview = model.snapshot.preview
        VStack(alignment: .leading, spacing: 0) {
            header
            filters
                .padding(.top, 16)
            list
                .padding(.top, 12)
            statusLine(preview)
                .padding(.top, 8)
            textAndSpeed
                .padding(.top, 12)
            footer
                .padding(.top, 16)
        }
        .padding(28)
        .frame(width: Self.width, height: Self.height)
        .background(Palette.page)
        .font(Typo.body)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Bibliothèque des voix")
        .accessibilityIdentifier("bibliotheque-voix")
        .task {
            await library.load()
            focus = .list
        }
        .onChange(of: preview.pending || preview.playing) { _, active in library.previewChanged(active: active) }
        // Le chargement d'un autre modèle, et l'échec d'un aperçu, se disent à VoiceOver.
        .onChange(of: preview.preparing) { _, preparing in
            if preparing { Announcement.post(Self.preparing) }
        }
        .onDisappear { Task { await library.close() } }
    }

    static let preparing = "Préparation de la voix…"

    // MARK: - Parties

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Bibliothèque des voix")
                .font(.system(size: 22, weight: .heavy))
                .foregroundStyle(Palette.ink)
                .accessibilityAddTraits(.isHeader)
            Text("Écoutez chaque voix sur votre texte, puis choisissez celle qui lira.")
                .font(Typo.body)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var filters: some View {
        HStack(spacing: 14) {
            TextField("Rechercher une voix", text: $library.filter.search)
                .textFieldStyle(.notion)
                .focused($focus, equals: .search)
                // Retour : la recherche est faite, le clavier passe à la liste.
                .onSubmit { focus = .list }
                .frame(minWidth: 180)
                .accessibilityLabel("Rechercher une voix par son nom")
                .accessibilityIdentifier("bibliotheque-recherche")
            filter("Modèle", identifier: "bibliotheque-modele",
                   options: VoiceLibrary.modelOptions(library.entries), selection: $library.filter.model)
            filter("Accent", identifier: "bibliotheque-accent",
                   options: VoiceLibrary.accentOptions(library.entries), selection: $library.filter.accent)
            filter("Genre", identifier: "bibliotheque-genre",
                   options: VoiceLibrary.genderOptions, selection: $library.filter.gender)
        }
    }

    private func filter(_ title: String, identifier: String, options: [(title: String, value: String)],
                        selection: Binding<String>) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
            PopUpPicker(label: title, identifier: identifier, options: options, selection: selection)
                .fixedSize()
        }
    }

    private var list: some View {
        let rows = library.visible
        return Group {
            if library.loaded && rows.isEmpty {
                Text("Aucune voix ne correspond à ces filtres.")
                    .foregroundStyle(Palette.muted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
                    .accessibilityIdentifier("bibliotheque-vide")
            } else {
                List(rows, selection: $library.selection) { entry in
                    VoiceLibraryRow(entry: entry, playing: library.playing == entry.id,
                                    selected: library.selection == entry.id) {
                        Task { await library.toggle(entry) }
                    }
                    .tag(entry.id)
                }
                .listStyle(.inset)
                .scrollContentBackground(.hidden)
                .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .focused($focus, equals: .list)
                .onKeyPress(.space) {
                    guard let entry = library.selected else { return .ignored }
                    Task { await library.toggle(entry) }
                    return .handled
                }
                .accessibilityLabel("Voix")
                .accessibilityIdentifier("bibliotheque-liste")
            }
        }
        .frame(maxHeight: .infinity)
    }

    @ViewBuilder
    private func statusLine(_ preview: PreviewView) -> some View {
        let refusal = library.error ?? useError.text
        let line = Self.status(preview, refusal: refusal, requested: library.requested)
        let failed = refusal != nil || (library.requested && preview.error != nil)
        Text(line ?? " ")
            .font(Typo.detail)
            .foregroundStyle(failed ? ErrorStyle.color : Palette.muted)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityHidden(line == nil)
            .accessibilityIdentifier("bibliotheque-etat")
    }

    /// L'état de l'aperçu, en clair : refus, échec, préparation, synthèse,
    /// écoute. L'échec du moteur ne se montre qu'une fois un aperçu demandé
    /// ici (`requested`) : un vieil échec d'« Écouter » n'est pas le sien.
    static func status(_ preview: PreviewView, refusal: String?, requested: Bool) -> String? {
        if let refusal { return refusal }
        if requested, let error = preview.error { return error }
        if preview.preparing { return preparing }
        if preview.pending { return "Synthèse de l'aperçu…" }
        if preview.playing { return "Écoute…" }
        return nil
    }

    private var textAndSpeed: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Texte à lire")
                    .font(Typo.rowTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityHidden(true)
                TextEditor(text: $library.text)
                    .font(Typo.body)
                    .scrollContentBackground(.hidden)
                    .padding(6)
                    .frame(height: 64)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Palette.control))
                    .focused($focus, equals: .text)
                    // Échap ferme la feuille, même ici (le texte en ferait
                    // sinon une demande de complétion).
                    .onKeyPress(.escape) {
                        dismiss()
                        return .handled
                    }
                    .accessibilityLabel("Texte à lire")
                    .accessibilityHint("500 caractères au plus")
                    .accessibilityIdentifier("bibliotheque-texte")
                Text(VoiceLibraryStore.countLine(library.text))
                    .font(Typo.detail)
                    .foregroundStyle(library.text.count > VoiceLibraryStore.maxCharacters
                                     ? ErrorStyle.color : Palette.muted)
                    .accessibilityIdentifier("bibliotheque-compte")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            SettingRow(title: "Vitesse de l'essai", detail: "Pour l'écoute seulement : la lecture garde la sienne") {
                Slider(value: Binding(get: { library.speed },
                                      set: { library.speed = SettingFormat.rounded($0) }),
                       in: VoiceLibraryStore.speedRange, step: 0.05) {
                    Text("Vitesse de l'essai")
                }
                .notionSlider()
                .accessibilityLabel("Vitesse de l'essai")
                .accessibilityValue("\(SettingFormat.decimal(library.speed)) fois")
                .accessibilityIdentifier("bibliotheque-vitesse")
                ValueText("\(SettingFormat.decimal(library.speed))×")
            }
        }
        .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
    }

    private var footer: some View {
        HStack(spacing: 10) {
            if let selected = library.selected {
                Text("Choisie : \(selected.label)")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
            }
            Spacer()
            Button("Annuler") { dismiss() }
                .buttonStyle(.notion)
                .keyboardShortcut(.cancelAction)
                .accessibilityIdentifier("bibliotheque-annuler")
            Button("Utiliser") { Task { await use() } }
                .buttonStyle(.notionPrimary)
                // Retour n'utilise que hors des champs : là, il écrit ou valide la recherche.
                .keyboardShortcut(focus == .search || focus == .text ? nil : .defaultAction)
                .disabled(library.selected == nil || using)
                .accessibilityLabel(library.selected.map { "Utiliser \($0.label)" } ?? "Utiliser")
                .accessibilityIdentifier("bibliotheque-utiliser")
        }
    }

    private func use() async {
        guard let entry = library.selected, !using else { return }
        using = true
        defer { using = false }
        await library.stop()
        do {
            try await onUse(entry)
            useError.clear()
            dismiss()
        } catch {
            useError.fail(SettingsStore.message(of: error))
        }
    }
}

/// Une ligne : ▶ Écouter (■ Arrêter), le nom et ses étiquettes, le modèle,
/// l'accent et le genre dessous ; ✓ sur la voix qui lit.
private struct VoiceLibraryRow: View {
    let entry: VoiceLibraryEntry
    let playing: Bool
    let selected: Bool
    let toggle: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: toggle) {
                Image(systemName: playing ? "stop.fill" : "play.fill")
                    .font(.system(size: 11))
                    .frame(width: 14)
            }
            .buttonStyle(.notion)
            .help(playing ? "Arrêter" : "Écouter")
            .accessibilityLabel(playing ? "Arrêter \(entry.label)" : "Écouter \(entry.label)")
            .accessibilityIdentifier("ecouter-\(entry.id)")
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(entry.label)
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                    if let badge = entry.qualityBadge { Tag(text: badge, tint: .blue) }
                    if entry.recommended { Tag(text: "Recommandée", tint: .green) }
                }
                Text(entry.details)
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(VoiceLibrary.accessibilityLabel(entry))
            .accessibilityAddTraits(selected ? .isSelected : [])
            Spacer(minLength: 8)
            if entry.active {
                HStack(spacing: 4) {
                    Image(systemName: "checkmark")
                        .font(.system(size: 12, weight: .semibold))
                    Text("Utilisée")
                        .font(Typo.caption)
                }
                .foregroundStyle(Palette.ink)
                .accessibilityHidden(true)
            }
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("voix-\(entry.id)")
    }
}
