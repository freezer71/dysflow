import KeyboardShortcuts
import ServiceManagement

extension KeyboardShortcuts.Name {
    static let readSelection = Self("readSelection", default: .init(.space, modifiers: [.control, .option]))
    static let showPanel = Self("showPanel", default: .init(.s, modifiers: [.control, .option]))
}

/// « Ouvrir à la connexion », activé au premier lancement.
enum LoginItem {
    private static let firstLaunchKey = "loginItemOffered"

    static var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    static func set(_ on: Bool) {
        try? on ? SMAppService.mainApp.register() : SMAppService.mainApp.unregister()
    }

    static func enableOnFirstLaunch() {
        guard !UserDefaults.standard.bool(forKey: firstLaunchKey) else { return }
        UserDefaults.standard.set(true, forKey: firstLaunchKey)
        set(true)
    }
}
