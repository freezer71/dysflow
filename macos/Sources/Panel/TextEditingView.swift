import SwiftUI

/// Le mode édition du panneau : le texte lu, modifiable, à la police et à
/// la taille de lecture. Les « # » des titres restent visibles : c'est le
/// texte tel qu'il a été ouvert. Échap ou ⌘↩ valide.
struct TextEditingView: View {
    @Bindable var controller: PanelController
    let style: ReadingStyle
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextEditor(text: $controller.draft)
                .font(Font(style.font.font(size: style.size)))
                .lineSpacing(style.size * (0.35 + 0.25 * CGFloat(style.lineSpacing)))
                .foregroundStyle(Color(nsColor: style.theme.text))
                .scrollContentBackground(.hidden)
                .scrollIndicators(.automatic)
                .focused($focused)
                .accessibilityLabel("Texte à modifier")
                .accessibilityIdentifier("edition-texte")
                // Le champ n'accepte le focus qu'une fois posé dans la fenêtre.
                .onAppear { DispatchQueue.main.async { focused = true } }
            EditingBar(controller: controller, theme: style.theme)
        }
    }
}

/// Sous le texte modifié : ce qu'on fait, et comment en sortir.
struct EditingBar: View {
    let controller: PanelController
    let theme: Theme

    var body: some View {
        HStack(spacing: 10) {
            Label("Modification du texte", systemImage: "pencil")
                .labelStyle(.titleAndIcon)
                .foregroundStyle(Color(nsColor: theme.chrome))
            Spacer(minLength: 8)
            HStack(spacing: 6) {
                Keycap(key: "esc", color: Color(nsColor: theme.chrome),
                       fill: Color(nsColor: theme.text).opacity(0.07))
                Text("valider").foregroundStyle(Color(nsColor: theme.chrome))
            }
            .accessibilityElement(children: .combine)
            Button("Annuler") { controller.cancelEditing() }
                .buttonStyle(.notionPlain)
                .accessibilityIdentifier("edition-annuler")
            Button("Terminé") { controller.finishEditing() }
                .buttonStyle(.notionPrimary)
                .accessibilityIdentifier("edition-terminer")
        }
        .font(.system(size: 13))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("barre-edition")
    }
}
