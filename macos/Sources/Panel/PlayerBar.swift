import SwiftUI

/// Lecture, progression, temps, vitesse. Contrôles fixes à 13 pt.
struct PlayerBar: View {
    let model: ReaderModel
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let s = model.snapshot
        HStack(spacing: 12) {
            PlayButton(model: model)

            Timeline(progress: Double(s.progress), markers: markers, theme: theme)
                .frame(height: 9)
                // Une barre de progression pour VoiceOver : un rôle connu,
                // sa valeur en pourcentage.
                .accessibilityRepresentation {
                    ProgressView(value: Timeline.clamped(Double(s.progress)))
                        .accessibilityLabel("Progression")
                        .accessibilityValue("\(Int(Timeline.clamped(Double(s.progress)) * 100)) %")
                }

            Text(Self.time(s.elapsed) + " / ~" + Self.time(s.total))
                .monospacedDigit()
                // Graisse moyenne : les traits fins de « –:–– » restent
                // assez contrastés à 13 pt.
                .fontWeight(.medium)
            SpeedLabel(speed: s.speed)
            if let volume = Self.volumeText(volume: s.volume, muted: s.muted) {
                Label(volume, systemImage: s.muted ? "speaker.slash.fill" : "speaker.wave.1.fill")
                    .labelStyle(.titleAndIcon)
                    .monospacedDigit()
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("Volume")
                    .accessibilityValue(volume)
                    .accessibilityIdentifier("volume")
            }
            if s.bookmarked {
                Image(systemName: "bookmark.fill")
                    .accessibilityLabel("Signet posé sur cette phrase")
                    .accessibilityIdentifier("signet-pose")
            }
            if let skipped = Self.skippedText(s.skipped) {
                Text(skipped)
                    .accessibilityIdentifier("phrases-sautees")
            }
            // L'écrou : les réglages de la voix, pour en essayer une autre.
            VoiceSettingsButton()
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

    /// Le volume, seulement s'il n'est pas plein : « Muet » ou « 60 % ».
    static func volumeText(volume: Float, muted: Bool) -> String? {
        if muted { return "Muet" }
        let percent = Int((volume * 100).rounded())
        return percent >= 100 ? nil : "\(percent) %"
    }

    /// Les phrases que la voix n'a pas su dire, comme au terminal.
    static func skippedText(_ count: UInt32) -> String? {
        switch count {
        case 0: return nil
        case 1: return "1 phrase sautée"
        default: return "\(count) phrases sautées"
        }
    }

    static func time(_ seconds: Double?) -> String {
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

/// Lecture / pause, dans la barre du panneau comme dans la barre réduite.
struct PlayButton: View {
    let model: ReaderModel

    var body: some View {
        let playing = model.snapshot.playing
        Button {
            model.send(.togglePlay)
        } label: {
            Image(systemName: playing ? "pause.fill" : "play.fill")
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("lecture")
        .accessibilityLabel(playing ? "Pause" : "Lecture")
    }
}

/// La vitesse, lue « 1,5 fois » par VoiceOver.
struct SpeedLabel: View {
    let speed: Float

    var body: some View {
        Text(PlayerBar.speedText(speed))
            .accessibilityLabel("Vitesse")
            .accessibilityValue(PlayerBar.speedSpoken(speed))
    }
}

/// L'écrou : les réglages de la voix (onglet Voix).
struct VoiceSettingsButton: View {
    var body: some View {
        Button {
            SettingsOpener.shared.open(tab: .voice)
        } label: {
            Image(systemName: "gearshape")
        }
        .buttonStyle(.plain)
        // Cible élargie sans toucher à la hauteur de la barre.
        .padding(.horizontal, 5)
        .contentShape(Rectangle())
        .help("Réglages de la voix (v)")
        .accessibilityLabel("Réglages de la voix")
        .accessibilityIdentifier("reglages-voix")
    }
}
