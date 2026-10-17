import Foundation

/// Les appels au moteur que les réglages et la voix font par le modèle.
/// Ils passent par le fil du lecteur et bloquent jusqu'à 5 s : tous hors du
/// fil principal (`call`, `tryCall`).
extension ReaderModel {
    /// Un appel au moteur, hors du fil principal.
    func call<T: Sendable>(_ body: @escaping @Sendable (SillonReader) -> T) async -> T {
        let reader = reader
        return await Task.detached { body(reader) }.value
    }

    /// Un appel au moteur qui peut échouer, hors du fil principal.
    func tryCall<T: Sendable>(_ body: @escaping @Sendable (SillonReader) throws -> T) async throws -> T {
        let reader = reader
        return try await Task.detached { try body(reader) }.value
    }

    /// Le dossier d'un paquet installé (un modèle de dictée), `nil` s'il
    /// manque. Appel court : il ne passe pas par le fil du lecteur.
    func modelDirectory(_ id: String) -> URL? {
        reader.modelDirectory(id: id).map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    // MARK: - Voix

    /// Voix, langues et réglages honorés par le modèle actif.
    func loadCapabilities() async -> ModelCapabilities {
        await call { $0.capabilities() }
    }
    func stopPreview() async {
        await call { $0.stopPreview() }
        wake()
    }
    /// « Écouter » une voix dans sa langue.
    func previewVoice(in language: String) async throws {
        try await tryCall { try $0.previewVoiceIn(language: language) }
        wake()
    }

    // MARK: - Bibliothèque des voix

    /// Toutes les voix des modèles de lecture installés, et celles de macOS.
    func loadVoiceLibrary() async -> [LibraryVoice] {
        let locale = Locale.current.identifier
        return await call { $0.voiceLibrary(systemLocale: locale) }
    }

    /// Fait entendre `text` avec une voix de la bibliothèque, à `speed` pour
    /// l'aperçu seulement (un autre modèle se charge pour lui).
    func previewLibraryVoice(model: String, voice: String, text: String, speed: Float) async throws {
        try await tryCall { try $0.previewVoiceWith(modelId: model, voice: voice, text: text, speed: speed) }
        wake()
    }

    /// La bibliothèque se ferme : le modèle chargé pour l'aperçu est relâché.
    func releaseVoicePreview() async {
        await call { $0.releaseVoicePreview() }
        wake()
    }

    // MARK: - Modèles

    /// Les lignes de la table : la voix de macOS, puis le catalogue.
    func loadModels() async -> [VoiceModel] { await call { $0.voices() } }

    func installModel(_ id: String) async throws {
        try await tryCall { try $0.installVoice(id: id) }
        wake()
    }

    func useModel(_ id: String) async throws {
        try await tryCall { try $0.useVoice(id: id) }
        wake()
        onVoiceMayHaveChanged?()
    }

    func removeModel(_ id: String) async throws {
        try await tryCall { try $0.removeModel(id: id) }
        wake()
        onVoiceMayHaveChanged?()
    }

    func cancelDownload() async throws {
        try await tryCall { try $0.cancelDownload() }
        wake()
    }

    func modelsDiskUsage() async -> UInt64 { await call { $0.modelsDiskUsage() } }
    func freeSpace() async -> UInt64? { await call { $0.freeSpace() } }

    // MARK: - Passage à Kokoro, configuration

    /// Le passage unique à Kokoro : appelé une fois au lancement. Rend
    /// l'avis à montrer (passage fait).
    func migrateVoice() async -> String? {
        let notice = await call { $0.migrateVoice() }
        wake()
        return notice
    }
    /// L'installation automatique de la voix (plan 1) : accepte l'offre de
    /// Kokoro, ou télécharge le Kokoro recommandé, sans rien demander ;
    /// sans effet pendant un téléchargement. La progression : `snapshot.download`.
    func ensureDefaultVoice() async throws {
        try await tryCall { try $0.ensureDefaultVoice() }
        wake()
        onVoiceMayHaveChanged?()
    }
    /// « Remplacer » un `config.toml` illisible (une copie est gardée à
    /// côté), hors du fil principal.
    func replaceUnreadableConfig() async throws {
        try await tryCall { try $0.replaceUnreadableConfig() }
        wake()
    }
}
