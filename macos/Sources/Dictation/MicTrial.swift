import Foundation

// L'essai du micro (section Dictée des réglages) : ce qui se calcule sans
// micro ni modèle — la précision, les délais, le débit, la mémoire, les
// phrases à lire et les derniers résultats de chaque modèle. Testable seul
// (Foundation seulement).

/// La précision : 100 − le taux d'erreur sur les mots (WER), en %.
enum TrialScore {
    /// Les mots d'un texte, comparables : Unicode NFC, minuscules, toute
    /// ponctuation et tout symbole retirés (« » … — ’ ' - compris : ils
    /// séparent les mots, « l'été » donne « l » et « été »), espaces
    /// regroupées. Les nombres restent tels qu'écrits (« 3 » ≠ « trois »).
    static func words(_ text: String) -> [String] {
        let composed = text.precomposedStringWithCanonicalMapping.lowercased()
        var cleaned = String.UnicodeScalarView()
        for scalar in composed.unicodeScalars {
            let keep = CharacterSet.letters.contains(scalar) || CharacterSet.decimalDigits.contains(scalar)
                || CharacterSet.nonBaseCharacters.contains(scalar)
            cleaned.append(keep ? scalar : " ")
        }
        return String(cleaned).split(separator: " ").map(String.init)
    }

    /// La distance d'édition en mots : substitutions, suppressions et
    /// insertions, au plus petit total.
    static func errors(reference: [String], heard: [String]) -> Int {
        guard !reference.isEmpty else { return heard.count }
        guard !heard.isEmpty else { return reference.count }
        var previous = Array(0...heard.count)
        for (i, word) in reference.enumerated() {
            var row = [i + 1] + Array(repeating: 0, count: heard.count)
            for j in 1...heard.count {
                let substitution = previous[j - 1] + (word == heard[j - 1] ? 0 : 1)
                row[j] = min(substitution, previous[j] + 1, row[j - 1] + 1)
            }
            previous = row
        }
        return previous[heard.count]
    }

    /// Vrai si `heard` dit la phrase mot pour mot (ponctuation et
    /// majuscules mises à part) : l'essai peut s'arrêter seul.
    static func isComplete(reference: String, heard: String) -> Bool {
        let expected = words(reference)
        return !expected.isEmpty && words(heard) == expected
    }

    /// 0 à 100 : la part des mots de la phrase bien reconnus.
    static func precision(reference: String, heard: String) -> Double {
        let expected = words(reference)
        guard !expected.isEmpty else { return 0 }
        let wer = Double(errors(reference: expected, heard: words(heard))) / Double(expected.count)
        return max(0, 100 * (1 - wer))
    }
}

/// Les instants d'un essai, en secondes sur une horloge monotone.
struct TrialTimeline: Equatable {
    /// D'où part le délai du premier texte.
    enum Basis: String, Codable {
        /// Le début de la parole (le spectre du micro s'allume).
        case speech
        /// L'ouverture du micro : le début de la parole n'a pas été repéré.
        case micOpen
    }

    /// Le micro est ouvert (« Parlez »).
    var listening: Double?
    var speechOnset: Double?
    /// Le premier texte, provisoire ou non.
    var firstText: Double?
    /// « Arrêter ».
    var stop: Double?
    /// Le texte final est rendu.
    var final: Double?

    /// Du début de la parole au premier texte ; à défaut (début non repéré,
    /// ou repéré après ce texte), de l'ouverture du micro.
    var firstTextDelay: (seconds: Double, basis: Basis)? {
        guard let firstText else { return nil }
        if let speechOnset, speechOnset <= firstText { return (firstText - speechOnset, .speech) }
        guard let listening else { return nil }
        return (max(0, firstText - listening), .micOpen)
    }

    /// De « Arrêter » au texte final.
    var finalDelay: Double? {
        guard let stop, let final else { return nil }
        return max(0, final - stop)
    }

    /// L'audio entendu : de l'ouverture du micro à « Arrêter ».
    var audioSeconds: Double? {
        guard let listening, let stop else { return nil }
        return max(0, stop - listening)
    }

