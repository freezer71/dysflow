import SwiftUI

/// Le lecteur : l'en-tête, le texte (le sommaire à côté s'il est ouvert),
/// la barre, et une ligne d'état seulement quand il y a quelque chose à dire.
struct ReaderView: View {
    let controller: PanelController
    let style: ReadingStyle

    var body: some View {
        let model = controller.model
        VStack(alignment: .leading, spacing: 12) {
            ReaderHeader(controller: controller, theme: style.theme)
            HStack(alignment: .top, spacing: 18) {
                if controller.sidebarVisible {
                    Sidebar(controller: controller, theme: style.theme)
                    Divider()
                }
                if controller.editing {
                    TextEditingView(controller: controller, style: style)
                } else {
                    ReadingView(model: model, style: style, lines: controller.lines,
                                onCopy: { controller.copy() },
                                onReadFrom: { controller.readFrom(location: $0) })
                        .overlay {
                            if !model.snapshot.hasDocument { ReaderInvitation(theme: style.theme) }
                        }
                }
            }
            if !controller.editing {
                ReaderBar(controller: controller, theme: style.theme, markers: model.sectionPositions)
            }
            if let line = PanelStatus.line(message: model.message, snapshot: model.snapshot,
                                           preparing: model.voicePreparation.isRunning) {
                Text(line)
                    .font(.system(size: 13))
                    .lineLimit(2)
                    .foregroundStyle(Color(nsColor: style.theme.chrome))
                    .accessibilityIdentifier("etat-lecteur")
            }
        }
    }
}

/// Sans document : l'invitation, au centre de la zone du texte.
struct ReaderInvitation: View {
    let theme: Theme

    var body: some View {
        VStack(spacing: 10) {
            LogoMark()
                .frame(width: 28, height: 28)
                .foregroundStyle(Color(nsColor: theme.past))
            Text(PanelInvitation.text)
                .font(.system(size: 13))
                .multilineTextAlignment(.center)
                .foregroundStyle(Color(nsColor: theme.chrome))
                .accessibilityIdentifier("invitation")
        }
    }
}

/// Le titre courant, la recherche quand elle est ouverte, les trois
/// boutons et la fermeture.
struct ReaderHeader: View {
    let controller: PanelController
    let theme: Theme
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        let model = controller.model
        let title = model.breadcrumb.isEmpty ? "sillon" : model.breadcrumb
        HStack(spacing: 6) {
            Text(title)
                .lineLimit(1)
                .truncationMode(.middle)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("fil-ariane")
            Spacer(minLength: 8)
            if controller.searchActive {
                TextField("Rechercher", text: $query)
                    .textFieldStyle(.notion)
                    .frame(width: 200)
                    .focused($searchFocused)
                    .accessibilityIdentifier("recherche")
                    .onSubmit {
                        model.send(.search(query: query))
                        searchFocused = false
                    }
                    // Le champ n'accepte le focus qu'une fois posé dans la fenêtre.
                    .onAppear { DispatchQueue.main.async { searchFocused = true } }
                    // « / » de nouveau, champ déjà ouvert : le focus y revient.
                    .onChange(of: controller.openSearchRequests) { searchFocused = true }
            }
            if let search = model.snapshot.search {
                Text(search.count == 0 ? "Aucun résultat" : "\(search.position + 1) / \(search.count)")
                    .monospacedDigit()
                    .accessibilityIdentifier("resultats-recherche")
            }
            HeaderButton(title: "Sommaire", symbol: "list.bullet", help: "Sommaire (t)",
                         identifier: "bouton-sommaire", selected: controller.sidebarVisible) {
                controller.perform(.toggleSidebar)
            }
            HeaderButton(title: "Rechercher", symbol: "magnifyingglass", help: "Rechercher (/ ou ⌘F)",
                         identifier: "bouton-rechercher", selected: controller.searchActive) {
                controller.perform(.openSearch)
            }
            HeaderButton(title: "Modifier", symbol: "pencil", help: "Modifier le texte (e)",
                         identifier: "bouton-modifier", selected: controller.editing) {
                controller.perform(.edit)
            }
            .disabled(!model.snapshot.hasDocument || controller.editing)
            BarIconButton(symbol: "xmark", label: "Fermer", help: "Fermer, la lecture s'arrête (⌘W)",
                          identifier: "fermer") {
                controller.perform(.close)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        // Recherche fermée (Échap) : le champ rouvert repart vide.
        .onChange(of: controller.searchActive) { _, active in
            if !active { query = "" }
        }
    }
}

/// ⏮ ▶ ⏭, la progression cliquable, le temps, la vitesse, « Réduire ».
struct ReaderBar: View {
    let controller: PanelController
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let model = controller.model
        let s = model.snapshot
        HStack(spacing: 10) {
            HStack(spacing: 2) {
                TransportButton(item: SentenceTransport.previous, send: controller.perform)
                PlayButton(model: model)
                TransportButton(item: SentenceTransport.next, send: controller.perform)
            }
            .disabled(!s.hasDocument)
            SeekableTimeline(progress: Double(s.progress), markers: markers, theme: theme,
                             enabled: s.hasDocument) { fraction in
                guard let sentence = ProgressSeek.sentence(fraction: fraction,
                                                           sentenceCount: model.document.sentenceCount) else { return }
                controller.perform(.command(.seekTo(sentence: sentence)))
            }
            Text(PlayerBar.time(s.elapsed) + " / ~" + PlayerBar.time(s.total))
                .monospacedDigit()
                // Graisse moyenne : les traits fins de « –:–– » restent
                // assez contrastés à 13 pt.
                .fontWeight(.medium)
                .fixedSize()
            SpeedMenu(speed: s.speed) { controller.perform(.command(.setSpeed(speed: $0))) }
            BarIconButton(symbol: "arrow.down.right.and.arrow.up.left", label: "Réduire",
                          help: "Réduire en mini (⌘M)", identifier: "reduire") {
                controller.perform(.toggleSize)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }
}
