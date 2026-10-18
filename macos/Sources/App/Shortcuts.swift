import KeyboardShortcuts
import os
import ServiceManagement

extension KeyboardShortcuts.Name {
    /// Lire la sélection, sinon le presse-papiers.
    static let readSelection = Self("readSelection", default: .init(.space, modifiers: [.control, .option]))
    /// Afficher le lecteur dans sa dernière taille, ou le masquer.
    static let showPanel = Self("showPanel", default: .init(.s, modifiers: [.control, .option]))
    /// Ouvrir la dictée, puis saisir ce qui a été dit.
    static let dictate = Self("dictate", default: .init(.d, modifiers: [.control, .option]))
}

/// « Ouvrir à la connexion », activé au premier lancement.
enum LoginItem {
    private static let firstLaunchKey = "loginItemOffered"
    private static let log = Logger(subsystem: "fr.sillage.app", category: "connexion")

    static var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    /// Vrai si l'inscription a réussi (ou le retrait).
    @discardableResult
    static func set(_ on: Bool) -> Bool {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            return true
        } catch {
            log.error("ouverture à la connexion (\(on, privacy: .public)) : \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    /// Seule une version Release installée dans /Applications s'inscrit :
    /// jamais une copie de développement (DerivedData, build/).
    static func eligible(bundlePath: String, debug: Bool) -> Bool {
        !debug && bundlePath.hasPrefix("/Applications/")
    }

    static func enableOnFirstLaunch() {
        #if DEBUG
        let debug = true
        #else
        let debug = false
        #endif
        guard eligible(bundlePath: Bundle.main.bundleURL.path, debug: debug),
              !UserDefaults.standard.bool(forKey: firstLaunchKey) else { return }
        // Proposé une fois, mais seulement si l'inscription a vraiment réussi.
        if set(true) { UserDefaults.standard.set(true, forKey: firstLaunchKey) }
    }
}
