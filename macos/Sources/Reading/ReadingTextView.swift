import AppKit
import QuartzCore

/// Le texte lu, avec son mot surligné qui glisse, sa ligne soulignée, et le
/// déjà-lu atténué. TextKit 2 ne met en page que ce qui est visible.
final class ReadingTextView: NSTextView {
    var theme: Theme = .paper { didSet { if theme != oldValue { needsDisplay = true } } }
    var ruler = true
    var onContentHeight: ((CGFloat) -> Void)?

    /// Le rectangle surligné tel qu'il est dessiné, en glissement compris.
    private(set) var shown: CGRect?
    private var from: CGRect?
    private var target: CGRect?
    private var lineBand: CGRect?
    private var animationStart: CFTimeInterval = 0
    private var link: CADisplayLink?
    private var dimmedUpTo = 0
    private var underlined: NSRange?
    private var colored: NSRange?
    private var lastLineY: CGFloat?
    private static let glide: CFTimeInterval = 0.08

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
        setAccessibilityRole(.staticText)
        setAccessibilityLabel("Texte lu")
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
        lastLineY = nil
        needsDisplay = true
        onContentHeight?(contentHeight)
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

    func show(word: NSRange?, sentence: NSRange?, pastUpTo: Int, follow: Bool) {
        applyPast(upTo: pastUpTo)
        applyUnderline(sentence)
        applyWordColor(word)
        guard let word, let rect = rect(for: word) else {
            target = nil; shown = nil; needsDisplay = true
            return
        }
        lineBand = ruler ? CGRect(x: 0, y: rect.minY - 2, width: bounds.width, height: rect.height + 4) : nil
        if shown == nil || reduceMotion {
            shown = rect; target = rect
            needsDisplay = true
        } else if rect != target {
            from = shown; target = rect
            animationStart = CACurrentMediaTime()
            startLink()
        }
        if follow, lastLineY != rect.minY {
            lastLineY = rect.minY
            keepLineInPlace(rect)
        }
        setAccessibilityValue(sentence.flatMap { range in
            (textStorage?.string as NSString?)?.substring(with: range)
        })
    }

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
    private func keepLineInPlace(_ rect: CGRect) {
        guard let clip = enclosingScrollView?.contentView else { return }
        let y = max(0, rect.midY - clip.bounds.height * 0.35)
        let origin = NSPoint(x: 0, y: min(y, max(0, frame.height - clip.bounds.height)))
        if reduceMotion {
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
