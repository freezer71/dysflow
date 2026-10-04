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
                .accessibilityValue("\(Int(s.progress * 100)) %")

            Text(Self.time(s.elapsed) + " / ~" + Self.time(s.total))
                .monospacedDigit()
            Text(String(format: "%.1f×", s.speed).replacingOccurrences(of: ".", with: ","))
                .accessibilityLabel("Vitesse")
                .accessibilityValue(String(format: "%.1f fois", s.speed))
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }

    static func time(_ seconds: Double?) -> String {
        guard let seconds else { return "–:––" }
        let t = Int(seconds.rounded())
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}

/// La ligne de temps : un trait, la part lue, un repère par section.
struct Timeline: View {
    let progress: Double
    let markers: [Double]
    let theme: Theme

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(nsColor: theme.border)).frame(height: 3)
                Capsule().fill(Color(nsColor: theme.line))
                    .frame(width: geo.size.width * progress, height: 3)
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
