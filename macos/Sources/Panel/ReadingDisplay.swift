/// Ce que le panneau lit des réglages du moteur, relu à chaque ouverture et
/// à chaque réglage écrit.
struct ReadingDisplay: Equatable {
    var lineSpacing: UInt32 = 1
    var readingRuler = true

    init() {}

    init(_ s: SettingsView) {
        lineSpacing = s.lineSpacing
        readingRuler = s.readingRuler
    }
}
