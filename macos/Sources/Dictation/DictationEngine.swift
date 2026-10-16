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

/// Le modèle de dictée choisi dans les réglages : `"system"` (la dictée de
/// macOS), ou l'id d'un paquet du catalogue (`parakeet_tdt_v3`, …).
enum DictationModel {
    static let defaultsKey = "dictationModel"
    static let system = "system"

    static func stored(_ defaults: UserDefaults = .standard) -> String {
        defaults.string(forKey: defaultsKey) ?? system
    }

    /// Le nom montré (« Parakeet v3 »).
    static func label(_ id: String) -> String {
        switch id {
        case system: "Dictée de macOS"
        case "parakeet_tdt_v3": "Parakeet v3"
        case "whisper_tiny_4bit": "Whisper tiny"
        case "whisper_small_4bit": "Whisper small"
        case "whisper_large_v3_turbo_4bit": "Whisper large-v3-turbo"
        case "canary_1b_v2_q8": "Canary 1b-v2"
        case "voxtral_realtime_4bit": "Voxtral Realtime"
        default: id.isEmpty ? "Le modèle de dictée" : id
        }
    }
}

/// La dictée demande la reconnaissance de la parole de macOS 26
/// (`SpeechAnalyzer`), sur l'appareil : rien ne quitte le Mac. Les modèles
/// de dictée (MLX) passent par le même aiguilleur, avec la dictée de macOS
/// en repli.
enum Dictation {
    static var isSupported: Bool {
        #if compiler(>=6.2)
        if #available(macOS 26, *) { return true }
        #endif
        return false
    }

    /// L'aiguilleur : le modèle choisi, sinon la dictée de macOS.
    /// `directory` : le dossier d'un paquet installé (le moteur Rust), `nil`
    /// s'il manque. Un seul coureur de modèles, partagé par toutes les
    /// dictées : le modèle reste chargé d'une dictée à l'autre.
    @MainActor
    static func makeEngine(directory: @escaping (String) -> URL?) -> RoutingDictationEngine? {
        #if compiler(>=6.2)
        if #available(macOS 26, *) {
            let runner = SpeechModelRunner(load: SpeechModels.load, clearCache: SpeechModels.release)
            return RoutingDictationEngine(
                system: SystemDictationEngine(),
                chosen: { DictationModel.stored() },
                directory: directory,
                make: { id, folder in makeModelEngine(id: id, directory: folder, runner: runner) })
        }
        #endif
        return nil
    }

    /// Le moteur d'un modèle : Voxtral au fil de la parole, les autres en
    /// relecture glissante.
    @MainActor
    static func makeModelEngine(id: String, directory: URL, runner: SpeechModelRunner) -> DictationEngine? {
        switch id {
        case "voxtral_realtime_4bit":
            VoxtralDictationEngine(runner: runner, directory: directory)
        case "parakeet_tdt_v3", "canary_1b_v2_q8", _ where id.hasPrefix("whisper_"):
            MLXDictationEngine(runner: runner, directory: directory)
        default:
            nil
        }
    }
}
