import SwiftUI

/// Onglet Modèles : le catalogue, installer, annuler, supprimer.
///
/// Le magasin vient de la fenêtre : elle suit la fin des téléchargements
/// quel que soit l'onglet affiché.
struct ModelsTab: View {
    let model: ReaderModel
    let store: ModelsStore
    let settings: SettingsStore

    private var download: DownloadView? { model.snapshot.download }

    var body: some View {
        SettingsPage(title: "Modèles", subtitle: "Les voix installées sur ce Mac. Tout tourne en local, rien ne part en ligne.") {
            ErrorText(store.error, identifier: "erreur-modeles")
            ErrorText(settings.error, identifier: "erreur-reglages")
            SettingsGroup(title: "Catalogue") {
                if store.catalogue.isEmpty {
                    SettingNote("Aucun modèle proposé par cette version : la voix du système lit.")
                }
                ForEach(store.catalogue, id: \.id) { m in row(m) }
            }
            if let free = store.freeSpace {
                SettingNote("Espace libre : \(SizeFormat.bytes(free))", identifier: "espace-libre")
                    .padding(.top, 8)
            }
        }
        .accessibilityIdentifier("onglet-modeles")
        .task { await store.reload() }
        .alert(deletionTitle, isPresented: deletionShown, presenting: store.pendingDeletion) { m in
            // Annuler est le bouton par défaut : Retour et Échap n'effacent rien.
            Button("Annuler", role: .cancel) { store.cancelDeletion() }
                .keyboardShortcut(.defaultAction)
            Button("Supprimer", role: .destructive) { Task { await store.delete(m) } }
        } message: { m in
            if case .confirm(_, let message) = ModelsStore.deletionRequest(for: m) { Text(message) }
        }
    }

    @ViewBuilder private func row(_ m: VoiceModel) -> some View {
        HStack(alignment: .top, spacing: 16) {
            Image(systemName: "waveform")
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Palette.ink)
                .frame(width: 34, height: 34)
                .background(RoundedRectangle(cornerRadius: 8).fill(Palette.callout))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(m.name).font(.system(size: 14, weight: .semibold)).foregroundStyle(Palette.ink)
                    if m.recommended {
                        Tag(text: "Recommandé", tint: .green).accessibilityIdentifier("recommande")
                    }
                    if m.active { Tag(text: "Actif", tint: .blue) }
                }
                Text(m.summary).font(Typo.body).foregroundStyle(Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Group {
                    if !m.voicesLabel.isEmpty { Text(m.voicesLabel) }
                    Text("\(m.languagesLabel) · \(SizeFormat.bytes(m.bytes)) · \(m.precision.uppercased())")
                    Text(m.pace)
                    if !m.licenses.isEmpty { Text(m.licenses) }
                }
                .font(Typo.detail)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
                if let d = download, d.id == m.id, let failed = d.failed {
                    ErrorText("Téléchargement interrompu : \(failed)", identifier: "erreur-telechargement")
                }
            }
            Spacer(minLength: 8)
            actions(m)
        }
        .padding(.vertical, 16)
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private func actions(_ m: VoiceModel) -> some View {
        if m.active {
            // L'étiquette « Actif » est à côté du nom.
            EmptyView()
        } else if let d = download, d.id == m.id, d.failed == nil {
            VStack(alignment: .trailing, spacing: 8) {
                ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                    .tint(Palette.accent)
                    .frame(width: 120)
                    .accessibilityLabel("Téléchargement de \(m.name)")
                Button(store.cancelling ? "Annulation…" : "Annuler") {
                    Task { await store.cancelDownload() }
                }
                .buttonStyle(.notion)
                .disabled(store.cancelling)
                .accessibilityLabel("Annuler le téléchargement de \(m.name)")
            }
        } else if m.installed {
            HStack(spacing: 6) {
                Button("Supprimer…") { store.askDeletion(m) }
                    .buttonStyle(.notionPlain)
                    .accessibilityLabel("Supprimer \(m.name)")
                Button("Utiliser") { Task { await store.use(m.id) } }
                    .buttonStyle(.notionPrimary)
                    .accessibilityLabel("Utiliser \(m.name)")
            }
        } else {
            // Après un échec, le même bouton relance l'installation.
            Button(download?.id == m.id ? "Réessayer" : "Installer") {
                Task { await store.install(m.id) }
            }
            .buttonStyle(.notionPrimary)
            .accessibilityLabel("Installer \(m.name)")
            .disabled(ReaderModel.isDownloading(download))
        }
    }

    private var deletionTitle: String {
        guard let m = store.pendingDeletion,
              case .confirm(let title, _) = ModelsStore.deletionRequest(for: m) else { return "" }
        return title
    }

    private var deletionShown: Binding<Bool> {
        Binding(get: { store.pendingDeletion != nil },
                set: { if !$0, store.pendingDeletion != nil { store.cancelDeletion() } })
    }
}
