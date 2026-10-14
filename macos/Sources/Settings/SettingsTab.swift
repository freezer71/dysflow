import SwiftUI

/// Les six pages de la fenêtre des réglages.
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
}

/// La barre latérale des réglages, à la manière de Notion : la marque en
/// tête, une ligne par page, la page ouverte sur fond gris.
struct SettingsSidebar: View {
    let navigation: SettingsNavigation

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 8) {
                LogoMark(small: true)
                    .frame(width: 18, height: 18)
                    .foregroundStyle(Palette.ink)
                Text("sillon")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Palette.ink)
            }
            .padding(.horizontal, 8)
            // La barre de titre est transparente : les boutons de la fenêtre
            // occupent le haut.
            .padding(.top, 48)
            .padding(.bottom, 18)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("sillon")

            Text("Réglages")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .padding(.horizontal, 8)
                .padding(.bottom, 6)
                .accessibilityHidden(true)

            ForEach(SettingsTab.allCases, id: \.self) { tab in
                SettingsSidebarItem(tab: tab, selected: navigation.tab == tab) {
                    navigation.select(tab)
                }
            }
            Spacer(minLength: 0)
            if let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
                Text("Version \(version)")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 14)
            }
        }
        .padding(.horizontal, 8)
        .frame(width: 200)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Palette.sidebar)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Pages des réglages")
    }
}

private struct SettingsSidebarItem: View {
    let tab: SettingsTab
    let selected: Bool
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: tab.symbol)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(selected ? Palette.ink : Palette.faint)
                    .frame(width: 18)
                Text(tab.label)
                    .font(.system(size: 13, weight: selected ? .semibold : .medium))
                    .foregroundStyle(selected ? Palette.ink : Palette.muted)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(height: 30)
            .background(RoundedRectangle(cornerRadius: 6)
                .fill(selected ? Palette.selected : (hovering ? Palette.hover : .clear)))
            .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityLabel(tab.label)
        .accessibilityIdentifier("page-\(tab.rawValue)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
