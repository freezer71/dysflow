import SwiftUI

/// Le lecteur : l'en-tête, le texte (le sommaire à côté s'il est ouvert),
/// la barre, et une ligne d'état à la hauteur toujours réservée.
struct ReaderView: View {
    let controller: PanelController
    let style: ReadingStyle

    var body: some View {
        let model = controller.model
        VStack(alignment: .leading, spacing: 12) {
            ReaderHeader(controller: controller, theme: style.theme)
            // Le filet sous l'en-tête : les commandes d'un côté, le texte de l'autre.
            Rectangle().fill(Color(nsColor: style.theme.border)).frame(height: 1)
                .accessibilityHidden(true)
            HStack(alignment: .top, spacing: 20) {
                if controller.sidebarVisible {
                    Sidebar(controller: controller, theme: style.theme)
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
            VStack(alignment: .leading, spacing: 6) {
                if !controller.editing {
                    ReaderBar(controller: controller, theme: style.theme, markers: model.sectionPositions)
                }
                ReaderStatusLine(line: PanelStatus.line(message: model.message, snapshot: model.snapshot),
                                 color: Color(nsColor: style.theme.chrome))
                    .padding(.horizontal, 4)
                ModelLine(snapshot: model.snapshot, color: Color(nsColor: style.theme.chrome))
                    .padding(.horizontal, 4)
                VoiceStatusLine(model: model, color: Color(nsColor: style.theme.chrome))
                    .padding(.horizontal, 4)
            }
        }
    }
}

/// Les messages passagers (« Phrase copiée », « Texte modifié ») et l'état
/// du lecteur. La hauteur d'une ligne est toujours réservée : un message qui
/// vient ou part ne fait bouger ni le texte ni la barre. Un message trop long
/// est coupé ; il se lit en entier au survol et pour VoiceOver.
struct ReaderStatusLine: View {
    static let height: CGFloat = 16
    let line: String?
    let color: Color

    var body: some View {
        Group {
            if let line {
                Text(line)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .help(line)
                    .accessibilityLabel(line)
                    .accessibilityIdentifier("etat-lecteur")
            } else {
                Color.clear.accessibilityHidden(true)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(color)
        .frame(maxWidth: .infinity, minHeight: Self.height, maxHeight: Self.height, alignment: .leading)
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

/// Le titre courant, la recherche quand elle est ouverte, les quatre
/// boutons (Coller, Sommaire, Rechercher, Modifier), les réglages et la
/// fermeture.
struct ReaderHeader: View {
    let controller: PanelController
    let theme: Theme
    @State private var query = ""
    @FocusState private var searchFocused: Bool
    /// La largeur de l'en-tête, relue quand elle change.
    @State private var width: CGFloat = 0

    /// Sous cette largeur, les boutons perdent leurs intitulés (le champ de
    /// recherche ouvert en demande davantage).
    static func compact(width: CGFloat, searching: Bool) -> Bool {
        width > 0 && width < 660 + (searching ? 210 : 0)
    }

    var body: some View {
        let model = controller.model
        let title = model.breadcrumb.isEmpty ? "sillage" : model.breadcrumb
        HStack(spacing: 4) {
            LogoMark(small: true)
                .frame(width: 16, height: 16)
                .foregroundStyle(Color(nsColor: theme.past))
                .padding(.trailing, 4)
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color(nsColor: theme.text))
                .lineLimit(1)
                .truncationMode(.middle)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("fil-ariane")
                // Le titre se coupe avant que les boutons ne perdent leurs intitulés.
                .layoutPriority(-1)
            Spacer(minLength: 8)
            if controller.searchActive {
                TextField("Search", text: $query)
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
                Text(search.count == 0 ? "No results" : "\(search.position + 1) / \(search.count)")
                    .monospacedDigit()
                    .accessibilityIdentifier("resultats-recherche")
            }
            // Les intitulés quand ils tiennent, les icônes seules sinon. Pas de
            // ViewThatFits : il mesure ses deux variantes à chaque mise à jour,
            // soit à chaque image pendant la lecture (~40 % d'un cœur).
            headerButtons(compact: Self.compact(width: width, searching: controller.searchActive))
            BarIconButton(symbol: "gearshape", label: "Settings", help: "Settings (⌘,)",
                          identifier: "bouton-reglages") {
                controller.perform(.openSettings)
            }
            BarIconButton(symbol: "minus", label: "Minimize to Dock", help: "Minimize to the Dock, reading continues",
                          identifier: "reduire-dock") {
                controller.perform(.minimize)
            }
            BarIconButton(symbol: "xmark", label: "Close", help: "Close, reading stops (⌘W)",
                          identifier: "fermer") {
                controller.perform(.close)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .background(GeometryReader { geo in
            Color.clear
                .onAppear { width = geo.size.width }
                .onChange(of: geo.size.width) { _, w in width = w }
        })
        // Recherche fermée (Échap) : le champ rouvert repart vide.
        .onChange(of: controller.searchActive) { _, active in
            if !active { query = "" }
        }
    }

    private func headerButtons(compact: Bool) -> some View {
        HStack(spacing: 6) {
            HeaderButton(title: "Paste", symbol: "doc.on.clipboard", help: "Read the clipboard (⌘V)",
                         identifier: "bouton-coller", compact: compact) {
                controller.perform(.paste)
            }
            HeaderButton(title: "Contents", symbol: "list.bullet", help: "Contents (t)",
                         identifier: "bouton-sommaire", selected: controller.sidebarVisible, compact: compact) {
                controller.perform(.toggleSidebar)
            }
            HeaderButton(title: "Search", symbol: "magnifyingglass", help: "Search (/ or ⌘F)",
                         identifier: "bouton-rechercher", selected: controller.searchActive, compact: compact) {
                controller.perform(.openSearch)
            }
            HeaderButton(title: "Edit", symbol: "pencil", help: "Edit the text (e)",
                         identifier: "bouton-modifier", selected: controller.editing, compact: compact) {
                controller.perform(.edit)
            }
            .disabled(!controller.model.snapshot.hasDocument || controller.editing)
        }
    }
}

/// ⏮ ▶ ⏭, la progression cliquable, le temps, la vitesse, le volume, la sortie audio,
/// « Réduire ».
struct ReaderBar: View {
    let controller: PanelController
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let model = controller.model
        let s = model.snapshot
        HStack(spacing: 10) {
            HStack(spacing: 0) {
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
            VolumeButton(model: model)
            OutputMenu(model: model)
            BarIconButton(symbol: "arrow.down.right.and.arrow.up.left", label: "Collapse",
                          help: "Collapse to the mini player (⌘M)", identifier: "reduire") {
                controller.perform(.toggleSize)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        // La barre posée sur la page, en pilule.
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .background(PanelSurface(theme: theme, radius: 22))
    }
}

/// Le fond des zones posées sur la page (sommaire, barre) : la surface du
/// thème, pleine, sans filet ; un filet en contraste élevé seulement.
struct PanelSurface: View {
    let theme: Theme
    var radius: CGFloat = 16

    var body: some View {
        RoundedRectangle(cornerRadius: radius)
            .fill(Color(nsColor: theme.surface))
            .overlay {
                if theme == .contrast {
                    RoundedRectangle(cornerRadius: radius)
                        .strokeBorder(Color(nsColor: theme.border), lineWidth: 1)
                }
            }
            .accessibilityHidden(true)
    }
}
