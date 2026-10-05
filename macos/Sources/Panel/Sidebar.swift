import SwiftUI

/// Sommaire et signets. Tab pour y entrer, flèches pour parcourir, Entrée
/// pour y aller.
struct Sidebar: View {
    let controller: PanelController
    let theme: Theme
    @State private var selection: UInt32?
    /// Les signets passent par le fil du lecteur (jusqu'à 5 s) : relus hors
    /// du fil principal, à l'ouverture de la liste et quand un signet change,
    /// jamais à chaque évaluation du corps.
    @State private var bookmarks: [BookmarkView] = []
    /// Vrai quand la liste a le focus clavier : on la parcourt.
    @FocusState private var listFocused: Bool

    private struct Item: Identifiable {
        let id: UInt32
        let title: String
        let indent: Int
        let sentence: UInt32?
    }

    var body: some View {
        let model = controller.model
        let showingBookmarks = controller.bookmarksVisible
        let items: [Item] = showingBookmarks
            ? bookmarks.map { Item(id: $0.sentence, title: "« \($0.snippet) » · \($0.when)",
                                   indent: 0, sentence: $0.sentence) }
            : model.tableOfContents.map {
                Item(id: $0.id, title: $0.title, indent: max(Int($0.level) - 1, 0),
                     sentence: $0.firstSentence)
            }
        let go: (Set<UInt32>) -> Void = { ids in
            guard let id = ids.first,
                  let s = items.first(where: { $0.id == id })?.sentence else { return }
            model.send(.seekTo(sentence: s))
        }

        VStack(alignment: .leading, spacing: 6) {
            Text(showingBookmarks ? "Signets" : "Sommaire")
                .font(.system(size: 13, weight: .semibold))
            List(items, selection: $selection) { item in
                Text(item.title)
                    .font(.system(size: 13))
                    .padding(.leading, CGFloat(item.indent) * 12)
                    .lineLimit(1)
                    .tag(item.id)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .focused($listFocused)
            .accessibilityIdentifier("sommaire")
            // Entrée (ou double clic) sur la ligne choisie : on y va.
            .contextMenu(forSelectionType: UInt32.self, menu: { _ in }, primaryAction: go)
            .overlay {
                if items.isEmpty {
                    Text(showingBookmarks ? "Aucun signet" : "Aucun titre")
                        .font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: theme.chrome))
                }
            }
        }
        .foregroundStyle(Color(nsColor: theme.text))
        .frame(width: 220)
        .onAppear { selection = showingBookmarks ? nil : model.snapshot.section }
        .onChange(of: showingBookmarks) { _, now in
            selection = now ? nil : model.snapshot.section
        }
        // La ligne choisie suit la section lue, sauf pendant qu'on parcourt
        // la liste au clavier : ↓ part alors de la ligne où l'on est. En
        // quittant la liste, elle se recale sur la section lue.
        .onChange(of: model.snapshot.section) { _, section in
            if !showingBookmarks && !listFocused { selection = section }
        }
        .onChange(of: listFocused) { _, focused in
            if !showingBookmarks && !focused { selection = model.snapshot.section }
        }
        // Rechargés à l'ouverture des signets, et quand le signet de la
        // phrase en cours change (« b »).
        .task(id: BookmarkKey(visible: showingBookmarks, bookmarked: model.snapshot.bookmarked,
                              revision: model.snapshot.revision)) {
            guard showingBookmarks else { return }
            bookmarks = await model.loadBookmarks()
        }
    }

    private struct BookmarkKey: Equatable {
        let visible: Bool
        let bookmarked: Bool
        let revision: UInt64
    }
}
