import SwiftUI

/// Une table de modèles : choix, nom, taille et une petite action (corbeille,
/// progression). Un clic sur une ligne absente la télécharge. Des lignes à
/// colonnes fixes plutôt qu'une `Table` : la page des réglages défile déjà,
/// une `Table` s'y coincerait ; et chaque ligne reste un seul élément pour
/// VoiceOver. Lecture et dictée partagent la table : chacune dit quelle
/// ligne est utilisée et comment en choisir une autre.
struct ModelsTable: View {
    let rows: [VoiceModel]
    let model: ReaderModel
    /// Ce qui prend la main si le modèle utilisé est supprimé.
    let fallback: String
    /// La ligne utilisée. Lecture : `row.active` (le moteur) ; dictée : le
    /// choix des réglages.
    var isUsed: (VoiceModel) -> Bool = { $0.active }
    /// Utiliser une ligne. Lecture : le moteur.
    var use: ((VoiceModel) async throws -> Void)?
    /// Après la suppression d'une ligne (la dictée repasse à macOS si c'était la sienne).
    var removed: (VoiceModel) -> Void = { _ in }
    let onChange: () async -> Void
    /// L'erreur et la suppression à confirmer, gardées par la fenêtre : un
    /// échec arrivé pendant qu'une autre section est affichée se retrouve au retour.
    let state: ModelsTableState

