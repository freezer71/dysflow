import AppKit
import QuartzCore

/// Le texte lu, avec son mot surligné qui glisse, sa ligne soulignée, et le
/// déjà-lu atténué. TextKit 2 ne met en page que ce qui est visible.
final class ReadingTextView: NSTextView {
    var theme: Theme = .paper { didSet { if theme != oldValue { needsDisplay = true } } }
    var ruler = true
    var onContentHeight: ((CGFloat) -> Void)?
    /// L'horloge du suivi, remplaçable pour les tests.
    var now: () -> Date = Date.init

    /// Le rectangle surligné tel qu'il est dessiné, en glissement compris.
    private(set) var shown: CGRect?
    /// La bande sous la ligne lue (ou sous le bloc annoncé).
    private(set) var lineBand: CGRect?
    private var from: CGRect?
    private var target: CGRect?
    private var animationStart: CFTimeInterval = 0
    private var link: CADisplayLink?
    private var dimmedUpTo = 0
    private var underlined: NSRange?
    private var colored: NSRange?
    private var lastLineY: CGFloat?
    private var followPolicy = FollowPolicy()
    private var liveScrollObserver: NSObjectProtocol?
    private static let glide: CFTimeInterval = 0.08

    /// Le dernier appel à `show`, pour l'ignorer s'il se répète (60 à 120 fois
    /// par seconde pendant la lecture) et pour tout replacer si la largeur
    /// change.
    private struct Shown: Equatable {
        var word: NSRange?
        var sentence: NSRange?
        var pastUpTo: Int
        var follow: Bool
        var width: CGFloat
    }
    private var last: Shown?

