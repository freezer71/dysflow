import AppKit
import XCTest

/// Le mode headless de l'hôte des tests (`HeadlessTestHost`) et son témoin.
@MainActor
final class HeadlessTests: XCTestCase {
    func testTheHostIsHeadlessBeforeTheFirstTest() {
        XCTAssertTrue(Headless.installed)
        XCTAssertTrue(NSStatusBar.system === Headless.statusBar, "l'icône de la barre des menus n'est pas affichée")
        XCTAssertEqual(NSPasteboard.general.name, Headless.pasteboardName, "jamais le presse-papiers de l'utilisateur")
        XCTAssertEqual(NSPasteboard(name: .general).name, Headless.pasteboardName)
        XCTAssertEqual(NSApp.activationPolicy(), .accessory)
    }

    /// Une fenêtre mise devant est visible pour l'app (`isVisible`), mais
    /// pas pour l'utilisateur : opacité nulle, ni souris ni clavier.
    func testAWindowPutInFrontStaysInvisibleToTheUser() throws {
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 200, height: 100),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.makeKeyAndOrderFront(nil)
        XCTAssertTrue(window.isVisible)
        XCTAssertFalse(window.isKeyWindow)
        window.alphaValue = 1
        window.ignoresMouseEvents = false
        XCTAssertEqual(window.alphaValue, 0)
        XCTAssertTrue(window.ignoresMouseEvents)
        // AppKit resynchronise la fenêtre après coup : elle doit le rester.
        RunLoop.main.run(until: Date().addingTimeInterval(1))

        let listed = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(window.windowNumber))
            as? [[String: Any]] ?? []
        let info = try XCTUnwrap(listed.first)
        XCTAssertEqual((info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue, 0, "à l'écran : opacité nulle")
    }

    func testOnlyThisProcessWindowsWithAnOpacityCount() {
        let me = getpid()
        let windows: [[String: Any]] = [
            [kCGWindowOwnerPID as String: NSNumber(value: me), kCGWindowAlpha as String: NSNumber(value: 1.0),
             kCGWindowName as String: "Dysflow", kCGWindowLayer as String: NSNumber(value: 3)],
            [kCGWindowOwnerPID as String: NSNumber(value: me), kCGWindowAlpha as String: NSNumber(value: 0.0)],
            [kCGWindowOwnerPID as String: NSNumber(value: me + 1), kCGWindowAlpha as String: NSNumber(value: 1.0)],
        ]
        let found = HeadlessWitness.visibleWindows(in: windows, pid: me)
        XCTAssertEqual(found.count, 1)
        XCTAssertTrue(found.first?.contains("« Dysflow », couche 3, opacité 1.0") == true, String(describing: found))
    }

    /// Un écart relevé pendant un test fait échouer ce test, après son tearDown.
    func testTheWitnessFailsTheTestDuringWhichTheHostShowedItself() {
        XCTExpectFailure("le témoin doit faire échouer ce test", strict: true)
        HeadlessWitness.shared.note("écart simulé par HeadlessTests")
    }
}
