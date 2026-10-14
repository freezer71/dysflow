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
            OutputMenu(model: model)
            // L'écrou : les réglages de la voix, pour en essayer une autre.
            VoiceSettingsButton()
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }

    /// La vitesse à la française, virgule décimale, à l'écran comme à l'oral.
    nonisolated static func speedText(_ speed: Float) -> String { decimal(speed) + "×" }
    nonisolated static func speedSpoken(_ speed: Float) -> String { decimal(speed) + " fois" }
    nonisolated private static func decimal(_ speed: Float) -> String {
        String(format: "%.1f", speed).replacingOccurrences(of: ".", with: ",")
    }

    /// Le volume, seulement s'il n'est pas plein : « Muet » ou « 60 % ».
    nonisolated static func volumeText(volume: Float, muted: Bool) -> String? {
        if muted { return "Muet" }
        let percent = Int((volume * 100).rounded())
        return percent >= 100 ? nil : "\(percent) %"
    }

    /// Les phrases que la voix n'a pas su dire, comme au terminal.
    nonisolated static func skippedText(_ count: UInt32) -> String? {
        switch count {
        case 0: return nil
        case 1: return "1 phrase sautée"
        default: return "\(count) phrases sautées"
        }
    }

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
                Capsule().fill(Color(nsColor: theme.text).opacity(0.1)).frame(height: 4)
                Capsule().fill(Color(nsColor: theme.text))
                    .frame(width: geo.size.width * Self.clamped(progress), height: 4)
                ForEach(markers, id: \.self) { m in
                    Capsule().fill(Color(nsColor: theme.background))
                        .frame(width: 2, height: 4)
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
        .help("Lecture / pause (Espace)")
        .accessibilityIdentifier("lecture")
        .accessibilityLabel(playing ? "Pause" : "Lecture")
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
/// à l'encre du texte (monochrome, comme Notion), s'éclaircit au survol et
/// à l'appui.
struct BarButtonStyle: ButtonStyle {
    var disc = false

    func makeBody(configuration: Configuration) -> some View {
        BarButtonBody(configuration: configuration, disc: disc)
    }
}

private struct BarButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let disc: Bool
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.panelTheme) private var theme

    var body: some View {
        let pressed = configuration.isPressed
        let size = disc ? BarButtonMetrics.disc : BarButtonMetrics.target
        let hover = BarButtonMetrics.showsHover(hovering: hovering, enabled: enabled)
        configuration.label
            .foregroundStyle(disc ? AnyShapeStyle(Color(nsColor: theme.background)) : AnyShapeStyle(.foreground))
            .frame(minWidth: size, minHeight: size)
            .background {
                if disc {
                    Circle().fill(Color(nsColor: theme.text))
                        .opacity(pressed ? 0.7 : (hover ? 0.85 : 1))
                } else {
                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color(nsColor: theme.text).opacity(pressed ? 0.14 : (hover ? 0.07 : 0)))
                }
            }
            .contentShape(Rectangle())
            .opacity(enabled ? 1 : 0.4)
            .onHover { hovering = $0 }
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
    @Environment(\.openSettingsTab) private var openSettings

    var body: some View {
        Button {
            openSettings(.voice)
        } label: {
            Image(systemName: "gearshape")
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        .help("Réglages de la voix (v)")
        .accessibilityLabel("Réglages de la voix")
        .accessibilityIdentifier("reglages-voix")
    }
}

/// Les sorties, en liste à cocher : la même dans la barre des menus et dans
/// le bouton du panneau.
struct OutputPicker: View {
    let model: ReaderModel

    var body: some View {
        Picker("Sortie audio", selection: Binding(get: { model.outputDevice },
                                                  set: { model.chooseOutput($0) })) {
            ForEach(AudioOutputChoices.choices(model.outputs, chosen: model.outputDevice)) { choice in
                Text(choice.title).tag(choice.value)
            }
        }
        .pickerStyle(.inline)
        .labelsHidden()
    }
}

/// Le bouton de sortie audio du panneau : une icône (écouteurs, enceinte,
/// sortie du système), la liste au clic.
struct OutputMenu: View {
    let model: ReaderModel
    /// Le mode petit n'a la place que de l'icône (le menu s'ouvre pareil).
    var compact = false

    var body: some View {
        Menu {
            OutputPicker(model: model)
        } label: {
            // Le nom et la flèche du menu : on voit que c'est la sortie, et
            // qu'on peut la changer.
            HStack(spacing: 4) {
                Image(systemName: AudioOutputChoices.symbol(for: model.outputDevice))
                    .font(.system(size: BarButtonMetrics.glyph))
                if !compact {
                    Text(AudioOutputChoices.buttonTitle(model.outputs, chosen: model.outputDevice))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: 150)
                }
            }
            .padding(.horizontal, compact ? 0 : 6)
            .frame(minWidth: BarButtonMetrics.target, minHeight: BarButtonMetrics.target)
            .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(compact ? .hidden : .visible)
        .fixedSize()
        .focusable(false)
        .help("Sortie audio : où le son sort")
        .accessibilityLabel("Sortie audio")
        .accessibilityValue(model.outputDevice.isEmpty
                            ? AudioOutputChoices.systemTitle(model.outputs) : model.outputDevice)
        .accessibilityIdentifier("sortie-audio")
    }
}
