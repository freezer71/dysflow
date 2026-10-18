import AppKit

struct ReadingStyle: Equatable {
    var font: ReadingFont
    var size: CGFloat
    var lineSpacing: UInt32
    var ruler: Bool
    var theme: Theme
}

/// Un mot affiché : sa plage dans le texte, sa phrase, son rang dans la
/// phrase (ce que vise un saut absolu) et son indice global.
struct WordPlace: Equatable {
    let range: NSRange
    let sentence: UInt32
    let word: UInt32
    let index: UInt32
}

/// Le document mis en texte, avec la plage de chaque mot et de chaque phrase.
///
/// Les mots sont séparés d'une espace, les blocs d'un retour à la ligne ; un
/// bloc qui a des lignes à montrer telles quelles (code, tableau) les montre
/// en chasse fixe, et ses mots — une annonce — ne sont pas suivis ; sa
/// phrase couvre le bloc entier.
struct DocumentLayout {
    let text: NSAttributedString
    let wordRanges: [UInt32: NSRange]
    let sentenceRanges: [UInt32: NSRange]
    /// Les mots suivis, dans l'ordre du texte.
    let words: [WordPlace]

    /// Le texte s'écrit d'un bloc, puis ses attributs se posent par plage :
    /// ceux du bloc d'abord, puis ceux d'un mot seulement là où il diffère
    /// (gras, italique, code, lien, barré). Les polices sont créées une fois
    /// par taille et par style, pas une fois par mot.
    static func build(_ doc: DocView, style: ReadingStyle) -> DocumentLayout {
        let text = NSMutableString()
        var runs: [(range: NSRange, attrs: [NSAttributedString.Key: Any], replace: Bool)] = []
        var words: [UInt32: NSRange] = [:]
        var sentences: [UInt32: NSRange] = [:]
        var places: [WordPlace] = []
        let mono = NSFont.monospacedSystemFont(ofSize: style.size * 0.85, weight: .regular)
        var fonts = FontCache(font: style.font)

        for (i, block) in doc.blocks.enumerated() {
            // Le retour à la ligne entre deux blocs reste sans attribut.
            if i > 0 { text.append("\n") }
            let blockStart = text.length
            let paragraph = paragraphStyle(for: block.kind, style: style)
            let base = baseFont(for: block.kind, style: style, fonts: &fonts)
            var attrs: [NSAttributedString.Key: Any] = [
                .font: base, .foregroundColor: color(for: block.kind, style: style),
                .paragraphStyle: paragraph,
            ]

            if isVerbatim(block.kind), !block.displayLines.isEmpty {
                attrs[.font] = mono
                text.append(block.displayLines.joined(separator: "\n"))
                // Ses mots ne se suivent pas, mais sa phrase — l'annonce —
                // couvre le bloc entier : pendant qu'on l'annonce, la vue
                // sait où il est.
                let whole = NSRange(location: blockStart, length: text.length - blockStart)
                runs.append((whole, attrs, true))
                for sentence in block.sentences { sentences[sentence.id] = whole }
                continue
            }
            if let prefix = prefix(for: block.kind) { text.append(prefix) }
            var wordRuns: [(range: NSRange, attrs: [NSAttributedString.Key: Any], replace: Bool)] = []
            for (s, sentence) in block.sentences.enumerated() {
                if s > 0 { text.append(" ") }
                let start = text.length
                for (w, word) in sentence.words.enumerated() {
                    if w > 0 { text.append(" ") }
                    let range = NSRange(location: text.length, length: (word.text as NSString).length)
                    text.append(word.text)
                    words[word.index] = range
                    places.append(WordPlace(range: range, sentence: sentence.id,
                                            word: UInt32(w), index: word.index))
                    var own: [NSAttributedString.Key: Any] = [:]
                    let font = word.code ? mono
                        : fonts.font(size: base.pointSize, bold: word.strong || isHeading(block.kind),
                                     italic: word.italic)
                    if font != base { own[.font] = font }
                    if word.link {
                        own[.underlineStyle] = NSUnderlineStyle.single.rawValue
                        own[.underlineColor] = style.theme.line
                    }
                    if word.struck {
                        own[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                    }
                    if !own.isEmpty { wordRuns.append((range, own, false)) }
                }
                sentences[sentence.id] = NSRange(location: start, length: text.length - start)
            }
            runs.append((NSRange(location: blockStart, length: text.length - blockStart), attrs, true))
            runs.append(contentsOf: wordRuns)
        }

        let out = NSMutableAttributedString(string: text as String)
        out.beginEditing()
        for run in runs where run.range.length > 0 {
            if run.replace {
                out.setAttributes(run.attrs, range: run.range)
            } else {
                out.addAttributes(run.attrs, range: run.range)
            }
        }
        out.endEditing()
        return DocumentLayout(text: out, wordRanges: words, sentenceRanges: sentences,
                              words: places)
    }

    /// Les polices d'une mise en page, par taille, graisse et italique :
    /// `NSFontManager` cherche la famille à chaque appel.
    struct FontCache {
        let font: ReadingFont
        private struct Key: Hashable { let size: CGFloat; let bold: Bool; let italic: Bool }
        private var fonts: [Key: NSFont] = [:]

        init(font: ReadingFont) { self.font = font }

        mutating func font(size: CGFloat, bold: Bool = false, italic: Bool = false) -> NSFont {
            let key = Key(size: size, bold: bold, italic: italic)
            if let cached = fonts[key] { return cached }
            let made = font.font(size: size, bold: bold, italic: italic)
            fonts[key] = made
            return made
        }
    }

    /// Comme au terminal : seuls le code et les tableaux se montrent tels
    /// quels. Rust remplit `displayLines` pour tous les blocs (les lignes du
    /// fichier source), la prose se compose ici à partir de ses mots.
    static func isVerbatim(_ kind: BlockKindView) -> Bool {
        switch kind {
        case .code, .tableRow: return true
        default: return false
        }
    }

    static func isHeading(_ kind: BlockKindView) -> Bool {
        if case .heading = kind { return true }
        return false
    }

    private static func baseFont(for kind: BlockKindView, style: ReadingStyle,
                                 fonts: inout FontCache) -> NSFont {
        switch kind {
        case .heading(let level):
            let scale: CGFloat = level == 1 ? 1.6 : level == 2 ? 1.35 : 1.15
            return fonts.font(size: style.size * scale, bold: true)
        default:
            return fonts.font(size: style.size)
        }
    }

    /// Toute la prose, citations comprises, garde la couleur du texte : la
    /// teinte des contrôles serait trop pâle pour lire. La citation se
    /// distingue par son retrait.
    private static func color(for kind: BlockKindView, style: ReadingStyle) -> NSColor {
        style.theme.text
    }

    static func prefix(for kind: BlockKindView) -> String? {
        if case .listItem(_, let ordinal) = kind {
            return ordinal.map { "\($0). " } ?? "• "
        }
        return nil
    }

    static func paragraphStyle(for kind: BlockKindView, style: ReadingStyle) -> NSParagraphStyle {
        let p = NSMutableParagraphStyle()
        // `line_spacing` compte des lignes vides en terminal ; ici, une fraction
        // de ligne par cran.
        p.lineHeightMultiple = 1.35 + 0.25 * CGFloat(style.lineSpacing)
        p.paragraphSpacing = style.size * 0.6
        switch kind {
        case .heading:
            p.paragraphSpacingBefore = style.size * 0.8
        case .listItem(let depth, _):
            let indent = style.size * 1.2 * CGFloat(depth + 1)
            p.headIndent = indent
            p.firstLineHeadIndent = indent - style.size * 0.9
        case .quote:
            p.headIndent = style.size * 1.2
            p.firstLineHeadIndent = style.size * 1.2
        default:
            break
        }
        return p
    }
}
