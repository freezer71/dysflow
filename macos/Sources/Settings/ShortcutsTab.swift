import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les raccourcis globaux. Un raccourci déjà pris est
/// signalé par l'enregistreur lui-même.
struct ShortcutsTab: View {
    var body: some View {
        SettingsPage(title: "Raccourcis", subtitle: "Des touches qui marchent depuis n'importe quelle application.") {
            SettingsGroup(title: "Raccourcis globaux") {
                row("Lire la sélection", "Le texte sélectionné, ou copié", .readSelection, id: "raccourci-lecture-selection")
                row("Afficher sillon", "Ramène le panneau devant", .showPanel, id: "raccourci-afficher")
                row("Lecture / pause", nil, .playPause, id: "raccourci-lecture")
                row("Phrase suivante", nil, .nextSentence, id: "raccourci-phrase-suivante")
                row("Phrase précédente", nil, .previousSentence, id: "raccourci-phrase-precedente")
            }
            Callout(symbol: "info.circle", text: Self.voiceOverNote, identifier: "raccourcis-note-voiceover")
        }
        .accessibilityIdentifier("onglet-raccourcis")
    }

    static let voiceOverNote = "⌃⌥ est aussi la touche de VoiceOver : avec VoiceOver actif, choisissez d'autres touches."

    private func row(_ title: String, _ detail: String?, _ name: KeyboardShortcuts.Name, id: String) -> some View {
        SettingRow(title: title, detail: detail) {
            KeyboardShortcuts.Recorder(for: name)
                .frame(width: 150)
                .accessibilityLabel(title)
                .accessibilityIdentifier(id)
        }
    }
}
