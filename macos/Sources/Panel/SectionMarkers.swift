/// Repères de section sur la ligne de temps, sommaire et fil d'Ariane.
///
/// Le document a toujours une section implicite au début (niveau 0,
/// « Introduction ») : sans phrase tant qu'aucune prose ne précède le premier
/// titre, elle n'a ni repère, ni ligne au sommaire.
enum SectionMarkers {
    static func positions(_ doc: DocView) -> [Double] {
        let total = Double(max(doc.sentenceCount, 1))
        return doc.sections.compactMap { s in s.firstSentence.map { Double($0) / total } }
    }

    /// Les sections où l'on peut aller : celles qui ont une phrase.
    static func tableOfContents(_ doc: DocView) -> [SectionView] {
        doc.sections.filter { $0.firstSentence != nil }
    }

    static func breadcrumb(_ doc: DocView, section: UInt32?) -> String {
        guard let section, let index = doc.sections.firstIndex(where: { $0.id == section })
        else { return doc.title }
        var trail = [doc.sections[index]]
        var level = doc.sections[index].level
        // La section implicite (niveau 0) n'est le parent d'aucun titre.
        for s in doc.sections[..<index].reversed() where s.level < level && s.level > 0 {
            trail.insert(s, at: 0)
            level = s.level
        }
        return trail.map(\.title).joined(separator: " ▸ ")
    }
}
