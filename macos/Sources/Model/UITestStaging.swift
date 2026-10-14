#if DEBUG
/// Pour les tests d'interface seulement : le lecteur muet n'a ni offre, ni
/// avis, ni configuration illisible. Ces arguments les posent dans
/// l'instantané publié, pour que les tests les atteignent au clavier et les
/// passent à l'audit d'accessibilité. Compilé en Debug seulement : l'app
/// livrée (Release) n'en contient rien.
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
            languages: ["fr", "en"], honoured: ["voice", "language"], requiresReference: false)
    }

    static func staging(for arguments: [String]) -> (@Sendable (inout Snapshot) -> Void)? {
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

    /// Le lecteur donné, avec la mise en scène demandée par les arguments ;
    /// le même lecteur s'il n'y a rien à poser. Le modèle n'en sait rien :
    /// il lit un lecteur comme un autre.
    static func reader(_ base: SillonReader, arguments: [String]) -> SillonReader {
        let stage = staging(for: arguments)
        let capabilities = capabilities(for: arguments)
        guard stage != nil || capabilities != nil else { return base }
        return StagedReader(base, stage: stage, capabilities: capabilities)
    }
}

/// Un lecteur qui partage le moteur d'un autre (même objet Rust) et pose la
/// mise en scène sur ce qu'il en rend.
final class StagedReader: SillonReader, @unchecked Sendable {
    private let stage: (@Sendable (inout Snapshot) -> Void)?
    private let stagedCapabilities: ModelCapabilities?

    init(_ base: SillonReader, stage: (@Sendable (inout Snapshot) -> Void)?,
         capabilities: ModelCapabilities?) {
        self.stage = stage
        stagedCapabilities = capabilities
        // Une référence de plus sur le même lecteur Rust (Arc cloné).
        super.init(unsafeFromHandle: base.uniffiCloneHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        stage = nil
        stagedCapabilities = nil
        super.init(unsafeFromHandle: handle)
    }

    override func snapshot() -> Snapshot {
        var s = super.snapshot()
        stage?(&s)
        return s
    }

    override func capabilities() -> ModelCapabilities {
        stagedCapabilities ?? super.capabilities()
    }
}
#endif
