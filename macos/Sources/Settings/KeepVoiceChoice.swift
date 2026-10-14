/// Combien de temps la voix reste chargée après la dernière lecture.
enum KeepVoiceChoice: Hashable {
    case end
    case minutes(Int64)
    case always

    static let steps: [Int64] = [2, 5, 10, 30]
    static var allCases: [KeepVoiceChoice] { [.end] + steps.map { .minutes($0) } + [.always] }

    /// Une durée hors liste (écrite à la main) reste telle quelle.
    init(minutes: Int64) {
        switch minutes {
        case ..<0: self = .always
        case 0: self = .end
        default: self = .minutes(minutes)
        }
    }

    var minutes: Int64 {
        switch self {
        case .end: 0
        case .minutes(let m): m
        case .always: -1
        }
    }

    var label: String {
        switch self {
        case .end: "Relâchée dès l'arrêt"
        case .minutes(let m): m == 1 ? "1 minute" : "\(m) minutes"
        case .always: "Toujours chargée"
        }
    }

    /// Les choix du menu, plus la durée en place si elle n'en fait pas
    /// partie, rangée à sa place.
    static func options(stored: Int64) -> [KeepVoiceChoice] {
        let current = KeepVoiceChoice(minutes: stored)
        guard !allCases.contains(current) else { return allCases }
        return [.end] + (steps + [stored]).sorted().map { .minutes($0) } + [.always]
    }
}
