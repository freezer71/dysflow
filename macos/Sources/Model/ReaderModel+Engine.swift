import Foundation

/// Les appels au moteur que les réglages, les signets et les voix font par
/// le modèle. Ils passent par le fil du lecteur et bloquent jusqu'à 5 s :
/// tous hors du fil principal (`call`, `tryCall`).
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

    // MARK: - Signets

    /// Les signets du document, lus hors du fil principal.
    func loadBookmarks() async -> [BookmarkView] {
        await call { $0.bookmarks() }
    }
    /// Tous les signets, tous documents confondus.
    func loadAllBookmarks() async -> [StoredBookmarkView] {
        await call { $0.allBookmarks() }
    }
    func removeBookmarkAsync(document: String, sentence: UInt32) async throws {
        try await tryCall { try $0.removeBookmark(document: document, sentence: sentence) }
    }

    // MARK: - Voix et modèles

    /// Voix, langues et réglages honorés par le modèle actif.
    func loadCapabilities() async -> ModelCapabilities {
        await call { $0.capabilities() }
    }
    /// Les voix du catalogue, lues hors du fil principal.
    func loadVoices() async -> [VoiceModel] {
        await call { $0.voices() }
    }
    func installVoiceAsync(_ id: String) async throws {
        try await tryCall { try $0.installVoice(id: id) }
        wake()
    }
    /// Change de modèle : la voix se recharge, le suivi montre le chargement.
    func useVoiceAsync(_ id: String) async throws {
        try await tryCall { try $0.useVoice(id: id) }
        wake()
        onVoiceMayHaveChanged?()
    }
    func cancelDownloadAsync() async throws {
        try await tryCall { try $0.cancelDownload() }
        wake()
    }
    func removeModelAsync(_ id: String) async throws {
        try await tryCall { try $0.removeModel(id: id) }
    }
    func freeSpace() async -> UInt64? {
        await call { $0.freeSpace() }
    }
    func stopPreview() async {
        await call { $0.stopPreview() }
        wake()
    }
    /// La langue du document gardé (`nil` : automatique), hors du fil principal.
    func setDocumentLanguage(_ language: String?) async throws {
        try await tryCall { try $0.setDocumentLanguage(language: language) }
        wake()
    }
    /// « Écouter » une voix dans sa langue.
    func previewVoice(in language: String) async throws {
        try await tryCall { try $0.previewVoiceIn(language: language) }
        wake()
    }

    // MARK: - Passage à Kokoro, avis

    /// Le passage unique à Kokoro : appelé une fois au lancement. Rend
    /// l'avis à montrer (passage fait, ou proposition dans `snapshot.offer`).
    func migrateVoice() async -> String? {
        let notice = await call { $0.migrateVoice() }
        wake()
        return notice
    }
    /// « Passer à Kokoro » : le téléchargement démarre, hors du fil principal.
    func acceptVoiceOffer() async throws {
        try await tryCall { try $0.acceptVoiceOffer() }
        wake()
        onVoiceMayHaveChanged?()
    }
    /// « Plus tard » : le refus est retenu.
    /// Le refus s'écrit dans `config.toml` : hors du fil principal.
    func dismissVoiceOffer() async {
        await call { $0.dismissVoiceOffer() }
        wake()
    }
    /// « Remplacer » un `config.toml` illisible (une copie est gardée à
    /// côté), hors du fil principal.
    func replaceUnreadableConfig() async throws {
        try await tryCall { try $0.replaceUnreadableConfig() }
        wake()
    }
    /// « Compris » : l'appel attend le verrou du lecteur, hors du fil
    /// principal comme les autres réponses à l'offre.
    func dismissNotice() async {
        await call { $0.dismissNotice() }
        wake()
    }

    // MARK: - Fichiers

    /// Vide le cache des documents ; rend les octets libérés.
    func clearDocumentCache() async throws -> UInt64 {
        try await tryCall { try $0.clearDocumentCache() }
    }
    /// Le dossier de `config.toml` (lecture directe, sans le fil du lecteur).
    var configDirectory: String? { reader.configDir() }
}
