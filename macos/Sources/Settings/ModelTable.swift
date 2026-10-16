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
            if let failed = d.failed { return .failed(failed) }
            if d.received < d.total { return .downloading(d.total == 0 ? 0 : Double(d.received) / Double(d.total)) }
        }
        return row.installed ? .installed : .notInstalled
    }

    /// Un seul téléchargement à la fois.
    static func canDownload(_ row: VoiceModel, download: DownloadView?) -> Bool {
        let running = download.map { $0.failed == nil } ?? false   // comme `ReaderModel.isDownloading`
        return !row.system && !row.installed && !running
    }

    static func bytes(_ count: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(count), countStyle: .file)
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
