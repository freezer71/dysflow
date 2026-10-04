import SwiftUI

struct MenuContent: View {
    var body: some View {
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
