/// Les réglages qu'un modèle peut ignorer, sous le nom que leur donne le
/// moteur (`models::honoured_options`).
enum SettingKey: String, CaseIterable {
    case voice, language
    case speed = "speaking_rate"
    case pitch = "pitch_shift"
    case energy, emotion, instruction, temperature
    case topP = "top_p"
    case topK = "top_k"
    case guidance = "guidance_scale"
    case repetition = "repetition_penalty"
    case steps = "num_inference_steps"
    case seed

    /// Une liste vide : famille inconnue, rien n'est affirmé, tout compte.
    func isIgnored(honoured: [String]) -> Bool {
        !honoured.isEmpty && !honoured.contains(rawValue)
    }
}
