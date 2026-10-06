import AppKit
import QuartzCore

/// Ce qui cadence le suivi : une image à la fois, à la cadence de l'écran en
/// production, à la main dans les tests (un écran en veille n'envoie plus
/// d'images, et un test ne doit pas dépendre de l'état de l'écran).
@MainActor
protocol FrameSource: AnyObject {
    /// Appelé à chaque image tant que la source tourne.
    var onFrame: (() -> Void)? { get set }
    var isRunning: Bool { get }
    func start()
    func stop()
}

/// La source de production : un `CADisplayLink` (celui de l'écran principal par
/// défaut).
@MainActor
final class DisplayLinkFrames: NSObject, FrameSource {
    /// Relais faible : `CADisplayLink` retient sa cible, le relais évite
    /// le cycle avec la source et laisse `deinit` invalider le lien.
    private final class Relay: NSObject {
        weak var frames: DisplayLinkFrames?
        @objc func step(_ link: CADisplayLink) {
            MainActor.assumeIsolated { frames?.onFrame?() }
        }
    }

    private var link: CADisplayLink?
    var onFrame: (() -> Void)?
    /// Fabrique le lien d'affichage ; nil si rien à suivre (pas d'écran).
    /// Par défaut, l'écran principal ; une vue passe `NSView.displayLink`,
    /// qui suit la fenêtre d'un écran à l'autre.
    private let makeLink: (AnyObject, Selector) -> CADisplayLink?

    init(makeLink: ((AnyObject, Selector) -> CADisplayLink?)? = nil) {
        self.makeLink = makeLink ?? { target, selector in
            MainActor.assumeIsolated { NSScreen.main?.displayLink(target: target, selector: selector) }
        }
    }

    deinit {
        link?.invalidate()
    }

    var isRunning: Bool { link != nil }

    func start() {
        guard link == nil else { return }
        let relay = Relay()
        relay.frames = self
        guard let link = makeLink(relay, #selector(Relay.step)) else { return }
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }
}
