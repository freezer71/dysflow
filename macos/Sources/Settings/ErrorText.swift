import AppKit
import SwiftUI

/// Dit à VoiceOver ce qui vient d'apparaître à l'écran : refus, erreurs,
/// résultats d'une action. Sans VoiceOver, rien.
@MainActor
enum Announcement {
    static func post(_ text: String) {
        // Les tests unitaires ne parlent jamais à VoiceOver.
        if AppDelegate.shared?.isUnitTesting == true { return }
        guard NSWorkspace.shared.isVoiceOverEnabled, let window = NSApp.keyWindow else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }
}

/// Ce qui a déjà été dit à VoiceOver : un avis qui reste affiché ne se
/// redit pas à chaque retour sur l'onglet.
struct AnnounceOnce {
    private var said: Set<String> = []

    mutating func shouldSay(_ text: String) -> Bool {
        said.insert(text).inserted
    }

    /// Le registre de l'app : il vit autant qu'elle.
    @MainActor static var shared = AnnounceOnce()

    /// Dit `text` à VoiceOver, la première fois seulement.
    @MainActor static func post(_ text: String) {
        if shared.shouldSay(text) { Announcement.post(text) }
    }
}

/// Le rouge des erreurs : 4,5:1 au moins sur le fond de la fenêtre et des
/// lignes de formulaire, en clair (rouge sombre) comme en sombre (rouge clair).
/// `.red` et `systemRed` n'y arrivent pas.
enum ErrorStyle {
    static let nsColor = NSColor(name: NSColor.Name("erreur-lisible")) { appearance in
        if appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua {
            return NSColor(srgbRed: 0xFF / 255, green: 0xA8 / 255, blue: 0xA3 / 255, alpha: 1)
        }
        return NSColor(srgbRed: 0xA8 / 255, green: 0x14 / 255, blue: 0x0C / 255, alpha: 1)
    }
    static let color = Color(nsColor: nsColor)
}

/// Une erreur qu'on montre : son texte, et une annonce VoiceOver à chaque
/// échec, même quand le texte se répète. Le chemin unique des erreurs des
/// réglages, pour les magasins comme pour les onglets.
@MainActor
@Observable
final class ErrorNote {
    private(set) var text: String?
    @ObservationIgnored private let announce: (String) -> Void

    init(announce: ((String) -> Void)? = nil) {
        self.announce = announce ?? { Announcement.post($0) }
    }

    func fail(_ message: String) {
        text = message
        announce(message)
    }

    func clear() { text = nil }
}

/// Le texte d'une erreur, dans le rouge lisible ; rien sans erreur.
struct ErrorText: View {
    let text: String?
    let identifier: String

    init(_ text: String?, identifier: String) {
        self.text = text
        self.identifier = identifier
    }

    var body: some View {
        if let text {
            Text(text)
                .foregroundStyle(ErrorStyle.color)
                .accessibilityIdentifier(identifier)
        }
    }
}
