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

    /// Un téléchargement tourne-t-il encore ? Sert à ne pas laisser
    /// « Annulation… » collé quand aucune fin de transfert ne suivra.
    @ObservationIgnored private let isDownloading: () -> Bool
    /// Dit un refus à VoiceOver, chaque fois, même s'il est identique au précédent.
    @ObservationIgnored private let announce: (String) -> Void

    init(backend: ModelsBackend,
         onActiveModelChanged: @escaping () async -> Void = {},
         isDownloading: @escaping () -> Bool = { false },
         announce: @escaping (String) -> Void = { _ in }) {
        self.backend = backend
        self.onActiveModelChanged = onActiveModelChanged
        self.isDownloading = isDownloading
        self.announce = announce
    }

    private func fail(_ text: String) {
        error = text
        announce(text)
    }

    func reload() async {
        if cancelling, !isDownloading() { cancelling = false }
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
        case .refuse(let reason): fail(reason)
        case .confirm: error = nil; pendingDeletion = model
        }
    }

    /// Supprime après confirmation. Le modèle est passé par l'alerte : sa
    /// fermeture remet `pendingDeletion` à nil avant que la tâche ne parte.
    /// L'alerte est refusée : rien n'a changé, un refus ancien ne reste pas.
    func cancelDeletion() {
        pendingDeletion = nil
        error = nil
    }

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
        let ok = await run { try await self.backend.cancelDownloadAsync() }
        // Échec, ou transfert déjà fini : aucune fin de transfert ne suivra.
        if !ok || !isDownloading() { cancelling = false }
    }

    /// Un téléchargement terminé, échoué ou annulé : on relit le catalogue.
    func downloadSettled() async {
        cancelling = false
        let before = catalogue.first(where: \.active)?.id
        await reload()
        // Un modèle fraîchement installé s'active seul s'il est le premier.
        if catalogue.first(where: \.active)?.id != before { await onActiveModelChanged() }
    }

    @discardableResult
    private func run(_ action: () async throws -> Void) async -> Bool {
        error = nil
        var ok = true
        do {
            try await action()
        } catch {
            ok = false
            fail(SettingsStore.message(of: error))
        }
        await reload()
        return ok
    }
}
