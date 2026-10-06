import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les deux raccourcis globaux.
struct ShortcutsTab: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-raccourcis")
    }
}
