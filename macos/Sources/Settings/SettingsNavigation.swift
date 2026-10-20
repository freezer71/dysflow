import Foundation
import Observation

/// Une section des réglages : une ligne de la liste à gauche, son volet à
/// droite. L'ordre des cas est celui de la liste.
enum SettingsSection: String, CaseIterable, Identifiable, Sendable {
    case voix, affichage, dictee, raccourcis, autorisations, demarrage

    var id: String { rawValue }

    /// Le mot de la liste, et le titre du volet.
    var title: String {
        switch self {
        case .voix: String(localized: "Voice")
        case .affichage: String(localized: "Display")
        case .dictee: String(localized: "Dictation")
        case .raccourcis: String(localized: "Shortcuts")
        case .autorisations: String(localized: "Permissions")
        case .demarrage: String(localized: "Startup")
        }
    }

    /// L'icône SF Symbols de la ligne.
    var symbol: String {
        switch self {
        case .voix: "speaker.wave.2"
        case .affichage: "textformat.size"
        case .dictee: "mic"
        case .raccourcis: "command"
        case .autorisations: "hand.raised"
        case .demarrage: "power"
        }
    }

    /// L'identifiant d'accessibilité de la ligne (les tests d'interface la choisissent par lui).
    var identifier: String { "section-\(rawValue)" }

    /// Une valeur gardée inconnue, ou une section absente de ce Mac : Voix.
    static let fallback = SettingsSection.voix

    /// Les sections listées : la Dictée seulement là où elle existe (macOS 26).
    static func available(dictationSupported: Bool) -> [SettingsSection] {
        allCases.filter { $0 != .dictee || dictationSupported }
    }

    /// La section gardée, si elle est connue et listée ; sinon Voix.
    static func resolve(stored: String?, available: [SettingsSection]) -> SettingsSection {
        guard let stored, let section = SettingsSection(rawValue: stored), available.contains(section) else {
            return available.contains(fallback) ? fallback : (available.first ?? fallback)
        }
        return section
    }

    /// `--section-reglages=<section>` : la section où ouvrir les réglages au
    /// lancement (tests d'interface).
    static let launchArgumentPrefix = "--section-reglages="

    static func requested(in arguments: [String]) -> SettingsSection? {
        arguments.lazy
            .filter { $0.hasPrefix(launchArgumentPrefix) }
            .compactMap { SettingsSection(rawValue: String($0.dropFirst(launchArgumentPrefix.count))) }
            .first
    }
}

/// La section affichée dans la fenêtre des réglages. La dernière choisie
/// est gardée (préférences, clé `reglagesSection`) et retrouvée à la
/// prochaine ouverture ; `SettingsOpener` peut en demander une autre.
@MainActor
@Observable
final class SettingsNavigation {
    static let defaultsKey = "reglagesSection"
    static let shared = SettingsNavigation(defaults: processDefaults())

    /// `--ui-testing-reglages-neufs` : la section gardée par les tests est
    /// oubliée (un test d'interface le passe à son premier lancement).
    static let resetArgument = "--ui-testing-reglages-neufs"

    /// Les préférences où garder la section : celles de l'utilisateur, sauf
    /// sous les tests (`--ui-testing`, XCTest), qui ont leur suite à part,
    /// gardée d'un lancement à l'autre et vidée seulement à la demande. Même
    /// règle que `DisplayModeStore.processDefaults()`.
    static func processDefaults(arguments: [String] = ProcessInfo.processInfo.arguments,
                                environment: [String: String] = ProcessInfo.processInfo.environment)
        -> UserDefaults {
        let testing = arguments.contains("--ui-testing") || environment["XCTestConfigurationFilePath"] != nil
        guard testing else { return .standard }
        let name = "fr.sillage.tests.reglages"
        guard let suite = UserDefaults(suiteName: name) else { return .standard }
        if arguments.contains(resetArgument) { suite.removePersistentDomain(forName: name) }
        return suite
    }

    let available: [SettingsSection]
    private(set) var selection: SettingsSection
    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults, dictationSupported: Bool = Dictation.isSupported) {
        self.defaults = defaults
        available = SettingsSection.available(dictationSupported: dictationSupported)
        selection = SettingsSection.resolve(stored: defaults.string(forKey: Self.defaultsKey), available: available)
    }

    /// Passe à `section` et la garde. `nil` (la liste désélectionnée) ou une
    /// section absente de ce Mac : rien ne change, la liste n'est jamais vide.
    func select(_ section: SettingsSection?) {
        guard let section, available.contains(section) else { return }
        if section != selection { selection = section }
        defaults.set(section.rawValue, forKey: Self.defaultsKey)
    }

    /// ↑ (-1) ou ↓ (+1) : la section voisine dans la liste, sans boucler.
    func move(by offset: Int) {
        guard let index = available.firstIndex(of: selection) else { return }
        let target = min(max(index + offset, 0), available.count - 1)
        select(available[target])
    }
}
