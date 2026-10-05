/// Combien de temps la voix reste chargée après la dernière lecture.
enum KeepVoiceChoice: Hashable {
    case end
    case minutes(Int64)
    case always

    static let steps: [Int64] = [2, 5, 10, 30]
    static var allCases: [KeepVoiceChoice] { [.end] + steps.map { .minutes($0) } + [.always] }

    init(minutes: Int64) {
        switch minutes {
        case ..<0: self = .always
        case 0: self = .end
        default:
            let nearest = Self.steps.min { abs($0 - minutes) < abs($1 - minutes) } ?? 10
            self = .minutes(nearest)
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
        case .minutes(let m): "\(m) minutes"
        case .always: "Toujours chargée"
        }
    }
}
