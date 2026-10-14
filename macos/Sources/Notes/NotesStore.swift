import Foundation
import Observation

/// Une page de notes : un titre, un corps, la date de la dernière frappe.
struct NotePage: Identifiable, Equatable {
    let id: String
    var title: String
    var body: String
    var modified: Date

    /// Ce que le lecteur reçoit : le titre en tête, comme un titre Markdown.
    var markdown: String {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return body }
        return "# \(title)\n\n\(body)"
    }

    /// Le titre affiché : « Sans titre » tant qu'il est vide.
    var displayTitle: String {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? "Sans titre" : title
    }
}

/// Les pages de notes, une par fichier Markdown dans un dossier : le titre
/// en première ligne (« # Titre »), le corps ensuite. Les fichiers restent
/// lisibles et modifiables hors de sillon. Une frappe s'écrit après une
/// courte pause, jamais à chaque caractère.
@MainActor
@Observable
final class NotesStore {
    /// Les pages, la plus récemment modifiée en tête.
    private(set) var pages: [NotePage] = []
    @ObservationIgnored let directory: URL
    @ObservationIgnored private var pendingWrites: Set<String> = []
    @ObservationIgnored private var writeTask: Task<Void, Never>?
    @ObservationIgnored var writeDelay: Duration = .milliseconds(600)
    @ObservationIgnored private let now: () -> Date

    init(directory: URL, now: @escaping () -> Date = Date.init) {
        self.directory = directory
        self.now = now
        load()
    }

    /// Le dossier des notes de l'utilisateur.
    nonisolated static var userDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("sillon/Notes", isDirectory: true)
    }

    func page(_ id: String) -> NotePage? { pages.first { $0.id == id } }

    func load() {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        pages = files.filter { $0.pathExtension == "md" }.compactMap { url in
            guard let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
            let date = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?
                .contentModificationDate ?? .distantPast
            let (title, body) = Self.split(text)
            return NotePage(id: url.deletingPathExtension().lastPathComponent,
                            title: title, body: body, modified: date)
        }
        sort()
    }

    /// Une page vierge, en tête de liste, écrite tout de suite.
    @discardableResult
    func create(title: String = "", body: String = "") -> NotePage {
        let page = NotePage(id: UUID().uuidString, title: title, body: body, modified: now())
        pages.insert(page, at: 0)
        write(page)
        return page
    }

    /// Une frappe : la page remonte en tête, l'écriture attend la pause.
    func update(_ id: String, title: String? = nil, body: String? = nil) {
        guard let index = pages.firstIndex(where: { $0.id == id }) else { return }
        var page = pages[index]
        if let title { page.title = title }
        if let body { page.body = body }
        guard page != pages[index] else { return }
        page.modified = now()
        pages[index] = page
        sort()
        pendingWrites.insert(id)
        scheduleWrite()
    }

    func delete(_ id: String) {
        pages.removeAll { $0.id == id }
        pendingWrites.remove(id)
        try? FileManager.default.removeItem(at: url(of: id))
    }

    /// Écrit tout ce qui attend (fenêtre fermée, app qui quitte).
    func flush() {
        writeTask?.cancel()
        writeTask = nil
        for id in pendingWrites {
            if let page = page(id) { write(page) }
        }
        pendingWrites.removeAll()
    }

    private func scheduleWrite() {
        writeTask?.cancel()
        let delay = writeDelay
        writeTask = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            self?.flush()
        }
    }

    private func write(_ page: NotePage) {
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try Self.join(title: page.title, body: page.body)
                .write(to: url(of: page.id), atomically: true, encoding: .utf8)
        } catch {
            // Une écriture refusée (disque plein, droits) : la page reste en
            // mémoire et repartira à la prochaine frappe.
            pendingWrites.insert(page.id)
        }
    }

    private func url(of id: String) -> URL {
        directory.appendingPathComponent(id).appendingPathExtension("md")
    }

    private func sort() {
        pages.sort { $0.modified > $1.modified }
    }

    /// « # Titre », une ligne vide, puis le corps.
    nonisolated static func join(title: String, body: String) -> String {
        "# \(title.replacingOccurrences(of: "\n", with: " "))\n\n\(body)"
    }

    /// L'inverse : sans « # » en première ligne, tout est corps.
    nonisolated static func split(_ text: String) -> (title: String, body: String) {
        let lines = text.components(separatedBy: "\n")
        guard let first = lines.first, first.hasPrefix("# ") || first == "#" else { return ("", text) }
        let title = String(first.dropFirst(first == "#" ? 1 : 2))
        var rest = Array(lines.dropFirst())
        if rest.first?.isEmpty == true { rest.removeFirst() }
        return (title, rest.joined(separator: "\n"))
    }
}
