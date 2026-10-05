import SwiftUI

@main
struct SillonApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra("sillon", systemImage: "text.line.first.and.arrowtriangle.forward") {
            MenuContent()
        }
        Settings {
            if let model = AppDelegate.shared?.model {
                ReglagesView(model: model)
            }
        }
    }
}
