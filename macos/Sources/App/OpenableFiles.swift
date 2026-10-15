import UniformTypeIdentifiers

/// Ce que sillon ouvre : Markdown, texte, PDF, images.
enum OpenableFiles {
    static let types: [UTType] = [UTType(filenameExtension: "md"), .plainText, .text, .pdf, .image]
        .compactMap { $0 }
}
