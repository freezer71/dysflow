import KeyboardShortcuts
import SwiftUI

/// La barre du mode petit : lecture, ligne de temps, temps restant,
/// vitesse, écrou. Contrôles à 13 pt, libellés comme la barre du panneau.
/// Elle lit l'instantané du modèle, à son rythme : rien ne tourne en pause.
struct MiniPlayerView: View {
    let model: ReaderModel
    let theme: Theme

    var body: some View {
        let s = model.snapshot
        HStack(spacing: 10) {
            if s.hasDocument {
                PlayButton(model: model)
                Timeline(progress: Double(s.progress), markers: [], theme: theme)
                    .frame(minWidth: 30)
                    .frame(height: 9)
                    .accessibilityRepresentation {
                        ProgressView(value: Timeline.clamped(Double(s.progress)))
                            .accessibilityLabel("Progression")
                            .accessibilityValue(PlaybackTime.timelineValue(elapsed: s.elapsed, total: s.total,
                                                                           progress: s.progress))
                    }
                Text(PlaybackTime.shortRemaining(elapsed: s.elapsed, total: s.total))
                    .monospacedDigit()
                    .fontWeight(.medium)
                    .fixedSize()
                    .accessibilityLabel("Temps restant")
                    .accessibilityValue(PlaybackTime.spokenRemaining(elapsed: s.elapsed, total: s.total))
                SpeedLabel(speed: s.speed)
                    .fixedSize()
            } else {
                Text(PlaybackTime.emptyText(shortcut: KeyboardShortcuts.getShortcut(for: .readSelection)))
                    .lineLimit(1)
                    .accessibilityIdentifier("rien-a-lire")
                Spacer(minLength: 0)
            }
            VoiceSettingsButton()
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .padding(.horizontal, 14)
        // Souple : la fenêtre fixe la taille (SmallPlacement.size).
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("barre-mini")
    }
}
