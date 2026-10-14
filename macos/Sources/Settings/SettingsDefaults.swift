/// Les défauts du moteur (`Config::default`), demandés au moteur lui-même
/// par le pont : ce qu'un onglet montre avant le premier chargement, et la
/// base des tests. Aucune valeur n'est recopiée ici.
extension SettingsView {
    static let defaults = defaultSettings()
}

extension SettingsPatch {
    /// Un patch qui ne touche rien : on n'y pose que ce qui change.
    static var empty: SettingsPatch {
        SettingsPatch(
            voice: nil, language: nil, speed: nil, pitch: nil, energy: nil, emotion: nil,
            instruction: nil, volume: nil, engine: nil, compute: nil, threads: nil, sessions: nil,
            prefetch: nil, temperature: nil, topP: nil, topK: nil, guidanceScale: nil,
            repetitionPenalty: nil, inferenceSteps: nil, seed: nil, codeBlocks: nil,
            autoplay: nil, seekStep: nil, lineWidth: nil, lineSpacing: nil, readingRuler: nil,
            keepVoiceMinutes: nil, outputDevice: nil, extraSet: [], extraRemove: [], voiceForLanguage: nil)
    }

    var isEmpty: Bool { self == .empty }
}
