import AppKit
import SwiftUI

/// Relit l'état du lecteur à intervalle régulier tant que la fenêtre des
/// réglages est visible, et s'arrête dès qu'elle ne l'est plus : fermée,
/// réduite ou entièrement couverte. L'arrêt ne dépend pas de la scène
/// Settings, qui garde sa vue en vie fenêtre fermée.
@MainActor
final class RefreshLoop {
    private let interval: Duration
    private let tick: () -> Void
    private var task: Task<Void, Never>?

    init(interval: Duration = .milliseconds(250), tick: @escaping () -> Void) {
        self.interval = interval
        self.tick = tick
    }

    var isRunning: Bool { task != nil }

    func setVisible(_ visible: Bool) {
        if visible {
            guard task == nil else { return }
            task = Task { [weak self, interval] in
                while !Task.isCancelled {
                    self?.tick()
                    try? await Task.sleep(for: interval)
                }
            }
        } else {
            task?.cancel()
            task = nil
        }
    }

    deinit { task?.cancel() }
}

/// Dit si la fenêtre qui porte la vue est visible à l'écran, et quand elle
/// se ferme. Suit la fenêtre réelle (`NSWindow`), pas le cycle de vie SwiftUI.
struct WindowVisibility: NSViewRepresentable {
    let onVisible: (Bool) -> Void
    let onClose: () -> Void

    func makeNSView(context: Context) -> WatcherView {
        let view = WatcherView()
        view.onVisible = onVisible
        view.onClose = onClose
        return view
    }

    func updateNSView(_ view: WatcherView, context: Context) {
        view.onVisible = onVisible
        view.onClose = onClose
    }

    final class WatcherView: NSView {
        var onVisible: (Bool) -> Void = { _ in }
        var onClose: () -> Void = {}
        /// Lu par `deinit`, hors de l'acteur principal : la vue est alors
        /// seule à le tenir, aucun accès concurrent n'est possible.
        nonisolated(unsafe) private var observers: [NSObjectProtocol] = []

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
            guard let window else { onVisible(false); return }
            let center = NotificationCenter.default
            observers.append(center.addObserver(forName: NSWindow.didChangeOcclusionStateNotification,
                                                object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.report() }
            })
            observers.append(center.addObserver(forName: NSWindow.willCloseNotification,
                                                object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.onVisible(false)
                    self?.onClose()
                }
            })
            report()
        }

        private func report() {
            guard let window else { onVisible(false); return }
            onVisible(window.isVisible && window.occlusionState.contains(.visible))
        }

        deinit { observers.forEach(NotificationCenter.default.removeObserver) }
    }
}
