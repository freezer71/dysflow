import Foundation

/// Ce que le moteur de dictée dit en cours de route.
enum DictationEvent: Equatable, Sendable {
    /// La langue s'installe (première dictée dans cette langue) : 0 à 1.
    case downloading(Double)
    /// Le micro est ouvert : ce qui est dit à partir d'ici est entendu.
    case listening
    /// Un résultat : provisoire (réécrit ensuite) ou définitif.
    case text(String, isFinal: Bool)
    /// Des tranches du spectre de la voix (voir `SpectrumAnalyzer`).
    case spectrum([[Float]])
}

enum DictationError: Error, Equatable {
    case microphoneDenied
    case noMicrophone
    case languageUnavailable
    case failed(String)

    var message: String {
        switch self {
        case .microphoneDenied:
            "sillon n'a pas accès au micro : autorisez-le pour dicter"
        case .noMicrophone:
            "Aucun micro branché"
        case .languageUnavailable:
            "Cette langue n'est pas disponible pour la dictée"
        case .failed(let reason):
            "La dictée n'a pas pu démarrer : \(reason)"
        }
    }
}

/// La reconnaissance de la parole : le micro, puis le texte, au fil de l'eau.
@MainActor
protocol DictationEngine: AnyObject {
    /// Ouvre le micro et commence à transcrire. Rend la main quand le micro
    /// est ouvert ; les résultats arrivent ensuite par `events`.
    func start(locale: Locale, events: @escaping @MainActor @Sendable (DictationEvent) -> Void) async throws
    /// Ferme le micro et attend les derniers résultats.
    func finish() async
    /// Ferme le micro et oublie ce qui reste.
    func cancel() async
}

/// La langue dans laquelle on dicte.
enum DictationLanguage: String, CaseIterable {
    case french = "fr-FR"
    case english = "en-US"

    static let defaultsKey = "dictationLanguage"

    var label: String {
        switch self {
        case .french: "Français"
        case .english: "Anglais"
        }
    }

    var locale: Locale { Locale(identifier: rawValue) }

    /// Celle des réglages ; sinon celle du système si c'est l'anglais, le
    /// français dans tous les autres cas.
    static func stored(_ defaults: UserDefaults = .standard,
                       preferred: [String] = Locale.preferredLanguages) -> DictationLanguage {
        if let raw = defaults.string(forKey: defaultsKey), let language = DictationLanguage(rawValue: raw) {
            return language
        }
        return preferred.first?.hasPrefix("en") == true ? .english : .french
    }
}

/// La dictée demande la reconnaissance de la parole de macOS 26
/// (`SpeechAnalyzer`), sur l'appareil : rien ne quitte le Mac.
enum Dictation {
    static var isSupported: Bool {
        #if compiler(>=6.2)
        if #available(macOS 26, *) { return true }
        #endif
        return false
    }

    @MainActor
    static func makeEngine() -> DictationEngine? {
        #if compiler(>=6.2)
        if #available(macOS 26, *) { return SystemDictationEngine() }
        #endif
        return nil
    }
}
