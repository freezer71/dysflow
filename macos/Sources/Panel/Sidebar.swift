import SwiftUI

/// Le sommaire. Tab pour y entrer, flèches pour parcourir, Entrée pour y aller.
struct Sidebar: View {
    let controller: PanelController
    let theme: Theme
    @State private var selection: UInt32?
    /// Vrai quand la liste a le focus clavier : on la parcourt.
    @FocusState private var listFocused: Bool

    var body: some View {
        let model = controller.model
        let items = model.tableOfContents
        VStack(alignment: .leading, spacing: 4) {
            Text("Sommaire")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: theme.chrome))
                .padding(.leading, 16)
                .padding(.top, 12)
                .accessibilityAddTraits(.isHeader)
            List(items, id: \.id, selection: $selection) { item in
                Text(item.title)
                    .font(.system(size: 13))
                    .padding(.leading, CGFloat(max(Int(item.level) - 1, 0)) * 12)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .pointingHandCursor()
                    .tag(item.id)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .focused($listFocused)
            .accessibilityIdentifier("sommaire")
            // Entrée (ou double clic) sur la ligne choisie : on y va.
            .contextMenu(forSelectionType: UInt32.self, menu: { _ in }, primaryAction: { ids in
                guard let id = ids.first, let item = items.first(where: { $0.id == id }),
                      let sentence = item.firstSentence else { return }
                model.send(.seekTo(sentence: sentence))
            })
            .overlay {
                if items.isEmpty {
                    Text("Aucun titre")
                        .font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: theme.chrome))
                }
            }
        }
        .foregroundStyle(Color(nsColor: theme.text))
        .frame(width: 220)
        .background(PanelSurface(theme: theme))
        .onAppear { selection = model.snapshot.section }
        // La ligne choisie suit la section lue, sauf pendant qu'on parcourt
        // la liste au clavier ; en la quittant, elle se recale.
        .onChange(of: model.snapshot.section) { _, section in
            if !listFocused { selection = section }
        }
        .onChange(of: listFocused) { _, focused in
            if !focused { selection = model.snapshot.section }
        }
    }
}
