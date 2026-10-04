import AppKit

/// Point d'entrée AppKit : le lecteur vit aussi longtemps que l'app.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static private(set) var shared: AppDelegate!

    /// Lancée par les tests d'interface : lecteur muet, texte d'exemple.
    let isUITesting = ProcessInfo.processInfo.arguments.contains("--ui-testing")
    private(set) var reader: SillonReader!
    private(set) var model: ReaderModel!
    private(set) var panel: PanelController!

    func applicationDidFinishLaunching(_ notification: Notification) {
        ReadingFont.registerBundled()
        Self.shared = self
        reader = isUITesting ? SillonReader.silent() : SillonReader()
        model = ReaderModel(reader: reader)
        model.refresh()
        panel = PanelController(model: model)
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.show() }
        }
    }

    static let sample = """
    # Les monades

    Une monade est une structure qui encapsule une valeur et un contexte. \
    Les monades permettent d'enchaîner des calculs qui portent un contexte, \
    sans le manipuler à la main à chaque étape.

    ## Exemples

    On les retrouve partout : options, listes, promesses.
    """
}
