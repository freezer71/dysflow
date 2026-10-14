import Foundation

/// Le texte qu'on modifie en mode édition : celui qu'on a ouvert, tel quel.
/// Sans lui (une image passée à l'OCR), le document est réécrit en Markdown
/// depuis ses blocs : titres, listes, citations, code et tableaux gardent
/// leur forme.
enum EditableText {
    static func markdown(from document: DocView) -> String {
        var parts: [String] = []
        var table: [String] = []

        func flushTable() {
            guard !table.isEmpty else { return }
            parts.append(table.joined(separator: "\n"))
            table = []
        }

        for block in document.blocks {
            let lines = block.displayLines
            let joined = lines.joined(separator: " ")
            if case .tableRow(let header) = block.kind {
                table.append("| " + lines.joined(separator: " | ") + " |")
                if header {
                    table.append("|" + Array(repeating: " --- |", count: max(lines.count, 1)).joined())
                }
                continue
            }
            flushTable()
            switch block.kind {
            case .heading(let level):
                parts.append(String(repeating: "#", count: Int(max(level, 1))) + " " + joined)
            case .listItem(let depth, let ordinal):
                let indent = String(repeating: "  ", count: Int(depth))
                let marker = ordinal.map { "\($0)." } ?? "-"
                parts.append(indent + marker + " " + joined)
            case .quote:
                parts.append(lines.map { "> " + $0 }.joined(separator: "\n"))
            case .code(let language):
                parts.append("```" + (language ?? "") + "\n" + lines.joined(separator: "\n") + "\n```")
            case .paragraph, .tableRow:
                parts.append(joined)
            }
        }
        flushTable()
        return parts.joined(separator: "\n\n")
    }
}