    static func == (a: Self, b: Self) -> Bool {
        a.listening == b.listening && a.speechOnset == b.speechOnset && a.firstText == b.firstText
            && a.stop == b.stop && a.final == b.final
    }

    /// Le débit : secondes d'audio par seconde de calcul (× temps réel).
    static func speed(audioSeconds: Double, computeSeconds: Double) -> Double? {
        guard audioSeconds > 0, computeSeconds > 0 else { return nil }
        return audioSeconds / computeSeconds
    }
}

/// Le début de la parole, d'après les tranches du spectre du micro
/// (`SpectrumAnalyzer`, 40 par seconde, 0 à 1) : une bande au-dessus de
/// `threshold` pendant `run` tranches de suite (75 ms). Le silence de la
/// pièce reste sous le plancher du spectre ; un claquement bref ne compte pas.
struct SpeechOnsetDetector {
    static let threshold: Float = 0.35
    static let run = 3

    private(set) var heard = false
    private var streak = 0

    /// `true` la seule fois où le début de la parole est repéré.
    mutating func push(_ columns: [[Float]]) -> Bool {
        guard !heard else { return false }
        for column in columns {
            streak = (column.max() ?? 0) >= Self.threshold ? streak + 1 : 0
            if streak >= Self.run {
                heard = true
                return true
            }
        }
        return false
    }
}

/// La fin de la parole : après le début repéré, `silence` tranches de suite
/// (1,5 s) sans bande au-dessus du seuil de `SpeechOnsetDetector`. Une
/// pause entre deux mots, plus courte, ne compte pas.
struct SpeechEndDetector {
    static let silence = 60

    private var quiet = 0

    /// Vrai dès que le silence dure assez (puis à chaque tranche, tant qu'il
    /// dure).
    mutating func push(_ columns: [[Float]]) -> Bool {
        for column in columns {
            quiet = (column.max() ?? 0) >= SpeechOnsetDetector.threshold ? 0 : quiet + 1
        }
        return quiet >= Self.silence
    }
}

/// La mémoire que le modèle occupe une fois chargé.
struct TrialMemory: Equatable {
    var bytes: Int
    /// Mesurée (la mémoire active de MLX a grandi au chargement), sinon
    /// estimée (le poids des fichiers du modèle).
    var measured: Bool

    /// Une croissance de moins de 16 Mo ne dit rien (le modèle était déjà
    /// chargé) : l'estimation la remplace.
    static func reading(before: Int?, after: Int?, estimate: Int) -> TrialMemory? {
        if let before, let after, after - before >= 16 << 20 {
            return TrialMemory(bytes: after - before, measured: true)
        }
        return estimate > 0 ? TrialMemory(bytes: estimate, measured: false) : nil
    }
}

/// Les phrases à lire, courtes et sans piège de nombre ; une autre à chaque essai.
enum TrialSentences {
    static let french = [
        "Le petit chat dort au soleil près de la fenêtre.",
        "Nous partirons demain matin vers la mer avec nos amis.",
        "Elle lit un livre passionnant chaque soir avant de dormir.",
        "Le boulanger du village prépare le pain très tôt.",
        "Il pleut depuis ce matin, alors je reste à la maison.",
    ]
    static let english = [
        "The little cat is sleeping in the sun by the window.",
        "We will leave for the sea tomorrow morning with our friends.",
        "She reads an exciting book every evening before bed.",
        "The village baker makes fresh bread very early.",
        "It has been raining since this morning, so I am staying home.",
    ]

    /// `language` : « fr », « en » ; une autre langue, le français.
    static func list(_ language: String) -> [String] {
        language.hasPrefix("en") ? english : french
    }

    static func sentence(_ language: String, trial: Int) -> String {
        let list = list(language)
        return list[((trial % list.count) + list.count) % list.count]
    }
}

