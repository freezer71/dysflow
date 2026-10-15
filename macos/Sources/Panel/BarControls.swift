import SwiftUI

/// La vitesse à la française : virgule, deux décimales au plus, sans zéro
/// inutile (« 1× », « 1,25× », « 1,1× »).
enum SpeedFormat {
    static func number(_ speed: Float) -> String {
        var s = String(format: "%.2f", (Double(speed) * 100).rounded() / 100)
        while s.hasSuffix("0") { s.removeLast() }
        if s.hasSuffix(".") { s.removeLast() }
        return s.replacingOccurrences(of: ".", with: ",")
    }

    static func text(_ speed: Float) -> String { number(speed) + "×" }
    static func spoken(_ speed: Float) -> String { number(speed) + " fois" }
}

/// Une fraction de la barre de progression devient une phrase (la
/// progression du moteur vaut phrase / nombre de phrases).
enum ProgressSeek {
    static func sentence(fraction: Double, sentenceCount: UInt32) -> UInt32? {
        let count = Int(sentenceCount)
        guard count > 0, fraction.isFinite else { return nil }
        let f = min(max(fraction, 0), 1)
        return UInt32(min(Int(f * Double(count)), count - 1))
    }
}

/// Un bouton de transport : ce qu'il montre, et l'action de sa touche.
struct BarTransportItem: Identifiable, Equatable {
    let identifier: String
    let label: String
    let help: String
    let symbol: String
    let action: ReaderAction
    var id: String { identifier }
}

/// ⏮ et ⏭ : une phrase en arrière, une en avant, comme ← et →.
enum SentenceTransport {
    static let previous = BarTransportItem(identifier: "phrase-precedente", label: "Phrase précédente",
                                           help: "Phrase précédente (←)", symbol: "backward.end.fill",
                                           action: .command(.sentences(delta: -1)))
    static let next = BarTransportItem(identifier: "phrase-suivante", label: "Phrase suivante",
                                       help: "Phrase suivante (→)", symbol: "forward.end.fill",
                                       action: .command(.sentences(delta: 1)))
}

struct TransportButton: View {
    let item: BarTransportItem
    let send: (ReaderAction) -> Void

    func fire() { send(item.action) }

    var body: some View {
        Button(action: fire) {
            Image(systemName: item.symbol)
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        // Les touches restent à la fenêtre après un clic.
        .focusable(false)
        .help(item.help)
        .accessibilityLabel(item.label)
        .accessibilityIdentifier(item.identifier)
    }
}

/// Un bouton d'icône de barre (fermer, réduire, agrandir).
struct BarIconButton: View {
    let symbol: String
    let label: String
    let help: String
    let identifier: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        .focusable(false)
        .help(help)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }
}

/// Un bouton titré de l'en-tête (Sommaire, Rechercher, Modifier).
struct HeaderButton: View {
    let title: String
    let symbol: String
    let help: String
    let identifier: String
    var selected = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .labelStyle(.titleAndIcon)
                .padding(.horizontal, 8)
        }
        .buttonStyle(BarButtonStyle())
        .focusable(false)
        .help(help)
        .accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// La ligne de temps, cliquable : un clic mène à la phrase sous le pointeur.
struct SeekableTimeline: View {
    let progress: Double
    var markers: [Double] = []
    let theme: Theme
    var enabled = true
    let seek: (Double) -> Void

    /// La part de la barre sous le pointeur, bornée à 0…1.
    static func fraction(x: CGFloat, width: CGFloat) -> Double {
        guard width > 0, x.isFinite else { return 0 }
        return Double(min(max(x / width, 0), 1))
    }

    var body: some View {
        GeometryReader { geo in
            Timeline(progress: progress, markers: markers, theme: theme)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture(coordinateSpace: .local) { location in
                    guard enabled else { return }
                    seek(Self.fraction(x: location.x, width: geo.size.width))
                }
        }
        // Une cible plus haute que le trait : facile à viser.
        .frame(height: 20)
        .accessibilityRepresentation {
            ProgressView(value: Timeline.clamped(progress))
                .accessibilityLabel("Progression")
                .accessibilityValue("\(Int(Timeline.clamped(progress) * 100)) %")
        }
        .accessibilityIdentifier("progression")
    }
}

/// La vitesse, cliquable : 0,75 · 1 · 1,25 · 1,5 · 2×.
struct SpeedMenu: View {
    static let choices: [Float] = [0.75, 1, 1.25, 1.5, 2]
    let speed: Float
    let choose: (Float) -> Void

    var body: some View {
        Menu {
            ForEach(Self.choices, id: \.self) { value in
                Button {
                    choose(value)
                } label: {
                    if abs(value - speed) < 0.001 {
                        Label(SpeedFormat.text(value), systemImage: "checkmark")
                    } else {
                        Text(SpeedFormat.text(value))
                    }
                }
            }
        } label: {
            Text(SpeedFormat.text(speed))
                .monospacedDigit()
                .fontWeight(.medium)
                .frame(minWidth: BarButtonMetrics.target, minHeight: BarButtonMetrics.target)
                .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .focusable(false)
        .help("Vitesse (+ −)")
        .accessibilityLabel("Vitesse")
        .accessibilityValue(SpeedFormat.spoken(speed))
        .accessibilityIdentifier("vitesse-lecteur")
    }
}
