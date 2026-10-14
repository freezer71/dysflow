import AppKit

/// ↑ ↓ d'une ligne affichée à l'autre : le mot où la voix reprend.
///
/// Calculé à la touche, sur la mise en page que la vue montre ; le résultat
/// est un mot précis (phrase, rang), que le moteur rejoint d'un saut absolu
/// — un écart relatif compté ici serait faussé par la voix qui avance.
@MainActor
final class LineNavigator {
    private weak var view: ReadingTextView?
    private var layout: DocumentLayout?

    /// La vue de lecture affichée et sa mise en page, à chaque document.
    func attach(_ view: ReadingTextView, layout: DocumentLayout) {
        self.view = view
        self.layout = layout
    }

    /// Le premier mot de la ligne voisine de celle du mot lu (`word`, indice
    /// global), ou du début de la phrase s'il n'y a pas de mot lu. Rien à la
    /// première ou à la dernière ligne. Seul le signe de `delta` compte.
    func target(word: UInt32?, sentence: UInt32, delta: Int) -> WordPlace? {
        guard let view, let layout, delta != 0 else { return nil }
        let current = word.flatMap { layout.wordRanges[$0] } ?? layout.sentenceRanges[sentence]
        guard let from = current?.location else { return nil }
        var target: WordPlace?
        _ = view.adjacentLine(from: from, direction: delta) { line in
            // Un mot coupé sur deux lignes ne retient pas sur place.
            guard let w = Self.word(on: line, in: layout.words), w.index != word else { return false }
            target = w
            return true
        }
        return target
    }

    /// Le mot où commence `line` : le premier qui y débute, ou celui qu'elle
    /// continue si elle s'ouvre au milieu d'un mot. Une puce n'est pas un mot.
    /// `words` est dans l'ordre du texte.
    static func word(on line: NSRange, in words: [WordPlace]) -> WordPlace? {
        // Le premier mot qui finit après le début de la ligne.
        var low = 0, high = words.count
        while low < high {
            let mid = (low + high) / 2
            if NSMaxRange(words[mid].range) <= line.location { low = mid + 1 } else { high = mid }
        }
        guard low < words.count, words[low].range.location < NSMaxRange(line) else { return nil }
        return words[low]
    }
}
