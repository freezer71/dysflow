import AppKit
import SwiftUI

/// Onglet Général : démarrage, voix gardée, données.
struct GeneralTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var login = false
    @State private var bookmarks: [StoredBookmarkView] = []
    @State private var cacheReport: String?
    @State private var dataError = ErrorNote()
    @State private var pendingRemoval: StoredBookmarkView?

    var body: some View {
        SettingsPage(title: "Général", subtitle: "Démarrage, mémoire de la voix, données et signets.") {
            ErrorText(store.error, identifier: "erreur-reglages")
            SettingsGroup(title: "Démarrage") {
                SettingRow(title: "Ouvrir sillon à la connexion", detail: "Dans la barre des menus, prêt à lire") {
                    Toggle("Ouvrir sillon à la connexion", isOn: $login)
                        .notionSwitch()
                        .onAppear { login = LoginItem.isEnabled }
                        .onChange(of: login) { _, on in
                            // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                            if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                        }
                        .accessibilityLabel("Ouvrir sillon à la connexion")
                }
                SettingRow(title: "Garder la voix chargée", detail: "Après la dernière lecture, avant de libérer la mémoire") {
                    PopUpPicker(label: "Garder la voix chargée", identifier: "garder-voix",
                                options: KeepVoiceChoice.options(stored: store.values.keepVoiceMinutes)
                                    .map { ($0.label, $0) },
                                selection: keepVoice)
                        .fixedSize()
                }
            }
            SettingsGroup(title: "Données") {
                SettingRow(title: "Cache des documents",
                           detail: cacheReport ?? "Les documents se reconstruisent à la prochaine ouverture") {
                    Button("Vider le cache") { clearCache() }
                        .buttonStyle(.notion)
                        .accessibilityLabel("Vider le cache des documents")
                }
                SettingRow(title: "Dossier de configuration", detail: "config.toml, signets, modèles") {
                    Button("Ouvrir le dossier") { openConfigFolder() }
                        .buttonStyle(.notion)
                        .accessibilityLabel("Ouvrir le dossier de configuration")
                        .disabled(model.configDirectory == nil)
                }
                ErrorText(dataError.text, identifier: "erreur-donnees")
            }
            SettingsGroup(title: "Signets") {
                if bookmarks.isEmpty { SettingNote("Aucun signet") }
                ForEach(BookmarkGroups.group(bookmarks), id: \.document) { group in
                    Label(group.label, systemImage: "doc.text")
                        .font(Typo.caption)
                        .foregroundStyle(Palette.muted)
                        .padding(.top, 12)
                        .padding(.bottom, 2)
                        .accessibilityAddTraits(.isHeader)
                    ForEach(group.items, id: \.self) { b in
                        BookmarkRow(bookmark: b) { pendingRemoval = b }
                    }
                }
            }
        }
        .accessibilityIdentifier("onglet-general")
        .task { await reload() }
        .onAppear { cacheReport = nil; Task { await reload() } }
        .onChange(of: model.snapshot.bookmarked) { _, _ in Task { await reload() } }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await reload() }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            Task { await reload() }
        }
        .alert(pendingRemoval.map(BookmarkRemoval.question(for:)) ?? "",
               isPresented: Binding(get: { pendingRemoval != nil },
                                    set: { if !$0 { pendingRemoval = nil } }),
               presenting: pendingRemoval) { b in
            Button("Annuler", role: .cancel) {}
                .keyboardShortcut(.defaultAction)
            Button("Supprimer", role: .destructive) { remove(b) }
        }
    }

    private var keepVoice: Binding<KeepVoiceChoice> {
        Binding(get: { KeepVoiceChoice(minutes: store.values.keepVoiceMinutes) },
                set: { store.set(\.keepVoiceMinutes, $0.minutes) })
    }

    private func reload() async { bookmarks = await model.loadAllBookmarks() }

    private func clearCache() {
        Task {
            do {
                let text = CacheReport.text(freed: try await model.clearDocumentCache())
                cacheReport = text
                Announcement.post(text)
                dataError.clear()
            } catch {
                cacheReport = nil
                dataError.fail(SettingsStore.message(of: error))
            }
        }
    }

    private func remove(_ b: StoredBookmarkView) {
        Task {
            do {
                try await model.removeBookmarkAsync(document: b.document, sentence: b.sentence)
                dataError.clear()
            } catch {
                dataError.fail(SettingsStore.message(of: error))
            }
            await reload()
        }
    }

    private func openConfigFolder() {
        guard let dir = model.configDirectory else { return }
        let url = URL(fileURLWithPath: dir, isDirectory: true)
        // Pas encore de config.toml : le dossier s'ouvre quand même.
        do {
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        } catch {
            dataError.fail("Le dossier de configuration est introuvable : \(error.localizedDescription)")
            return
        }
        if NSWorkspace.shared.open(url) { dataError.clear() } else {
            dataError.fail("Le dossier de configuration ne s'ouvre pas")
        }
    }
}

/// Un signet : l'extrait, la section et la date ; « Supprimer » au survol
/// comme au clavier (le bouton reste toujours là pour VoiceOver).
private struct BookmarkRow: View {
    let bookmark: StoredBookmarkView
    let remove: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "bookmark")
                .font(.system(size: 12))
                .foregroundStyle(Palette.faint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(bookmark.snippet)
                    .font(Typo.body)
                    .foregroundStyle(Palette.ink)
                    .lineLimit(2)
                Text("\(bookmark.sectionTitle) · \(bookmark.when)")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
            }
            Spacer()
            Button("Supprimer") { remove() }
                .buttonStyle(.notionPlain)
                .opacity(hovering ? 1 : 0.6)
                .accessibilityLabel("Supprimer le signet « \(bookmark.snippet) » de \(bookmark.label)")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 6).fill(hovering ? Palette.hover : .clear))
        .padding(.horizontal, -8)
        .onHover { hovering = $0 }
        .accessibilityElement(children: .contain)
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
        return "\(SizeFormat.bytes(bytes)) libérés"
    }
}

enum BookmarkRemoval {
    /// La question posée avant de supprimer un signet.
    static func question(for b: StoredBookmarkView) -> String {
        let snippet = b.snippet.count > 60 ? String(b.snippet.prefix(60)) + "…" : b.snippet
        return "Supprimer le signet « \(snippet) » de \(b.label) ?"
    }
}
