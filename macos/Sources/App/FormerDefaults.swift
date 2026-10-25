import Foundation

/// Les réglages d'avant les changements de nom : sillage (jusqu'au 25 oct.
/// 2026), et sillon avant lui (jusqu'au 18 oct. 2026).
///
/// L'identifiant de l'app est passé de `fr.sillon.app` à `fr.sillage.app`,
/// puis à `fr.dysflow.app` : macOS range les préférences sous l'identifiant,
/// et l'app renommée repartirait de zéro — modèle de dictée, raccourcis,
/// affichage. Au premier lancement, les anciens domaines sont recopiés, du
/// plus récent au plus ancien, sans écraser une clé déjà posée. Ils restent
/// en place : revenir à un ancien nom reste possible.
enum FormerDefaults {
    static let formerDomains = ["fr.sillage.app", "fr.sillon.app"]
    static let doneKey = "reglagesReprisDesAnciensNoms"
    /// Les marques des reprises précédentes : elles ne voyagent pas.
    static let formerDoneKeys: Set<String> = ["reglagesReprisDeSillon"]

    static func adopt(into defaults: UserDefaults = .standard,
                      former: [[String: Any]?] = formerDomains.map { UserDefaults.standard.persistentDomain(forName: $0) },
                      environment: [String: String] = ProcessInfo.processInfo.environment) {
        // Jamais sous les tests : l'hôte des tests est l'app elle-même.
        guard environment["XCTestConfigurationFilePath"] == nil else { return }
        guard !defaults.bool(forKey: doneKey) else { return }
        for domain in former {
            for (key, value) in domain ?? [:]
            where defaults.object(forKey: key) == nil && !formerDoneKeys.contains(key) {
                defaults.set(value, forKey: key)
            }
        }
        defaults.set(true, forKey: doneKey)
    }
}
