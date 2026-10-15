import KeyboardShortcuts
import SwiftUI

/// La barre du mode petit, à la façon d'un mini-lecteur de musique, sur
/// deux rangées : la timeline entre le temps écoulé et le temps restant ;
/// puis ⏮ ▶ ⏭ par phrase, et « Agrandir ». Contrôles à 13 pt au
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
            MiniControlsRow(model: model, transportEnabled: s.hasDocument, send: perform)
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
    /// La largeur réservée de chaque côté : le transport reste centré.
    static let side: CGFloat = 64
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

/// Rangée 2 : ⏮ ▶ ⏭ par phrase au centre, « Agrandir » à droite.
struct MiniControlsRow: View {
    let model: ReaderModel
    var transportEnabled = true
    let send: (ReaderAction) -> Void

    static let transport = [SentenceTransport.previous, SentenceTransport.next]
    static let growAction = ReaderAction.toggleSize

    var body: some View {
        HStack(spacing: 0) {
            // La même largeur des deux côtés : le transport reste centré.
            Color.clear.frame(width: MiniBarMetrics.side, height: 1)
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                TransportButton(item: SentenceTransport.previous, send: send)
                PlayButton(model: model, prominent: true)
                TransportButton(item: SentenceTransport.next, send: send)
            }
            .disabled(!transportEnabled)
            Spacer(minLength: 0)
            BarIconButton(symbol: "arrow.up.left.and.arrow.down.right", label: "Agrandir",
                          help: "Agrandir le lecteur (⌘M)", identifier: "agrandir") {
                send(Self.growAction)
            }
            .frame(width: MiniBarMetrics.side, alignment: .trailing)
        }
        .frame(height: BarButtonMetrics.disc + 4)
    }
}
