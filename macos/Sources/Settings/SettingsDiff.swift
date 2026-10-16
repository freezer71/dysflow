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
        if new.volume != old.volume { p.volume = new.volume }
        if new.engine != old.engine { p.engine = new.engine }
        if new.threads != old.threads { p.threads = new.threads }
        if new.sessions != old.sessions { p.sessions = new.sessions }
        if new.prefetch != old.prefetch { p.prefetch = new.prefetch }
        if new.codeBlocks != old.codeBlocks { p.codeBlocks = new.codeBlocks }
        if new.autoplay != old.autoplay { p.autoplay = new.autoplay }
        if new.seekStep != old.seekStep { p.seekStep = new.seekStep }
        if new.lineWidth != old.lineWidth { p.lineWidth = new.lineWidth }
        if new.lineSpacing != old.lineSpacing { p.lineSpacing = new.lineSpacing }
        if new.readingRuler != old.readingRuler { p.readingRuler = new.readingRuler }
        if new.readingFont != old.readingFont { p.readingFont = new.readingFont }
        if new.readingSize != old.readingSize { p.readingSize = new.readingSize }
        if new.keepVoiceMinutes != old.keepVoiceMinutes { p.keepVoiceMinutes = new.keepVoiceMinutes }
        if new.outputDevice != old.outputDevice { p.outputDevice = new.outputDevice }
        // Une voix par langue et par patch : la première langue changée part
        // avec les autres champs, les suivantes dans `patches`.
        // Plusieurs langues qui prennent la même voix : un seul champ, écrit
        // d'un coup par le moteur.
        if let family = sharedVoice(from: old, to: new) {
            p.familyVoice = family
        } else {
            p.voiceForLanguage = changedVoices(from: old, to: new).first
        }
        return p
    }

    /// Tous les patchs à envoyer, dans l'ordre : `patch(from:to:)`, puis un
    /// patch par langue changée en plus — deux voix changées dans la même
    /// pause sont écrites toutes les deux. Vide si rien n'a changé.
    static func patches(from old: SettingsView, to new: SettingsView) -> [SettingsPatch] {
        let first = patch(from: old, to: new)
        if first.isEmpty { return [] }
        if sharedVoice(from: old, to: new) != nil { return [first] }
        let others = changedVoices(from: old, to: new).dropFirst().map { voice -> SettingsPatch in
            var p = SettingsPatch.empty
            p.voiceForLanguage = voice
            return p
        }
        return [first] + others
    }

    /// La voix commune à toutes les langues changées, s'il y en a au moins
    /// deux et qu'elles prennent la même.
    private static func sharedVoice(from old: SettingsView, to new: SettingsView) -> String? {
        let changed = changedVoices(from: old, to: new)
        guard changed.count >= 2, let voice = changed.first?.voice,
              changed.allSatisfy({ $0.voice == voice }) else { return nil }
        return voice
    }

    private static func changedVoices(from old: SettingsView, to new: SettingsView) -> [LanguageVoice] {
        let voicesBefore = Dictionary(old.voicesByLanguage.map { ($0.language, $0.voice) },
                                      uniquingKeysWith: { $1 })
        return new.voicesByLanguage.filter { voicesBefore[$0.language] != $0.voice }
    }
}
