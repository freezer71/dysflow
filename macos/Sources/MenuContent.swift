import SwiftUI

struct MenuContent: View {
    var body: some View {
        Button("Afficher sillon") { AppDelegate.shared.panel.show() }
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
