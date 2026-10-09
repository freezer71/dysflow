import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les raccourcis globaux. Un raccourci déjà pris est
/// signalé par l'enregistreur lui-même.
struct ShortcutsTab: View {
    var body: some View {
        Form {
            row("Lire la sélection", .readSelection, id: "raccourci-lecture-selection")
            row("Afficher sillon", .showPanel, id: "raccourci-afficher")
            row("Lecture / pause", .playPause, id: "raccourci-lecture")
            row("Phrase suivante", .nextSentence, id: "raccourci-phrase-suivante")
            row("Phrase précédente", .previousSentence, id: "raccourci-phrase-precedente")
            Text(Self.voiceOverNote)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("raccourcis-note-voiceover")
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-raccourcis")
    }

    static let voiceOverNote = "⌃⌥ est aussi la touche de VoiceOver : avec VoiceOver actif, choisissez d'autres touches."

    private func row(_ title: String, _ name: KeyboardShortcuts.Name, id: String) -> some View {
        KeyboardShortcuts.Recorder(title + " :", name: name)
            .accessibilityLabel(title)
            .accessibilityIdentifier(id)
    }
}
