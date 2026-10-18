import SwiftUI

/// Le texte lu, dans SwiftUI. Le document n'est remis en page qu'à chaque
/// révision ou changement de style ; le reste n'est que mise en valeur.
struct ReadingView: NSViewRepresentable {
    let model: ReaderModel
    let style: ReadingStyle
    /// Reçoit la vue et sa mise en page, pour ↑ ↓ ligne par ligne.
    var lines: LineNavigator?
    /// Copie invisible (mode petit) : elle ne sert qu'à ↑ ↓. Ni trait animé,
    /// ni VoiceOver (une vue cachée sort de l'arbre d'accessibilité).
    var shadow = false
    /// « Copier » du menu contextuel.
    var onCopy: (() -> Void)?
    /// « Lire à partir d'ici » du menu contextuel.
    var onReadFrom: ((Int) -> Void)?
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
        scroll.isHidden = shadow
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let text = scroll.documentView as? ReadingTextView else { return }
        let c = context.coordinator
        let snapshot = model.snapshot
        // La fermeture change avec la vue SwiftUI : on garde la dernière.
        text.onContentHeight = onContentHeight
        text.onCopy = onCopy
        text.onReadFrom = onReadFrom
        text.theme = style.theme
        text.ruler = style.ruler
        text.playing = snapshot.playing && !shadow
        if c.revision != snapshot.revision || c.style != style {
            let layout = DocumentLayout.build(model.document, style: style)
            text.setDocument(layout.text)
            c.layout = layout
            lines?.attach(text, layout: layout)
            c.revision = snapshot.revision
            c.style = style
        }
        // Une commande de lecture ramène la vue sur la ligne lue, même si
        // l'utilisateur relisait plus haut. (Un nouveau document le fait déjà.)
        if c.commandCount != model.commandCount {
            if c.commandCount != nil { text.resumeFollowing() }
            c.commandCount = model.commandCount
        }
        // La copie cachée ne sert qu'à ↑ ↓, qui lisent la mise en page à la
        // touche : rien à suivre, et le mot lu n'y est pas observé.
        guard !shadow, let layout = c.layout else { return }
        let shownWord = model.word
        let sentence = layout.sentenceRanges[snapshot.sentence]
        let word = shownWord.flatMap { layout.wordRanges[$0] }
        // La part du mot dite, relue par la vue à chaque image dans le
        // dernier instantané lu par le modèle. Si la voix est déjà au mot
        // suivant sans que la vue le sache encore, le mot montré est dit en
        // entier.
        text.liveProgress = { [weak model] in
            guard let live = model?.liveSnapshot() else { return 0 }
            return live.word == shownWord ? live.wordProgress : 1
        }
        // Le déjà-lu s'arrête au mot lu ; sans mot (bloc annoncé), à la phrase.
        text.show(word: word, sentence: sentence,
                  pastUpTo: word?.location ?? sentence?.location ?? 0, follow: true)
    }
}
