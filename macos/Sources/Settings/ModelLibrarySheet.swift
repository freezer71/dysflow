import SwiftUI

/// La feuille « Bibliothèque des modèles », ouverte depuis la section
/// Dictée : tous les modèles de dictée, à chercher par nom, à filtrer, à
/// télécharger ou supprimer, puis à choisir. Même gabarit que la
/// bibliothèque des voix. Clavier : ↑ ↓ choisissent une ligne, Retour
/// l'utilise (hors de la recherche), Échap ferme sans rien changer.
///
/// La progression d'un téléchargement se dit à VoiceOver depuis la fenêtre
/// (`ReglagesView`, tous les 25 %) : elle reste en vie derrière la feuille,
/// et la feuille (760 pt) ne la couvre jamais toute (860 pt au moins), sa
/// relecture continue. Rien n'est redit d'ici : l'annonce viendrait deux fois.
struct ModelLibrarySheet: View {
    let model: ReaderModel
    /// Les modèles de dictée (`DictationSection.rows`) : la dictée de macOS
    /// en tête, l'utilisé marqué `active`.
    let rows: [VoiceModel]
    /// Un essai du micro est en cours : rien ne change (comme la table).
    let locked: Bool
    /// Choisit le modèle pour la dictée (`DictationSection.use`).
    let onUse: (VoiceModel) -> Void
    /// Après la suppression d'un modèle (`DictationSection.removed`).
    let removed: (VoiceModel) -> Void
    /// Relit les modèles et les réglages après une action.
    let onChange: () async -> Void
    @State private var filter = ModelLibrary.Filter()
    /// La ligne choisie (`VoiceModel.id`).
    @State private var selection: String?
    @State private var confirming: VoiceModel?
    @State private var error = ErrorNote()
    /// Où est le clavier : Retour n'« utilise » que depuis la liste, jamais
    /// en écrivant la recherche.
    enum Field: Hashable { case list, search }
    @FocusState private var focus: Field?
    @Environment(\.dismiss) private var dismiss

    static let width = VoiceLibrarySheet.width
    static let height = VoiceLibrarySheet.height

