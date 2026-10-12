import Foundation

/// La première synthèse après une installation ou une mise à jour compile les
/// noyaux Metal (≈ 9 s), gardés ensuite dans le cache du système. L'app la
/// fait hors du fil principal, une fois par version, dès que Kokoro devient la
/// voix active (lancement, choix de la voix, fin du téléchargement accepté),
/// et libère le modèle aussitôt : les 9 s ne tombent jamais sur le premier
/// mot lu.
@MainActor
enum KokoroWarmUp {
    static let versionKey = "kokoroPreparedVersion"
    /// Vrai pendant la préparation : l'onglet Voix dit « Voix en préparation
    /// (première fois)… ».
    private(set) static var isRunning = false

    /// La version de l'app pour la préparation : numéros du paquet et date
    /// de l'exécutable. Le projet ne pose pas `CFBundleVersion` : sans la
    /// date, une mise à jour garderait la même clé et ne préparerait rien.
    nonisolated static func version(of bundle: Bundle) -> String {
        let info = bundle.infoDictionary ?? [:]
        let short = info["CFBundleShortVersionString"] as? String ?? "?"
        let build = info["CFBundleVersion"] as? String ?? "?"
        let built = bundle.executableURL
            .flatMap { try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate }
            .map { String(Int($0.timeIntervalSince1970)) } ?? "?"
        return "\(short)(\(build))-\(built)"
    }

    /// Le dossier du paquet MLX actif ; aucun pour un autre modèle.
    static func modelDirectory(settings: SettingsView) -> String? {
        guard settings.family == "kokoro", settings.modelPath.hasSuffix(".safetensors") else { return nil }
        return URL(fileURLWithPath: settings.modelPath).deletingLastPathComponent().path
    }

    @discardableResult
    static func runIfNeeded(compute: KokoroCompute, modelDirectory: String?, version: String,
                            defaults: UserDefaults = .standard) -> Task<Void, Never>? {
        guard let directory = modelDirectory, !isRunning,
              defaults.string(forKey: versionKey) != version else { return nil }
        isRunning = true
        // `.utility` et non `.background` : le lecteur peut attendre le verrou
        // MLX derrière elle, et une préparation bridée le ferait attendre.
        let work = Task.detached(priority: .utility) { () -> Bool in
            do {
                try compute.load(modelDir: directory)
                defer { compute.release() }
                try compute.warmUp()
                return true
            } catch {
                return false
            }
        }
        return Task { @MainActor in
            let prepared = await work.value
            isRunning = false
            if prepared { defaults.set(version, forKey: versionKey) }
        }
    }
}
