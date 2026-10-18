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
    /// le cycle avec la source et laisse `deinit` invalider le lien. Le lien
    /// est ajouté à la boucle principale : `step` y est toujours appelé.
    @MainActor
    private final class Relay: NSObject {
        weak var frames: DisplayLinkFrames?
        @objc func step(_ link: CADisplayLink) {
            frames?.onFrame?()
        }
    }

    /// Lu par `deinit`, qui n'est pas isolé : seul endroit hors de l'acteur
    /// principal, quand plus rien d'autre ne tient la source.
    nonisolated(unsafe) private var link: CADisplayLink?
    var onFrame: (() -> Void)?
    /// Fabrique le lien d'affichage ; nil si rien à suivre (pas d'écran).
    /// Par défaut, l'écran principal ; une vue passe `NSView.displayLink`,
    /// qui suit la fenêtre d'un écran à l'autre.
    /// Appelée sur l'acteur principal, comme tout le reste : le lien naît
    /// et vit sur le fil principal, il n'a jamais à le quitter (CADisplayLink
    /// n'est pas Sendable).
    private let makeLink: @MainActor (AnyObject, Selector) -> CADisplayLink?
    /// La cadence demandée à l'écran : un écran à 120 Hz n'impose pas 120
    /// relectures par seconde à qui n'en a pas besoin.
    private let rate: CAFrameRateRange

    /// Le suivi du modèle : Rust publie à 60 Hz, le mot change 3 à 5 fois
    /// par seconde ; 30 images suffisent.
    static let modelRate = CAFrameRateRange(minimum: 15, maximum: 30, preferred: 30)
    /// Le trait qui suit la voix : jamais plus de 60 images par seconde.
    static let lineRate = CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)

    init(rate: CAFrameRateRange = DisplayLinkFrames.modelRate,
         makeLink: (@MainActor (AnyObject, Selector) -> CADisplayLink?)? = nil) {
        self.rate = rate
        self.makeLink = makeLink ?? { target, selector in
            NSScreen.main?.displayLink(target: target, selector: selector)
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
        link.preferredFrameRateRange = rate
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }
}
