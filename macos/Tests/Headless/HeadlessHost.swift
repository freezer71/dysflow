import AppKit
import ObjectiveC
import XCTest

/// Les tests de l'app tournent sans rien montrer : l'hôte des tests est
/// Dysflow.app (TEST_HOST), au même identifiant que l'app de l'utilisateur,
/// qui travaille pendant ce temps sur ce Mac.
///
/// Classe principale du bundle de test (`NSPrincipalClass`) : XCTest la crée
/// au chargement du bundle, avant que l'app ne pose ses scènes et avant le
/// premier test. Tout se joue ici, dans le bundle de test : l'app livrée
/// (Release) n'en contient rien.
///
/// - Les fenêtres de l'hôte s'ordonnent et se placent comme d'habitude
///   (`isVisible`, cadre, mise en page), mais leur opacité reste nulle et
///   elles laissent passer la souris, quoi que l'app demande. Les lectures
///   (`alphaValue`, `ignoresMouseEvents`) rendent l'état réel : AppKit les
///   relit pour resynchroniser la fenêtre avec le serveur de fenêtres, et
///   une lecture qui rendrait la valeur demandée la remettrait visible
///   (mesuré : opacité 1 une demi-seconde après la mise devant).
/// - Aucune ne devient la fenêtre clé : le panneau du lecteur (un
///   `NSPanel` non activant) prendrait le clavier de l'utilisateur.
/// - L'hôte ne s'active jamais.
/// - L'icône de la barre des menus vit dans une barre à part, jamais
///   affichée.
/// - Le presse-papiers général est un presse-papiers nommé, propre aux
///   tests : rien n'est lu ni écrit dans celui de l'utilisateur.
///
/// `HeadlessWitness` fait échouer tout test pendant lequel l'hôte s'est
/// montré quand même.
@objc(DysflowHeadlessTestHost)
final class HeadlessTestHost: NSObject {
    override init() {
        super.init()
        MainActor.assumeIsolated {
            Headless.install()
            HeadlessWitness.shared.start()
        }
    }
}

@MainActor
enum Headless {
    /// Le presse-papiers qui remplace le presse-papiers général dans l'hôte.
    static let pasteboardName = NSPasteboard.Name("fr.dysflow.tests.presse-papiers")
    private(set) static var installed = false

