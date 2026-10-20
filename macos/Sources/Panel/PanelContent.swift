import KeyboardShortcuts
import SwiftUI

struct PanelContent: View {
    let controller: PanelController
    @Environment(\.colorScheme) private var colorScheme
    /// Suit « Augmenter le contraste » en direct, sans relancer l'app.
    @State private var increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast

    private var theme: Theme {
        Theme.resolve(dark: colorScheme == .dark,
                      increaseContrast: increaseContrast)
    }

    private var style: ReadingStyle {
        ReadingStyle(font: controller.reading.font,
                     size: controller.reading.size,
                     lineSpacing: controller.reading.lineSpacing,
                     ruler: controller.reading.readingRuler,
                     theme: theme)
    }

    var body: some View {
        content(controller.model)
            .environment(\.panelTheme, theme)
            .background(
                RoundedRectangle(cornerRadius: 20)
                    .fill(Color(nsColor: theme.background))
                    .overlay(RoundedRectangle(cornerRadius: 20)
                        .strokeBorder(Color(nsColor: theme.border), lineWidth: 1))
            )
            .overlay { if controller.helpVisible { HelpOverlay(theme: theme) } }
            // La barre de titre est transparente et invisible : sans cela, sa
            // zone sûre repousse le contenu de ~30 pt et rogne le texte.
            .ignoresSafeArea()
            // Un fichier (Markdown, texte, PDF, image) déposé sur le lecteur ou le mini.
            .dropDestination(for: URL.self) { urls, _ in controller.openDropped(urls) }
            .onChange(of: controller.model.snapshot) { _, s in controller.announce(s) }
            // Un réglage écrit (interlignage, règle, pas d'avance) s'applique
            // au panneau ouvert, sans attendre sa prochaine ouverture.
            .onChange(of: controller.model.settingsRevision) { _, _ in controller.reloadReading() }
            .onReceive(NSWorkspace.shared.notificationCenter.publisher(
                for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
                increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            }
    }

    @ViewBuilder
    private func content(_ model: ReaderModel) -> some View {
        switch controller.mode {
        case .mini:
            MiniPlayerView(model: model, theme: theme, perform: controller.perform)
                // La fenêtre fixe la taille (MiniPlacement.size) ; le contenu,
                // centré, n'impose rien.
                .frame(minHeight: 0, maxHeight: .infinity)
                // ↑ ↓ suivent le texte même invisible : une copie cachée de
                // la vue de lecture, à la largeur du lecteur. Un fond ne
                // compte pas dans la taille du mini.
                .background(alignment: .topLeading) {
                    ReadingView(model: model, style: style, lines: controller.lines, shadow: true)
                        .frame(width: controller.shadowTextWidth, height: 40)
                }
        case .reader:
            ReaderView(controller: controller, style: style)
                .padding(.horizontal, PanelGeometry.horizontalPadding)
                .padding(.top, 18)
                // La ligne d'état réserve déjà sa hauteur sous la barre.
                .padding(.bottom, 10)
        case .hidden:
            // Fenêtre retirée : rien à tenir à jour (une vue de lecture gardée
            // là serait remise à jour à chaque instantané publié).
            Color.clear
        }
    }
}

/// Les raccourcis, en surimpression ; « ? » ou Échap la ferment (les autres
/// touches agissent dessous). Trop haute pour le lecteur, la liste défile
/// entre le titre et le pied, qui restent visibles.
struct HelpOverlay: View {
    let theme: Theme
    static let rows: [(String, String)] = [
        (String(localized: "Space", comment: "Key name"), String(localized: "Play / pause")),
        ("← →", String(localized: "Previous / next sentence")),
        ("↑ ↓", String(localized: "Previous / next line")),
        (String(localized: "Home", comment: "Key name"), String(localized: "Back to the start")),
        ("n p", String(localized: "Next / previous heading")),
        ("/ ⌘F", String(localized: "Search")),
        ("N P", String(localized: "Next / previous result")),
        ("t", String(localized: "Contents")),
        ("e", String(localized: "Edit the text")),
        ("+ −", String(localized: "Speed")),
        ("⌘C", String(localized: "Copy the selection, or else the sentence")),
        ("⌘V", String(localized: "Read the clipboard")),
        ("⌘O", String(localized: "Open a file")),
        ("⌘M", String(localized: "Collapse / expand")),
        ("⌘W", String(localized: "Close (reading stops)")),
        (String(localized: "Esc", comment: "Key name"), String(localized: "Close search or contents, otherwise close")),
        ("?", String(localized: "This help")),
        ("⌘,", String(localized: "Settings")),
    ]

    /// Ce qui ferme vraiment l'aide.
    static let footer = String(localized: "? or Esc to close")

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Shortcuts")
                .font(.system(size: 14, weight: .semibold))
                .accessibilityAddTraits(.isHeader)
            // Entière si elle tient, sinon elle défile dans la place laissée.
            ViewThatFits(in: .vertical) {
                keys
                ScrollView(.vertical) { keys }
                    .scrollIndicators(.automatic)
                    // La largeur des touches, pas celle du lecteur.
                    .fixedSize(horizontal: true, vertical: false)
            }
            Text(Self.footer)
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: theme.chrome))
        }
        .padding(20)
        .background(RoundedRectangle(cornerRadius: 16).fill(Color(nsColor: theme.background)))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Color(nsColor: theme.border)))
        .shadow(color: .black.opacity(0.12), radius: 18, y: 8)
        .foregroundStyle(Color(nsColor: theme.text))
        .padding(16)
        .accessibilityIdentifier("aide-raccourcis")
    }

    private var keys: some View {
        Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 7) {
            ForEach(Self.rows, id: \.0) { row in
                GridRow {
                    Keycap(key: row.0, color: Color(nsColor: theme.text),
                           fill: Color(nsColor: theme.text).opacity(0.07))
                    Text(row.1).font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: theme.chrome))
                }
            }
        }
    }
}

/// La ligne de message du lecteur : ce que l'app vient de dire (« Phrase
/// copiée »), sinon ce que dit le lecteur (« Sortie audio débranchée ») ;
/// la voix a sa ligne à elle (`VoiceStatusLine`).
enum PanelStatus {
    static func line(message: String?, snapshot: Snapshot) -> String? {
        if let message { return message }
        if !snapshot.status.isEmpty { return snapshot.status }
        return nil
    }
}

/// L'invitation du lecteur vide, et un raccourci tel qu'il s'écrit.
enum PanelInvitation {
    static let text = String(localized: "Paste some text (⌘V) or drop in a file")

    /// Le raccourci tel qu'il s'écrit (« ⌃⌥Espace »). La bibliothèque nomme
    /// la barre d'espace dans la langue de l'app, qui peut retomber sur
    /// l'anglais.
    @MainActor
    static func keys(_ shortcut: KeyboardShortcuts.Shortcut) -> String {
        var keys = shortcut.description
        if shortcut.key == .space, keys.hasSuffix("Space") {
            keys = String(keys.dropLast("Space".count)) + String(localized: "Space", comment: "Key name")
        }
        return keys
    }
}
