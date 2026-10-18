import AppKit
import XCTest
@testable import Sillage

/// Ce que montre sillage au lancement et à la réouverture.
@MainActor
final class LaunchKindTests: XCTestCase {
    func testALaunchRightAfterBootCountsAsLogin() {
        XCTAssertTrue(LaunchKind.launchedAtLogin(event: nil, uptime: 30))
        XCTAssertFalse(LaunchKind.launchedAtLogin(event: nil, uptime: 3600))
    }

    func testALaunchToOpenFilesIsRecognised() {
        let open = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenDocuments,
                                          targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                          transactionID: AETransactionID(kAnyTransactionID))
        let plain = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenApplication,
                                           targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                           transactionID: AETransactionID(kAnyTransactionID))
        XCTAssertTrue(LaunchKind.launchedWithFiles(event: open))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: plain))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: nil))
    }

    /// À la connexion, rien ; avec des fichiers, c'est l'ouverture du
    /// fichier qui montre le lecteur.
    func testLoginAndFilesShowNothingAtLaunch() {
        XCTAssertEqual(LaunchKind.presentation(atLogin: true, withFiles: false, hasDocument: false), .nothing)
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: true, hasDocument: false), .nothing)
        XCTAssertEqual(LaunchKind.presentation(atLogin: true, withFiles: true, hasDocument: true), .nothing)
    }

    /// Lancé à la main : le lecteur vide, toujours en grand ; un document
    /// déjà là, la dernière taille.
    func testAManualLaunchOpensTheReader() {
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: false, hasDocument: false), .emptyReader)
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: false, hasDocument: true), .lastSize)
    }

    /// Rouvert depuis le Dock, le Finder ou Spotlight : comme un lancement manuel.
    func testReopeningShowsTheReader() {
        XCTAssertEqual(LaunchKind.reopenPresentation(hasDocument: false), .emptyReader)
        XCTAssertEqual(LaunchKind.reopenPresentation(hasDocument: true), .lastSize)
    }

    /// Aucune icône de Dock, jamais : `LSUIElement`, et l'app reste accessoire.
    func testSillageStaysOutOfTheDock() {
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "LSUIElement") as? Bool, true)
        XCTAssertEqual(NSApp.activationPolicy(), .accessory)
    }
}
