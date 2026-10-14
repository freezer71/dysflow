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
    var failCancel: String?
    var failUse: String?
    var failInstall: String?

    func loadVoices() async -> [VoiceModel] { loads += 1; return catalogue }
    func installVoiceAsync(_ id: String) async throws {
        if let failInstall { throw SillonError.Failed(message: failInstall) }
    }
    func cancelDownloadAsync() async throws {
        if let failCancel { throw SillonError.Failed(message: failCancel) }
    }
    func removeModelAsync(_ id: String) async throws {
        if let failRemoval { throw SillonError.Failed(message: failRemoval) }
        removed.append(id)
        catalogue.removeAll { $0.id == id }
    }
    func useVoiceAsync(_ id: String) async throws {
        if let failUse { throw SillonError.Failed(message: failUse) }
        used.append(id)
        for i in catalogue.indices { catalogue[i].active = catalogue[i].id == id }
    }
    func freeSpace() async -> UInt64? { free }

    /// Un téléchargement fini qui active le modèle, comme le moteur le fait.
    func finishDownloadActivating(_ id: String) {
        for i in catalogue.indices { catalogue[i].active = catalogue[i].id == id }
    }
}
