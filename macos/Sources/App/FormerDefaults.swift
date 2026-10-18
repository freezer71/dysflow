import Foundation

/// Les réglages d'avant le changement de nom (sillon, jusqu'au 18 oct. 2026).
///
/// L'identifiant de l'app est passé de `fr.sillon.app` à `fr.sillage.app` :
/// macOS range les préférences sous l'identifiant, et l'app renommée
/// repartirait de zéro — modèle de dictée, raccourcis, affichage. Au premier
/// lancement, l'ancien domaine est recopié, sans écraser une clé déjà posée.
/// L'ancien domaine reste en place : revenir à sillon reste possible.
enum FormerDefaults {
    static let formerDomain = "fr.sillon.app"
    static let doneKey = "reglagesReprisDeSillon"

    static func adopt(into defaults: UserDefaults = .standard,
                      former: [String: Any]? = UserDefaults.standard.persistentDomain(forName: formerDomain),
                      environment: [String: String] = ProcessInfo.processInfo.environment) {
        // Jamais sous les tests : l'hôte des tests est l'app elle-même.
        guard environment["XCTestConfigurationFilePath"] == nil else { return }
        guard !defaults.bool(forKey: doneKey) else { return }
        for (key, value) in former ?? [:] where defaults.object(forKey: key) == nil {
            defaults.set(value, forKey: key)
        }
        defaults.set(true, forKey: doneKey)
    }
}
