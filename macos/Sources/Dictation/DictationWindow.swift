import AppKit
import KeyboardShortcuts
import SwiftUI

extension KeyboardShortcuts.Name {
    /// Échap pendant la dictée : pris seulement tant que la fenêtre est là.
    static let cancelDictation = Self("cancelDictation", default: .init(.escape))
}

enum DictationMetrics {
    static let width: CGFloat = 300
    /// L'icône et les barres, puis deux lignes.
    static let height: CGFloat = 76
    /// Deux lignes de texte.
    static let textHeight: CGFloat = 32
    /// La marge de chaque côté de la carte.
    static let padding: CGFloat = 14
    /// L'écart avec le bas de l'écran (au-dessus du Dock).
    static let bottomGap: CGFloat = 24
    /// La fenêtre : la carte, et une marge autour.
    static let margin: CGFloat = 14
    static let windowWidth: CGFloat = width + 2 * margin
    static let windowHeight: CGFloat = height + 2 * margin
}

/// La fenêtre de la dictée : posée en bas de l'écran, au-dessus de tout,
/// sans jamais prendre le clavier. Le champ où l'on écrivait garde le focus,
/// et c'est là que le texte arrive.
final class DictationPanel: NSPanel {
    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: DictationMetrics.windowWidth,
                                       height: DictationMetrics.windowHeight),
                   styleMask: [.nonactivatingPanel, .borderless],
                   backing: .buffered, defer: true)
        isFloatingPanel = true
        level = .statusBar
        title = String(localized: "Dictation")
        backgroundColor = .clear
        isOpaque = false
        // Pas d'ombre de fenêtre : calculée sur un fond transparent, elle
        // dessine un contour flou et irrégulier autour de la pilule.
        hasShadow = false
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        becomesKeyOnlyIfNeeded = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Les boutons répondent au premier clic, sans que la fenêtre devienne active.
private final class FirstClickHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// Montre et retire la fenêtre, avec l'ouverture et la fermeture de la
/// carte ; Échap n'est pris que pendant ce temps.
@MainActor
final class DictationWindow: DictationPresenter {
    private let panel = DictationPanel()
    private weak var controller: DictationController?
    private let host: NSHostingView<DictationView>
    /// Le fond et le filet de la carte : sa largeur s'ouvre depuis le centre.
    private let card = CALayer()
    /// Le masque du contenu : il s'ouvre avec la carte.
    private let reveal = CALayer()
    private(set) var shown = false

    /// L'ouverture : rapide au départ, puis un long ralenti à l'arrivée.
    static let openCurve = CAMediaTimingFunction(controlPoints: 0.22, 1, 0.36, 1)
    static let openDuration: CFTimeInterval = 0.5
    /// La fermeture : une accélération puis un ralenti, sans à-coup.
    static let closeCurve = CAMediaTimingFunction(controlPoints: 0.65, 0, 0.35, 1)
    static let closeDuration: CFTimeInterval = 0.34

    init(controller: DictationController) {
        self.controller = controller
        host = FirstClickHostingView(rootView: DictationView(controller: controller))
        host.setAccessibilityIdentifier("dictee")
        let size = NSSize(width: DictationMetrics.windowWidth, height: DictationMetrics.windowHeight)
        let container = NSView(frame: NSRect(origin: .zero, size: size))
        container.wantsLayer = true
        host.frame = container.bounds
        host.autoresizingMask = [.width, .height]
        host.wantsLayer = true
        container.addSubview(host)
        panel.contentView = container
        // Les calques au centre de la fenêtre, à la taille de la carte.
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        for layer in [card, reveal] {
            layer.bounds = CGRect(x: 0, y: 0, width: 0, height: DictationMetrics.height)
            layer.position = center
            layer.cornerRadius = 16
            layer.cornerCurve = .continuous
        }
        card.borderWidth = 1
        reveal.backgroundColor = NSColor.black.cgColor
        container.layer?.insertSublayer(card, at: 0)
        host.layer?.mask = reveal
        host.layer?.opacity = 0
    }

    func show() {
        KeyboardShortcuts.enable(.cancelDictation)
        // Les clics traversent la fenêtre, sauf pour « Autoriser ».
        panel.ignoresMouseEvents = controller?.missingPermission == nil
        if !panel.isVisible {
            panel.setFrame(Self.frame(), display: false)
            paint()
            panel.orderFrontRegardless()
        }
        guard !shown else { return }
        shown = true
        animate(open: true)
    }

    func hide() {
        KeyboardShortcuts.disable(.cancelDictation)
        panel.ignoresMouseEvents = true
        guard panel.isVisible, shown else { return }
        shown = false
        animate(open: false)
    }

    /// Les couleurs de la carte, d'après l'apparence du moment.
    private func paint() {
        let dark = panel.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let theme = Theme.resolve(dark: dark,
                                  increaseContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast)
        card.backgroundColor = theme.background.cgColor
        card.borderColor = theme.border.cgColor
    }

    /// Ouvre ou referme la carte, sur le serveur d'affichage (Core Animation) :
    /// rien ne se recalcule à chaque image, l'animation ne saccade pas même
    /// quand l'app travaille. Ouverture : la largeur part de zéro, le contenu
    /// arrive un peu après. Fermeture : le contenu part d'abord.
    private func animate(open: Bool) {
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let full = DictationMetrics.width
        let width: CGFloat = open ? full : 0
        let from = (card.presentation() ?? card).bounds.width
        let duration = reduce ? 0.15 : (open ? Self.openDuration : Self.closeDuration)
        let content = host.layer
        let fromOpacity = (content?.presentation() ?? content)?.opacity ?? 0

        CATransaction.begin()
        // Pas d'animation implicite : seules les nôtres jouent.
        CATransaction.setDisableActions(true)
        CATransaction.setCompletionBlock { [weak self] in
            MainActor.assumeIsolated {
                // Une dictée rouverte pendant la sortie garde sa fenêtre.
                guard let self, !open, !self.shown else { return }
                self.panel.orderOut(nil)
            }
        }
        for layer in [card, reveal] {
            let animation = CABasicAnimation(keyPath: "bounds.size.width")
            animation.fromValue = reduce ? width : from
            animation.toValue = width
            animation.duration = duration
            animation.timingFunction = open ? Self.openCurve : Self.closeCurve
            if !open { animation.beginTime = CACurrentMediaTime() + (reduce ? 0 : 0.05) }
            animation.fillMode = .backwards
            layer.bounds.size.width = width
            layer.add(animation, forKey: "ouverture")
        }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = fromOpacity
        fade.toValue = open ? 1 : 0
        fade.duration = open ? 0.28 : 0.12
        fade.timingFunction = CAMediaTimingFunction(name: open ? .easeOut : .easeIn)
        if open { fade.beginTime = CACurrentMediaTime() + (reduce ? 0 : 0.1) }
        fade.fillMode = .backwards
        content?.opacity = open ? 1 : 0
        content?.add(fade, forKey: "fondu")
        CATransaction.commit()
    }

    /// En bas, au centre de l'écran où se trouve le pointeur.
    private static func frame() -> NSRect {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        let visible = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1280, height: 800)
        return NSRect(x: (visible.midX - DictationMetrics.windowWidth / 2).rounded(),
                      y: visible.minY + DictationMetrics.bottomGap - DictationMetrics.margin,
                      width: DictationMetrics.windowWidth, height: DictationMetrics.windowHeight)
    }
}
