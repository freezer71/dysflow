import SwiftUI

/// Le panneau agrandi : sommaire, fil d'Ariane, recherche, texte pleine
/// largeur, ligne de temps à repères.
struct ExpandedReader: View {
    let controller: PanelController
    let style: ReadingStyle
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        let model = controller.model
        HStack(alignment: .top, spacing: 18) {
            if controller.sidebarVisible {
                Sidebar(controller: controller, theme: style.theme)
                Divider()
            }
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text(SectionMarkers.breadcrumb(model.document, section: model.snapshot.section))
                        .lineLimit(1)
                        .accessibilityIdentifier("fil-ariane")
                    Spacer()
                    if controller.searchActive {
                        TextField("Chercher", text: $query)
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 220)
                            .focused($searchFocused)
                            .accessibilityIdentifier("recherche")
                            .onSubmit {
                                model.send(.search(query: query))
                                searchFocused = false
                            }
                            // Le champ n'accepte le focus qu'une fois posé dans la fenêtre :
                            // au tour suivant.
                            .onAppear { DispatchQueue.main.async { searchFocused = true } }
                    }
                    if let search = model.snapshot.search {
                        Text(search.count == 0 ? "Aucun résultat"
                             : "\(search.position + 1) / \(search.count)")
                            .accessibilityIdentifier("resultats-recherche")
                    }
                    if let message = model.message {
                        Text(message).lineLimit(1)
                    }
                }
                .font(.system(size: 13))
                .foregroundStyle(Color(nsColor: style.theme.chrome))

                ReadingView(model: model, style: style)
                PlayerBar(model: model, theme: style.theme,
                          markers: SectionMarkers.positions(model.document))
            }
        }
    }
}
