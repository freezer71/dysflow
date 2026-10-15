import AppKit
import QuartzCore

/// Le texte lu, « encre calme » : le mot lu d'une encre plus sombre, le
/// déjà-lu atténué mot par mot, et un trait fin sous la ligne qui avance avec
/// la voix. TextKit 2 ne met en page que ce qui est visible.
final class ReadingTextView: NSTextView {
    var theme: Theme = .paper { didSet { if theme != oldValue { needsDisplay = true } } }
    var ruler = true
    var onContentHeight: ((CGFloat) -> Void)?
    /// « Copier » du menu contextuel : l'app décide (sélection, sinon phrase).
    var onCopy: (() -> Void)?
    /// « Lire à partir d'ici » : l'endroit cliqué, en caractère du texte.
    var onReadFrom: ((Int) -> Void)?
    /// L'horloge du suivi, remplaçable pour les tests.
    var now: () -> Date = Date.init
    /// Réduire les animations, remplaçable pour les tests.
    var reduceMotion: () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
    /// La part du mot lu que la voix a dite (0 à 1), relue à chaque image
    /// pendant la lecture — hors de SwiftUI, qui ne doit pas la voir changer.
    var liveProgress: (() -> Float)?
    /// La lecture avance : le trait se montre et suit la voix. En pause, il
    /// disparaît et le lien d'affichage s'arrête.
    var playing = false {
        didSet { if playing != oldValue { updateCursorLine() } }
    }

