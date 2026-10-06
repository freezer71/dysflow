@testable import Sillon

/// Un catalogue pour les tests : il note ce qu'on lui demande.
@MainActor
final class FakeModelsBackend: ModelsBackend {
    var catalogue: [VoiceModel] = []
    var free: UInt64? = 10_000_000_000
    private(set) var loads = 0
    private(set) var removed: [String] = []
    private(set) var used: [String] = []
    var failRemoval: String?

    func loadVoices() async -> [VoiceModel] { loads += 1; return catalogue }
    func installVoiceAsync(_ id: String) async throws {}
    func cancelDownloadAsync() async throws {}
    func removeModelAsync(_ id: String) async throws {
        if let failRemoval { throw SillonError.Failed(message: failRemoval) }
        removed.append(id)
        catalogue.removeAll { $0.id == id }
    }
    func useVoiceAsync(_ id: String) async throws {
        used.append(id)
        for i in catalogue.indices { catalogue[i].active = catalogue[i].id == id }
    }
    func freeSpace() async -> UInt64? { free }

    /// Un téléchargement fini qui active le modèle, comme le moteur le fait.
    func finishDownloadActivating(_ id: String) {
        for i in catalogue.indices { catalogue[i].active = catalogue[i].id == id }
    }
}
