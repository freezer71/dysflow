import AppKit
import QuartzCore

/// Appelle `tick` à chaque rafraîchissement de l'écran principal.
@MainActor
final class DisplayTicker: NSObject {
    private var link: CADisplayLink?
    private let tick: () -> Void

    init(tick: @escaping () -> Void) {
        self.tick = tick
    }

    func start() {
        guard link == nil, let screen = NSScreen.main else { return }
        let link = screen.displayLink(target: self, selector: #selector(step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func step(_ link: CADisplayLink) {
        tick()
    }
}
