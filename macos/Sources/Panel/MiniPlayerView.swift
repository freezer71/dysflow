import KeyboardShortcuts
import SwiftUI

/// La barre du mode petit, à la façon d'un mini-lecteur de musique, sur
/// deux rangées : la timeline entre le temps écoulé et le temps restant ;
/// puis la vitesse, le transport centré, l'écrou. Contrôles à 13 pt au
/// moins. Elle lit l'instantané du modèle, à son rythme : rien ne tourne
/// en pause.
struct MiniPlayerView: View {
    let model: ReaderModel
    let theme: Theme
    /// Le chemin des touches : un bouton envoie la même action que sa touche.
    let perform: (ReaderAction) -> Void

    var body: some View {
        MiniPlayerBar(model: model, snapshot: model.snapshot, message: model.message,
                      theme: theme, perform: perform)
    }
}

/// Ce que montre la rangée du haut.
enum MiniTop: Equatable {
    case message(String), times, empty

    static func choose(message: String?, hasDocument: Bool) -> MiniTop {
        if let message { return .message(message) }
        return hasDocument ? .times : .empty
    }
}

/// La barre, pour un instantané donné (les tests lui donnent les pires
/// valeurs).
struct MiniPlayerBar: View {
    let model: ReaderModel
    let snapshot: Snapshot
    let message: String?
    let theme: Theme
    let perform: (ReaderAction) -> Void

    var body: some View {
        let s = snapshot
        VStack(spacing: 6) {
            switch MiniTop.choose(message: message, hasDocument: s.hasDocument) {
            case .message(let message):
                // Un message passager prend la place des temps, sans changer
                // la hauteur de la barre.
                Text(message)
                    .lineLimit(1)
                    .frame(height: 16)
                    .accessibilityIdentifier("message-mini")
            case .times:
                MiniTimeRow(progress: s.progress, elapsed: PlayerBar.time(s.elapsed),
                            remaining: PlaybackTime.shortRemaining(elapsed: s.elapsed, total: s.total),
                            spokenRemaining: PlaybackTime.spokenRemaining(elapsed: s.elapsed, total: s.total),
                            value: PlaybackTime.timelineValue(elapsed: s.elapsed, total: s.total,
                                                              progress: s.progress),
                            theme: theme)
            case .empty:
                Text(PlaybackTime.emptyText(shortcut: KeyboardShortcuts.getShortcut(for: .readSelection)))
                    .lineLimit(1)
                    .accessibilityIdentifier("rien-a-lire")
                    .frame(height: 16)
            }
            // Désactivés sans document, pas cachés : la barre garde sa forme.
            MiniControlsRow(model: model, speed: s.speed, transportEnabled: MiniTransport.enabled(snapshot: s),
                            send: perform)
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .padding(.horizontal, MiniBarMetrics.margin)
        // Souple : la fenêtre fixe la taille (SmallPlacement.size).
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("barre-mini")
    }
}

enum MiniBarMetrics {
    static let margin: CGFloat = 14
    /// La largeur réservée à la vitesse et à l'écrou, de chaque côté : le
    /// transport reste centré quelle que soit la vitesse.
    static let side: CGFloat = 56
}

/// Rangée 1 : temps écoulé, timeline, temps restant.
struct MiniTimeRow: View {
    let progress: Float
    let elapsed: String
    let remaining: String
    let spokenRemaining: String
    let value: String
    let theme: Theme

    static let elapsedLabel = "Temps écoulé"
    /// Ce que VoiceOver lit : le temps lui-même.
    static func elapsedValue(_ elapsed: String) -> String { elapsed }

    var body: some View {
        HStack(spacing: 8) {
            Text(elapsed)
                .monospacedDigit().fixedSize()
                .accessibilityLabel(Self.elapsedLabel)
                .accessibilityValue(Self.elapsedValue(elapsed))
                .accessibilityIdentifier("temps-ecoule")
            Timeline(progress: Double(progress), markers: [], theme: theme)
                .frame(minWidth: 30)
                .frame(height: 9)
                .accessibilityRepresentation {
                    ProgressView(value: Timeline.clamped(Double(progress)))
                        .accessibilityLabel("Progression")
                        .accessibilityValue(value)
                }
            Text(remaining)
                .monospacedDigit().fixedSize()
                .accessibilityLabel("Temps restant")
                .accessibilityValue(spokenRemaining)
                .accessibilityIdentifier("temps-restant")
        }
        .frame(height: 16)
    }
}

/// Rangée 2 : vitesse à gauche, transport au centre, écrou à droite.
struct MiniControlsRow: View {
    let model: ReaderModel
    let speed: Float
    var transportEnabled = true
    let send: (ReaderAction) -> Void

    var body: some View {
        HStack(spacing: 0) {
            SpeedLabel(speed: speed)
                .fixedSize()
                .frame(width: MiniBarMetrics.side, alignment: .leading)
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                ForEach(MiniTransport.before) { MiniTransportButton(item: $0, send: send) }
                PlayButton(model: model, prominent: true)
                ForEach(MiniTransport.after) { MiniTransportButton(item: $0, send: send) }
            }
            .disabled(!transportEnabled)
            Spacer(minLength: 0)
            VoiceSettingsButton()
                .frame(width: MiniBarMetrics.side, alignment: .trailing)
        }
        .frame(height: BarButtonMetrics.disc + 4)
    }
}

/// Un bouton de transport de la barre : ligne ou mot, avant ou après.
struct MiniTransportItem: Identifiable {
    let identifier: String
    let label: String
    let help: String
    let symbol: String
    let action: ReaderAction
    var id: String { identifier }
}

enum MiniTransport {
    /// À gauche de lecture/pause : ligne précédente, mot précédent.
    static let before = [
        MiniTransportItem(identifier: "ligne-precedente", label: "Ligne précédente",
                          help: "Ligne précédente (↑)", symbol: "chevron.up", action: .line(delta: -1)),
        MiniTransportItem(identifier: "mot-precedent", label: "Mot précédent",
                          help: "Mot précédent (⌥←)", symbol: "chevron.left",
                          action: .command(.words(delta: -1))),
    ]
    /// À droite : mot suivant, ligne suivante.
    static let after = [
        MiniTransportItem(identifier: "mot-suivant", label: "Mot suivant",
                          help: "Mot suivant (⌥→)", symbol: "chevron.right",
                          action: .command(.words(delta: 1))),
        MiniTransportItem(identifier: "ligne-suivante", label: "Ligne suivante",
                          help: "Ligne suivante (↓)", symbol: "chevron.down", action: .line(delta: 1)),
    ]

    static func enabled(snapshot: Snapshot) -> Bool { snapshot.hasDocument }
}

struct MiniTransportButton: View {
    let item: MiniTransportItem
    let send: (ReaderAction) -> Void

    func fire() { send(item.action) }

    var body: some View {
        Button(action: fire) {
            Image(systemName: item.symbol)
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        // Les touches de la barre restent à la fenêtre après un clic.
        .focusable(false)
        .help(item.help)
        .accessibilityLabel(item.label)
        .accessibilityIdentifier(item.identifier)
    }
}
