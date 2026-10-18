import XCTest
@testable import Sillon

@MainActor
final class DocumentLayoutTests: XCTestCase {
    private func doc(_ markdown: String) -> DocView {
        let r = SillonReader.silent()
        r.openText(markdown: markdown, source: "t")
        XCTAssertTrue(waitUntil { r.snapshot().revision == 1 })
        return r.document()
    }

    private let style = ReadingStyle(font: .literata, size: 20, lineSpacing: 1,
                                     ruler: true, theme: .paper)

    func testEveryWordMapsToItsOwnText() {
        let d = doc("# Titre\n\nUn **deux** trois. Quatre.")
        let layout = DocumentLayout.build(d, style: style)
        let s = layout.text.string as NSString
        for block in d.blocks {
            for sentence in block.sentences {
                for word in sentence.words {
                    let range = try! XCTUnwrap(layout.wordRanges[word.index])
                    XCTAssertEqual(s.substring(with: range), word.text)
                }
            }
        }
    }

    func testSentenceRangeCoversItsWords() {
        let d = doc("Un deux. Trois quatre.")
        let layout = DocumentLayout.build(d, style: style)
        let s = layout.text.string as NSString
        XCTAssertEqual(s.substring(with: layout.sentenceRanges[1]!), "Trois quatre.")
    }

    func testCodeBlockShowsItsLinesAndMapsNoWord() {
        let d = doc("Avant.\n\n```rust\nfn main() {}\n```\n\nAprès.")
        let layout = DocumentLayout.build(d, style: style)
        XCTAssertTrue(layout.text.string.contains("fn main() {}"))
        let code = d.blocks.first { if case .code = $0.kind { true } else { false } }!
        for w in code.sentences.flatMap(\.words) {
            XCTAssertNil(layout.wordRanges[w.index])
        }
    }

    func testStrongWordIsBold() {
        let d = doc("Un **deux** trois.")
        let layout = DocumentLayout.build(d, style: style)
        let deux = d.blocks[0].sentences[0].words[1]
        let font = layout.text.attribute(.font, at: layout.wordRanges[deux.index]!.location,
                                         effectiveRange: nil) as! NSFont
        XCTAssertTrue(font.fontDescriptor.symbolicTraits.contains(.bold))
    }

    /// Un bloc de code n'a pas de mots à suivre, mais sa phrase (l'annonce)
    /// couvre tout le bloc : la vue sait où il est pendant qu'on l'annonce.
    func testCodeBlockSentenceCoversTheBlock() {
        let d = doc("Avant.\n\n```rust\nfn main() {}\n```\n\nAprès.")
        let layout = DocumentLayout.build(d, style: style)
        let s = layout.text.string as NSString
        let code = d.blocks.first { if case .code = $0.kind { true } else { false } }!
        let range = try! XCTUnwrap(layout.sentenceRanges[code.sentences[0].id])
        XCTAssertEqual(s.substring(with: range), "fn main() {}")
    }

    /// Une citation garde la couleur du texte : la teinte des contrôles
    /// serait trop pâle pour lire.
    func testQuoteUsesTheTextColor() {
        let d = doc("> Une citation.")
        let layout = DocumentLayout.build(d, style: style)
        let color = layout.text.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        XCTAssertEqual(color?.hex, Theme.paper.text.hex)
    }

    /// La mise en page par plages rend exactement le texte d'avant, mot par
    /// mot : mêmes caractères, mêmes attributs partout.
    func testBuildMatchesTheWordByWordReference() {
        let d = doc("""
        # Titre *penché*

        Un **deux** *trois* `code` [lien](https://a.b) ~~barré~~ fin. Encore une.

        - premier **point**
        1. numéro

        > Une citation.

        ```rust
        fn main() {}
        ```

        ## Sous-titre
        """)
        for font in ReadingFont.allCases {
            let s = ReadingStyle(font: font, size: 18, lineSpacing: 2, ruler: true, theme: .paper)
            let built = DocumentLayout.build(d, style: s)
            XCTAssertTrue(built.text.isEqual(to: Self.reference(d, style: s)), "police \(font)")
        }
    }

    /// L'ancienne construction, un NSAttributedString par mot et par espace.
    private static func reference(_ doc: DocView, style: ReadingStyle) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let mono = NSFont.monospacedSystemFont(ofSize: style.size * 0.85, weight: .regular)
        for (i, block) in doc.blocks.enumerated() {
            if i > 0 { out.append(NSAttributedString(string: "\n")) }
            let heading = DocumentLayout.isHeading(block.kind)
            let base: NSFont
            if case .heading(let level) = block.kind {
                let scale: CGFloat = level == 1 ? 1.6 : level == 2 ? 1.35 : 1.15
                base = style.font.font(size: style.size * scale, bold: true)
            } else {
                base = style.font.font(size: style.size)
            }
            var attrs: [NSAttributedString.Key: Any] = [
                .font: base, .foregroundColor: style.theme.text,
                .paragraphStyle: DocumentLayout.paragraphStyle(for: block.kind, style: style),
            ]
            if DocumentLayout.isVerbatim(block.kind), !block.displayLines.isEmpty {
                attrs[.font] = mono
                out.append(NSAttributedString(string: block.displayLines.joined(separator: "\n"),
                                              attributes: attrs))
                continue
            }
            if let prefix = DocumentLayout.prefix(for: block.kind) {
                out.append(NSAttributedString(string: prefix, attributes: attrs))
            }
            for (s, sentence) in block.sentences.enumerated() {
                if s > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                for (w, word) in sentence.words.enumerated() {
                    if w > 0 { out.append(NSAttributedString(string: " ", attributes: attrs)) }
                    var wordAttrs = attrs
                    wordAttrs[.font] = word.code ? mono
                        : style.font.font(size: base.pointSize, bold: word.strong || heading,
                                          italic: word.italic)
                    if word.link {
                        wordAttrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                        wordAttrs[.underlineColor] = style.theme.line
                    }
                    if word.struck {
                        wordAttrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                    }
                    out.append(NSAttributedString(string: word.text, attributes: wordAttrs))
                }
            }
        }
        return out
    }
}
