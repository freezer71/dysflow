import XCTest

/// La table des modèles de la page des réglages. Lancé à la main, avec
/// l'accord de l'utilisateur : `-only-testing:SillonUITests/ModelsUITests`.
/// La ligne « Voix de macOS » vient du moteur lui-même (`voices()`), même
/// sous le lecteur muet.
@MainActor
final class ModelsUITests: XCTestCase {
    func testTheSystemVoiceRowIsThereAndCannotBeDeleted() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let any = window.descendants(matching: .any)
        XCTAssertTrue(any["modele-system"].waitForExistence(timeout: 5), "ligne de la voix de macOS absente")
        XCTAssertFalse(any["supprimer-system"].exists, "la voix de macOS ne se supprime pas")
        XCTAssertFalse(any["telecharger-system"].exists, "ni ne se télécharge")
        XCTAssertTrue(any["utiliser-system"].exists)
        XCTAssertTrue(any["place-modeles"].exists, "place occupée et libre absente")
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }

    /// La table Dictée (macOS 26) : la dictée de macOS en tête, jamais
    /// supprimée ni téléchargée.
    func testTheSystemDictationRowIsThereAndCannotBeDeleted() throws {
        guard #available(macOS 26, *) else { throw XCTSkip("La dictée demande macOS 26") }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-settings"]
        app.launch()
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let any = window.descendants(matching: .any)
        XCTAssertTrue(any["modele-system-dictation"].waitForExistence(timeout: 5), "ligne de la dictée de macOS absente")
        XCTAssertFalse(any["supprimer-system-dictation"].exists, "la dictée de macOS ne se supprime pas")
        XCTAssertFalse(any["telecharger-system-dictation"].exists, "ni ne se télécharge")
        XCTAssertTrue(any["utiliser-system-dictation"].exists)
    }
}