    private static let choiceWidth: CGFloat = 22
    private static let sizeWidth: CGFloat = 80
    /// La place à droite : une corbeille, ou la progression et sa croix.
    private static let actionWidth: CGFloat = 104

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 14) {
                Color.clear.frame(width: Self.choiceWidth + 14 + MakerBadge.size, height: 1)
                Text("Model").frame(maxWidth: .infinity, alignment: .leading)
                Text("Size").frame(width: Self.sizeWidth, alignment: .trailing)
                Color.clear.frame(width: Self.actionWidth, height: 1)
            }
            .font(Typo.caption)
            .foregroundStyle(Palette.muted)
            .padding(.horizontal, Space.inset)
            .padding(.top, 14)
            .padding(.bottom, 10)
            .accessibilityHidden(true)
            Rule().padding(.horizontal, Space.inset)
            ForEach(shown, id: \.id) { row in
                line(row)
                    .padding(.horizontal, Space.inset)
                Rule().padding(.horizontal, Space.inset)
            }
            ErrorText(state.error.text, identifier: "erreur-modeles")
                .padding(.horizontal, Space.inset)
                .padding(.top, 8)
        }
        .padding(.bottom, 8)
        .alert(state.confirming.map { ModelTable.confirmation($0, fallback: fallback).title } ?? "",
               isPresented: Binding(get: { state.confirming != nil },
                                    set: { if !$0 { state.confirming = nil } }),
               presenting: state.confirming) { row in
            // Annuler par défaut : Retour et Échap ne suppriment rien.
            Button("Cancel", role: .cancel) {}
                .keyboardShortcut(.defaultAction)
            Button("Delete", role: .destructive) {
                act {
                    try await model.removeModel(row.id)
                    removed(row)
                }
            }
        } message: { row in
            Text(ModelTable.confirmation(row, fallback: fallback).message)
        }
        // La progression (tous les 25 %) se dit à VoiceOver depuis la
        // fenêtre (`ReglagesView`) : quelle que soit la section affichée.
    }

    /// Les lignes, l'utilisée marquée d'après `isUsed`.
    private var shown: [VoiceModel] {
        rows.map { row in
            var row = row
            row.active = isUsed(row)
            return row
        }
    }

    private func line(_ row: VoiceModel) -> some View {
        let state = ModelTable.state(row, download: model.snapshot.download)
        return VStack(alignment: .leading, spacing: 4) {
            columns(row, state)
            // Le message d'échec en entier, sous la ligne, sur toute sa
            // largeur : pas coupé ni caché dans une bulle d'aide.
            if case .failed(let message) = state {
                Text("Download failed: \(message)")
                    .font(Typo.detail)
                    .foregroundStyle(ErrorStyle.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, Self.choiceWidth + 14 + MakerBadge.size + 14)
                    .accessibilityHidden(true)   // déjà dans l'étiquette de la ligne
            }
        }
        .padding(.vertical, 10)
        .frame(minHeight: 64)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Self.label(row, state))
        .accessibilityIdentifier("modele-\(row.id)")
    }

    private func columns(_ row: VoiceModel, _ state: ModelRowState) -> some View {
        // `return` explicite plus bas : des déclarations précèdent la vue.
        let download = model.snapshot.download
        let downloads = ModelTable.downloadsOnClick(row, download: download)
        return HStack(spacing: 14) {
            // Le rond, le nom et la taille : un seul bouton. Installé, il
            // choisit le modèle ; pas encore là, il le télécharge (le rond
            // devient une flèche). La corbeille et la progression restent à part.
            Button {
                act {
                    if downloads {
                        try await model.installModel(row.id)
                    } else if let use {
                        try await use(row)
                    } else {
                        try await model.useModel(row.id)
                    }
                }
            } label: {
                choice(row)
            }
            .buttonStyle(ModelRowButtonStyle())
            .disabled(!(ModelTable.canUse(row) || downloads))
            .help(downloads ? String(localized: "Download \(row.name) (\(ModelTable.size(row)))")
                : state == .notInstalled ? String(localized: "A download is in progress") : "")
            .accessibilityLabel(row.active ? [row.name, ModelTable.used(row)].joined(separator: ", ")
                : downloads ? String(localized: "Download \(row.name)") : String(localized: "Use \(row.name)"))
            .accessibilityIdentifier("utiliser-\(row.id)")
            HStack(spacing: 6) {
                action(row, state)
                if state != .installed, ModelTable.canRemove(row, state) {
                    removeButton(row)
                }
            }
            .frame(width: Self.actionWidth, alignment: .trailing)
        }
    }

    private func choice(_ row: VoiceModel) -> some View {
        HStack(spacing: 14) {
            Image(systemName: Self.choiceSymbol(row))
                .foregroundStyle(row.active ? Palette.accent : Palette.muted)
                .frame(width: Self.choiceWidth)
            MakerBadge(maker: ModelTable.maker(row))
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name)
                    .font(Typo.rowTitle)
                    .foregroundStyle(Palette.ink)
                let detail = [row.languagesLabel, row.precision].filter { !$0.isEmpty }.joined(separator: " · ")
                if !detail.isEmpty {
                    Text(detail)
                        .font(Typo.detail)
                        .foregroundStyle(Palette.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                ModelScoreLine(id: row.id)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityHidden(true)
            Text(ModelTable.size(row))
                .monospacedDigit()
                .foregroundStyle(Palette.ink)
                .frame(width: Self.sizeWidth, alignment: .trailing)
                .accessibilityHidden(true)
        }
    }

    /// Le rond (plein : utilisé) ; une flèche pour un modèle à télécharger.
    nonisolated static func choiceSymbol(_ row: VoiceModel) -> String {
        if row.active { return "largecircle.fill.circle" }
        return row.installed || row.system ? "circle" : "arrow.down.circle"
    }

    private func removeButton(_ row: VoiceModel) -> some View {
        IconButton(symbol: "trash", help: String(localized: "Delete \(row.name)")) { state.confirming = row }
            .accessibilityIdentifier("supprimer-\(row.id)")
    }

    /// La petite action de droite. Rien pour la voix de macOS, ni pour un
    /// modèle à télécharger (un clic sur la ligne suffit).
    @ViewBuilder private func action(_ row: VoiceModel, _ state: ModelRowState) -> some View {
        switch state {
        case .system, .notInstalled:
            EmptyView()
        case .downloading(let fraction):
            ProgressView(value: fraction)
                .frame(width: 64)
                .accessibilityLabel("Downloading \(row.name)")
                .accessibilityValue("\(Int(fraction * 100))%")
            IconButton(symbol: "xmark.circle.fill", help: String(localized: "Cancel downloading \(row.name)")) {
                act { try await model.cancelDownload() }
            }
            .accessibilityIdentifier("annuler-\(row.id)")
        case .failed:
            // Le message s'affiche en entier sous la ligne.
            IconButton(symbol: "arrow.clockwise", help: String(localized: "Retry downloading \(row.name)")) {
                act { try await model.installModel(row.id) }
            }
            .accessibilityIdentifier("telecharger-\(row.id)")
        case .installed:
            removeButton(row)
        }
    }

    /// Ce que VoiceOver dit d'une ligne : son nom, s'il est utilisé, son état
    /// et sa taille.
    nonisolated static func label(_ row: VoiceModel, _ state: ModelRowState) -> String {
        var parts = [row.name, row.active ? ModelTable.used(row) : ModelTable.notUsed(row)]
        // Ce que vaut le modèle, avant ce qu'il en est sur ce Mac.
        if let score = ModelScore.of(row.id) { parts.append(score.spoken) }
        switch state {
        case .system: parts.append(String(localized: "on this Mac"))
        case .notInstalled: parts.append(String(localized: "not installed, \(ModelTable.size(row)) to download"))
        case .downloading(let fraction): parts.append(String(localized: "downloading \(Int(fraction * 100))%"))
        case .failed(let message): parts.append(String(localized: "download failed: \(message)"))
        case .installed: parts.append(String(localized: "installed, \(ModelTable.size(row))"))
        }
        if state != .installed, ModelTable.canRemove(row, state) {
            parts.append(String(localized: "incomplete download, \(ModelTable.bytes(row.diskBytes)) on disk"))
        }
        return parts.joined(separator: ", ")
    }

    private func act(_ body: @escaping () async throws -> Void) {
        Task {
            do {
                try await body()
                state.error.clear()
            } catch {
                state.error.fail(SettingsStore.message(of: error))
            }
            await onChange()
        }
    }
}

