import SwiftUI

/// Une table de modèles : choix, nom, taille, état et action. Des lignes à
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
    @State private var confirming: VoiceModel?
    @State private var error = ErrorNote()

    private static let choiceWidth: CGFloat = 22
    private static let sizeWidth: CGFloat = 80
    private static let actionWidth: CGFloat = 190

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                Color.clear.frame(width: Self.choiceWidth, height: 1)
                Text("Modèle").frame(maxWidth: .infinity, alignment: .leading)
                Text("Taille").frame(width: Self.sizeWidth, alignment: .trailing)
                Text("État").frame(width: Self.actionWidth, alignment: .leading)
            }
            .font(Typo.caption)
            .foregroundStyle(Palette.muted)
            .padding(.vertical, 6)
            .accessibilityHidden(true)
            Rule()
            ForEach(shown, id: \.id) { row in
                line(row)
                Rule()
            }
            ErrorText(error.text, identifier: "erreur-modeles")
                .padding(.top, 6)
        }
        .padding(.vertical, 6)
        .alert(confirming.map { ModelTable.confirmation($0, fallback: fallback).title } ?? "",
               isPresented: Binding(get: { confirming != nil },
                                    set: { if !$0 { confirming = nil } }),
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
            Text(ModelTable.confirmation(row, fallback: fallback).message)
        }
        // VoiceOver entend la progression tous les 25 %, pour les lignes de
        // cette table seulement (l'autre table la dit pour les siennes).
        .onChange(of: model.snapshot.download) { old, new in
            guard let new, let row = rows.first(where: { $0.id == new.id }),
                  let current = ModelTable.fraction(new) else { return }
            let previous = old?.id == new.id ? ModelTable.fraction(old) : nil
            if let text = ModelTable.progressAnnouncement(name: row.name, previous: previous, current: current) {
                Announcement.post(text)
            }
        }
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
                Text("Échec du téléchargement : \(message)")
                    .font(Typo.detail)
                    .foregroundStyle(ErrorStyle.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, Self.choiceWidth + 12)
                    .accessibilityHidden(true)   // déjà dans l'étiquette de la ligne
            }
        }
        .padding(.vertical, 8)
        .frame(minHeight: 44)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Self.label(row, state))
        .accessibilityIdentifier("modele-\(row.id)")
    }

    private func columns(_ row: VoiceModel, _ state: ModelRowState) -> some View {
        HStack(spacing: 12) {
            Button {
                act {
                    if let use { try await use(row) } else { try await model.useModel(row.id) }
                }
            } label: {
                Image(systemName: row.active ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(row.active ? Palette.accent : Palette.muted)
                    .frame(width: Self.choiceWidth)
            }
            .buttonStyle(.plain)
            .disabled(!(row.installed || row.system) || row.active)
            .accessibilityLabel(row.active ? "\(row.name), \(ModelTable.used(row))" : "Utiliser \(row.name)")
            .accessibilityIdentifier("utiliser-\(row.id)")
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
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityHidden(true)
            Text(ModelTable.size(row))
                .monospacedDigit()
                .foregroundStyle(Palette.ink)
                .frame(width: Self.sizeWidth, alignment: .trailing)
                .accessibilityHidden(true)
            HStack(spacing: 8) {
                action(row, state)
                if state != .installed, ModelTable.canRemove(row, state) {
                    removeButton(row)
                }
            }
            .frame(width: Self.actionWidth, alignment: .leading)
        }
    }

    private func removeButton(_ row: VoiceModel) -> some View {
        Button("Supprimer…") { confirming = row }
            .buttonStyle(.notionPlain)
            .fixedSize()
            .accessibilityLabel("Supprimer \(row.name)")
            .accessibilityIdentifier("supprimer-\(row.id)")
    }

    @ViewBuilder private func action(_ row: VoiceModel, _ state: ModelRowState) -> some View {
        switch state {
        case .system:
            Text(row.active ? "Utilisée" : "Sur ce Mac")   // la voix de macOS : féminin
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
        case .notInstalled:
            let allowed = ModelTable.canDownload(row, download: model.snapshot.download)
            Button("Télécharger") { act { try await model.installModel(row.id) } }
                .buttonStyle(.notion)
                .disabled(!allowed)
                .help(allowed ? "" : "Un téléchargement est en cours")
                .accessibilityLabel("Télécharger \(row.name)")
                .accessibilityIdentifier("telecharger-\(row.id)")
        case .downloading(let fraction):
            HStack(spacing: 8) {
                ProgressView(value: fraction)
                    .frame(width: 90)
                    .accessibilityLabel("Téléchargement de \(row.name)")
                    .accessibilityValue("\(Int(fraction * 100)) %")
                Button("Annuler") { act { try await model.cancelDownload() } }
                    .buttonStyle(.notionPlain)
                    .accessibilityLabel("Annuler le téléchargement de \(row.name)")
                    .accessibilityIdentifier("annuler-\(row.id)")
            }
        case .failed:
            // Le message s'affiche en entier sous la ligne.
            Button("Réessayer") { act { try await model.installModel(row.id) } }
                .buttonStyle(.notion)
                .fixedSize()
                .accessibilityLabel("Réessayer de télécharger \(row.name)")
                .accessibilityIdentifier("telecharger-\(row.id)")
        case .installed:
            removeButton(row)
        }
    }

    /// Ce que VoiceOver dit d'une ligne : son nom, s'il est utilisé, son état
    /// et sa taille.
    nonisolated static func label(_ row: VoiceModel, _ state: ModelRowState) -> String {
        var parts = [row.name, row.active ? ModelTable.used(row) : "non \(ModelTable.used(row))"]
        switch state {
        case .system: parts.append("sur ce Mac")
        case .notInstalled: parts.append("non installé, \(ModelTable.size(row)) à télécharger")
        case .downloading(let fraction): parts.append("téléchargement \(Int(fraction * 100)) %")
        case .failed(let message): parts.append("échec du téléchargement : \(message)")
        case .installed: parts.append("installé, \(ModelTable.size(row))")
        }
        if state != .installed, ModelTable.canRemove(row, state) {
            parts.append("téléchargement incomplet, \(ModelTable.bytes(row.diskBytes)) sur le disque")
        }
        return parts.joined(separator: ", ")
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
