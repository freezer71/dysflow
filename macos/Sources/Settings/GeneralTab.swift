import AppKit
import SwiftUI

/// Onglet Général : démarrage, voix gardée, données.
struct GeneralTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var login = false
    @State private var bookmarks: [StoredBookmarkView] = []
    @State private var cacheReport: String?
    @State private var dataError: String?

    var body: some View {
        Form {
            Section("Démarrage") {
                Toggle("Ouvrir sillon à la connexion", isOn: $login)
                    .onAppear { login = LoginItem.isEnabled }
                    .onChange(of: login) { _, on in
                        // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                        if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                    }
                    .accessibilityLabel("Ouvrir sillon à la connexion")
                LabeledContent("Garder la voix chargée") {
                    PopUpPicker(label: "Garder la voix chargée", identifier: "garder-voix",
                                options: KeepVoiceChoice.options(stored: store.values.keepVoiceMinutes)
                                    .map { ($0.label, $0) },
                                selection: keepVoice)
                        .fixedSize()
                }
            }
            Section("Données") {
                HStack {
                    Button("Vider le cache des documents") { clearCache() }
                    if let cacheReport { Text(cacheReport) }
                }
                Button("Ouvrir le dossier de configuration") { openConfigFolder() }
                    .disabled(model.configDirectory == nil)
                if let dataError { Text(dataError).foregroundStyle(.red) }
            }
            Section("Signets") {
                if bookmarks.isEmpty { Text("Aucun signet") }
                ForEach(BookmarkGroups.group(bookmarks), id: \.document) { group in
                    Text(group.label).fontWeight(.semibold)
                    ForEach(group.items, id: \.self) { b in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(b.snippet)
                                Text("\(b.sectionTitle) · \(b.when)")
                            }
                            Spacer()
                            Button("Supprimer") { remove(b) }
                                .accessibilityLabel("Supprimer le signet « \(b.snippet) »")
                        }
                        .accessibilityElement(children: .contain)
                    }
                }
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-general")
        .task { bookmarks = await model.loadAllBookmarks() }
    }

    private var keepVoice: Binding<KeepVoiceChoice> {
        Binding(get: { KeepVoiceChoice(minutes: store.values.keepVoiceMinutes) },
                set: { store.set(\.keepVoiceMinutes, $0.minutes) })
    }

    private func clearCache() {
        Task {
            do {
                cacheReport = CacheReport.text(freed: try await model.clearDocumentCache())
                dataError = nil
            } catch {
                dataError = SettingsStore.message(of: error)
            }
        }
    }

    private func remove(_ b: StoredBookmarkView) {
        Task {
            do {
                try await model.removeBookmarkAsync(document: b.document, sentence: b.sentence)
                dataError = nil
            } catch {
                dataError = SettingsStore.message(of: error)
            }
            bookmarks = await model.loadAllBookmarks()
        }
    }

    private func openConfigFolder() {
        guard let dir = model.configDirectory else { return }
        let url = URL(fileURLWithPath: dir, isDirectory: true)
        // Pas encore de config.toml : le dossier s'ouvre quand même.
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        NSWorkspace.shared.open(url)
    }
}

/// Les signets d'un document.
struct BookmarkGroup: Equatable {
    let document: String
    let label: String
    let items: [StoredBookmarkView]
}

enum BookmarkGroups {
    /// Par document, dans l'ordre de leur premier signet ; dans chacun,
    /// l'ordre de lecture. Deux fichiers de même nom restent séparés.
    static func group(_ items: [StoredBookmarkView]) -> [BookmarkGroup] {
        var order: [String] = []
        var byDocument: [String: [StoredBookmarkView]] = [:]
        for b in items {
            if byDocument[b.document] == nil { order.append(b.document) }
            byDocument[b.document, default: []].append(b)
        }
        return order.map { document in
            let list = (byDocument[document] ?? []).sorted { $0.sentence < $1.sentence }
            return BookmarkGroup(document: document, label: list.first?.label ?? document, items: list)
        }
    }
}

enum CacheReport {
    static func text(freed bytes: UInt64) -> String {
        guard bytes > 0 else { return "Le cache était déjà vide" }
        return "\(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)) libérés"
    }
}
