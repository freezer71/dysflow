import XCTest
@testable import Dysflow

/// La saisie sécurisée de macOS vaut pour toute la session : une application
/// en arrière-plan (Ghostty, Terminal avec « Secure Keyboard Entry ») la tient
/// et `IsSecureEventInputEnabled()` répond vrai partout. Seule celle qui est
/// au premier plan compte.
final class SecureInputTests: XCTestCase {
    let ghostty = "/Applications/Ghostty.app/Contents/MacOS/ghostty"

    func testOffMeansNothingIsBlocked() {
        XCTAssertFalse(SecureInput.applies(enabled: false, holderPath: ghostty,
                                           frontmostBundlePath: "/Applications/Ghostty.app"))
    }

    func testHeldByAnAppInTheBackgroundDoesNotBlock() {
        XCTAssertFalse(SecureInput.applies(enabled: true, holderPath: ghostty,
                                           frontmostBundlePath: "/Applications/Notes.app"))
    }

    func testHeldByTheFrontmostAppBlocks() {
        XCTAssertTrue(SecureInput.applies(enabled: true, holderPath: ghostty,
                                          frontmostBundlePath: "/Applications/Ghostty.app"))
    }

    /// Un processus auxiliaire, rangé dans le paquet de l'application.
    func testHeldByAHelperOfTheFrontmostAppBlocks() {
        let helper = "/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"
        XCTAssertTrue(SecureInput.applies(enabled: true, holderPath: helper,
                                          frontmostBundlePath: "/Applications/Google Chrome.app"))
    }

    /// « Ghostty.app » n'est pas dans « Ghost.app ».
    func testAPrefixOfTheNameIsNotTheSameApp() {
        XCTAssertFalse(SecureInput.applies(enabled: true, holderPath: ghostty,
                                           frontmostBundlePath: "/Applications/Ghost"))
    }

    /// Sans savoir qui la tient, ou qui est au premier plan : prudence.
    func testUnknownHolderOrFrontmostBlocks() {
        XCTAssertTrue(SecureInput.applies(enabled: true, holderPath: nil,
                                          frontmostBundlePath: "/Applications/Notes.app"))
        XCTAssertTrue(SecureInput.applies(enabled: true, holderPath: ghostty,
                                          frontmostBundlePath: nil))
    }
}
