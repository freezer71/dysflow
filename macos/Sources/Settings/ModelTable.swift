import Foundation

/// L'état d'une ligne de la table des modèles.
enum ModelRowState: Equatable {
    case system
    case notInstalled
    case downloading(Double)
    case failed(String)
    case installed
}

/// Qui a fait un modèle : son logo distingue les lignes de la table d'un
/// coup d'œil. Sans logo public (hexgrad, Fermion Research), une initiale.
enum ModelMaker: Equatable {
    case apple, nvidia, openai, mistral, qwen, cohere
    case initial(String)

    /// Le nom de l'image du catalogue, `nil` pour une initiale.
    var logo: String? {
        switch self {
        case .apple: "logo-apple"
        case .nvidia: "logo-nvidia"
        case .openai: "logo-openai"
        case .mistral: "logo-mistral"
        case .qwen: "logo-qwen"
        case .cohere: "logo-cohere"
        case .initial: nil
        }
    }

    /// Le nom de l'éditeur, en clair : le filtre « Éditeur » de la
    /// bibliothèque et VoiceOver. Les initiales connues ont le leur (K :
    /// hexgrad, qui fait Kokoro ; F : Fermion Research, Phonon-2) ; une
    /// autre reste sa lettre.
    var name: String {
        switch self {
        case .apple: "Apple"
        case .nvidia: "NVIDIA"
        case .openai: "OpenAI"
        case .mistral: "Mistral"
        case .qwen: "Qwen"
        case .cohere: "Cohere"
        case .initial("K"): "hexgrad"
        case .initial("F"): "Fermion Research"
        case .initial(let letter): letter
        }
    }
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

    /// Une ligne se choisit (clic sur le rond, le nom ou la taille) si son
    /// modèle est là (installé, ou la voix de macOS) et pas déjà utilisé.
    static func canUse(_ row: VoiceModel) -> Bool {
        (row.installed || row.system) && !row.active
    }

    /// Un clic sur une ligne pas encore installée la télécharge, si aucun
    /// autre téléchargement ne court (comme le bouton « Télécharger »).
    static func downloadsOnClick(_ row: VoiceModel, download: DownloadView?) -> Bool {
        canDownload(row, download: download) && !(download?.id == row.id && download?.failed == nil)
    }

    /// Supprimer : un modèle installé, ou les fichiers qu'un téléchargement
    /// échoué a laissés (une annulation n'en laisse pas). Pas pendant un
    /// téléchargement (l'annuler d'abord), jamais la voix de macOS.
    static func canRemove(_ row: VoiceModel, _ state: ModelRowState) -> Bool {
        switch state {
        case .installed: return true
        case .notInstalled, .failed: return row.hasFiles
        case .system, .downloading: return false
        }
    }

    /// L'éditeur d'un modèle, d'après son identifiant (`src/models.rs`).
    static func maker(_ row: VoiceModel) -> ModelMaker {
        if row.system { return .apple }
        let prefixes: [(String, ModelMaker)] = [
            ("parakeet", .nvidia), ("canary", .nvidia), ("magpie", .nvidia), ("nemotron", .nvidia),
            ("whisper", .openai), ("voxtral", .mistral), ("qwen", .qwen), ("cohere", .cohere),
            ("phonon", .initial("F")), ("kokoro", .initial("K")),
        ]
        return prefixes.first { row.id.hasPrefix($0.0) }?.1
            ?? .initial(row.name.first.map { String($0).uppercased() } ?? "?")
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

    /// Ce que VoiceOver entend d'un téléchargement : un palier de 25 % franchi
    /// entre deux relectures (« Téléchargement de Parakeet v3 : 25 % »),
    /// sinon rien.
    static func progressAnnouncement(name: String, previous: Double?, current: Double) -> String? {
        let before = Int((min(max(previous ?? 0, 0), 1) * 4).rounded(.down))
        let now = Int((min(max(current, 0), 1) * 4).rounded(.down))
        guard now > before else { return nil }
        return "Téléchargement de \(name) : \(now * 25) %"
    }

    /// La part reçue d'un téléchargement en cours, `nil` sinon.
    static func fraction(_ download: DownloadView?) -> Double? {
        guard let d = download, d.failed == nil, d.total > 0 else { return nil }
        return Double(d.received) / Double(d.total)
    }

    static func footer(used: UInt64, free: UInt64?) -> String {
        let freeText = free.map { " · \(bytes($0)) libres" } ?? ""
        return "\(bytes(used)) utilisés par les modèles\(freeText)"
    }
}
