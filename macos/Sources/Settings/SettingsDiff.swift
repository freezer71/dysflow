/// Le patch qui mène d'une valeur des réglages à une autre : seuls les
/// champs changés y figurent. C'est ce qui garantit qu'une valeur écrite à
/// la main, ou changée au clavier entre-temps, n'est pas réécrite.
enum SettingsDiff {
    static func patch(from old: SettingsView, to new: SettingsView) -> SettingsPatch {
        var p = SettingsPatch.empty
        if new.voice != old.voice { p.voice = new.voice }
        if new.language != old.language { p.language = new.language }
        if new.speed != old.speed { p.speed = new.speed }
        if new.pitch != old.pitch { p.pitch = new.pitch }
        if new.energy != old.energy { p.energy = new.energy }
        if new.emotion != old.emotion { p.emotion = new.emotion }
        if new.instruction != old.instruction { p.instruction = new.instruction }
        if new.volume != old.volume { p.volume = new.volume }
        if new.engine != old.engine { p.engine = new.engine }
        if new.compute != old.compute { p.compute = new.compute }
        if new.threads != old.threads { p.threads = new.threads }
        if new.sessions != old.sessions { p.sessions = new.sessions }
        if new.prefetch != old.prefetch { p.prefetch = new.prefetch }
        if new.temperature != old.temperature { p.temperature = float(new.temperature) }
        if new.topP != old.topP { p.topP = float(new.topP) }
        if new.topK != old.topK { p.topK = int(new.topK) }
        if new.guidanceScale != old.guidanceScale { p.guidanceScale = float(new.guidanceScale) }
        if new.repetitionPenalty != old.repetitionPenalty {
            p.repetitionPenalty = float(new.repetitionPenalty)
        }
        if new.inferenceSteps != old.inferenceSteps { p.inferenceSteps = int(new.inferenceSteps) }
        if new.seed != old.seed { p.seed = new.seed.map { .value(value: $0) } ?? .random }
        if new.codeBlocks != old.codeBlocks { p.codeBlocks = new.codeBlocks }
        if new.autoplay != old.autoplay { p.autoplay = new.autoplay }
        if new.seekStep != old.seekStep { p.seekStep = new.seekStep }
        if new.lineWidth != old.lineWidth { p.lineWidth = new.lineWidth }
        if new.lineSpacing != old.lineSpacing { p.lineSpacing = new.lineSpacing }
        if new.readingRuler != old.readingRuler { p.readingRuler = new.readingRuler }
        if new.keepVoiceMinutes != old.keepVoiceMinutes { p.keepVoiceMinutes = new.keepVoiceMinutes }
        let before = Dictionary(old.extra.map { ($0.key, $0.value) }, uniquingKeysWith: { $1 })
        let after = Dictionary(new.extra.map { ($0.key, $0.value) }, uniquingKeysWith: { $1 })
        p.extraSet = new.extra.filter { before[$0.key] != $0.value }
        p.extraRemove = old.extra.map(\.key).filter { after[$0] == nil }
        // Une voix par langue : la première langue changée part (un geste
        // n'en touche qu'une).
        let voicesBefore = Dictionary(old.voicesByLanguage.map { ($0.language, $0.voice) },
                                      uniquingKeysWith: { $1 })
        if let changed = new.voicesByLanguage.first(where: { voicesBefore[$0.language] != $0.voice }) {
            p.voiceForLanguage = changed
        }
        return p
    }

    private static func float(_ v: Float?) -> SamplingFloat { v.map { .value(value: $0) } ?? .model }
    private static func int(_ v: UInt32?) -> SamplingInt { v.map { .value(value: $0) } ?? .model }
}