    private var visible: [VoiceModel] { ModelLibrary.filtered(rows, filter) }
    private var selected: VoiceModel? { ModelLibrary.selected(visible, selection: selection) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            filters
                .padding(.top, 16)
            list
                .padding(.top, 12)
            statusLine
                .padding(.top, 8)
            footer
                .padding(.top, 16)
        }
        .padding(28)
        .frame(width: Self.width, height: Self.height)
        .background(Palette.page)
        .font(Typo.body)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Bibliothèque des modèles")
        .accessibilityIdentifier("bibliotheque-modeles")
        .onAppear {
            // Le modèle utilisé, choisi d'avance : Retour seul ne change rien.
            if selection == nil { selection = rows.first(where: \.active)?.id }
            focus = .list
        }
        .alert(confirming.map { ModelTable.confirmation($0, fallback: DictationSection.fallback).title } ?? "",
               isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
               presenting: confirming) { row in
            // Annuler par défaut : Retour et Échap ne suppriment rien.
            Button("Annuler", role: .cancel) {}
                .keyboardShortcut(.defaultAction)
            Button("Supprimer", role: .destructive) {
                act {
                    try await model.removeModel(row.id)
                    removed(row)
                }
            }
        } message: { row in
            Text(ModelTable.confirmation(row, fallback: DictationSection.fallback).message)
        }
    }

    // MARK: - Parties

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Bibliothèque des modèles")
                .font(.system(size: 22, weight: .heavy))
                .foregroundStyle(Palette.ink)
                .accessibilityAddTraits(.isHeader)
            Text("Tous les modèles de dictée : téléchargez ceux qui vous intéressent, puis choisissez celui qui écrira.")
                .font(Typo.body)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var filters: some View {
        HStack(spacing: 14) {
            TextField("Rechercher un modèle", text: $filter.search)
                .textFieldStyle(.notion)
                .focused($focus, equals: .search)
                // Retour : la recherche est faite, le clavier passe à la liste.
                .onSubmit { focus = .list }
                .frame(minWidth: 160)
                .accessibilityLabel("Rechercher un modèle par son nom")
                .accessibilityIdentifier("modeles-recherche")
            filter("Éditeur", identifier: "modeles-editeur",
                   options: ModelLibrary.makerOptions(rows), selection: $filter.maker)
            filter("Langue", identifier: "modeles-langue",
                   options: ModelLibrary.languageOptions, selection: $filter.language)
            filter("État", identifier: "modeles-etat",
                   options: ModelLibrary.stateOptions, selection: $filter.state)
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
        let shown = visible
        let download = model.snapshot.download
        return Group {
            if shown.isEmpty {
                Text("Aucun modèle ne correspond à ces filtres.")
                    .foregroundStyle(Palette.muted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
                    .accessibilityIdentifier("modeles-vide")
            } else {
                List(shown, id: \.id, selection: $selection) { row in
                    ModelLibraryRow(row: row, state: ModelTable.state(row, download: download),
                                    download: download, locked: locked, selected: selection == row.id,
                                    install: { act { try await model.installModel(row.id) } },
                                    cancel: { act { try await model.cancelDownload() } },
                                    remove: { confirming = row })
                        .tag(row.id)
                }
                .listStyle(.inset)
                .scrollContentBackground(.hidden)
                .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .focused($focus, equals: .list)
                .accessibilityLabel("Modèles")
                .accessibilityIdentifier("modeles-liste")
            }
        }
        .frame(maxHeight: .infinity)
    }

    /// L'essai du micro qui bloque tout, ou l'échec d'une action.
    @ViewBuilder
    private var statusLine: some View {
        let line = error.text ?? (locked ? DictationSection.lockedByTrial : nil)
        Text(line ?? " ")
            .font(Typo.detail)
            .foregroundStyle(error.text != nil ? ErrorStyle.color : Palette.muted)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityHidden(line == nil)
            .accessibilityIdentifier("modeles-etat-feuille")
    }

    private var footer: some View {
        HStack(spacing: 10) {
            if let selected {
                Text("Choisi : \(selected.name)")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
            }
            Spacer()
            Button("Annuler") { dismiss() }
                .buttonStyle(.notion)
                .keyboardShortcut(.cancelAction)
                .accessibilityIdentifier("modeles-annuler")
            Button("Utiliser") { use() }
                .buttonStyle(.notionPrimary)
                // Retour n'utilise que hors de la recherche : là, il la valide.
                .keyboardShortcut(focus == .search ? nil : .defaultAction)
                .disabled(!ModelLibrary.canUse(selected) || locked)
                .help(selected.map { ModelLibrary.canUse($0) ? "" : "Téléchargez-le d'abord" } ?? "")
                .accessibilityLabel(selected.map { "Utiliser \($0.name)" } ?? "Utiliser")
                .accessibilityIdentifier("modeles-utiliser")
        }
    }

    private func use() {
        guard let selected, ModelLibrary.canUse(selected), !locked else { return }
        if !selected.active { onUse(selected) }
        dismiss()
    }

    private func act(_ body: @escaping () async throws -> Void) {
        Task {
            do {
                try await body()
                error.clear()
            } catch {
                self.error.fail(SettingsStore.message(of: error))
            }
            await onChange()
        }
    }
}

/// Une ligne : le logo de l'éditeur, le nom (✓ s'il écrit), langues et
/// précision, le résumé en une ligne ; la taille et l'action à droite.
private struct ModelLibraryRow: View {
    let row: VoiceModel
    let state: ModelRowState
    let download: DownloadView?
    let locked: Bool
    let selected: Bool
    let install: () -> Void
    let cancel: () -> Void
    let remove: () -> Void

    private static let sizeWidth: CGFloat = 76
    private static let actionWidth: CGFloat = 180

    var body: some View {
        HStack(spacing: 12) {
            MakerBadge(maker: ModelTable.maker(row))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(row.name)
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                    if row.active {
                        HStack(spacing: 3) {
                            Image(systemName: "checkmark")
                                .font(.system(size: 11, weight: .semibold))
                            Text(row.system ? "Utilisée" : "Utilisé")
                                .font(Typo.caption)
                        }
                        .foregroundStyle(Palette.ink)
                    }
                }
                let details = ModelLibrary.details(row)
                if !details.isEmpty {
                    Text(details)
                        .font(Typo.detail)
                        .foregroundStyle(Palette.muted)
                        .lineLimit(1)
                }
                if !row.summary.isEmpty {
                    Text(row.summary)
                        .font(Typo.detail)
                        .foregroundStyle(Palette.muted)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .help(row.summary)
                }
                // L'échec en entier, en rouge : pas caché dans une bulle d'aide.
                if case .failed(let message) = state {
                    Text("Échec du téléchargement : \(message)")
                        .font(Typo.detail)
                        .foregroundStyle(ErrorStyle.color)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityHidden(true)   // dit par l'étiquette de la ligne
            Text(ModelTable.size(row))
                .monospacedDigit()
                .foregroundStyle(Palette.ink)
                .frame(width: Self.sizeWidth, alignment: .trailing)
                .accessibilityHidden(true)
            HStack(spacing: 8) {
                action
                if state != .installed, ModelTable.canRemove(row, state) {
                    removeButton
                }
            }
            .frame(width: Self.actionWidth, alignment: .leading)
            .disabled(locked)
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .accessibilityElement(children: .contain)
        .accessibilityLabel(ModelLibrary.accessibilityLabel(row, state))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("modele-biblio-\(row.id)")
    }

    @ViewBuilder private var action: some View {
        switch state {
        case .system:
            Text("Sur ce Mac")
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
        case .notInstalled:
            let allowed = ModelTable.canDownload(row, download: download)
            Button("Télécharger", action: install)
                .buttonStyle(.notion)
                .disabled(!allowed)
                .help(allowed ? "" : "Un téléchargement est en cours")
                .accessibilityLabel("Télécharger \(row.name)")
                .accessibilityIdentifier("biblio-telecharger-\(row.id)")
        case .downloading(let fraction):
            HStack(spacing: 8) {
                ProgressView(value: fraction)
                    .frame(width: 80)
                    .accessibilityLabel("Téléchargement de \(row.name)")
                    .accessibilityValue("\(Int(fraction * 100)) %")
                Button("Annuler", action: cancel)
                    .buttonStyle(.notionPlain)
                    .accessibilityLabel("Annuler le téléchargement de \(row.name)")
                    .accessibilityIdentifier("biblio-annuler-\(row.id)")
            }
        case .failed:
            Button("Réessayer", action: install)
                .buttonStyle(.notion)
                .fixedSize()
                .disabled(!ModelTable.canDownload(row, download: download))
                .accessibilityLabel("Réessayer de télécharger \(row.name)")
                .accessibilityIdentifier("biblio-telecharger-\(row.id)")
        case .installed:
            removeButton
        }
    }

    private var removeButton: some View {
        Button("Supprimer…", action: remove)
            .buttonStyle(.notionPlain)
            .fixedSize()
            .accessibilityLabel("Supprimer \(row.name)")
            .accessibilityIdentifier("biblio-supprimer-\(row.id)")
    }
}