    /// Le cadre du mot lu, s'il y en a un.
    private(set) var wordRect: CGRect?
    /// La bande sous la ligne lue (ou sous le bloc annoncé).
    private(set) var lineBand: CGRect?
    /// Le trait sous la ligne tel qu'il est dessiné.
    private(set) var cursorLine: CGRect?
    /// Le début de la ligne du mot lu.
    private var lineStartX: CGFloat = 0
    /// La part du mot dite, qui ne recule pas dans un même mot.
    private var progress: CGFloat = 0
    /// Les images qui font avancer le trait : le lien de la vue (voir `init`),
    /// ou une source manuelle dans les tests.
    var frames: FrameSource = DisplayLinkFrames() {
        didSet { oldValue.stop(); wireFrames() }
    }
    private var dimmedUpTo = 0
    private var colored: NSRange?
    private var spoken: NSRange?
    private var lastLineY: CGFloat?
    private var followPolicy = FollowPolicy()
    // Retirés par `deinit` (non isolé) : NotificationCenter l'accepte de
    // n'importe quel fil, et plus rien d'autre ne tient la vue.
    nonisolated(unsafe) private var liveScrollObserver: NSObjectProtocol?
    /// La fenêtre entièrement cachée par d'autres (ou l'écran verrouillé) :
    /// su par la notification d'occultation, qui arrive en différé.
    private var occluded = false
    nonisolated(unsafe) private var occlusionObserver: NSObjectProtocol?
    /// La fenêtre retirée ou remise (l'arrière-plan la garde, la vue
    /// dedans) : su tout de suite, en observant `isVisible`.
    private var visibilityObservation: NSKeyValueObservation?

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
        // Le lien de la vue suit sa fenêtre d'un écran à l'autre (cadence et
        // écran changent avec elle), ce que ne fait pas l'écran principal.
        frames = DisplayLinkFrames { [weak self] target, selector in
            self?.displayLink(target: target, selector: selector)
        }
        isEditable = false
        // Le texte se sélectionne (glisser, double clic : un mot, triple
        // clic : le paragraphe) ; il ne se modifie qu'en édition, ailleurs.
        isSelectable = true
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
        if let occlusionObserver { NotificationCenter.default.removeObserver(occlusionObserver) }
    }

    func setDocument(_ text: NSAttributedString) {
        textStorage?.setAttributedString(text)
        // Les attributs de rendu tiennent au gestionnaire de mise en page, pas
        // au texte : ceux de l'ancien document ne doivent pas survivre.
        if let tlm = textLayoutManager {
            tlm.removeRenderingAttribute(.foregroundColor, for: tlm.documentRange)
        }
        wordRect = nil; lineBand = nil; dimmedUpTo = 0; spoken = nil; colored = nil; announced = nil
        lastLineY = nil; last = nil
        updateCursorLine()
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

    /// Le texte sélectionné, nil sans sélection.
    var selectedText: String? {
        let range = selectedRange()
        guard range.length > 0, NSMaxRange(range) <= (string as NSString).length else { return nil }
        return (string as NSString).substring(with: range)
    }

    // MARK: - Menu contextuel

    override func menu(for event: NSEvent) -> NSMenu? {
        let point = convert(event.locationInWindow, from: nil)
        let index = characterIndexForInsertion(at: point)
        let menu = NSMenu()
        let copy = NSMenuItem(title: "Copier", action: #selector(copyFromMenu(_:)), keyEquivalent: "")
        copy.target = self
        let read = NSMenuItem(title: "Lire à partir d'ici", action: #selector(readFromMenu(_:)), keyEquivalent: "")
        read.target = self
        read.representedObject = index
        menu.items = [copy, read]
        return menu
    }

    @objc private func copyFromMenu(_ sender: Any?) {
        onCopy?()
    }

    @objc private func readFromMenu(_ sender: NSMenuItem) {
        guard let index = sender.representedObject as? Int else { return }
        onReadFrom?(index)
    }

    /// Montre le mot lu et sa phrase. Sans mot (un bloc de code ou un tableau
    /// qu'on annonce), la bande se pose sur la phrase, c'est-à-dire le bloc,
    /// et il n'y a pas de trait.
    func show(word: NSRange?, sentence: NSRange?, pastUpTo: Int, follow: Bool) {
        let call = Shown(word: word, sentence: sentence, pastUpTo: pastUpTo, follow: follow,
                         width: bounds.width)
        guard call != last else { return }
        let newSentence = sentence != last?.sentence
        last = call
        applyPast(upTo: pastUpTo)
        announce(sentence)
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
    /// géométrie du mot lu, de la bande et du trait est refaite, et la hauteur du
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
        if let line = cursorLine, line.width > 0 {
            theme.line.withAlphaComponent(0.9).setFill()
            let radius = line.height / 2
            NSBezierPath(roundedRect: line, xRadius: radius, yRadius: radius).fill()
        }
    }

    /// Pose le mot lu, la bande, le trait et, si demandé, la ligne tenue.
    private func place(word: NSRange?, sentence: NSRange?, animated: Bool, follow: Bool) {
        let rect = word.flatMap { rect(for: $0) }
        if word != spoken { progress = 0 }
        spoken = word
        wordRect = rect
        if let word, let rect { lineStartX = lineStart(of: word, rect: rect) }
        updateCursorLine()
        guard let lineRect = rect ?? sentence.flatMap({ self.rect(for: $0) }) else {
            lineBand = nil
            needsDisplay = true
            return
        }
        lineBand = ruler
            ? CGRect(x: 0, y: lineRect.minY - 2, width: bounds.width, height: lineRect.height + 4)
            : nil
        needsDisplay = true
        guard follow else { return }
        // Un bloc haut se tient par sa première ligne, pour qu'on le voie
        // commencer.
        let anchor = rect
            ?? sentence.flatMap { self.rect(for: NSRange(location: $0.location, length: min(1, $0.length))) }
            ?? lineRect
        if lastLineY != anchor.minY {
            lastLineY = anchor.minY
            keepLineInPlace(anchor, animated: animated)
        }
    }

    // MARK: - Le trait sous la ligne

    /// Vrai quand le trait suit la voix à chaque image.
    var isAnimatingLine: Bool { frames.isRunning }

    /// Montre, déplace ou retire le trait, et ne fait tourner le lien
    /// d'affichage que si la lecture avance et que le mouvement est permis.
    private func updateCursorLine() {
        let motion = !reduceMotion()
        if playing, wordRect != nil, motion, windowShows {
            startLink()
        } else {
            stopLink()
        }
        if playing, wordRect != nil, motion { readProgress() }
        setCursorLine(playing ? wordRect.map {
            CursorLine.rect(word: $0, lineStartX: lineStartX, progress: progress,
                            reduceMotion: !motion)
        } : nil)
    }

    /// Relit la voix ; dans un même mot, le trait ne recule pas.
    private func readProgress() {
        guard let live = liveProgress?() else { return }
        progress = max(progress, CGFloat(live))
    }

    private func setCursorLine(_ line: CGRect?) {
        guard line != cursorLine else { return }
        // Seul le trait est redessiné, pas tout le texte.
        for r in [cursorLine, line].compactMap({ $0 }) {
            setNeedsDisplay(r.insetBy(dx: -2, dy: -2))
        }
        cursorLine = line
    }

    private func wireFrames() {
        frames.onFrame = { [weak self] in self?.step() }
    }

    private func startLink() {
        frames.start()
    }

    private func step() {
        guard playing, let wordRect, windowShows else { stopLink(); return }
        readProgress()
        setCursorLine(CursorLine.rect(word: wordRect, lineStartX: lineStartX,
                                      progress: progress, reduceMotion: false))
    }

    private func stopLink() {
        frames.stop()
    }

    /// La fenêtre se voit : ni retirée (l'arrière-plan la garde, la vue
    /// dedans), ni cachée tout entière. Sinon le trait ne tourne pas.
    private var windowShows: Bool {
        guard let window else { return false }
        return window.isVisible && !occluded
    }

    /// Hors fenêtre, le lien s'arrête (le lien retient sa cible) ; de retour, il
    /// reprend si la lecture avance. La fenêtre retirée ou cachée, de même.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let occlusionObserver { NotificationCenter.default.removeObserver(occlusionObserver) }
        occlusionObserver = nil
        visibilityObservation = nil
        occluded = false
        if let window {
            visibilityObservation = window.observe(\.isVisible) { [weak self] _, _ in
                MainActor.assumeIsolated { self?.updateCursorLine() }
            }
            occlusionObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main
            ) { [weak self] note in
                let window = note.object as? NSWindow
                MainActor.assumeIsolated {
                    guard let self, let w = window else { return }
                    self.occluded = !w.occlusionState.contains(.visible)
                    self.updateCursorLine()
                }
            }
        }
        updateCursorLine()
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

    /// Le mot lu prend une encre plus sombre — la couleur seule : changer
    /// la graisse referait la ligne.
    private func applyWordColor(_ word: NSRange?) {
        guard word != colored, let tlm = textLayoutManager else { return }
        if let old = colored, let r = textRange(old) {
            // L'ancien mot, s'il est maintenant derrière, rejoint le déjà-lu.
            if old.location < dimmedUpTo {
                tlm.addRenderingAttribute(.foregroundColor, value: theme.past, for: r)
            } else {
                tlm.removeRenderingAttribute(.foregroundColor, for: r)
            }
        }
        if let new = word, let r = textRange(new) {
            tlm.addRenderingAttribute(.foregroundColor, value: theme.wordText, for: r)
        }
        colored = word
    }

    /// La phrase lue devient la valeur lue par VoiceOver — une fois par
    /// phrase, sans notification.
    private var announced: NSRange?
    private func announce(_ sentence: NSRange?) {
        guard sentence != announced else { return }
        announced = sentence
        setAccessibilityValue(sentence.flatMap { range in
            (textStorage?.string as NSString?)?.substring(with: range)
        })
    }

    // MARK: - Lignes affichées

    /// La ligne affichée voisine de celle qui contient `location` — au-dessus
    /// si `direction` est négatif, au-dessous sinon —, la première que
    /// `accept` retient : les lignes sans mot (code, ligne vide) se sautent.
    /// Les retours à la ligne comptent, d'un paragraphe à l'autre. Calculée à
    /// la touche, jamais à chaque image.
    func adjacentLine(from location: Int, direction: Int,
                      where accept: (NSRange) -> Bool) -> NSRange? {
        guard let tlm = textLayoutManager, let here = textRange(NSRange(location: location, length: 0)),
              let fragment = tlm.textLayoutFragment(for: here.location) else { return nil }
        let current = lineRanges(of: fragment).first { NSLocationInRange(location, $0) }?.location ?? location
        let down = direction > 0
        let start = down ? fragment.rangeInElement.location : fragment.rangeInElement.endLocation
        var found: NSRange?
        tlm.enumerateTextLayoutFragments(from: start, options: down ? [.ensuresLayout]
                                                                    : [.reverse, .ensuresLayout]) { f in
            let lines = lineRanges(of: f)
            for line in down ? lines : lines.reversed() {
                let beyond = down ? line.location > current : line.location < current
                if beyond, accept(line) { found = line; return false }
            }
            return true
        }
        return found
    }

    /// Les lignes d'un fragment, en plages du texte entier.
    private func lineRanges(of fragment: NSTextLayoutFragment) -> [NSRange] {
        guard let tcm = textLayoutManager?.textContentManager else { return [] }
        let element = fragment.textElement?.elementRange?.location ?? fragment.rangeInElement.location
        let base = tcm.offset(from: tcm.documentRange.location, to: element)
        return fragment.textLineFragments.map {
            NSRange(location: base + $0.characterRange.location, length: $0.characterRange.length)
        }
    }

    // MARK: - Géométrie

    private func textRange(_ range: NSRange) -> NSTextRange? {
        guard let tcm = textLayoutManager?.textContentManager,
              let start = tcm.location(tcm.documentRange.location, offsetBy: range.location),
              let end = tcm.location(start, offsetBy: range.length) else { return nil }
        return NSTextRange(location: start, end: end)
    }

    func rectForTesting(_ range: NSRange) -> CGRect? { rect(for: range) }

    /// Le début de la ligne où se trouve le mot : le bord gauche du premier
    /// segment de son paragraphe qui est sur la même ligne.
    private func lineStart(of word: NSRange, rect: CGRect) -> CGFloat {
        guard let tlm = textLayoutManager, let tcm = tlm.textContentManager,
              let wordRange = textRange(word),
              let fragment = tlm.textLayoutFragment(for: wordRange.location) else { return rect.minX }
        let paragraph = tcm.offset(from: tcm.documentRange.location,
                                   to: fragment.rangeInElement.location)
        guard paragraph <= word.location,
              let span = textRange(NSRange(location: paragraph, length: NSMaxRange(word) - paragraph))
        else { return rect.minX }
        var start = rect.minX
        tlm.enumerateTextSegments(in: span, type: .standard, options: []) { _, frame, _, _ in
            let f = frame.offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
            if abs(f.minY - rect.minY) < 1 { start = min(start, f.minX) }
            return true
        }
        return start
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
        if reduceMotion() || !animated {
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
