import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les raccourcis globaux.
struct ShortcutsTab: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
            KeyboardShortcuts.Recorder("Lecture / pause :", name: .playPause)
            KeyboardShortcuts.Recorder("Phrase suivante :", name: .nextSentence)
            KeyboardShortcuts.Recorder("Phrase précédente :", name: .previousSentence)
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-raccourcis")
    }
}
