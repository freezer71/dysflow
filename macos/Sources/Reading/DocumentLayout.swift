import AppKit

struct ReadingStyle: Equatable {
    var font: ReadingFont
    var size: CGFloat
    var lineSpacing: UInt32
    var ruler: Bool
    var theme: Theme
}

/// Le document mis en texte, avec la plage de chaque mot et de chaque phrase.
///
/// Les mots sont séparés d'une espace, les blocs d'un retour à la ligne ; un
/// bloc qui a des lignes à montrer telles quelles (code, tableau) les montre
/// en chasse fixe, et ses mots — une annonce — ne sont pas surlignés ; sa
/// phrase couvre le bloc entier.
struct DocumentLayout {
    let text: NSAttributedString
    let wordRanges: [UInt32: NSRange]
    let sentenceRanges: [UInt32: NSRange]

    static func build(_ doc: DocView, style: ReadingStyle) -> DocumentLayout {
        let out = NSMutableAttributedString()
        var words: [UInt32: NSRange] = [:]
        var sentences: [UInt32: NSRange] = [:]
        let mono = NSFont.monospacedSystemFont(ofSize: style.size * 0.85, weight: .regular)

        for (i, block) in doc.blocks.enumerated() {
            if i > 0 { out.append(NSAttributedString(string: "\n")) }
            let paragraph = paragraphStyle(for: block.kind, style: style)
            let base = baseFont(for: block.kind, style: style)
            var attrs: [NSAttributedString.Key: Any] = [
                .font: base, .foregroundColor: color(for: block.kind, style: style),
                .paragraphStyle: paragraph,
            ]

            if isVerbatim(block.kind), !block.displayLines.isEmpty {
                attrs[.font] = mono
                let start = out.length
                out.append(NSAttributedString(string: block.displayLines.joined(separator: "\n"),
                                              attributes: attrs))
                // Ses mots ne se suivent pas, mais sa phrase — l'annonce —
                // couvre le bloc entier : pendant qu'on l'annonce, la vue
                // sait où il est.
                let whole = NSRange(location: start, length: out.length - start)
                for sentence in block.sentences { sentences[sentence.id] = whole }
                continue
            }
            if let prefix = prefix(for: block.kind) {
                out.append(NSAttributedString(string: prefix, attributes: attrs))
            }
            for (s, sentence) in block.sentences.enumerated() {
                if s > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                let start = out.length
                for (w, word) in sentence.words.enumerated() {
                    if w > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                    var wordAttrs = attrs
                    wordAttrs[.font] = word.code ? mono
                        : style.font.font(size: base.pointSize,
                                          bold: word.strong || isHeading(block.kind),
                                          italic: word.italic)
                    if word.link {
                        wordAttrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                        wordAttrs[.underlineColor] = style.theme.line
                    }
                    if word.struck {
                        wordAttrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                    }
                    words[word.index] = NSRange(location: out.length,
                                                length: (word.text as NSString).length)
                    out.append(NSAttributedString(string: word.text, attributes: wordAttrs))
                }
                sentences[sentence.id] = NSRange(location: start, length: out.length - start)
            }
        }
        return DocumentLayout(text: out, wordRanges: words, sentenceRanges: sentences)
    }

    /// Comme au terminal : seuls le code et les tableaux se montrent tels
    /// quels. Rust remplit `displayLines` pour tous les blocs (les lignes du
    /// fichier source), la prose se compose ici à partir de ses mots.
    private static func isVerbatim(_ kind: BlockKindView) -> Bool {
        switch kind {
        case .code, .tableRow: return true
        default: return false
        }
    }

    private static func isHeading(_ kind: BlockKindView) -> Bool {
        if case .heading = kind { return true }
        return false
    }

    private static func baseFont(for kind: BlockKindView, style: ReadingStyle) -> NSFont {
        switch kind {
        case .heading(let level):
            let scale: CGFloat = level == 1 ? 1.6 : level == 2 ? 1.35 : 1.15
            return style.font.font(size: style.size * scale, bold: true)
        default:
            return style.font.font(size: style.size)
        }
    }

    private static func color(for kind: BlockKindView, style: ReadingStyle) -> NSColor {
        if case .quote = kind { return style.theme.chrome }
        return style.theme.text
    }

    private static func prefix(for kind: BlockKindView) -> String? {
        if case .listItem(_, let ordinal) = kind {
            return ordinal.map { "\($0). " } ?? "• "
        }
        return nil
    }

    private static func paragraphStyle(for kind: BlockKindView, style: ReadingStyle) -> NSParagraphStyle {
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
