import KeyboardShortcuts
import SwiftUI

struct PanelContent: View {
    let controller: PanelController
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize
    @Environment(\.colorScheme) private var colorScheme
    /// Suit « Augmenter le contraste » en direct, sans relancer l'app.
    @State private var increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast

    private var theme: Theme {
        Theme.resolve(dark: colorScheme == .dark,
                      increaseContrast: increaseContrast)
    }

    private var style: ReadingStyle {
        ReadingStyle(font: ReadingFont(rawValue: fontName) ?? .literata,
                     size: CGFloat(size),
                     lineSpacing: controller.reading.lineSpacing,
                     ruler: controller.reading.readingRuler,
                     theme: theme)
    }

    var body: some View {
        content(controller.model)
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .fill(Color(nsColor: theme.background))
                    .overlay(RoundedRectangle(cornerRadius: 14)
                        .stroke(Color(nsColor: theme.border), lineWidth: 1))
            )
            .overlay { if controller.helpVisible { HelpOverlay(theme: theme) } }
            // La barre de titre est transparente et invisible : sans cela, sa
            // zone sûre repousse le contenu de ~30 pt et rogne le texte.
            .ignoresSafeArea()
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
        case .small:
            MiniPlayerView(model: model, theme: theme, perform: controller.perform)
                // La vue hôte agrandit la fenêtre jusqu'au minimum du contenu,
                // plus la zone sûre de la barre de titre (32 pt) : 16 + 32
                // ferait 48, le haut fixe. La fenêtre fixe la taille
                // (SmallPlacement.size) ; le contenu, centré, n'impose rien.
                .frame(minHeight: 0, maxHeight: .infinity)
                // ↑ ↓ suivent le texte même invisible : une copie cachée de
                // la vue de lecture, à la largeur du mode moyen. Un fond ne
                // compte pas dans la taille de la barre.
                .background(alignment: .topLeading) {
                    ReadingView(model: model, style: style, lines: controller.lines, shadow: true)
                        .frame(width: controller.shadowTextWidth, height: 40)
                }
        case .large:
            ExpandedReader(controller: controller, style: style)
                .padding(.horizontal, PanelGeometry.horizontalPadding)
                .padding(.vertical, 18)
        case .background:
            // Fenêtre retirée : rien à tenir à jour. Une vue de lecture gardée
            // là serait remise à jour à chaque instantané publié (mesuré :
            // 30 fois en 10 s de lecture), et la vue hôte agrandirait la
            // fenêtre cachée à la taille du moyen.
            Color.clear
        case .medium:
            VStack(alignment: .leading, spacing: 14) {
                ReadingView(model: model, style: style, lines: controller.lines) { height in
                    controller.contentHeightChanged(height)
                }
                // Sans document, l'invitation occupe la zone du texte, au
                // centre, à la taille des contrôles.
                .overlay {
                    if !model.snapshot.hasDocument {
                        Text(PanelInvitation.text(for: KeyboardShortcuts.getShortcut(for: .readSelection)))
                            .font(.system(size: 13))
                            .multilineTextAlignment(.center)
                            .foregroundStyle(Color(nsColor: theme.chrome))
                            .accessibilityIdentifier("invitation")
                    }
                }
                PlayerBar(model: model, theme: theme)
                HStack(spacing: 14) {
                    hint("⏎", "agrandir")
                    hint("esc", "masquer")
                    hint("?", "raccourcis")
                    // Le message prend place dans la rangée des aides : la
                    // hauteur réservée autour du texte (PanelGeometry.chrome)
                    // ne change pas quand il paraît.
                    if let message = PanelStatus.line(message: model.message, snapshot: model.snapshot,
                                                         preparing: model.voicePreparation.isRunning) {
                        Spacer(minLength: 8)
                        Text(message)
                            .font(.system(size: 13))
                            .lineLimit(1)
                            .foregroundStyle(Color(nsColor: theme.chrome))
                    }
                }
            }
            .padding(.horizontal, PanelGeometry.horizontalPadding)
            .padding(.vertical, 18)
        }
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key)
                .font(.system(size: 13, design: .monospaced))
                .padding(.horizontal, 5).padding(.vertical, 1)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color(nsColor: theme.border)))
            Text(label)
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .accessibilityElement(children: .combine)
    }
}

/// Les raccourcis, en surimpression ; n'importe quelle touche la ferme.
struct HelpOverlay: View {
    let theme: Theme
    static let rows: [(String, String)] = [
        ("Espace", "Lecture / pause"), ("⌥← ⌥→", "Mot précédent / suivant"),
        ("← →", "Phrase précédente / suivante"), ("⇧← ⇧→", "Recul / avance rapide"),
        ("↑ ↓", "Ligne précédente / suivante"), ("⇧↑ ⇧↓", "Bloc précédent / suivant"),
        ("n p", "Titre suivant / précédent"),
        ("+ −", "Vitesse"), ("1…4 · & é \" '", "Vitesses 0,75× 1× 1,5× 2×"),
        ("⌥↑ ⌥↓", "Volume"), ("r", "Reprendre la phrase"),
        ("/ · N P", "Chercher · résultat suivant / précédent"),
        ("t · B · b", "Sommaire · signets · poser un signet"),
        ("⏎", "Agrandir"), ("⌘1 ⌘2 ⌘3", "Petit · moyen · grand"),
        ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),
        ("v", "Réglages de la voix"),
        ("Échap", "Revenir · arrière-plan"),
    ]

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 6) {
            ForEach(Self.rows, id: \.0) { row in
                GridRow {
                    Text(row.0).font(.system(size: 13, design: .monospaced))
                    Text(row.1).font(.system(size: 13))
                }
            }
        }
        .padding(20)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: theme.background)))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color(nsColor: theme.border)))
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

/// L'invitation du panneau vide, avec le raccourci « Lire » réellement choisi.
enum PanelInvitation {
    @MainActor
    static func text(for shortcut: KeyboardShortcuts.Shortcut?) -> String {
        let start = "Sélectionnez ou copiez du texte, puis "
        guard let shortcut else { return start + "choisissez un raccourci : ⌘," }
        return start + keys(shortcut)
    }

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
