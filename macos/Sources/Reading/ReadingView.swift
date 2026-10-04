import SwiftUI

/// Le texte lu, dans SwiftUI. Le document n'est remis en page qu'à chaque
/// révision ou changement de style ; le reste n'est que surlignage.
struct ReadingView: NSViewRepresentable {
    let model: ReaderModel
    let style: ReadingStyle
    var onContentHeight: (CGFloat) -> Void = { _ in }

    final class Coordinator {
        var revision: UInt64 = .max
        var style: ReadingStyle?
        var layout: DocumentLayout?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        let text = ReadingTextView()
        text.onContentHeight = onContentHeight
        scroll.documentView = text
        scroll.setAccessibilityIdentifier("texte-lu")
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let text = scroll.documentView as? ReadingTextView else { return }
        let c = context.coordinator
        let snapshot = model.snapshot
        text.theme = style.theme
        text.ruler = style.ruler
        if c.revision != snapshot.revision || c.style != style {
            let layout = DocumentLayout.build(model.document, style: style)
            text.setDocument(layout.text)
            c.layout = layout
            c.revision = snapshot.revision
            c.style = style
        }
        guard let layout = c.layout else { return }
        let sentence = layout.sentenceRanges[snapshot.sentence]
        let word = snapshot.word.flatMap { layout.wordRanges[$0] }
        text.show(word: word, sentence: sentence,
                  pastUpTo: sentence?.location ?? 0, follow: true)
    }
}
