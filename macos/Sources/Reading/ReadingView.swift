import SwiftUI

/// Le texte lu, dans SwiftUI. Le document n'est remis en page qu'à chaque
/// révision ou changement de style ; le reste n'est que mise en valeur.
struct ReadingView: NSViewRepresentable {
    let model: ReaderModel
    let style: ReadingStyle
    var onContentHeight: (CGFloat) -> Void = { _ in }

    final class Coordinator {
        var revision: UInt64 = .max
        var style: ReadingStyle?
        var layout: DocumentLayout?
        var commandCount: Int?
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
        // La fermeture change avec la vue SwiftUI : on garde la dernière.
        text.onContentHeight = onContentHeight
        text.theme = style.theme
        text.ruler = style.ruler
        text.playing = snapshot.playing
        if c.revision != snapshot.revision || c.style != style {
            let layout = DocumentLayout.build(model.document, style: style)
            text.setDocument(layout.text)
            c.layout = layout
            c.revision = snapshot.revision
            c.style = style
        }
        // Une commande de lecture ramène la vue sur la ligne lue, même si
        // l'utilisateur relisait plus haut. (Un nouveau document le fait déjà.)
        if c.commandCount != model.commandCount {
            if c.commandCount != nil { text.resumeFollowing() }
            c.commandCount = model.commandCount
        }
        guard let layout = c.layout else { return }
        let sentence = layout.sentenceRanges[snapshot.sentence]
        let word = snapshot.word.flatMap { layout.wordRanges[$0] }
        // La part du mot dite, relue par la vue à chaque image. Si la voix
        // est déjà au mot suivant sans que la vue le sache encore, le mot
        // montré est dit en entier.
        let shownWord = snapshot.word
        text.liveProgress = { [weak model] in
            guard let live = model?.liveSnapshot() else { return 0 }
            return live.word == shownWord ? live.wordProgress : 1
        }
        // Le déjà-lu s'arrête au mot lu ; sans mot (bloc annoncé), à la phrase.
        text.show(word: word, sentence: sentence,
                  pastUpTo: word?.location ?? sentence?.location ?? 0, follow: true)
    }
}
