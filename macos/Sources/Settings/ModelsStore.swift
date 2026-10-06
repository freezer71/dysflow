import Foundation

/// Ce que l'onglet Modèles demande au moteur.
@MainActor
protocol ModelsBackend: AnyObject {
    func loadVoices() async -> [VoiceModel]
    func installVoiceAsync(_ id: String) async throws
    func cancelDownloadAsync() async throws
    func removeModelAsync(_ id: String) async throws
    func useVoiceAsync(_ id: String) async throws
    func freeSpace() async -> UInt64?
}

extension ReaderModel: ModelsBackend {}

/// Ce que l'onglet répond à une demande de suppression.
enum DeletionRequest: Equatable {
    /// Une alerte, avant d'effacer des centaines de mégaoctets.
    case confirm(title: String, message: String)
    /// Le modèle actif : on en choisit un autre d'abord.
    case refuse(String)
}

/// Le catalogue et ce qu'on y fait. Chaque action relit le catalogue, qu'elle
/// réussisse ou non : l'écran dit l'état réel du disque.
@MainActor
@Observable
final class ModelsStore {
    @ObservationIgnored private let backend: ModelsBackend
    private(set) var catalogue: [VoiceModel] = []
    private(set) var freeSpace: UInt64?
    private(set) var error: String?
    /// Le modèle dont la suppression attend confirmation.
    var pendingDeletion: VoiceModel?
    /// Une annulation est demandée ; le transfert la voit à son prochain bloc.
    private(set) var cancelling = false

    /// Prévenu quand le modèle actif change (« Utiliser », téléchargement qui
    /// l'active) : les réglages relisent voix, langues et filtres.
    @ObservationIgnored private let onActiveModelChanged: () async -> Void

    init(backend: ModelsBackend, onActiveModelChanged: @escaping () async -> Void = {}) {
        self.backend = backend
        self.onActiveModelChanged = onActiveModelChanged
    }

    func reload() async {
        catalogue = await backend.loadVoices()
        freeSpace = await backend.freeSpace()
    }

    static func deletionRequest(for model: VoiceModel) -> DeletionRequest {
        if model.active {
            return .refuse("\(model.name) est le modèle actif : choisissez-en un autre avant de le supprimer.")
        }
        let size = ByteCountFormatter.string(fromByteCount: Int64(model.bytes), countStyle: .file)
        return .confirm(title: "Supprimer \(model.name) ?",
                        message: "\(size) seront libérés. Il faudra le télécharger à nouveau pour l'utiliser.")
    }

    func askDeletion(_ model: VoiceModel) {
        switch Self.deletionRequest(for: model) {
        case .refuse(let reason): error = reason
        case .confirm: pendingDeletion = model
        }
    }

    /// Supprime après confirmation. Le modèle est passé par l'alerte : sa
    /// fermeture remet `pendingDeletion` à nil avant que la tâche ne parte.
    func delete(_ model: VoiceModel) async {
        pendingDeletion = nil
        await run { try await self.backend.removeModelAsync(model.id) }
    }

    func install(_ id: String) async { await run { try await self.backend.installVoiceAsync(id) } }

    func use(_ id: String) async {
        await run { try await self.backend.useVoiceAsync(id) }
        await onActiveModelChanged()
    }

    func cancelDownload() async {
        cancelling = true
        await run { try await self.backend.cancelDownloadAsync() }
    }

    /// Un téléchargement terminé, échoué ou annulé : on relit le catalogue.
    func downloadSettled() async {
        cancelling = false
        let before = catalogue.first(where: \.active)?.id
        await reload()
        // Un modèle fraîchement installé s'active seul s'il est le premier.
        if catalogue.first(where: \.active)?.id != before { await onActiveModelChanged() }
    }

    private func run(_ action: () async throws -> Void) async {
        do {
            try await action()
            error = nil
        } catch {
            self.error = SettingsStore.message(of: error)
        }
        await reload()
    }
}
