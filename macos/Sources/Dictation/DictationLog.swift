import os

/// Le journal de la dictée : ce qui arrive entre le relâchement et la
/// saisie (texte reçu, collé ou copié, et pourquoi). Jamais le texte : sa
/// longueur seulement. À lire avec :
/// `log show --last 10m --predicate 'subsystem == "fr.dysflow.app" AND category == "dictee"' --info`
enum DictationLog {
    static let log = Logger(subsystem: "fr.dysflow.app", category: "dictee")
}
