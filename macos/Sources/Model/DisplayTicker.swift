import AppKit
import QuartzCore

/// Appelle `tick` à chaque rafraîchissement de l'écran principal.
@MainActor
final class DisplayTicker: NSObject {
    /// Relais faible : `CADisplayLink` retient sa cible, le relais évite
    /// le cycle avec le ticker et laisse `deinit` invalider le lien.
    private final class Relay: NSObject {
        weak var ticker: DisplayTicker?
        @objc func step(_ link: CADisplayLink) {
            MainActor.assumeIsolated { ticker?.tick() }
        }
    }

    private var link: CADisplayLink?
    private let tick: () -> Void

    init(tick: @escaping () -> Void) {
        self.tick = tick
    }

    deinit {
        link?.invalidate()
    }

    var isRunning: Bool { link != nil }

    func start() {
        guard link == nil, let screen = NSScreen.main else { return }
        let relay = Relay()
        relay.ticker = self
        let link = screen.displayLink(target: relay, selector: #selector(Relay.step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }
}