    convenience init() {
        self.init(usingTextLayoutManager: true)
        isEditable = false
        isSelectable = false
        drawsBackground = false
        textContainerInset = NSSize(width: 4, height: 8)
        isVerticallyResizable = true
        isHorizontallyResizable = false
        autoresizingMask = [.width]
        textContainer?.widthTracksTextView = true
        // Dans un NSScrollView, la vue suit la largeur du panneau et grandit
        // en hauteur avec son texte.
        minSize = .zero
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude,
                         height: CGFloat.greatestFiniteMagnitude)
        textContainer?.containerSize = NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        setAccessibilityLabel("Texte lu")
    }

    deinit {
        if let liveScrollObserver { NotificationCenter.default.removeObserver(liveScrollObserver) }
    }

    private var reduceMotion: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }

    func setDocument(_ text: NSAttributedString) {
        textStorage?.setAttributedString(text)
        // Les attributs de rendu tiennent au gestionnaire de mise en page, pas
        // au texte : ceux de l'ancien document ne doivent pas survivre.
        if let tlm = textLayoutManager {
            for key in [NSAttributedString.Key.foregroundColor, .underlineStyle, .underlineColor] {
                tlm.removeRenderingAttribute(key, for: tlm.documentRange)
            }
        }
        stopLink()
        shown = nil; target = nil; lineBand = nil; dimmedUpTo = 0; underlined = nil; colored = nil
        lastLineY = nil; last = nil
        followPolicy.resume()
        setAccessibilityValue(nil)
        needsDisplay = true
        // Sans largeur, la hauteur ne veut rien dire : elle sera envoyée quand
        // SwiftUI aura donné son cadre (voir `setFrameSize`).
        if bounds.width > 0 { onContentHeight?(contentHeight) }
    }

    /// Hauteur du texte, sans mettre en page au-delà de ce qu'un panneau
    /// peut montrer.
    var contentHeight: CGFloat {
        guard let tlm = textLayoutManager else { return 0 }
        let length = textStorage?.length ?? 0
        if length > 4000 { return .greatestFiniteMagnitude }
        tlm.ensureLayout(for: tlm.documentRange)
        return tlm.usageBoundsForTextContainer.height + textContainerInset.height * 2
    }

    /// Montre le mot lu et sa phrase. Sans mot (un bloc de code ou un tableau
    /// qu'on annonce), la bande se pose sur la phrase, c'est-à-dire le bloc.
    func show(word: NSRange?, sentence: NSRange?, pastUpTo: Int, follow: Bool) {
        let call = Shown(word: word, sentence: sentence, pastUpTo: pastUpTo, follow: follow,
                         width: bounds.width)
        guard call != last else { return }
        let newSentence = sentence != last?.sentence
        last = call
        applyPast(upTo: pastUpTo)
        applyUnderline(sentence)
        applyWordColor(word)
        let wanted = follow && followPolicy.shouldFollow(newSentence: newSentence, at: now())
        place(word: word, sentence: sentence, animated: true, follow: wanted)
    }

    // MARK: - Suivi de la lecture

    /// L'utilisateur fait défiler le texte : la vue cesse de le suivre.
    func userDidScroll() {
        followPolicy.userScrolled(at: now())
        lastLineY = nil
    }

    /// Une commande de lecture : la vue reprend le suivi tout de suite.
    func resumeFollowing() {
        guard followPolicy.suspended else { return }
        followPolicy.resume()
        lastLineY = nil
        if let last, last.follow {
            place(word: last.word, sentence: last.sentence, animated: true, follow: true)
        }
    }

    override func scrollWheel(with event: NSEvent) {
        userDidScroll()
        super.scrollWheel(with: event)
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        if let liveScrollObserver { NotificationCenter.default.removeObserver(liveScrollObserver) }
        liveScrollObserver = nil
        // Glisser l'ascenseur ou défiler au trackpad : un défilement « vivant ».
        // Celui que fait `keepLineInPlace` n'en est pas un.
        guard let scroll = enclosingScrollView else { return }
        liveScrollObserver = NotificationCenter.default.addObserver(
            forName: NSScrollView.willStartLiveScrollNotification, object: scroll, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.userDidScroll() }
        }
    }

    // MARK: - Largeur qui change

    /// Le panneau s'élargit ou la fenêtre change de taille, même en pause : la
    /// géométrie du surlignage est refaite, sans glissement, et la hauteur du
    /// texte renvoyée. Seule la largeur compte : renvoyer la hauteur quand la
    /// hauteur change bouclerait avec le panneau qui se redimensionne.
    override func setFrameSize(_ newSize: NSSize) {
        let oldWidth = frame.width
        super.setFrameSize(newSize)
        guard abs(newSize.width - oldWidth) > 0.5 else { return }
        if var last {
            last.width = bounds.width
            self.last = last
            lastLineY = nil
            place(word: last.word, sentence: last.sentence, animated: false,
                  follow: last.follow && !followPolicy.suspended)
        }
        if newSize.width > 0, (textStorage?.length ?? 0) > 0 {
            onContentHeight?(contentHeight)
        }
    }

    // MARK: - Dessin

    override func drawBackground(in dirtyRect: NSRect) {
        super.drawBackground(in: dirtyRect)
        if let band = lineBand {
            theme.wordFill.withAlphaComponent(0.25).setFill()
            band.fill()
        }
        if let r = shown {
            theme.wordFill.setFill()
            NSBezierPath(roundedRect: r.insetBy(dx: -3, dy: -1), xRadius: 4, yRadius: 4).fill()
        }
    }

    /// Pose le surlignage, la bande et, si demandé, la ligne tenue.
    private func place(word: NSRange?, sentence: NSRange?, animated: Bool, follow: Bool) {
        let wordRect = word.flatMap { rect(for: $0) }
        guard let lineRect = wordRect ?? sentence.flatMap({ rect(for: $0) }) else {
            stopLink()
            shown = nil; target = nil; lineBand = nil
            needsDisplay = true
            return
        }
        lineBand = ruler
            ? CGRect(x: 0, y: lineRect.minY - 2, width: bounds.width, height: lineRect.height + 4)
            : nil
        if let wordRect {
            if shown == nil || !animated || reduceMotion {
                stopLink()
                shown = wordRect; target = wordRect
            } else if wordRect != target {
                from = shown; target = wordRect
                animationStart = CACurrentMediaTime()
                startLink()
            }
        } else {
            stopLink()
            shown = nil; target = nil
        }
        needsDisplay = true
        guard follow else { return }
        // Un bloc haut se tient par sa première ligne, pour qu'on le voie
        // commencer.
        let anchor = wordRect
            ?? sentence.flatMap { rect(for: NSRange(location: $0.location, length: min(1, $0.length))) }
            ?? lineRect
        if lastLineY != anchor.minY {
            lastLineY = anchor.minY
            keepLineInPlace(anchor, animated: animated)
        }
    }

    // MARK: - Glissement du surlignage

    private func startLink() {
        guard link == nil else { return }
        let link = displayLink(target: self, selector: #selector(step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func step(_ link: CADisplayLink) {
        guard let from, let target else { stopLink(); return }
        let t = min(1, (CACurrentMediaTime() - animationStart) / Self.glide)
        let e = 1 - pow(1 - t, 3)
        shown = CGRect(x: from.minX + (target.minX - from.minX) * e,
                       y: from.minY + (target.minY - from.minY) * e,
                       width: from.width + (target.width - from.width) * e,
                       height: from.height + (target.height - from.height) * e)
        needsDisplay = true
        if t >= 1 { stopLink() }
    }

    private func stopLink() {
        link?.invalidate()
        link = nil
    }

    // MARK: - Attributs de rendu

    private func applyPast(upTo location: Int) {
        guard location != dimmedUpTo, let tlm = textLayoutManager,
              let all = textRange(NSRange(location: 0, length: textStorage?.length ?? 0)) else { return }
        tlm.removeRenderingAttribute(.foregroundColor, for: all)
        if location > 0, let past = textRange(NSRange(location: 0, length: location)) {
            tlm.addRenderingAttribute(.foregroundColor, value: theme.past, for: past)
        }
        dimmedUpTo = location
        // La couleur du mot lu vient d'être effacée avec le reste.
        colored = nil
    }

    /// Le mot lu prend la couleur prévue sur son fond (noir sur jaune en
    /// contraste élevé, où le blanc du texte ne se lirait pas).
    private func applyWordColor(_ word: NSRange?) {
        guard word != colored, let tlm = textLayoutManager else { return }
        if let old = colored, let r = textRange(old) {
            tlm.removeRenderingAttribute(.foregroundColor, for: r)
        }
        if let new = word, let r = textRange(new) {
            tlm.addRenderingAttribute(.foregroundColor, value: theme.wordText, for: r)
        }
        colored = word
    }

    /// La phrase lue est soulignée ; elle devient aussi la valeur lue par
    /// VoiceOver — une fois par phrase, sans notification.
    private func applyUnderline(_ sentence: NSRange?) {
        guard sentence != underlined, let tlm = textLayoutManager else { return }
        if let old = underlined, let r = textRange(old) {
            tlm.removeRenderingAttribute(.underlineStyle, for: r)
            tlm.removeRenderingAttribute(.underlineColor, for: r)
        }
        if let new = sentence, let r = textRange(new) {
            tlm.addRenderingAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, for: r)
            tlm.addRenderingAttribute(.underlineColor, value: theme.line, for: r)
        }
        underlined = sentence
        setAccessibilityValue(sentence.flatMap { range in
            (textStorage?.string as NSString?)?.substring(with: range)
        })
    }

    // MARK: - Géométrie

    private func textRange(_ range: NSRange) -> NSTextRange? {
        guard let tcm = textLayoutManager?.textContentManager,
              let start = tcm.location(tcm.documentRange.location, offsetBy: range.location),
              let end = tcm.location(start, offsetBy: range.length) else { return nil }
        return NSTextRange(location: start, end: end)
    }

    private func rect(for range: NSRange) -> CGRect? {
        guard let tlm = textLayoutManager, let tr = textRange(range) else { return nil }
        tlm.ensureLayout(for: tr)
        var result: CGRect?
        tlm.enumerateTextSegments(in: tr, type: .standard, options: []) { _, frame, _, _ in
            result = result.map { $0.union(frame) } ?? frame
            return true
        }
        return result?.offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
    }

    /// La ligne lue reste au même endroit : c'est le texte qui monte.
    private func keepLineInPlace(_ rect: CGRect, animated: Bool) {
        guard let clip = enclosingScrollView?.contentView else { return }
        let y = max(0, rect.midY - clip.bounds.height * 0.35)
        let origin = NSPoint(x: 0, y: min(y, max(0, frame.height - clip.bounds.height)))
        if reduceMotion || !animated {
            clip.setBoundsOrigin(origin)
        } else {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.2
                clip.animator().setBoundsOrigin(origin)
            }
        }
        enclosingScrollView?.reflectScrolledClipView(clip)
    }
}
