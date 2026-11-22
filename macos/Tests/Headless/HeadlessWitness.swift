import AppKit
import ObjectiveC
import XCTest

/// Le témoin du mode headless (`HeadlessTestHost`) : un test échoue si,
/// pendant qu'il tournait (ou depuis le test d'avant), une fenêtre de l'hôte
/// a été visible à l'écran (opacité > 0), si l'hôte est devenu l'app active,
/// ou s'il a pris une fenêtre clé (le clavier de l'utilisateur).
///
/// L'écran est relevé toutes les 50 ms, et une dernière fois à la fin de
/// chaque test (`invokeTest`, setUp et tearDown compris).
final class HeadlessWitness: NSObject, @unchecked Sendable {
    static let shared = HeadlessWitness()

    private let lock = NSLock()
    private var noted: [String] = []
    private var timer: DispatchSourceTimer?
    private var activation: NSObjectProtocol?

    @MainActor
    func start() {
        guard timer == nil else { return }
        Self.exchangeInvokeTest()
        activation = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: nil
        ) { [weak self] _ in self?.note("l'hôte est devenu l'app active") }
        let timer = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
        timer.schedule(deadline: .now(), repeating: .milliseconds(50))
        timer.setEventHandler { [weak self] in self?.sampleScreen() }
        timer.resume()
        self.timer = timer
    }

    func note(_ violation: String) {
        lock.lock()
        defer { lock.unlock() }
        // Un relevé toutes les 50 ms : la même fenêtre n'est dite qu'une fois.
        if !noted.contains(violation) { noted.append(violation) }
    }

    private func sampleScreen() {
        let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
        for violation in Self.visibleWindows(in: windows, pid: getpid()) { note(violation) }
    }

    /// Ce qui a été relevé depuis l'appel précédent, avec l'état présent.
    @MainActor
    func takeViolations() -> [String] {
        sampleScreen()
        if NSApp.isActive || NSWorkspace.shared.frontmostApplication?.processIdentifier == getpid() {
            note("l'hôte est l'app active")
        }
        if let key = NSApp.keyWindow {
            note("l'hôte a une fenêtre clé : \(type(of: key)) « \(key.title) »")
        }
        lock.lock()
        defer { lock.unlock() }
        let taken = noted
        noted = []
        return taken
    }

    /// Les fenêtres de `pid` visibles dans un relevé de `CGWindowListCopyWindowInfo`.
    static func visibleWindows(in windows: [[String: Any]], pid: pid_t) -> [String] {
        windows.compactMap { info in
            guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
                  let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue, alpha > 0
            else { return nil }
            let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0
            let name = info[kCGWindowName as String] as? String ?? ""
            let bounds = (info[kCGWindowBounds as String] as? [String: Any]).flatMap {
                CGRect(dictionaryRepresentation: $0 as CFDictionary)
            } ?? .zero
            return "fenêtre de l'hôte à l'écran : « \(name) », couche \(layer), opacité \(alpha), \(bounds)"
        }
    }

    /// `invokeTest` couvre setUp, le test et tearDown : le relevé suit, et
    /// l'échec se rattache encore au test qui tourne.
    @MainActor
    private static func exchangeInvokeTest() {
        let cls: AnyClass = XCTestCase.self
        guard let original = class_getInstanceMethod(cls, #selector(XCTestCase.invokeTest)),
              let replacement = class_getInstanceMethod(cls, #selector(XCTestCase.headless_invokeTest)) else {
            fatalError("headless : XCTestCase.invokeTest introuvable")
        }
        method_exchangeImplementations(original, replacement)
    }
}

extension XCTestCase {
    /// Après l'échange : appelle l'`invokeTest` d'origine, puis le témoin.
    @objc func headless_invokeTest() {
        headless_invokeTest()
        let violations = MainActor.assumeIsolated { HeadlessWitness.shared.takeViolations() }
        for violation in violations {
            record(XCTIssue(type: .assertionFailure, compactDescription: "Headless : \(violation)"))
        }
    }
}
