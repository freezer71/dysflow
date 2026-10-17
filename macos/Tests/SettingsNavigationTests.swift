import XCTest
@testable import Sillon

/// Les réglages en liste : six sections dans un ordre fixe, la Dictée
/// seulement là où elle existe, la dernière section choisie retrouvée.
@MainActor
final class SettingsNavigationTests: XCTestCase {
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.reglages.section"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testTheSectionsComeInTheApprovedOrder() {
        XCTAssertEqual(SettingsSection.available(dictationSupported: true),
                       [.voix, .affichage, .dictee, .raccourcis, .autorisations, .demarrage])
        XCTAssertEqual(SettingsSection.available(dictationSupported: true).map(\.title),
                       ["Voix", "Affichage", "Dictée", "Raccourcis", "Autorisations", "Démarrage"])
    }

    /// Sans dictée (avant macOS 26), sa section n'est pas listée.
    func testDictationIsListedOnlyWhereItIsSupported() {
        XCTAssertEqual(SettingsSection.available(dictationSupported: false),
                       [.voix, .affichage, .raccourcis, .autorisations, .demarrage])
    }

    /// Chaque ligne a son icône et son identifiant (`section-…`, sans accent).
    func testEveryRowHasAnIconAndAnIdentifier() {
        for section in SettingsSection.allCases {
            XCTAssertFalse(section.symbol.isEmpty, "\(section) sans icône")
            XCTAssertTrue(section.identifier.hasPrefix("section-"))
            XCTAssertTrue(section.identifier.allSatisfy(\.isASCII), section.identifier)
        }
        XCTAssertEqual(SettingsSection.voix.identifier, "section-voix")
        XCTAssertEqual(SettingsSection.dictee.identifier, "section-dictee")
        XCTAssertEqual(Set(SettingsSection.allCases.map(\.symbol)).count, SettingsSection.allCases.count,
                       "deux sections, une même icône")
    }

    /// La valeur gardée, si elle est connue et listée ; sinon Voix.
    func testTheStoredSectionIsResolvedSafely() {
        let all = SettingsSection.available(dictationSupported: true)
        let noDictation = SettingsSection.available(dictationSupported: false)
        XCTAssertEqual(SettingsSection.resolve(stored: "raccourcis", available: all), .raccourcis)
        XCTAssertEqual(SettingsSection.resolve(stored: nil, available: all), .voix)
        XCTAssertEqual(SettingsSection.resolve(stored: "avance", available: all), .voix)
        XCTAssertEqual(SettingsSection.resolve(stored: "", available: all), .voix)
        XCTAssertEqual(SettingsSection.resolve(stored: "dictee", available: noDictation), .voix)
    }

    func testANewNavigationStartsOnVoice() {
        let navigation = SettingsNavigation(defaults: freshDefaults(), dictationSupported: true)
        XCTAssertEqual(navigation.selection, .voix)
        XCTAssertEqual(navigation.available.count, 6)
    }

    /// La section choisie est retrouvée à la prochaine ouverture (et au
    /// prochain lancement).
    func testTheLastSelectedSectionIsRemembered() {
        let defaults = freshDefaults()
        SettingsNavigation(defaults: defaults, dictationSupported: true).select(.autorisations)
        XCTAssertEqual(defaults.string(forKey: SettingsNavigation.defaultsKey), "autorisations")
        XCTAssertEqual(SettingsNavigation(defaults: defaults, dictationSupported: true).selection, .autorisations)
    }

    /// Une valeur illisible ou une section absente de ce Mac : Voix.
    func testAnUnknownOrUnavailableStoredSectionFallsBackToVoice() {
        let defaults = freshDefaults()
        defaults.set("modeles", forKey: SettingsNavigation.defaultsKey)
        XCTAssertEqual(SettingsNavigation(defaults: defaults, dictationSupported: true).selection, .voix)
        defaults.set("dictee", forKey: SettingsNavigation.defaultsKey)
        XCTAssertEqual(SettingsNavigation(defaults: defaults, dictationSupported: false).selection, .voix)
        XCTAssertEqual(SettingsNavigation(defaults: defaults, dictationSupported: true).selection, .dictee)
    }

    /// Une section que ce Mac n'a pas, ou aucune (la liste peut donner
    /// `nil`) : la sélection ne bouge pas, la liste n'est jamais vide.
    func testSelectingNothingOrAnUnavailableSectionKeepsTheSelection() {
        let defaults = freshDefaults()
        let navigation = SettingsNavigation(defaults: defaults, dictationSupported: false)
        navigation.select(.raccourcis)
        navigation.select(nil)
        XCTAssertEqual(navigation.selection, .raccourcis)
        navigation.select(.dictee)
        XCTAssertEqual(navigation.selection, .raccourcis)
        XCTAssertEqual(defaults.string(forKey: SettingsNavigation.defaultsKey), "raccourcis")
    }

    /// `--section-reglages=dictee` : les tests d'interface ouvrent à une section.
    func testASectionCanBeRequestedAtLaunch() {
        XCTAssertEqual(SettingsSection.requested(in: ["--ui-testing", "--section-reglages=dictee"]), .dictee)
        XCTAssertNil(SettingsSection.requested(in: ["--ui-testing", "--show-settings"]))
        XCTAssertNil(SettingsSection.requested(in: ["--section-reglages=inconnue"]))
    }

    /// ↓ et ↑ passent à la section voisine, sans boucler ni sortir de la
    /// liste ; la Dictée absente est sautée. Le choix est gardé.
    func testMovingGoesToTheNeighbourWithinTheList() {
        let defaults = freshDefaults()
        let navigation = SettingsNavigation(defaults: defaults, dictationSupported: false)
        navigation.move(by: -1)
        XCTAssertEqual(navigation.selection, .voix, "pas au-dessus de la première")
        navigation.move(by: 1)
        XCTAssertEqual(navigation.selection, .affichage)
        navigation.move(by: 1)
        XCTAssertEqual(navigation.selection, .raccourcis, "la Dictée absente est sautée")
        navigation.select(.demarrage)
        navigation.move(by: 1)
        XCTAssertEqual(navigation.selection, .demarrage, "pas au-dessous de la dernière")
        navigation.move(by: -1)
        XCTAssertEqual(navigation.selection, .autorisations)
        XCTAssertEqual(defaults.string(forKey: SettingsNavigation.defaultsKey), "autorisations")
    }

    /// Sous les tests, des préférences à part : jamais celles de l'utilisateur.
    /// `--ui-testing-reglages-neufs` les vide (au début d'un test d'interface).
    func testUnderTestsTheSelectionLivesInItsOwnSuite() {
        XCTAssertTrue(SettingsNavigation.processDefaults(arguments: ["--ui-testing"], environment: [:])
            !== UserDefaults.standard)
        XCTAssertTrue(SettingsNavigation.processDefaults(arguments: [], environment: [:]) === UserDefaults.standard)
        let suite = SettingsNavigation.processDefaults(arguments: ["--ui-testing"], environment: [:])
        suite.set("raccourcis", forKey: SettingsNavigation.defaultsKey)
        XCTAssertEqual(SettingsNavigation.processDefaults(arguments: ["--ui-testing"], environment: [:])
            .string(forKey: SettingsNavigation.defaultsKey), "raccourcis", "gardé d'un lancement à l'autre")
        let reset = SettingsNavigation.processDefaults(arguments: ["--ui-testing", "--ui-testing-reglages-neufs"],
                                                       environment: [:])
        XCTAssertNil(reset.string(forKey: SettingsNavigation.defaultsKey))
    }
}
