/// Pour les tests d'interface seulement : le lecteur muet n'a ni offre, ni
/// avis, ni configuration illisible. Ces arguments les posent dans
/// l'instantané publié, pour que les tests les atteignent au clavier et les
/// passent à l'audit d'accessibilité.
enum UITestStaging {
    static let offer = "--ui-testing-offre"
    static let notice = "--ui-testing-avis"
    static let unreadableConfig = "--ui-testing-config-illisible"
    /// Les voix de Kokoro dans l'onglet Voix (le lecteur muet n'en a pas).
    static let kokoro = "--ui-testing-kokoro"

    /// Les capacités de Kokoro telles que le moteur les donne
    /// (`models::voices_for_family`), sans modèle chargé.
    static func capabilities(for arguments: [String]) -> ModelCapabilities? {
        guard arguments.contains(kokoro) else { return nil }
        return ModelCapabilities(
            family: "kokoro",
            voices: [VoiceChoice(id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr"),
                     VoiceChoice(id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en"),
                     VoiceChoice(id: "am_fenrir", label: "am_fenrir · américaine, homme", language: "en"),
                     VoiceChoice(id: "af_heart", label: "af_heart · américaine, femme", language: "en")],
            languages: ["fr", "en"], honoured: ["voice", "language"])
    }

    static func staging(for arguments: [String]) -> ((inout Snapshot) -> Void)? {
        let offer = arguments.contains(Self.offer)
        let notice = arguments.contains(Self.notice)
        let unreadable = arguments.contains(unreadableConfig)
        guard offer || notice || unreadable else { return nil }
        return { s in
            if offer {
                s.offer = VoiceOfferView(id: "kokoro_82m_mlx", bytes: 342_264_783, former: "F2", message: "")
            }
            if notice {
                s.notice = "Nouvelle voix : Kokoro remplace F2."
            }
            if unreadable { s.configUnreadable = true }
        }
    }
}