/// Le dernier essai d'un modèle.
struct TrialResult: Codable, Equatable {
    var date: Date
    /// « fr », « en ».
    var language: String
    /// Texte libre : pas de phrase, pas de précision.
    var freeText: Bool
    /// 0 à 100.
    var precision: Double?
    /// Secondes jusqu'au premier texte.
    var firstDelay: Double?
    /// Depuis le début de la parole (sinon l'ouverture du micro).
    var firstDelayFromSpeech: Bool
    /// Secondes de « Arrêter » au texte final.
    var finalDelay: Double?
    /// × temps réel ; `nil` pour la dictée de macOS.
    var speed: Double?
    var memoryBytes: Int?
    var memoryMeasured: Bool
}

/// Les derniers résultats, un par modèle (`DictationModel` : « system » ou
/// l'id du paquet), dans les préférences.
struct TrialResultsStore {
    static let key = "micTrialResults"
    var defaults: UserDefaults = .standard

    func all() -> [String: TrialResult] {
        guard let data = defaults.data(forKey: Self.key),
              let results = try? JSONDecoder().decode([String: TrialResult].self, from: data)
        else { return [:] }
        return results
    }

    func save(_ result: TrialResult, for id: String) {
        var results = all()
        results[id] = result
        write(results)
    }

    /// Le modèle est supprimé : ses résultats aussi.
    func clear(_ id: String) {
        var results = all()
        guard results.removeValue(forKey: id) != nil else { return }
        write(results)
    }

    private func write(_ results: [String: TrialResult]) {
        guard let data = try? JSONEncoder().encode(results) else { return }
        defaults.set(data, forKey: Self.key)
    }
}

/// Les résultats écrits à la française, à l'écran et pour VoiceOver.
enum TrialFormat {
    struct Values: Equatable {
        var precision, delay, speed, memory: String
    }

    static let none = "—"

    static func percent(_ v: Double) -> String { "\(Int(v.rounded())) %" }
    static func seconds(_ v: Double) -> String { "\(decimal(v)) s" }
    static func speed(_ v: Double) -> String { "\(decimal(v))×" }

    /// Un chiffre après la virgule.
    static func decimal(_ v: Double) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.minimumFractionDigits = 1
        f.maximumFractionDigits = 1
        return f.string(from: NSNumber(value: v)) ?? "\(v)"
    }

    /// Délai : « premier texte puis texte final » (« 0,8 s puis 0,3 s »).
    static func values(_ r: TrialResult, size: (UInt64) -> String) -> Values {
        let delay: String = switch (r.firstDelay, r.finalDelay) {
        case let (first?, final?): "\(seconds(first)) puis \(seconds(final))"
        case let (first?, nil): seconds(first)
        case let (nil, final?): "\(none) puis \(seconds(final))"
        case (nil, nil): none
        }
        return Values(precision: r.precision.map(percent) ?? none,
                      delay: delay,
                      speed: r.speed.map(speed) ?? none,
                      memory: r.memoryBytes.map { size(UInt64(max(0, $0))) } ?? none)
    }

    /// Ce que VoiceOver dit à la fin d'un essai, en phrases entières.
    static func announcement(_ r: TrialResult, model: String, size: (UInt64) -> String) -> String {
        var parts = ["Essai de \(model) terminé."]
        if let precision = r.precision {
            parts.append("Précision : \(percent(precision)).")
        } else if r.freeText {
            parts.append("Texte libre : pas de précision.")
        }
        if let first = r.firstDelay {
            let basis = r.firstDelayFromSpeech ? "le début de la parole" : "l'ouverture du micro"
            parts.append("Premier texte \(seconds(first)) après \(basis).")
        } else {
            parts.append("Aucun texte provisoire.")
        }
        if let final = r.finalDelay { parts.append("Texte final \(seconds(final)) après l'arrêt.") }
        if let s = r.speed {
            parts.append("Débit pendant la dictée, relectures comprises : \(decimal(s)) fois le temps réel.")
        }
        if let bytes = r.memoryBytes {
            parts.append("Mémoire : \(size(UInt64(max(0, bytes))))\(r.memoryMeasured ? "" : " environ").")
        }
        return parts.joined(separator: " ")
    }
}
