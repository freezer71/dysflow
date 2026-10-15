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
            .environment(\.openSettingsTab, controller.openSettingsAction)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(nsColor: theme.background))
                    .overlay(RoundedRectangle(cornerRadius: 12)
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
                .padding(.vertical, 18)
        case .hidden:
            // Fenêtre retirée : rien à tenir à jour (une vue de lecture gardée
            // là serait remise à jour à chaque instantané publié).
            Color.clear
        }
    }
}

/// Les raccourcis, en surimpression ; n'importe quelle touche la ferme.
struct HelpOverlay: View {
    let theme: Theme
    static let rows: [(String, String)] = [
        ("Espace", "Lecture / pause"),
        ("← →", "Phrase précédente / suivante"),
        ("↑ ↓", "Ligne précédente / suivante"),
        ("Début", "Revenir au début"),
        ("n p", "Titre suivant / précédent"),
        ("/ ⌘F", "Rechercher"),
        ("N P", "Résultat suivant / précédent"),
        ("t", "Sommaire"),
        ("e", "Modifier le texte"),
        ("+ −", "Vitesse"),
        ("⌘C", "Copier la sélection, sinon la phrase"),
        ("⌘V", "Lire le presse-papiers"),
        ("⌘O", "Ouvrir un fichier"),
        ("⌘M", "Réduire / agrandir"),
        ("⌘W", "Fermer (la lecture s'arrête)"),
        ("Échap", "Fermer la recherche ou le sommaire, sinon fermer"),
        ("?", "Cette aide"),
        ("⌘,", "Réglages"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Raccourcis")
                .font(.system(size: 14, weight: .semibold))
                .accessibilityAddTraits(.isHeader)
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
            Text("Une touche pour fermer")
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: theme.chrome))
        }
        .padding(20)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: theme.background)))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Color(nsColor: theme.border)))
        .shadow(color: .black.opacity(0.12), radius: 18, y: 8)
        .foregroundStyle(Color(nsColor: theme.text))
        .accessibilityIdentifier("aide-raccourcis")
    }
}

/// La ligne de message du panneau : ce que l'app vient de dire (« Phrase
/// copiée »), sinon le statut du lecteur (« Signet ajouté », « Aucun mot de
/// ce côté », sortie audio indisponible), sinon, voix de repli, comment en
/// installer une ; voix muette (aucune session ne s'ouvre), pourquoi ;
/// sinon la première préparation des noyaux Metal, ou le
/// chargement de la voix.
enum PanelStatus {
    static let fallbackHint = "Voix du système — installer une voix : ⌘,"
    static let preparingLine = "Voix en préparation (première fois)…"
    static let loadingLine = "Voix en chargement…"

    static func line(message: String?, snapshot: Snapshot, preparing: Bool = false) -> String? {
        if let message { return message }
        if !snapshot.status.isEmpty { return snapshot.status }
        if case .fallback = snapshot.voice { return fallbackHint }
        if case .unavailable(let reason) = snapshot.voice { return "Voix indisponible : \(reason)" }
        if preparing { return preparingLine }
        if case .loading = snapshot.voice { return loadingLine }
        return nil
    }
}

/// L'invitation du lecteur vide, et un raccourci tel qu'il s'écrit.
enum PanelInvitation {
    static let text = "Collez un texte (⌘V) ou déposez un fichier"

    /// Le raccourci tel qu'il s'écrit (« ⌃⌥Espace »). La bibliothèque nomme
    /// la barre d'espace dans la langue de l'app, qui peut retomber sur
    /// l'anglais.
    @MainActor
    static func keys(_ shortcut: KeyboardShortcuts.Shortcut) -> String {
        var keys = shortcut.description
        if shortcut.key == .space, keys.hasSuffix("Space") {
            keys = String(keys.dropLast("Space".count)) + "Espace"
        }
        return keys
    }
}
