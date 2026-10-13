/// Les réglages de l'onglet Avancé qu'un modèle peut ignorer, sous le nom
/// que leur donne le moteur (`models::honoured_options`). Ni la vitesse ni
/// le volume : ils s'appliquent au son, quelle que soit la voix. Voix,
/// langue, hauteur, énergie, intonation et consigne n'ont plus de contrôle
/// dans l'app (14 oct. 2026) : leurs clés restent lues dans `config.toml`.
enum SettingKey: String, CaseIterable {
    case temperature
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
