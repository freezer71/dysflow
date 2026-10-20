import SwiftUI

/// Le temps des barres, à la seconde.
enum PlayerBar {
    nonisolated static func time(_ seconds: Double?) -> String {
        guard let seconds, seconds.isFinite else { return "–:––" }
        let t = Int(max(0, seconds).rounded())
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}

/// La ligne de temps : un trait, la part lue, un repère par section.
struct Timeline: View {
    let progress: Double
    let markers: [Double]
    let theme: Theme

    /// La part lue, bornée à 0…1 (0 si la valeur n'a pas de sens).
    nonisolated static func clamped(_ p: Double) -> Double {
        p.isFinite ? min(max(p, 0), 1) : 0
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(nsColor: theme.text).opacity(0.12)).frame(height: 6)
                // La part lue à la couleur du trait sous la ligne lue.
                Capsule().fill(Color(nsColor: theme.line))
                    .frame(width: geo.size.width * Self.clamped(progress), height: 6)
                ForEach(markers, id: \.self) { m in
                    Capsule().fill(Color(nsColor: theme.background))
                        .frame(width: 2, height: 6)
                        .offset(x: geo.size.width * m - 1)
                }
            }
            .frame(maxHeight: .infinity)
        }
    }
}

/// Lecture / pause, dans la barre du panneau comme dans la barre réduite.
struct PlayButton: View {
    let model: ReaderModel
    /// Le mode petit : un disque plein, l'ancre visuelle de la barre.
    var prominent = false

    var body: some View {
        let playing = model.snapshot.playing
        let button = Button {
            model.send(.togglePlay)
        } label: {
            Image(systemName: playing ? "pause.fill" : "play.fill")
                .font(.system(size: prominent ? BarButtonMetrics.discGlyph : BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle(disc: prominent))
        .help("Play / pause (Space)")
        .accessibilityIdentifier("lecture")
        .accessibilityLabel(playing ? "Pause" : "Play")
        // Le mode petit garde le focus aux touches de la barre.
        if prominent { button.focusable(false) } else { button }
    }
}

enum BarButtonMetrics {
    /// La cible de chaque bouton de barre : au moins 32 × 32 pt.
    static let target: CGFloat = 32
    static let glyph: CGFloat = 16
    /// Le disque de lecture du mode petit.
    static let disc: CGFloat = 36
    static let discGlyph: CGFloat = 18

    /// Le fond de survol : jamais sur un bouton désactivé.
    static func showsHover(hovering: Bool, enabled: Bool) -> Bool { hovering && enabled }
}

/// Le style de tous les boutons de barre : une cible confortable, un fond
/// arrondi léger au survol, plus marqué à l'appui. Le disque de lecture,
/// à la couleur du trait de lecture (l'accent), s'éclaircit au survol et
/// à l'appui.
struct BarButtonStyle: ButtonStyle {
    var disc = false
    /// Un bouton d'en-tête dont le volet est ouvert (sommaire, recherche) :
    /// son fond reste marqué.
    var selected = false

    func makeBody(configuration: Configuration) -> some View {
        BarButtonBody(configuration: configuration, disc: disc, selected: selected)
    }
}

private struct BarButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let disc: Bool
    let selected: Bool
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.panelTheme) private var theme

    var body: some View {
        let pressed = configuration.isPressed
        let size = disc ? BarButtonMetrics.disc : BarButtonMetrics.target
        let hover = BarButtonMetrics.showsHover(hovering: hovering, enabled: enabled)
        configuration.label
            .foregroundStyle(disc ? AnyShapeStyle(Color(nsColor: theme.onLine))
                : selected ? AnyShapeStyle(Color(nsColor: theme.text)) : AnyShapeStyle(.foreground))
            .frame(minWidth: size, minHeight: size)
            .background {
                if disc {
                    Circle().fill(Color(nsColor: theme.line))
                        .opacity(pressed ? 0.7 : (hover ? 0.85 : 1))
                } else {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Color(nsColor: theme.text).opacity(pressed ? 0.14 : (hover || selected ? 0.08 : 0)))
                }
            }
            .contentShape(Rectangle())
            .opacity(enabled ? 1 : 0.4)
            .onHover { hovering = $0 }
            .pointingHandCursor()
    }
}
