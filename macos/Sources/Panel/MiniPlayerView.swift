import KeyboardShortcuts
import SwiftUI

/// La barre du mode petit, à la façon d'un mini-lecteur de musique, sur
/// trois rangées : le titre de ce qui est lu, et « Fermer » ; la timeline entre le temps
/// écoulé et le temps restant ; puis ⏮ ▶ ⏭ par phrase, et « Agrandir ». Contrôles à 13 pt au
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

    static let closeAction = ReaderAction.close

    var body: some View {
        let s = snapshot
        VStack(spacing: 6) {
            MiniTitle(title: MiniTitle.text(hasDocument: s.hasDocument, title: model.document.title),
                      color: Color(nsColor: s.hasDocument ? theme.text : theme.chrome))
                // La même marge des deux côtés : le titre reste centré.
                .padding(.horizontal, MiniBarMetrics.closeInset)
                // Par-dessus : la cible de 32 pt ne grandit pas la rangée.
                .overlay(alignment: .trailing) {
                    BarIconButton(symbol: "xmark", label: "Close", help: "Close, reading stops (⌘W)",
                                  identifier: "fermer-mini") {
                        perform(Self.closeAction)
                    }
                }
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
            ModelLine(snapshot: s, color: Color(nsColor: theme.chrome))
                .frame(maxWidth: .infinity, alignment: .center)
                .frame(height: 14)
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

/// Rangée 0 : le titre de ce qui est lu (le premier titre du document,
/// sinon ses premiers caractères), centré, sur une ligne, coupé au bout ;
/// entier au survol.
struct MiniTitle: View {
    let title: String
    let color: Color

    /// Sans document, le nom de l'app.
    static func text(hasDocument: Bool, title: String) -> String {
        let t = title.trimmingCharacters(in: .whitespacesAndNewlines)
        return hasDocument && !t.isEmpty ? t : "sillage"
    }

    var body: some View {
        Text(title)
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(color)
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(maxWidth: .infinity, alignment: .center)
            .frame(height: 20)
            .help(title)
            .accessibilityLabel("Now playing: \(title)")
            .accessibilityIdentifier("titre-mini")
    }
}

enum MiniBarMetrics {
    static let margin: CGFloat = 14
    /// La largeur réservée de chaque côté : le transport reste centré.
    static let side: CGFloat = 64
    /// La place du bouton fermer, de chaque côté du titre.
    static let closeInset: CGFloat = 28
}

/// Rangée 1 : temps écoulé, timeline, temps restant.
struct MiniTimeRow: View {
    let progress: Float
    let elapsed: String
    let remaining: String
    let spokenRemaining: String
    let value: String
    let theme: Theme

    static let elapsedLabel = String(localized: "Elapsed time")
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
                        .accessibilityLabel("Progress")
                        .accessibilityValue(value)
                }
            Text(remaining)
                .monospacedDigit().fixedSize()
                .accessibilityLabel("Time remaining")
                .accessibilityValue(spokenRemaining)
                .accessibilityIdentifier("temps-restant")
        }
        .frame(height: 16)
    }
}

/// Rangée 2 : « Coller » et le volume à gauche, ⏮ ▶ ⏭ par phrase au
/// centre, la sortie audio et « Agrandir » à droite.
struct MiniControlsRow: View {
    let model: ReaderModel
    var transportEnabled = true
    let send: (ReaderAction) -> Void

    static let transport = [SentenceTransport.previous, SentenceTransport.next]
    static let growAction = ReaderAction.toggleSize
    static let pasteAction = ReaderAction.paste

    var body: some View {
        HStack(spacing: 0) {
            // La même largeur des deux côtés : le transport reste centré.
            // Coller marche aussi sans document : c'est ainsi qu'on en ouvre un.
            HStack(spacing: 0) {
                BarIconButton(symbol: "doc.on.clipboard", label: "Paste",
                              help: "Read the clipboard (⌘V)", identifier: "coller-mini") {
                    send(Self.pasteAction)
                }
                VolumeButton(model: model)
            }
            .frame(width: MiniBarMetrics.side, alignment: .leading)
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                TransportButton(item: SentenceTransport.previous, send: send)
                PlayButton(model: model, prominent: true)
                TransportButton(item: SentenceTransport.next, send: send)
            }
            .disabled(!transportEnabled)
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                OutputMenu(model: model)
                BarIconButton(symbol: "arrow.up.left.and.arrow.down.right", label: "Expand",
                              help: "Expand the reader (⌘M)", identifier: "agrandir") {
                    send(Self.growAction)
                }
            }
            .frame(width: MiniBarMetrics.side, alignment: .trailing)
        }
        .frame(height: BarButtonMetrics.disc + 4)
    }
}
