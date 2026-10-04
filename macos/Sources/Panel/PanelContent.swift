import SwiftUI

struct PanelContent: View {
    let controller: PanelController
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize
    @Environment(\.colorScheme) private var colorScheme

    private var theme: Theme {
        Theme.resolve(dark: colorScheme == .dark,
                      increaseContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast)
    }

    private var style: ReadingStyle {
        ReadingStyle(font: ReadingFont(rawValue: fontName) ?? .literata,
                     size: CGFloat(size),
                     lineSpacing: controller.reading.lineSpacing,
                     ruler: controller.reading.readingRuler,
                     theme: theme)
    }

    var body: some View {
        let model = controller.model
        VStack(alignment: .leading, spacing: 14) {
            if controller.mode == .expanded {
                ExpandedReader(controller: controller, style: style)
            } else {
                ReadingView(model: model, style: style) { height in
                    controller.contentHeightChanged(height)
                }
                PlayerBar(model: model, theme: theme)
                HStack(spacing: 14) {
                    hint("⏎", "agrandir")
                    hint("esc", "masquer")
                    hint("?", "raccourcis")
                    // Le message prend place dans la rangée des aides : la
                    // hauteur réservée autour du texte (PanelGeometry.chrome)
                    // ne change pas quand il paraît.
                    if let message = model.message ?? emptyMessage {
                        Spacer(minLength: 8)
                        Text(message)
                            .font(.system(size: 13))
                            .lineLimit(1)
                            .foregroundStyle(Color(nsColor: theme.chrome))
                    }
                }
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 18)
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
    }

    private var emptyMessage: String? {
        controller.model.snapshot.hasDocument ? nil : "Sélectionnez ou copiez du texte, puis ⌃⌥Espace"
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key)
                .font(.system(size: 11, design: .monospaced))
                .padding(.horizontal, 5).padding(.vertical, 1)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color(nsColor: theme.border)))
            Text(label)
        }
        .font(.system(size: 11))
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
        ("↑ ↓", "Paragraphe précédent / suivant"), ("n p", "Titre suivant / précédent"),
        ("+ −", "Vitesse"), ("1…4 · & é \" '", "Vitesses 0,75× 1× 1,5× 2×"),
        ("⌥↑ ⌥↓", "Volume"), ("r", "Reprendre la phrase"),
        ("/ · N P", "Chercher · résultat suivant / précédent"),
        ("t · B · b", "Sommaire · signets · poser un signet"),
        ("⏎", "Agrandir"), ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),
        ("Échap", "Revenir · masquer"),
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
