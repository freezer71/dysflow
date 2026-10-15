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

    // MARK: - Passage à Kokoro, configuration

    /// Le passage unique à Kokoro : appelé une fois au lancement. Rend
    /// l'avis à montrer (passage fait).
    func migrateVoice() async -> String? {
        let notice = await call { $0.migrateVoice() }
        wake()
        return notice
    }
    /// « Remplacer » un `config.toml` illisible (une copie est gardée à
    /// côté), hors du fil principal.
    func replaceUnreadableConfig() async throws {
        try await tryCall { try $0.replaceUnreadableConfig() }
        wake()
    }
}