    static func install() {
        guard !installed else { return }
        installed = true
        let window: AnyClass = NSWindow.self
        exchange(window, #selector(setter: NSWindow.alphaValue), #selector(NSWindow.headless_setAlphaValue(_:)))
        exchange(window, #selector(setter: NSWindow.ignoresMouseEvents),
                 #selector(NSWindow.headless_setIgnoresMouseEvents(_:)))
        exchange(window, #selector(NSWindow.order(_:relativeTo:)), #selector(NSWindow.headless_order(_:relativeTo:)))
        exchange(window, #selector(NSWindow.orderFront(_:)), #selector(NSWindow.headless_orderFront(_:)))
        exchange(window, #selector(NSWindow.orderBack(_:)), #selector(NSWindow.headless_orderBack(_:)))
        exchange(window, #selector(NSWindow.orderFrontRegardless), #selector(NSWindow.headless_orderFrontRegardless))
        exchange(window, #selector(NSWindow.setIsVisible(_:)), #selector(NSWindow.headless_setIsVisible(_:)))
        exchange(window, #selector(NSWindow.makeKeyAndOrderFront(_:)),
                 #selector(NSWindow.headless_makeKeyAndOrderFront(_:)))
        exchange(window, #selector(NSWindow.makeKey), #selector(NSWindow.headless_makeKey))

        let app: AnyClass = NSApplication.self
        exchange(app, #selector(NSApplication.activate(ignoringOtherApps:)),
                 #selector(NSApplication.headless_activate(ignoringOtherApps:)))
        exchange(app, #selector(NSApplication.activate as (NSApplication) -> () -> Void),
                 #selector(NSApplication.headless_activateNow))
        let running: AnyClass = NSRunningApplication.self
        exchange(running, #selector(NSRunningApplication.activate(options:)),
                 #selector(NSRunningApplication.headless_activate(options:)))
        exchange(running, #selector(NSRunningApplication.activate(from:options:)),
                 #selector(NSRunningApplication.headless_activate(from:options:)))

        // Méthodes de classe : sur la métaclasse.
        exchange(object_getClass(NSStatusBar.self)!, NSSelectorFromString("systemStatusBar"),
                 #selector(NSStatusBar.headless_system))
        let board: AnyClass = object_getClass(NSPasteboard.self)!
        exchange(board, NSSelectorFromString("generalPasteboard"), #selector(NSPasteboard.headless_general))
        exchange(board, NSSelectorFromString("pasteboardWithName:"), #selector(NSPasteboard.headless_pasteboard(name:)))
    }

    /// Échange deux implémentations ; une méthode héritée est d'abord copiée
    /// sur la classe, pour ne jamais toucher à celle du parent.
    private static func exchange(_ cls: AnyClass, _ original: Selector, _ replacement: Selector) {
        guard let o = class_getInstanceMethod(cls, original),
              let r = class_getInstanceMethod(cls, replacement) else {
            fatalError("headless : \(cls).\(original) introuvable")
        }
        if class_addMethod(cls, original, method_getImplementation(r), method_getTypeEncoding(r)) {
            class_replaceMethod(cls, replacement, method_getImplementation(o), method_getTypeEncoding(o))
        } else {
            method_exchangeImplementations(o, r)
        }
    }

    /// La barre des menus de l'hôte : une barre à part, qui n'a pas de
    /// fenêtre à l'écran.
    static let statusBar = NSStatusBar()
}

/// Après l'échange, `headless_x` appelle l'implémentation d'origine de `x`.
extension NSWindow {
    /// Avant toute mise à l'écran : invisible et traversée par la souris.
    private func headless_hide() {
        headless_setAlphaValue(0)
        headless_setIgnoresMouseEvents(true)
    }

    @objc func headless_setAlphaValue(_ value: CGFloat) {
        headless_setAlphaValue(0)
    }

    @objc func headless_setIgnoresMouseEvents(_ value: Bool) {
        headless_setIgnoresMouseEvents(true)
    }

    @objc func headless_order(_ place: NSWindow.OrderingMode, relativeTo other: Int) {
        if place != .out { headless_hide() }
        headless_order(place, relativeTo: other)
    }

    @objc func headless_orderFront(_ sender: Any?) {
        headless_hide()
        headless_orderFront(sender)
    }

    @objc func headless_orderBack(_ sender: Any?) {
        headless_hide()
        headless_orderBack(sender)
    }

    @objc func headless_orderFrontRegardless() {
        headless_hide()
        headless_orderFrontRegardless()
    }

    @objc func headless_setIsVisible(_ visible: Bool) {
        if visible { headless_hide() }
        headless_setIsVisible(visible)
    }

    /// Mise devant, jamais fenêtre clé.
    @objc func headless_makeKeyAndOrderFront(_ sender: Any?) {
        orderFront(sender)
    }

    @objc func headless_makeKey() {}
}

extension NSApplication {
    @objc func headless_activate(ignoringOtherApps flag: Bool) {}
    @objc func headless_activateNow() {}
}

extension NSRunningApplication {
    @objc func headless_activate(options: NSApplication.ActivationOptions) -> Bool {
        if processIdentifier == getpid() { return false }
        return headless_activate(options: options)
    }

    @objc func headless_activate(from application: NSRunningApplication,
                                 options: NSApplication.ActivationOptions) -> Bool {
        if processIdentifier == getpid() { return false }
        return headless_activate(from: application, options: options)
    }
}

extension NSStatusBar {
    @MainActor @objc class func headless_system() -> NSStatusBar { Headless.statusBar }
}

extension NSPasteboard {
    @MainActor @objc class func headless_general() -> NSPasteboard {
        headless_pasteboard(name: Headless.pasteboardName)
    }

    @MainActor @objc class func headless_pasteboard(name: NSPasteboard.Name) -> NSPasteboard {
        headless_pasteboard(name: name == .general ? Headless.pasteboardName : name)
    }
}
