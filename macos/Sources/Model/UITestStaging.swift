/// Pour les tests d'interface seulement : le lecteur muet n'a ni offre, ni
/// avis, ni configuration illisible. Ces arguments les posent dans
/// l'instantané publié, pour que les tests les atteignent au clavier et les
/// passent à l'audit d'accessibilité.
enum UITestStaging {
    static let offer = "--ui-testing-offre"
    static let notice = "--ui-testing-avis"
    static let unreadableConfig = "--ui-testing-config-illisible"

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
                s.notice = "Nouvelle voix : Kokoro remplace F2. L'ancien modèle reste installé : "
                    + "Réglages › Voix pour y revenir."
            }
            if unreadable { s.configUnreadable = true }
        }
    }
}
