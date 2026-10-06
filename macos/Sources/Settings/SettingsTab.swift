import SwiftUI

/// Les six onglets de la fenêtre des réglages.
enum SettingsTab: String, CaseIterable {
    case voice = "voix"
    case reading = "lecture"
    case models = "modeles"
    case shortcuts = "raccourcis"
    case advanced = "avance"
    case general = "general"

    var label: String {
        switch self {
        case .voice: "Voix"
        case .reading: "Lecture"
        case .models: "Modèles"
        case .shortcuts: "Raccourcis"
        case .advanced: "Avancé"
        case .general: "Général"
        }
    }

    var symbol: String {
        switch self {
        case .voice: "waveform"
        case .reading: "text.alignleft"
        case .models: "square.and.arrow.down"
        case .shortcuts: "keyboard"
        case .advanced: "slider.horizontal.3"
        case .general: "gearshape"
        }
    }
}

/// L'onglet affiché, retenu d'une ouverture et d'un lancement à l'autre :
/// `⌘,` rouvre le dernier, l'écrou du panneau choisit Voix.
@MainActor
@Observable
final class SettingsNavigation {
    static let shared = SettingsNavigation(defaults: processDefaults())
    static let key = "reglages.onglet"

    @ObservationIgnored private let defaults: UserDefaults
    private(set) var tab: SettingsTab

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        tab = defaults.string(forKey: Self.key).flatMap(SettingsTab.init(rawValue:)) ?? .voice
    }

    /// Les préférences de l'utilisateur, sauf sous les tests (unitaires ou
    /// d'interface) : une suite à part, vidée à chaque lancement, qui ne
    /// garde l'onglet que le temps du processus.
    nonisolated static func processDefaults() -> UserDefaults {
        let info = ProcessInfo.processInfo
        let testing = info.arguments.contains("--ui-testing")
            || info.environment["XCTestConfigurationFilePath"] != nil
        guard testing else { return .standard }
        let name = "fr.sillon.tests.navigation"
        guard let suite = UserDefaults(suiteName: name) else { return .standard }
        suite.removePersistentDomain(forName: name)
        return suite
    }

    func select(_ tab: SettingsTab) {
        self.tab = tab
        defaults.set(tab.rawValue, forKey: Self.key)
    }

    var binding: Binding<SettingsTab> {
        Binding(get: { self.tab }, set: { self.select($0) })
    }
}

extension View {
    /// L'étiquette et l'étiquette de sélection d'un onglet.
    func settingsTab(_ tab: SettingsTab) -> some View {
        tabItem { Label(tab.label, systemImage: tab.symbol) }.tag(tab)
    }
}