/// Ce qu'une table de modèles garde au-delà de son affichage : son erreur
/// (annoncée à VoiceOver à chaque échec) et la suppression qui attend sa
/// confirmation. La fenêtre des réglages en tient une par table.
@MainActor
@Observable
final class ModelsTableState {
    let error = ErrorNote()
    var confirming: VoiceModel?
}

/// Le logo de l'éditeur d'un modèle, dans une pastille : à l'encre, comme
/// le texte, pour suivre le thème. Décoratif : le nom suit, VoiceOver le dit.
struct MakerBadge: View {
    let maker: ModelMaker

    static let size: CGFloat = 28

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 7).fill(Palette.control)
            if let logo = maker.logo {
                Image(logo)
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: 16, height: 16)
            } else if case .initial(let letter) = maker {
                Text(letter).font(.system(size: 13, weight: .semibold))
            }
        }
        .foregroundStyle(Palette.ink)
        .frame(width: Self.size, height: Self.size)
        .accessibilityHidden(true)
    }
}

/// La zone de choix d'une ligne de modèle : un fond au survol et la main
/// quand on peut la choisir ; sinon (pas installé, ou déjà utilisé), rien —
/// et le texte garde sa couleur (désactivé ne veut pas dire pâle ici).
struct ModelRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        ModelRowButtonBody(configuration: configuration)
    }
}

private struct ModelRowButtonBody: View {
    let configuration: ButtonStyleConfiguration
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        configuration.label
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
            .background(RoundedRectangle(cornerRadius: 8).fill(
                enabled && configuration.isPressed ? Palette.selected
                    : (enabled && hovering ? Palette.hover : .clear)))
            .padding(.horizontal, -6)
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
            .pointingHandCursor()
    }
}

/// Un petit bouton d'icône, discret, la bulle d'aide pour nom (VoiceOver le
/// lit aussi).
struct IconButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(hovering ? Palette.ink : Palette.muted)
                .frame(width: 26, height: 26)
                .background(RoundedRectangle(cornerRadius: 6).fill(hovering ? Palette.hover : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .pointingHandCursor()
        .help(help)
        .accessibilityLabel(help)
    }
}
