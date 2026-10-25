import Foundation
import UniformTypeIdentifiers

/// Ce que Dysflow ouvre : Markdown, texte, PDF, images.
enum OpenableFiles {
    static let types: [UTType] = [markdown("md"), markdown("markdown"), .plainText, .text, .pdf, .image]

    /// Le type d'une extension Markdown. Sans application qui la déclare,
    /// `UTType(filenameExtension:)` rend nil et le fichier serait grisé :
    /// avec `conformingTo`, le système en invente un type, fondé sur le texte.
    static func markdown(_ fileExtension: String) -> UTType {
        UTType(filenameExtension: fileExtension, conformingTo: .plainText) ?? .plainText
    }
}

/// Ce qu'on dépose sur le lecteur : le dernier fichier (un seul document à
/// la fois) ; une adresse web n'est pas un fichier.
enum DroppedFiles {
    static func file(from urls: [URL]) -> URL? {
        urls.last(where: \.isFileURL)
    }
}
