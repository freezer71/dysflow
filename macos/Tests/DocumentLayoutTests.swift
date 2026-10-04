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
}
