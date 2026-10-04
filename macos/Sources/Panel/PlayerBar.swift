import SwiftUI

/// Lecture, progression, temps, vitesse. Contrôles fixes à 13 pt.
struct PlayerBar: View {
    let model: ReaderModel
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let s = model.snapshot
        HStack(spacing: 12) {
            Button {
                model.send(.togglePlay)
            } label: {
                Image(systemName: s.playing ? "pause.fill" : "play.fill")
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("lecture")
            .accessibilityLabel(s.playing ? "Pause" : "Lecture")

            Timeline(progress: Double(s.progress), markers: markers, theme: theme)
                .frame(height: 9)
                .accessibilityElement()
                .accessibilityLabel("Progression")
                .accessibilityValue("\(Int(Timeline.clamped(Double(s.progress)) * 100)) %")

            Text(Self.time(s.elapsed) + " / ~" + Self.time(s.total))
                .monospacedDigit()
            Text(Self.speedText(s.speed))
                .accessibilityLabel("Vitesse")
                .accessibilityValue(Self.speedSpoken(s.speed))
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }

    /// La vitesse à la française, virgule décimale, à l'écran comme à l'oral.
    static func speedText(_ speed: Float) -> String { decimal(speed) + "×" }
    static func speedSpoken(_ speed: Float) -> String { decimal(speed) + " fois" }
    private static func decimal(_ speed: Float) -> String {
        String(format: "%.1f", speed).replacingOccurrences(of: ".", with: ",")
    }

    static func time(_ seconds: Double?) -> String {
        guard let seconds, seconds.isFinite else { return "–:––" }
        let t = Int(seconds.rounded())
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}

/// La ligne de temps : un trait, la part lue, un repère par section.
struct Timeline: View {
    let progress: Double
    let markers: [Double]
    let theme: Theme

    /// La part lue, bornée à 0…1 (0 si la valeur n'a pas de sens).
    static func clamped(_ p: Double) -> Double {
        p.isFinite ? min(max(p, 0), 1) : 0
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(nsColor: theme.border)).frame(height: 3)
                Capsule().fill(Color(nsColor: theme.line))
                    .frame(width: geo.size.width * Self.clamped(progress), height: 3)
                ForEach(markers, id: \.self) { m in
                    Rectangle().fill(Color(nsColor: theme.chrome))
                        .frame(width: 1, height: 9)
                        .offset(x: geo.size.width * m)
                }
            }
            .frame(maxHeight: .infinity)
        }
    }
}
