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
        Form {
            ErrorText(store.error, identifier: "erreur-modeles")
            ErrorText(settings.error, identifier: "erreur-reglages")
            Section("Catalogue") {
                if store.catalogue.isEmpty {
                    Text("Aucun modèle proposé : cette version est compilée sans audio.cpp.")
                }
                ForEach(store.catalogue, id: \.id) { m in row(m) }
            }
            if let free = store.freeSpace {
                Text("Espace libre : \(SizeFormat.bytes(free))")
                    .accessibilityIdentifier("espace-libre")
            }
        }
        .formStyle(.grouped)
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
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(m.name).fontWeight(.semibold)
                if m.recommended {
                    Text("Recommandé").fontWeight(.semibold).accessibilityIdentifier("recommande")
                }
                Text(m.summary)
                if !m.voicesLabel.isEmpty { Text(m.voicesLabel) }
                Text("\(m.languagesLabel) · \(SizeFormat.bytes(m.bytes)) · \(m.precision.uppercased())")
                Text(m.pace)
                if !m.licenses.isEmpty { Text(m.licenses) }
                if let d = download, d.id == m.id, let failed = d.failed {
                    ErrorText("Téléchargement interrompu : \(failed)", identifier: "erreur-telechargement")
                }
            }
            Spacer()
            actions(m)
        }
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private func actions(_ m: VoiceModel) -> some View {
        if m.active {
            Text("Actif")
        } else if let d = download, d.id == m.id, d.failed == nil {
            VStack(alignment: .trailing) {
                ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                    .frame(width: 120)
                    .accessibilityLabel("Téléchargement de \(m.name)")
                Button(store.cancelling ? "Annulation…" : "Annuler") {
                    Task { await store.cancelDownload() }
                }
                .disabled(store.cancelling)
                .accessibilityLabel("Annuler le téléchargement de \(m.name)")
            }
        } else if m.installed {
            VStack(alignment: .trailing) {
                Button("Utiliser") { Task { await store.use(m.id) } }
                    .accessibilityLabel("Utiliser \(m.name)")
                Button("Supprimer…") { store.askDeletion(m) }
                    .accessibilityLabel("Supprimer \(m.name)")
            }
        } else {
            // Après un échec, le même bouton relance l'installation.
            Button(download?.id == m.id ? "Réessayer" : "Installer") {
                Task { await store.install(m.id) }
            }
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
