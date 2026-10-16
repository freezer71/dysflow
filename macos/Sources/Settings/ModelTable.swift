import Foundation

/// L'état d'une ligne de la table des modèles.
enum ModelRowState: Equatable {
    case system
    case notInstalled
    case downloading(Double)
    case failed(String)
    case installed
}

/// Ce que la table des modèles montre, sans vue : testable seul.
enum ModelTable {
    static func state(_ row: VoiceModel, download: DownloadView?) -> ModelRowState {
        if row.system { return .system }
        if let d = download, d.id == row.id {
            // Annulé : un choix, pas un échec — la ligne redevient ce qu'elle était.
            if let failed = d.failed, failed != cancelled { return .failed(failed) }
            if d.received < d.total { return .downloading(d.total == 0 ? 0 : Double(d.received) / Double(d.total)) }
        }
        return row.installed ? .installed : .notInstalled
    }

    /// Ce que le moteur dit d'un téléchargement annulé (`DownloadView.failed`).
    static let cancelled = "annulé"

    /// Un seul téléchargement à la fois.
    static func canDownload(_ row: VoiceModel, download: DownloadView?) -> Bool {
        let running = download.map { $0.failed == nil } ?? false   // comme `ReaderModel.isDownloading`
        return !row.system && !row.installed && !running
    }

    /// Supprimer : un modèle installé, ou les fichiers qu'un téléchargement
    /// annulé ou échoué a laissés. Pas pendant un téléchargement (l'annuler
    /// d'abord), jamais la voix de macOS.
    static func canRemove(_ row: VoiceModel, _ state: ModelRowState) -> Bool {
        switch state {
        case .installed: return true
        case .notInstalled, .failed: return row.hasFiles
        case .system, .downloading: return false
        }
    }

    /// « Voix de macOS » est féminin, un modèle masculin.
    static func used(_ row: VoiceModel) -> String {
        row.system ? "utilisée" : "utilisé"
    }

    static func bytes(_ count: UInt64) -> String {
        SizeFormat.bytes(count)
    }

    /// Sur le disque si installé, sinon ce qu'il faut télécharger.
    static func size(_ row: VoiceModel) -> String {
        if row.system { return "—" }
        return bytes(row.installed ? row.diskBytes : row.bytes)
    }

    static func confirmation(_ row: VoiceModel, fallback: String) -> (title: String, message: String) {
        var message = "\(bytes(row.diskBytes)) seront libérés. Il faudra le retélécharger pour l'utiliser."
        if row.active { message += " " + fallback }
        return ("Supprimer \(row.name) ?", message)
    }

    static func footer(used: UInt64, free: UInt64?) -> String {
        let freeText = free.map { " · \(bytes($0)) libres" } ?? ""
        return "\(bytes(used)) utilisés par les modèles\(freeText)"
    }
}
