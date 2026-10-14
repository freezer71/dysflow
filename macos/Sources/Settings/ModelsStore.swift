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
    var error: String? { note.text }
    @ObservationIgnored private let note: ErrorNote
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
    init(backend: ModelsBackend,
         onActiveModelChanged: @escaping () async -> Void = {},
         isDownloading: @escaping () -> Bool = { false },
         announce: ((String) -> Void)? = nil) {
        self.backend = backend
        self.onActiveModelChanged = onActiveModelChanged
        self.isDownloading = isDownloading
        // Un refus est dit à VoiceOver chaque fois, même identique au précédent.
        note = ErrorNote(announce: announce)
    }

    private func fail(_ text: String) { note.fail(text) }

    func reload() async {
        if cancelling, !isDownloading() { cancelling = false }
        catalogue = await backend.loadVoices()
        freeSpace = await backend.freeSpace()
    }

    static func deletionRequest(for model: VoiceModel) -> DeletionRequest {
        if model.active {
            return .refuse("\(model.name) est le modèle actif : choisissez-en un autre avant de le supprimer.")
        }
        let size = SizeFormat.bytes(model.bytes)
        return .confirm(title: "Supprimer \(model.name) ?",
                        message: "\(size) seront libérés. Il faudra le télécharger à nouveau pour l'utiliser.")
    }

    func askDeletion(_ model: VoiceModel) {
        switch Self.deletionRequest(for: model) {
        case .refuse(let reason): fail(reason)
        case .confirm: note.clear(); pendingDeletion = model
        }
    }

    /// Supprime après confirmation. Le modèle est passé par l'alerte : sa
    /// fermeture remet `pendingDeletion` à nil avant que la tâche ne parte.
    /// L'alerte est refusée : rien n'a changé, un refus ancien ne reste pas.
    func cancelDeletion() {
        pendingDeletion = nil
        note.clear()
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

    /// Un téléchargement suivi : son début relève le modèle actif, sa fin le
    /// compare. Appelé par la fenêtre, quel que soit l'onglet affiché.
    @ObservationIgnored private var downloading = false

    func downloadChanged(downloading now: Bool) async {
        let was = downloading
        downloading = now
        if now, !was {
            // L'onglet Modèles n'a peut-être jamais été ouvert : le catalogue
            // de départ sert de référence pour voir le modèle actif changer.
            await reload()
        } else if !now, was {
            await downloadSettled()
        }
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
        note.clear()
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
